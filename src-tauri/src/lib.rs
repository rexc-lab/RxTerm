pub mod commands;
pub mod session;

use commands::{delete_session, export_sessions, get_sessions, import_sessions, save_session};

/// Build and configure the Tauri application.
///
/// This is the main library entry point; it registers all IPC commands
/// so the React frontend can invoke them via `@tauri-apps/api`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_sessions,
            save_session,
            delete_session,
            export_sessions,
            import_sessions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
