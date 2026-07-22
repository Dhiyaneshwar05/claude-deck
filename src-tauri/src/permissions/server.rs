//! Local HTTP permission server for Claude Code PreToolUse hooks.
//!
//! Mirrors clui-cc/src/main/hooks/permission-server.ts. See
//! docs/PHASES.md + memory `project_permission_server_spec.md` for protocol details.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

const PERMISSION_TIMEOUT_SECS: u64 = 300;
const MAX_BODY_BYTES: usize = 1_048_576;
/// Cap a single socket request line so a misbehaving bridge can't OOM us.
const MAX_SOCKET_LINE_BYTES: u64 = MAX_BODY_BYTES as u64;

/// Request payload Claude Code sends to our hook endpoint.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct HookToolRequest {
    pub session_id: String,
    pub transcript_path: String,
    pub cwd: String,
    #[serde(default)]
    pub permission_mode: String,
    pub hook_event_name: String,
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: serde_json::Value,
    #[serde(default)]
    pub tool_use_id: String,
}

/// The UI's final decision on a permission request.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionDecision {
    Allow,
    AllowSession,
    AllowDomain,
    Deny,
}

/// A permission request surfaced to the frontend for user decision.
#[derive(Debug, Clone, Serialize)]
pub struct PendingPermission {
    pub request_id: String,
    pub run_token: String,
    pub tab_id: String,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub session_id: String,
    pub cwd: String,
    /// Seconds until the backend auto-denies this request. The frontend uses it
    /// to show a live countdown so a request never sits "pending" forever in the
    /// UI while the backend has silently timed it out.
    pub timeout_secs: u64,
}

/// Emitted to the frontend when a pending request is resolved without a UI
/// click — either it hit the backend timeout, or its run was unregistered.
/// The frontend removes the matching card so it can't become a ghost.
#[derive(Debug, Clone, Serialize)]
pub struct PermissionExpired {
    pub request_id: String,
    pub reason: String,
}

/// Per-run registration created when we spawn a claude process.
#[derive(Debug, Clone)]
struct RunRegistration {
    tab_id: String,
    /// Session ID we use internally (before Claude assigns its own).
    internal_session_id: String,
}

/// A permission waiting on a decision from the frontend.
struct PendingRequest {
    responder: oneshot::Sender<PermissionDecision>,
    /// Real Claude session id from the hook event — used to scope "allow session".
    session_id: String,
    /// Tool this request is for — "allow session" is scoped per (session, tool).
    tool_name: String,
    /// Run token that owns this request, so `unregister_run` can wake it.
    run_token: String,
}

/// Build the scoped-allow key for a (session, tool) pair. "Allow session" grants
/// this specific tool for this specific Claude session — never a machine-wide
/// blanket allow (which is what the old `session:global:*` key silently did).
fn scoped_allow_key(session_id: &str, tool_name: &str) -> String {
    format!("session:{}:tool:{}", session_id, tool_name)
}

/// Shared state for the axum handlers.
#[derive(Clone)]
struct ServerState {
    app_secret: String,
    run_tokens: Arc<Mutex<HashMap<String, RunRegistration>>>,
    pending: Arc<Mutex<HashMap<String, PendingRequest>>>,
    /// session:<sessionId>:tool:<toolName> → allow forever
    scoped_allows: Arc<Mutex<std::collections::HashSet<String>>>,
    app_handle: AppHandle,
}

/// Normalized result of evaluating a single tool-permission request. The
/// transport layer (HTTP handler or unix-socket handler) renders this into the
/// wire format its caller expects.
#[derive(Debug, Clone)]
pub struct HookOutcome {
    pub allow: bool,
    pub reason: String,
}

impl HookOutcome {
    fn allow(reason: impl Into<String>) -> Self {
        HookOutcome { allow: true, reason: reason.into() }
    }
    fn deny(reason: impl Into<String>) -> Self {
        HookOutcome { allow: false, reason: reason.into() }
    }
}

/// Top-level permission server. Owned by Tauri state.
pub struct PermissionServer {
    pub app_secret: String,
    pub port: u16,
    /// Absolute path to the unix domain socket the command-bridge hook connects to.
    pub socket_path: PathBuf,
    state: ServerState,
}

