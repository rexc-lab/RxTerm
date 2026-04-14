pub mod commands;
pub mod known_hosts;
pub mod rdp;
pub mod session;
pub mod ssh;
pub mod vnc;

use commands::{
    delete_session, export_sessions, get_sessions, import_sessions, save_session,
    ssh_accept_host_key, ssh_connect, ssh_disconnect, ssh_resize, ssh_send,
    rdp_connect, rdp_disconnect, rdp_mouse_event, rdp_key_event,
};
use rdp::RdpConnectionManager;
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
        .manage(RdpConnectionManager::new())
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
            rdp_connect,
            rdp_disconnect,
            rdp_mouse_event,
            rdp_key_event,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("Fatal: failed to start Tauri application: {e}");
            std::process::exit(1);
        });
}
