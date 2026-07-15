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
        // Fall-back: match local-loopback HTTP hooks (legacy crash recovery)
        h.get("type").and_then(|t| t.as_str()) == Some("http")
            && h.get("url")
                .and_then(|u| u.as_str())
                .map(|u| u.starts_with(URL_PREFIX) && u.contains("/hook/pre-tool-use/"))
                .unwrap_or(false)
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

/// Install our PreToolUse hook into `~/.claude/settings.json`.
///
/// `server_port`, `app_secret`, and `run_token` are baked into the hook URL.
/// First strips any stale entries from prior crashes, then prepends ours.
pub fn install_global_hook(
    server_port: u16,
    app_secret: &str,
    run_token: &str,
) -> std::io::Result<()> {
    let mut settings = strip_our_hooks(read_settings());

    let url = format!(
        "{}{}/hook/pre-tool-use/{}/{}",
        URL_PREFIX, server_port, app_secret, run_token
    );

    let our_entry = json!({
        "matcher": "^(Bash|Edit|Write|MultiEdit|mcp__.*)$",
        "hooks": [
            {
                "type": "http",
                "url": url,
                "timeout": 300,
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