impl PermissionServer {
    /// Start the HTTP server on 127.0.0.1, auto-incrementing port on conflict.
    pub async fn start(app_handle: AppHandle) -> Result<Self, String> {
        let app_secret = Uuid::new_v4().to_string();
        let state = ServerState {
            app_secret: app_secret.clone(),
            run_tokens: Arc::new(Mutex::new(HashMap::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
            scoped_allows: Arc::new(Mutex::new(std::collections::HashSet::new())),
            app_handle,
        };

        let app = Router::new()
            .route(
                "/hook/pre-tool-use/:secret/:token",
                post(handle_pre_tool_use),
            )
            .with_state(state.clone());

        // Bind with port auto-increment starting at 19837 (one above clui-cc's 19836
        // so we don't collide if both are running).
        let mut port = 19837u16;
        let listener = loop {
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => break l,
                Err(_) if port < 19900 => port += 1,
                Err(e) => return Err(format!("Port bind failed: {}", e)),
            }
        };

        let bound_port = listener.local_addr().map_err(|e| e.to_string())?.port();

        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        // Start the unix-socket listener for the command-bridge hook. This is the
        // Phase-1 transport that replaces the live-TCP-port http hook: a stale
        // socket path just makes the bridge's connect() fail fast instead of
        // hanging every Claude session (see docs/PERMISSION-HUB-V2-PLAN.md).
        let socket_path = default_socket_path();
        if let Err(e) = spawn_socket_listener(state.clone(), &socket_path).await {
            eprintln!("[permissions] failed to start unix socket listener: {}", e);
        }

        Ok(Self {
            app_secret,
            port: bound_port,
            socket_path,
            state,
        })
    }

    /// Register a new run, returning a run-token the hook URL will carry.
    pub async fn register_run(&self, tab_id: String, internal_session_id: String) -> String {
        let token = Uuid::new_v4().to_string();
        self.state.run_tokens.lock().await.insert(
            token.clone(),
            RunRegistration {
                tab_id,
                internal_session_id,
            },
        );
        token
    }

    /// Remove a run. Any pending requests owned by this run are woken with a
    /// deny so the blocking bridge process returns immediately instead of
    /// hanging until the 300s timeout (and its `oneshot` sender leaking).
    pub async fn unregister_run(&self, run_token: &str) {
        self.state.run_tokens.lock().await.remove(run_token);

        let mut pending = self.state.pending.lock().await;
        let orphaned: Vec<String> = pending
            .iter()
            .filter(|(_, req)| req.run_token == run_token)
            .map(|(id, _)| id.clone())
            .collect();
        for id in orphaned {
            if let Some(req) = pending.remove(&id) {
                let _ = req.responder.send(PermissionDecision::Deny);
                // Clear the card in the UI too — the run is gone, so this
                // request can never be resolved by a click.
                let expired = PermissionExpired {
                    request_id: id.clone(),
                    reason: "Session ended".to_string(),
                };
                let _ = self.state.app_handle.emit("permission-expired", &expired);
                let _ = self
                    .state
                    .app_handle
                    .emit_to("main", "permission-expired", &expired);
            }
        }
    }

    /// Frontend calls this to resolve a pending permission. The scoped-allow key
    /// comes from the pending request itself (real Claude session id + tool),
    /// NOT from the run token — so "allow session" grants exactly one tool for
    /// one session, never a machine-wide blanket allow.
    pub async fn resolve(
        &self,
        request_id: &str,
        decision: PermissionDecision,
        _run_token: &str,
    ) -> Result<(), String> {
        let mut pending = self.state.pending.lock().await;
        let req = pending
            .remove(request_id)
            .ok_or_else(|| format!("No pending request {}", request_id))?;

        if decision == PermissionDecision::AllowSession {
            let mut scoped = self.state.scoped_allows.lock().await;
            scoped.insert(scoped_allow_key(&req.session_id, &req.tool_name));
        }

        let _ = req.responder.send(decision);
        Ok(())
    }

    /// List currently active session-scoped allows (for the UI to display/revoke).
    pub async fn list_scoped_allows(&self) -> Vec<String> {
        let mut v: Vec<String> = self.state.scoped_allows.lock().await.iter().cloned().collect();
        v.sort();
        v
    }

    /// Revoke a single scoped allow (or all, when `key` is None).
    pub async fn clear_scoped_allows(&self, key: Option<&str>) {
        let mut scoped = self.state.scoped_allows.lock().await;
        match key {
            Some(k) => {
                scoped.remove(k);
            }
            None => scoped.clear(),
        }
    }
}

// ── HTTP handler ─────────────────────────────────────────────

#[derive(Serialize)]
struct HookResponse {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: HookSpecificOutput,
}

#[derive(Serialize)]
struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: &'static str,
    #[serde(rename = "permissionDecision")]
    permission_decision: &'static str,
    #[serde(rename = "permissionDecisionReason")]
    permission_decision_reason: String,
}

