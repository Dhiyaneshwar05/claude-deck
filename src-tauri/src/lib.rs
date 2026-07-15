mod commands;
mod permissions;
mod process;
mod session;

use std::sync::Arc;

use process::pool::ProcessPool;
use tauri::{Manager, RunEvent};

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(ProcessPool::new())
        .setup(|app| {
            // Start the permission HTTP server as soon as the app is ready.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match permissions::PermissionServer::start(handle.clone()).await {
                    Ok(server) => {
                        // Reserve a global run token covering every Claude session
                        // on the machine (Cursor, VS Code, terminal, hub-spawned).
                        let global_token = server
                            .register_run("global".to_string(), "global".to_string())
                            .await;

                        let server = Arc::new(server);
                        handle.manage(server.clone());

                        // Install our PreToolUse hook into ~/.claude/settings.json
                        // (claude hot-reloads this file, so it applies instantly).
                        match permissions::global_settings::install_global_hook(
                            server.port,
                            &server.app_secret,
                            &global_token,
                        ) {
                            Ok(_) => eprintln!(
                                "[permissions] global hook installed in ~/.claude/settings.json"
                            ),
                            Err(e) => eprintln!(
                                "[permissions] failed to install global hook: {}",
                                e
                            ),
                        }

                        eprintln!("[permissions] server started on port {}", server.port);
                    }
                    Err(e) => eprintln!("[permissions] failed to start: {}", e),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_sessions,
            commands::check_session_health,
            commands::create_session,
            commands::send_prompt,
            commands::cancel_session,
            commands::debug_info,
            commands::resolve_permission,
            commands::get_permission_server_info,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Claude Deck");

    app.run(|_handle, event| {
        if let RunEvent::Exit = event {
            // Best-effort cleanup so we don't leave a stale hook in settings.json
            // that points at a server which is about to die.
            //
            // KNOWN GAP (SIGKILL / crash): RunEvent::Exit only fires on a clean
            // shutdown. If the app is `kill -9`'d or crashes, this never runs and
            // the `"type":"http"` hook we injected stays in ~/.claude/settings.json
            // pointing at a now-dead port. Because Claude blocks on that hook for
            // EVERY session, it hangs all other Claude sessions' Bash/Edit/Write
            // until manually removed (strip_our_hooks self-heals only on our next
            // launch). Phase 1 of docs/PERMISSION-HUB-V2-PLAN.md fixes this
            // structurally by replacing the live-port http hook with a
            // short-lived `"type":"command"` bridge over a unix socket, where a
            // stale socket path just fails connect() fast instead of hanging.
            if let Err(e) = permissions::global_settings::uninstall_global_hook() {
                eprintln!("[permissions] failed to uninstall global hook on exit: {}", e);
            } else {
                eprintln!("[permissions] global hook removed from ~/.claude/settings.json");
            }
        }
    });
}
