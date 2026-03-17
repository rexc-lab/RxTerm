pub mod commands;
pub mod known_hosts;
pub mod session;
pub mod ssh;

use commands::{
    delete_session, export_sessions, get_sessions, import_sessions, save_session,
    ssh_accept_host_key, ssh_connect, ssh_disconnect, ssh_resize, ssh_send,
};
use ssh::SshConnectionManager;

/// Build and configure the Tauri application.
///
/// This is the main library entry point; it registers all IPC commands
/// so the React frontend can invoke them via `@tauri-apps/api`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(SshConnectionManager::new())
        .invoke_handler(tauri::generate_handler![
            get_sessions,
            save_session,
            delete_session,
            export_sessions,
            import_sessions,
            ssh_connect,
            ssh_accept_host_key,
            ssh_send,
            ssh_resize,
            ssh_disconnect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