fn deny_response(reason: impl Into<String>) -> HookResponse {
    HookResponse {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: "PreToolUse",
            permission_decision: "deny",
            permission_decision_reason: reason.into(),
        },
    }
}

fn allow_response(reason: impl Into<String>) -> HookResponse {
    HookResponse {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: "PreToolUse",
            permission_decision: "allow",
            permission_decision_reason: reason.into(),
        },
    }
}

async fn handle_pre_tool_use(
    State(state): State<ServerState>,
    Path((secret, token)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Fail-closed on all guard failures: return 200 + deny response.

    if body.len() > MAX_BODY_BYTES {
        return (StatusCode::PAYLOAD_TOO_LARGE, Json(deny_response("Body too large"))).into_response();
    }

    if secret != state.app_secret {
        return (StatusCode::FORBIDDEN, Json(deny_response("Invalid secret"))).into_response();
    }

    let req: HookToolRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(deny_response(format!("Bad JSON: {}", e)))).into_response();
        }
    };

    match evaluate(&state, &token, req).await {
        Ok(outcome) if outcome.allow => {
            (StatusCode::OK, Json(allow_response(outcome.reason))).into_response()
        }
        Ok(outcome) => (StatusCode::OK, Json(deny_response(outcome.reason))).into_response(),
        Err(EvaluateError::UnknownRunToken) => {
            (StatusCode::FORBIDDEN, Json(deny_response("Unknown run token"))).into_response()
        }
        Err(EvaluateError::WrongHookEvent) => {
            (StatusCode::BAD_REQUEST, Json(deny_response("Wrong hook event"))).into_response()
        }
    }
}

/// Why the shared decision core could not produce an allow/deny outcome.
enum EvaluateError {
    UnknownRunToken,
    WrongHookEvent,
}

