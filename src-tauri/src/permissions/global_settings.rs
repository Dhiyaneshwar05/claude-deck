//! Manages the global `~/.claude/settings.json` PreToolUse hook injection.
//!
//! This makes every Claude Code session on the machine (Cursor, VS Code, terminal,
//! Claude Deck-spawned) route permission prompts through our local HTTP server.
//!
//! Strategy:
//! - On app start: read settings.json, strip any stale `claude-deck` hooks,
//!   then inject a fresh PreToolUse hook pointing at our server URL with a
//!   per-app-launch run token.
//! - On app exit: remove the hook entry we installed.
//!
//! Crash safety: if the app dies without unregistering, the next start will
//! detect the stale entry (matched by URL prefix `http://127.0.0.1:` and our
//! marker tag) and replace it.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};

const MARKER: &str = "claude-deck";
const URL_PREFIX: &str = "http://127.0.0.1:";
/// Substring that identifies our command-bridge hook (Phase 1) for crash recovery.
const BRIDGE_BIN: &str = "claude-deck-hook";

/// Resolve the absolute path to the `claude-deck-hook` bridge binary. In dev it
/// sits next to the main app binary (`target/debug/claude-deck-hook`); in a
/// bundled release it's shipped alongside the app executable. We derive it from
/// the current executable's directory.
pub fn bridge_binary_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidate = dir.join(BRIDGE_BIN);
    if candidate.exists() {
        Some(candidate)
    } else {
        // Fall back to the plain name and let PATH resolve it (last resort).
        Some(PathBuf::from(BRIDGE_BIN))
    }
}

fn settings_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("settings.json"))
}

/// Read settings.json (returns empty object if missing or unparseable).
fn read_settings() -> Value {
    match settings_path().and_then(|p| fs::read_to_string(&p).ok()) {
        Some(s) => serde_json::from_str(&s).unwrap_or_else(|_| json!({})),
        None => json!({}),
    }
}

fn write_settings(value: &Value) -> std::io::Result<()> {
    let path = settings_path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir")
    })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let pretty = serde_json::to_string_pretty(value).unwrap();
    fs::write(&path, pretty)?;
    Ok(())
}

/// True if the given hook entry is one we (or a prior crash of us) installed.
fn is_ours(hook_entry: &Value) -> bool {
    let hooks = match hook_entry.get("hooks").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return false,
    };
    hooks.iter().any(|h| {
        let is_marker = h.get(MARKER).and_then(|v| v.as_bool()).unwrap_or(false);
        if is_marker {
            return true;
        }
        // Fall-back: match legacy local-loopback HTTP hooks (pre-Phase-1 crash recovery)
        let legacy_http = h.get("type").and_then(|t| t.as_str()) == Some("http")
            && h.get("url")
                .and_then(|u| u.as_str())
                .map(|u| u.starts_with(URL_PREFIX) && u.contains("/hook/pre-tool-use/"))
                .unwrap_or(false);
        // Fall-back: match our command-bridge hook by its binary name (crash recovery)
        let bridge_cmd = h.get("type").and_then(|t| t.as_str()) == Some("command")
            && h.get("command")
                .and_then(|c| c.as_str())
                .map(|c| c.contains(BRIDGE_BIN))
                .unwrap_or(false);
        legacy_http || bridge_cmd
    })
}

/// Strip every `claude-deck`-owned PreToolUse hook from the settings tree.
/// Returns the cleaned value.
fn strip_our_hooks(mut settings: Value) -> Value {
    let hooks_obj = match settings.get_mut("hooks").and_then(|v| v.as_object_mut()) {
        Some(o) => o,
        None => return settings,
    };

    if let Some(pre) = hooks_obj.get_mut("PreToolUse").and_then(|v| v.as_array_mut()) {
        pre.retain(|entry| !is_ours(entry));
        // If the PreToolUse array is now empty, remove the key entirely
        if pre.is_empty() {
            hooks_obj.remove("PreToolUse");
        }
    }

    // If the hooks object is now empty, remove the key entirely
    if hooks_obj.is_empty() {
        if let Value::Object(map) = &mut settings {
            map.remove("hooks");
        }
    }

    settings
}

/// Build the shell command string that Claude will invoke for the command hook.
/// Paths are single-quoted to survive spaces in the socket / binary path.
fn bridge_command(bridge_bin: &std::path::Path, socket_path: &std::path::Path, app_secret: &str, run_token: &str) -> String {
    format!(
        "'{}' --socket '{}' --secret '{}' --token '{}' --fail-native",
        bridge_bin.display(),
        socket_path.display(),
        app_secret,
        run_token,
    )
}

/// Install our PreToolUse hook into `~/.claude/settings.json`.
///
/// Phase 1: a `"type":"command"` bridge over a unix socket. The bridge binary,
/// socket path, app secret, and run token are baked into the command string.
/// First strips any stale entries from prior crashes, then prepends ours.
///
/// Unlike the old http hook (which pointed at a live TCP port and hung every
/// session when the app died), a stale command hook just fails connect() fast
/// and fails open — the dangling-hook bug is structurally gone.
pub fn install_global_hook(
    bridge_bin: &std::path::Path,
    socket_path: &std::path::Path,
    app_secret: &str,
    run_token: &str,
) -> std::io::Result<()> {
    let mut settings = strip_our_hooks(read_settings());

    let command = bridge_command(bridge_bin, socket_path, app_secret, run_token);

    let our_entry = json!({
        "matcher": "^(Bash|Edit|Write|MultiEdit|mcp__.*)$",
        "hooks": [
            {
                "type": "command",
                "command": command,
                "timeout": 310,
                MARKER: true
            }
        ]
    });

    // Make sure settings.hooks.PreToolUse is an array, then prepend our entry
    if !settings.is_object() {
        settings = json!({});
    }
    let root = settings.as_object_mut().unwrap();

    let hooks = root
        .entry("hooks".to_string())
        .or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let hooks_map = hooks.as_object_mut().unwrap();

    let pre = hooks_map
        .entry("PreToolUse".to_string())
        .or_insert_with(|| json!([]));
    if !pre.is_array() {
        *pre = json!([]);
    }
    let pre_arr = pre.as_array_mut().unwrap();

    pre_arr.insert(0, our_entry);

    write_settings(&settings)
}

/// Remove our PreToolUse hook(s) from `~/.claude/settings.json`. Best-effort.
pub fn uninstall_global_hook() -> std::io::Result<()> {
    let cleaned = strip_our_hooks(read_settings());
    write_settings(&cleaned)
}
