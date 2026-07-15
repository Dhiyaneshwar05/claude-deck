//! Local HTTP permission server for Claude Code PreToolUse hooks.
//!
//! Mirrors clui-cc/src/main/hooks/permission-server.ts. See
//! docs/PHASES.md + memory `project_permission_server_spec.md` for protocol details.

use std::collections::HashMap;
use std::net::SocketAddr;
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
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

const PERMISSION_TIMEOUT_SECS: u64 = 300;
const MAX_BODY_BYTES: usize = 1_048_576;

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

/// Top-level permission server. Owned by Tauri state.
pub struct PermissionServer {
    pub app_secret: String,
    pub port: u16,
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

        Ok(Self {
            app_secret,
            port: bound_port,
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

    /// Remove a run. Any pending requests for this run auto-deny.
    pub async fn unregister_run(&self, run_token: &str) {
        self.state.run_tokens.lock().await.remove(run_token);
        // Auto-deny any orphaned pending requests
        let mut pending = self.state.pending.lock().await;
        pending.retain(|_, _| true); // TODO: filter by run_token when we attach it
    }

    /// Frontend calls this to resolve a pending permission.
    pub async fn resolve(
        &self,
        request_id: &str,
        decision: PermissionDecision,
        run_token: &str,
    ) -> Result<(), String> {
        // Record scoped allow if applicable
        if decision == PermissionDecision::AllowSession {
            if let Some(reg) = self.state.run_tokens.lock().await.get(run_token) {
                let mut scoped = self.state.scoped_allows.lock().await;
                // Key needs tool_name — we don't have it here; simplified v1 stores
                // "session:<sid>:*" meaning allow everything for session.
                scoped.insert(format!("session:{}:*", reg.internal_session_id));
            }
        }

        let mut pending = self.state.pending.lock().await;
        let req = pending
            .remove(request_id)
            .ok_or_else(|| format!("No pending request {}", request_id))?;
        let _ = req.responder.send(decision);
        Ok(())
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

    let registration = {
        let map = state.run_tokens.lock().await;
        map.get(&token).cloned()
    };
    let registration = match registration {
        Some(r) => r,
        None => return (StatusCode::FORBIDDEN, Json(deny_response("Unknown run token"))).into_response(),
    };

    let req: HookToolRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(deny_response(format!("Bad JSON: {}", e)))).into_response();
        }
    };

    if req.hook_event_name != "PreToolUse" {
        return (StatusCode::BAD_REQUEST, Json(deny_response("Wrong hook event"))).into_response();
    }

    // Fast path: safe bash auto-approve
    if req.tool_name == "Bash" {
        if let Some(cmd) = req.tool_input.get("command").and_then(|v| v.as_str()) {
            if crate::permissions::safe_bash::is_safe_bash_command(cmd) {
                return (StatusCode::OK, Json(allow_response("Auto-approved: read-only command"))).into_response();
            }
        }
    }

    // Fast path: scoped allow-session
    {
        let scoped = state.scoped_allows.lock().await;
        if scoped.contains(&format!("session:{}:*", registration.internal_session_id)) {
            return (StatusCode::OK, Json(allow_response("Session-scoped allow"))).into_response();
        }
    }

    // Emit to frontend + wait for decision (or timeout)
    let request_id = Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<PermissionDecision>();

    state.pending.lock().await.insert(
        request_id.clone(),
        PendingRequest { responder: tx },
    );

    let pending_payload = PendingPermission {
        request_id: request_id.clone(),
        run_token: token.clone(),
        tab_id: registration.tab_id.clone(),
        tool_name: req.tool_name.clone(),
        tool_input: mask_sensitive(req.tool_input.clone()),
        session_id: req.session_id.clone(),
        cwd: req.cwd.clone(),
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
    use tauri::Emitter;
    match state
        .app_handle
        .emit_to("main", "permission-request", &pending_payload)
    {
        Ok(_) => eprintln!("[permissions] emit_to(main) succeeded"),
        Err(e) => eprintln!("[permissions] emit_to(main) FAILED: {}", e),
    }
    // List existing webviews for diagnostics
    use tauri::Manager;
    let labels: Vec<String> = state
        .app_handle
        .webview_windows()
        .keys()
        .cloned()
        .collect();
    eprintln!("[permissions] available webview windows: {:?}", labels);

    let decision = match tokio::time::timeout(Duration::from_secs(PERMISSION_TIMEOUT_SECS), rx).await {
        Ok(Ok(d)) => d,
        _ => {
            state.pending.lock().await.remove(&request_id);
            return (StatusCode::OK, Json(deny_response("Permission timed out after 5 minutes"))).into_response();
        }
    };

    match decision {
        PermissionDecision::Deny => (StatusCode::OK, Json(deny_response("User denied"))).into_response(),
        _ => (StatusCode::OK, Json(allow_response("User approved"))).into_response(),
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