/// Shared decision core used by every transport (HTTP handler + unix-socket
/// bridge). Applies the run-token check, fast paths, then emits to the UI and
/// blocks for a human decision (or times out → deny).
async fn evaluate(
    state: &ServerState,
    token: &str,
    req: HookToolRequest,
) -> Result<HookOutcome, EvaluateError> {
    let registration = {
        let map = state.run_tokens.lock().await;
        map.get(token).cloned()
    };
    let registration = registration.ok_or(EvaluateError::UnknownRunToken)?;

    if req.hook_event_name != "PreToolUse" {
        return Err(EvaluateError::WrongHookEvent);
    }

    // Fast path 1: honor the user's OWN Claude permission policy
    // (~/.claude/settings.json + settings.local.json). This is authoritative —
    // a matching allow/deny rule short-circuits without ever prompting, exactly
    // as `claude` itself would. Only an `ask` outcome falls through to the human
    // queue. Loaded per-call (stateless, like preloop) so live config edits and
    // `settings.local.json` changes take effect immediately.
    let policy = match crate::permissions::claude_policy::load_claude_permission_policy() {
        Ok(p) => p,
        Err(e) => {
            // A broken/unreadable settings file must not brick tool calls:
            // treat as no policy (everything escalates to `ask`) and log.
            eprintln!("[permissions] failed to load Claude policy (treating as empty): {}", e);
            crate::permissions::claude_policy::ClaudePermissionPolicy::default()
        }
    };
    use crate::permissions::claude_policy::PolicyDecision;
    match crate::permissions::claude_policy::evaluate_claude_permission_policy(
        &policy,
        &req.permission_mode,
        &req.tool_name,
        &req.tool_input,
    ) {
        PolicyDecision::Allow => {
            return Ok(HookOutcome::allow("Allowed by your Claude settings"));
        }
        PolicyDecision::Deny => {
            return Ok(HookOutcome::deny("Denied by your Claude settings"));
        }
        PolicyDecision::Ask => { /* fall through to the fast paths + human queue */ }
    }

    // Fast path 2: safe bash auto-approve. Secondary convenience for obviously
    // read-only commands the user didn't explicitly list — but an explicit
    // `ask` rule always wins, so this never overrides a deliberate "review this".
    if req.tool_name == "Bash"
        && !crate::permissions::claude_policy::has_explicit_ask_rule(
            &policy,
            &req.tool_name,
            &req.tool_input,
        )
    {
        if let Some(cmd) = req.tool_input.get("command").and_then(|v| v.as_str()) {
            if crate::permissions::safe_bash::is_safe_bash_command(cmd) {
                return Ok(HookOutcome::allow("Auto-approved: read-only command"));
            }
        }
    }

    // Fast path: scoped allow-session — keyed on the REAL Claude session id +
    // tool, so a prior "allow session" only short-circuits the same tool in the
    // same session (never every session on the machine).
    {
        let scoped = state.scoped_allows.lock().await;
        if scoped.contains(&scoped_allow_key(&req.session_id, &req.tool_name)) {
            return Ok(HookOutcome::allow("Session-scoped allow"));
        }
    }

    // Emit to frontend + wait for decision (or timeout)
    let request_id = Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<PermissionDecision>();

    state.pending.lock().await.insert(
        request_id.clone(),
        PendingRequest {
            responder: tx,
            session_id: req.session_id.clone(),
            tool_name: req.tool_name.clone(),
            run_token: token.to_string(),
        },
    );

    let pending_payload = PendingPermission {
        request_id: request_id.clone(),
        run_token: token.to_string(),
        tab_id: registration.tab_id.clone(),
        tool_name: req.tool_name.clone(),
        tool_input: mask_sensitive(req.tool_input.clone()),
        session_id: req.session_id.clone(),
        cwd: req.cwd.clone(),
        timeout_secs: PERMISSION_TIMEOUT_SECS,
    };

    eprintln!(
        "[permissions] emitting permission-request: tool={} session={} request_id={}",
        pending_payload.tool_name, pending_payload.session_id, request_id
    );
    // Try app-wide emit first
    match state.app_handle.emit("permission-request", &pending_payload) {
        Ok(_) => eprintln!("[permissions] emit (app) succeeded"),
        Err(e) => eprintln!("[permissions] emit (app) FAILED: {}", e),
    }
    // Also emit directly to the main window (more reliable from background tokio tasks)
    match state
        .app_handle
        .emit_to("main", "permission-request", &pending_payload)
    {
        Ok(_) => eprintln!("[permissions] emit_to(main) succeeded"),
        Err(e) => eprintln!("[permissions] emit_to(main) FAILED: {}", e),
    }

    let decision = match tokio::time::timeout(Duration::from_secs(PERMISSION_TIMEOUT_SECS), rx).await {
        Ok(Ok(d)) => d,
        _ => {
            state.pending.lock().await.remove(&request_id);
            // Tell the frontend so the card doesn't linger as a ghost the user
            // can never resolve (its request_id is gone from the backend now).
            let expired = PermissionExpired {
                request_id: request_id.clone(),
                reason: "Timed out after 5 minutes".to_string(),
            };
            let _ = state.app_handle.emit("permission-expired", &expired);
            let _ = state.app_handle.emit_to("main", "permission-expired", &expired);
            return Ok(HookOutcome::deny("Permission timed out after 5 minutes"));
        }
    };

    match decision {
        PermissionDecision::Deny => Ok(HookOutcome::deny("User denied")),
        _ => Ok(HookOutcome::allow("User approved")),
    }
}

// ── Unix-socket transport (Phase 1 command bridge) ───────────

