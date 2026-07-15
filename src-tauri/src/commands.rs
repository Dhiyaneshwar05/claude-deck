use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::permissions::{PermissionDecision, PermissionServer};
use crate::process::pool::{ProcessPool, SpawnedSessionInfo};
use crate::session::discovery;
use crate::session::types::DiscoveredSession;

#[tauri::command]
pub fn list_sessions() -> Vec<DiscoveredSession> {
    discovery::scan_sessions()
}

#[tauri::command]
pub fn check_session_health(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[tauri::command]
pub async fn create_session(
    app: AppHandle,
    pool: State<'_, ProcessPool>,
    cwd: String,
    prompt: String,
    model: Option<String>,
    resume_session_id: Option<String>,
) -> Result<SpawnedSessionInfo, String> {
    pool.spawn_session(app, cwd, prompt, model, resume_session_id)
        .await
}

#[tauri::command]
pub async fn send_prompt(
    pool: State<'_, ProcessPool>,
    session_id: String,
    prompt: String,
) -> Result<(), String> {
    pool.send_prompt(&session_id, &prompt).await
}

#[tauri::command]
pub async fn cancel_session(
    pool: State<'_, ProcessPool>,
    session_id: String,
) -> Result<(), String> {
    pool.cancel_session(&session_id).await
}

/// Debug command — returns diagnostics about the claude binary and environment.
#[tauri::command]
pub fn debug_info() -> String {
    let mut info = Vec::new();

    // Check binary
    for path in [
        "/opt/homebrew/bin/claude",
        "/usr/local/bin/claude",
    ] {
        info.push(format!(
            "{}: {}",
            path,
            if std::path::Path::new(path).exists() { "EXISTS" } else { "not found" }
        ));
    }

    if let Some(home) = dirs::home_dir() {
        let local = home.join(".claude").join("local").join("claude");
        info.push(format!(
            "~/.claude/local/claude: {}",
            if local.exists() { "EXISTS" } else { "not found" }
        ));
    }

    if let Ok(path) = which::which("claude") {
        info.push(format!("which claude: {}", path.display()));
    } else {
        info.push("which claude: not found".to_string());
    }

    info.push(format!("PATH: {}", std::env::var("PATH").unwrap_or_default()));
    info.push(format!("HOME: {:?}", dirs::home_dir()));

    info.join("\n")
}

#[derive(Serialize)]
pub struct PermissionServerInfo {
    pub port: u16,
    pub app_secret: String,
}

/// Frontend wants to know whether the permission server is up + on what port.
/// Returns None if the server hasn't finished booting yet.
#[tauri::command]
pub fn get_permission_server_info(app: AppHandle) -> Option<PermissionServerInfo> {
    app.try_state::<Arc<PermissionServer>>().map(|s| PermissionServerInfo {
        port: s.port,
        app_secret: s.app_secret.clone(),
    })
}

/// Frontend calls this when the user clicks Allow/Deny on a permission prompt.
#[tauri::command]
pub async fn resolve_permission(
    app: AppHandle,
    request_id: String,
    run_token: String,
    decision: PermissionDecision,
) -> Result<(), String> {
    let server = app
        .try_state::<Arc<PermissionServer>>()
        .ok_or("Permission server not ready")?
        .inner()
        .clone();
    server.resolve(&request_id, decision, &run_token).await
}
