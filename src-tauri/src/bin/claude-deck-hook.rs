//! `claude-deck-hook` — the stateless PreToolUse command-bridge.
//!
//! This binary is what `~/.claude/settings.json` invokes as a `"type":"command"`
//! hook (see permissions::global_settings). Modeled on preloop's
//! `agents permission-hook`, it exists to structurally eliminate the
//! dangling-hook bug: unlike the old `"type":"http"` hook that pointed at a
//! live TCP port (and hung EVERY Claude session for the full timeout when the
//! app was dead), this process:
//!
//!   1. Reads the Claude PreToolUse event JSON from STDIN.
//!   2. Connects to the running Claude Deck app over a UNIX DOMAIN SOCKET.
//!      A stale socket path just makes connect() fail instantly (ENOENT /
//!      ECONNREFUSED) — no hang, ever.
//!   3. Forwards the event (wrapped with the app secret + run token for auth),
//!      blocks for the decision (up to the hook timeout Claude enforces).
//!   4. Writes Claude's decision JSON to STDOUT and exits 0.
//!   5. If the app is unreachable, emits the configured fail default and exits 0.
//!
//! Kept dependency-light on purpose: only std + serde_json.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::{json, Value};

/// How long we allow the whole socket round-trip to take. The app's own
/// per-request timeout is 300s (deny-on-expiry); we add headroom so the app's
/// timeout fires before ours, mirroring preloop's 310s > 300s coordination.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(310);

/// What to do when Claude Deck is unreachable (socket connect fails) or the
/// bridge hits a hard error. `Native` is the default and the safe choice:
/// emit no decision so Claude Code falls back to its OWN permission flow —
/// identical to the hub not existing. `Open`/`Closed` are opt-in overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailMode {
    /// Defer to Claude's native permissions (empty stdout, exit 0).
    Native,
    /// Silently allow. Dangerous — only for trusted throwaway setups.
    Open,
    /// Deny everything. "Lockdown": if Deck isn't watching, block the call.
    Closed,
}

struct Args {
    socket: String,
    secret: String,
    token: String,
    fail_mode: FailMode,
}

fn parse_args() -> Result<Args, String> {
    let mut socket = String::new();
    let mut secret = String::new();
    let mut token = String::new();
    let mut fail_mode = FailMode::Native;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--socket" => socket = it.next().ok_or("--socket needs a value")?,
            "--secret" => secret = it.next().ok_or("--secret needs a value")?,
            "--token" => token = it.next().ok_or("--token needs a value")?,
            "--fail-native" => fail_mode = FailMode::Native,
            "--fail-open" => fail_mode = FailMode::Open,
            "--fail-closed" => fail_mode = FailMode::Closed,
            other => return Err(format!("unknown argument: {}", other)),
        }
    }
    if socket.is_empty() {
        return Err("--socket is required".into());
    }
    Ok(Args { socket, secret, token, fail_mode })
}

/// Emit the Claude PreToolUse decision to STDOUT and exit 0.
fn emit_decision(allow: bool, reason: &str) -> ! {
    let payload = json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": if allow { "allow" } else { "deny" },
            "permissionDecisionReason": reason,
        }
    });
    // A parse-failure here is impossible for this fixed shape, but never hang.
    let line = serde_json::to_string(&payload)
        .unwrap_or_else(|_| "{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"allow\",\"permissionDecisionReason\":\"encode error\"}}".to_string());
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "{}", line);
    let _ = stdout.flush();
    std::process::exit(0);
}

/// Defer to Claude's own permission flow: write NOTHING to stdout and exit 0.
/// Per Claude Code's hook contract, a success exit with no `permissionDecision`
/// is a no-op — the tool call proceeds through Claude's normal allow/deny/ask
/// evaluation, exactly as if this hook weren't installed. We log the reason to
/// stderr (shown in the transcript, non-blocking) so the fallback is visible.
fn defer_to_native(reason: &str) -> ! {
    let _ = writeln!(std::io::stderr(), "[claude-deck-hook] deferring to native permissions: {}", reason);
    std::process::exit(0);
}

/// Fail default when we cannot reach the app or hit a hard error.
fn fail_default(mode: FailMode, reason: &str) -> ! {
    match mode {
        FailMode::Native => defer_to_native(reason),
        FailMode::Open => emit_decision(true, &format!("Claude Deck unreachable, fail-open: {}", reason)),
        FailMode::Closed => emit_decision(false, &format!("Claude Deck unreachable, fail-closed: {}", reason)),
    }
}

fn run() -> Result<(), (FailMode, String)> {
    // (FailMode, String) = (fail mode for THIS error, reason). We can't know the
    // configured mode until args are parsed, so an arg-parse failure defers to
    // native — a broken install must never brick other sessions.
    let args = parse_args().map_err(|e| (FailMode::Native, e))?;

    // Read Claude's PreToolUse event from stdin.
    let mut raw = String::new();
    std::io::stdin()
        .read_to_string(&mut raw)
        .map_err(|e| (args.fail_mode, format!("failed to read stdin: {}", e)))?;

    let event: Value = if raw.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&raw)
            .map_err(|e| (args.fail_mode, format!("invalid hook event JSON: {}", e)))?
    };

    // Wrap with auth + forward over the socket.
    let request = json!({
        "secret": args.secret,
        "token": args.token,
        "event": event,
    });

    let mut stream = UnixStream::connect(&args.socket)
        .map_err(|e| (args.fail_mode, format!("connect {}: {}", args.socket, e)))?;
    let _ = stream.set_read_timeout(Some(SOCKET_TIMEOUT));
    let _ = stream.set_write_timeout(Some(SOCKET_TIMEOUT));

    let mut line = serde_json::to_string(&request)
        .map_err(|e| (args.fail_mode, format!("encode request: {}", e)))?;
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .map_err(|e| (args.fail_mode, format!("write socket: {}", e)))?;
    stream
        .flush()
        .map_err(|e| (args.fail_mode, format!("flush socket: {}", e)))?;

    // Read the single-line JSON response.
    let mut resp_raw = String::new();
    stream
        .read_to_string(&mut resp_raw)
        .map_err(|e| (args.fail_mode, format!("read socket: {}", e)))?;

    let resp: Value = serde_json::from_str(resp_raw.trim())
        .map_err(|e| (args.fail_mode, format!("invalid response JSON: {}", e)))?;

    let allow = resp.get("allow").and_then(|v| v.as_bool()).unwrap_or(false);
    let reason = resp
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or(if allow { "Approved" } else { "Denied" })
        .to_string();
    emit_decision(allow, &reason);
}

fn main() {
    if let Err((mode, reason)) = run() {
        fail_default(mode, &reason);
    }
}