/// Compute the default socket path: `$TMPDIR/claude-deck/perm.sock`.
pub fn default_socket_path() -> PathBuf {
    std::env::temp_dir().join("claude-deck").join("perm.sock")
}

/// Line-delimited JSON request the `claude-deck-hook` bridge sends over the
/// socket. Carries the app secret + run token (same auth as the HTTP URL path)
/// plus the raw Claude PreToolUse event.
#[derive(Deserialize)]
struct SocketRequest {
    secret: String,
    token: String,
    event: HookToolRequest,
}

/// Line-delimited JSON response we send back to the bridge.
#[derive(Serialize)]
struct SocketResponse {
    allow: bool,
    reason: String,
}

/// Bind the unix domain socket and accept bridge connections. Removes any stale
/// socket file first (a leftover path is harmless — connect just fails — but we
/// can't bind over an existing file).
async fn spawn_socket_listener(state: ServerState, socket_path: &PathBuf) -> std::io::Result<()> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }
    // A previous run may have left the socket file behind.
    let _ = std::fs::remove_file(socket_path);

    let listener = tokio::net::UnixListener::bind(socket_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600));
    }
    eprintln!("[permissions] unix socket listening at {}", socket_path.display());

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let state = state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_socket_conn(state, stream).await {
                            eprintln!("[permissions] socket conn error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    eprintln!("[permissions] socket accept error: {}", e);
                    break;
                }
            }
        }
    });
    Ok(())
}

/// Handle one bridge connection: read a single JSON request line, run the
/// shared decision core, write a single JSON response line.
async fn handle_socket_conn(
    state: ServerState,
    stream: tokio::net::UnixStream,
) -> std::io::Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half.take(MAX_SOCKET_LINE_BYTES));
    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Ok(());
    }

    let resp = match serde_json::from_str::<SocketRequest>(&line) {
        Ok(req) if req.secret != state.app_secret => {
            SocketResponse { allow: false, reason: "Invalid secret".into() }
        }
        Ok(req) => match evaluate(&state, &req.token, req.event).await {
            Ok(outcome) => SocketResponse { allow: outcome.allow, reason: outcome.reason },
            Err(EvaluateError::UnknownRunToken) => {
                SocketResponse { allow: false, reason: "Unknown run token".into() }
            }
            Err(EvaluateError::WrongHookEvent) => {
                SocketResponse { allow: false, reason: "Wrong hook event".into() }
            }
        },
        Err(e) => SocketResponse { allow: false, reason: format!("Bad JSON: {}", e) },
    };

    let mut out = serde_json::to_string(&resp).unwrap_or_else(|_| {
        "{\"allow\":false,\"reason\":\"encode error\"}".to_string()
    });
    out.push('\n');
    write_half.write_all(out.as_bytes()).await?;
    write_half.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::scoped_allow_key;

    #[test]
    fn scoped_key_is_per_session_and_tool() {
        // Two different sessions approving the same tool must NOT share a key,
        // and the same session's other tools must NOT be covered. This guards
        // the regression where a single "global:*" key allowed everything.
        let a = scoped_allow_key("sess-A", "Bash");
        let b = scoped_allow_key("sess-B", "Bash");
        let c = scoped_allow_key("sess-A", "Write");
        assert_ne!(a, b, "different sessions must have distinct keys");
        assert_ne!(a, c, "different tools must have distinct keys");
        assert_eq!(a, "session:sess-A:tool:Bash");
        // No wildcard: the key must never be a blanket per-session allow.
        assert!(!a.contains('*'));
    }
}

/// Recursively redact sensitive fields before sending to the UI.
fn mask_sensitive(val: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match val {
        Value::Object(mut map) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            for k in keys {
                let lower = k.to_lowercase();
                let is_secret = ["token", "password", "secret", "auth", "credential", "apikey", "api_key"]
                    .iter()
                    .any(|s| lower.contains(s));
                if is_secret {
                    map.insert(k, Value::String("***".into()));
                } else if let Some(v) = map.remove(&k) {
                    map.insert(k, mask_sensitive(v));
                }
            }
            Value::Object(map)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(mask_sensitive).collect()),
        other => other,
    }
}
