use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

/// Write a per-run Claude settings file containing a command-bridge PreToolUse hook.
/// Returns the absolute path to the file so the caller can pass `--settings <path>` to claude.
///
///   $TMPDIR/claude-deck-hook-config/claude-deck-hook-<runToken>.json
///   dir mode 0o700, file mode 0o600
///
/// Phase 1: emits the same `"type":"command"` bridge form as the global hook so
/// hub-spawned sessions never leave a live-port hook that can hang on crash.
pub fn write_hook_settings_file(
    run_token: &str,
    bridge_bin: &Path,
    socket_path: &Path,
    app_secret: &str,
) -> std::io::Result<PathBuf> {
    let tmp = std::env::temp_dir();
    let dir = tmp.join("claude-deck-hook-config");
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }

    let path = dir.join(format!("claude-deck-hook-{}.json", run_token));
    // --fail-native: if the app is unreachable, emit NO decision so Claude falls
    // back to its own permission prompt — never silently allow (fail-open) a tool
    // call that could be destructive. This matches the global hook installer.
    let command = format!(
        "'{}' --socket '{}' --secret '{}' --token '{}' --fail-native",
        bridge_bin.display(),
        socket_path.display(),
        app_secret,
        run_token,
    );

    // Matcher covers the tools that actually request permission.
    // Read/Glob/Grep/WebSearch etc. bypass the hook via --allowedTools.
    let body = json!({
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "^(Bash|Edit|Write|MultiEdit|mcp__.*)$",
                    "hooks": [
                        {
                            "type": "command",
                            "command": command,
                            "timeout": 310
                        }
                    ]
                }
            ]
        }
    });

    fs::write(&path, serde_json::to_string_pretty(&body).unwrap())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(path)
}

/// Remove a per-run settings file (best-effort).
pub fn remove_hook_settings_file(path: &std::path::Path) {
    let _ = fs::remove_file(path);
}
