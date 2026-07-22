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

                        // Install our PreToolUse command-bridge hook into
                        // ~/.claude/settings.json (claude hot-reloads this file,
                        // so it applies instantly). The hook invokes the
                        // `claude-deck-hook` bridge binary, which talks to our
                        // unix socket — see permissions::server + the bridge.
                        match permissions::global_settings::bridge_binary_path() {
                            Some(bridge_bin) => {
                                match permissions::global_settings::install_global_hook(
                                    &bridge_bin,
                                    &server.socket_path,
                                    &server.app_secret,
                                    &global_token,
                                ) {
                                    Ok(_) => eprintln!(
                                        "[permissions] global command-bridge hook installed (bridge={}, socket={})",
                                        bridge_bin.display(),
                                        server.socket_path.display(),
                                    ),
                                    Err(e) => eprintln!(
                                        "[permissions] failed to install global hook: {}",
                                        e
                                    ),
                                }
                            }
                            None => eprintln!(
                                "[permissions] could not locate claude-deck-hook bridge binary; hook not installed"
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
            commands::list_scoped_allows,
            commands::clear_scoped_allows,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Claude Deck");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            // We deliberately LEAVE the hook installed in ~/.claude/settings.json
            // on exit. Since Phase 1 the hook is a `"type":"command"` bridge, and
            // since we run it with `--fail-native` the bridge emits NO decision
            // when the app is down — Claude falls back to its own permission flow,
            // exactly as if the hub weren't installed. Removing it on exit created
            // a "restart blackout": every session silently reverted to native
            // prompts while the app bounced, and a fresh install had to race the
            // settings.json hot-reload. Leaving it means restarts are seamless and
            // a stale hook is harmless. strip_our_hooks() on next launch still
            // reconciles the socket/secret/token to the new run.
            //
            // Only clean up the socket file so the path is tidy for next launch.
            if let Some(server) = handle.try_state::<Arc<permissions::PermissionServer>>() {
                let _ = std::fs::remove_file(&server.socket_path);
            }
        }
    });
}
