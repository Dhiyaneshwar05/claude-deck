use std::fs;
use std::path::PathBuf;

use serde_json::json;

/// Write a per-run Claude settings file containing an HTTP PreToolUse hook.
/// Returns the absolute path to the file so the caller can pass `--settings <path>` to claude.
///
/// Matches clui-cc's scheme (see permissions research):
///   $TMPDIR/claude-deck-hook-config/claude-deck-hook-<runToken>.json
///   dir mode 0o700, file mode 0o600
pub fn write_hook_settings_file(
    run_token: &str,
    server_port: u16,
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
    let url = format!(
        "http://127.0.0.1:{}/hook/pre-tool-use/{}/{}",
        server_port, app_secret, run_token
    );

    // Matcher covers the tools that actually request permission (matches clui-cc).
    // Read/Glob/Grep/WebSearch etc. bypass the hook via --allowedTools.
    let body = json!({
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "^(Bash|Edit|Write|MultiEdit|mcp__.*)$",
                    "hooks": [
                        {
                            "type": "http",
                            "url": url,
                            "timeout": 300
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
