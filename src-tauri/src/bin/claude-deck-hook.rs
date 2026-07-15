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

struct Args {
    socket: String,
    secret: String,
    token: String,
    /// On unreachable app / hard failure: allow (true) vs deny (false).
    fail_open: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut socket = String::new();
    let mut secret = String::new();
    let mut token = String::new();
    let mut fail_open = false;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--socket" => socket = it.next().ok_or("--socket needs a value")?,
            "--secret" => secret = it.next().ok_or("--secret needs a value")?,
            "--token" => token = it.next().ok_or("--token needs a value")?,
            "--fail-open" => fail_open = true,
            "--fail-closed" => fail_open = false,
            other => return Err(format!("unknown argument: {}", other)),
        }
    }
    if socket.is_empty() {
        return Err("--socket is required".into());
    }
    Ok(Args { socket, secret, token, fail_open })
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

/// Fail default when we cannot reach the app or hit a hard error.
fn fail_default(fail_open: bool, reason: &str) -> ! {
    if fail_open {
        emit_decision(true, &format!("Claude Deck unreachable, fail-open: {}", reason));
    } else {
        emit_decision(false, &format!("Claude Deck unreachable, fail-closed: {}", reason));
    }
}

fn run() -> Result<(), (bool, String)> {
    // (bool, String) = (fail_open_used_for_this_error, reason). We resolve
    // fail_open before we know args on the earliest failures, so default to
    // fail-open there so a broken install never bricks other sessions.
    let args = parse_args().map_err(|e| (true, e))?;

    // Read Claude's PreToolUse event from stdin.
    let mut raw = String::new();
    std::io::stdin()
        .read_to_string(&mut raw)
        .map_err(|e| (args.fail_open, format!("failed to read stdin: {}", e)))?;

    let event: Value = if raw.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&raw)
            .map_err(|e| (args.fail_open, format!("invalid hook event JSON: {}", e)))?
    };

    // Wrap with auth + forward over the socket.
    let request = json!({
        "secret": args.secret,
        "token": args.token,
        "event": event,
    });

    let mut stream = UnixStream::connect(&args.socket)
        .map_err(|e| (args.fail_open, format!("connect {}: {}", args.socket, e)))?;
    let _ = stream.set_read_timeout(Some(SOCKET_TIMEOUT));
    let _ = stream.set_write_timeout(Some(SOCKET_TIMEOUT));

    let mut line = serde_json::to_string(&request)
        .map_err(|e| (args.fail_open, format!("encode request: {}", e)))?;
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .map_err(|e| (args.fail_open, format!("write socket: {}", e)))?;
    stream
        .flush()
        .map_err(|e| (args.fail_open, format!("flush socket: {}", e)))?;

    // Read the single-line JSON response.
    let mut resp_raw = String::new();
    stream
        .read_to_string(&mut resp_raw)
        .map_err(|e| (args.fail_open, format!("read socket: {}", e)))?;

    let resp: Value = serde_json::from_str(resp_raw.trim())
        .map_err(|e| (args.fail_open, format!("invalid response JSON: {}", e)))?;

    let allow = resp.get("allow").and_then(|v| v.as_bool()).unwrap_or(false);
    let reason = resp
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or(if allow { "Approved" } else { "Denied" })
        .to_string();
    emit_decision(allow, &reason);
}

fn main() {
    if let Err((fail_open, reason)) = run() {
        fail_default(fail_open, &reason);
    }
}
