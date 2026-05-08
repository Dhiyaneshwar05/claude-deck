mod commands;
mod process;
mod session;

use process::pool::ProcessPool;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(ProcessPool::new())
        .invoke_handler(tauri::generate_handler![
            commands::list_sessions,
            commands::check_session_health,
            commands::create_session,
            commands::send_prompt,
            commands::cancel_session,
            commands::debug_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Claude Deck");
}
