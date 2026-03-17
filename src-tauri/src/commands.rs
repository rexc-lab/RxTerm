use std::fs;
use std::path::PathBuf;

use crate::session::SshSession;

/// Application-level errors surfaced to the frontend via Tauri commands.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

// Tauri requires command errors to be serializable.
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Returns the directory used to persist session data.
///
/// On Windows this resolves to `%APPDATA%\RxTerm\`.
/// The directory is created on first access if it does not exist.
fn data_dir() -> Result<PathBuf, AppError> {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("RxTerm");
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Path to the JSON file that holds all saved sessions.
fn sessions_file() -> Result<PathBuf, AppError> {
    Ok(data_dir()?.join("sessions.json"))
}

/// Load all saved SSH sessions from disk.
///
/// Returns an empty list if the file does not yet exist.
#[tauri::command]
pub async fn get_sessions() -> Result<Vec<SshSession>, AppError> {
    let path = sessions_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path)?;
    let sessions: Vec<SshSession> = serde_json::from_str(&data)?;
    Ok(sessions)
}

/// Save a new or updated SSH session.
///
/// If a session with the same `id` already exists it is replaced;
/// otherwise the new session is appended.
#[tauri::command]
pub async fn save_session(session: SshSession) -> Result<Vec<SshSession>, AppError> {
    let mut sessions = get_sessions().await?;

    if let Some(pos) = sessions.iter().position(|s| s.id == session.id) {
        sessions[pos] = session;
    } else {
        sessions.push(session);
    }

    let path = sessions_file()?;
    let json = serde_json::to_string_pretty(&sessions)?;
    fs::write(&path, json)?;
    Ok(sessions)
}

/// Delete an SSH session by its `id`.
#[tauri::command]
pub async fn delete_session(id: String) -> Result<Vec<SshSession>, AppError> {
    let mut sessions = get_sessions().await?;
    sessions.retain(|s| s.id != id);

    let path = sessions_file()?;
    let json = serde_json::to_string_pretty(&sessions)?;
    fs::write(&path, json)?;
    Ok(sessions)
}

/// Export all sessions to a JSON string (for file-save dialog on the frontend).
#[tauri::command]
pub async fn export_sessions() -> Result<String, AppError> {
    let sessions = get_sessions().await?;
    let json = serde_json::to_string_pretty(&sessions)?;
    Ok(json)
}

/// Import sessions from a JSON string, merging with existing sessions.
///
/// Sessions with duplicate `id` values are overwritten by the import.
#[tauri::command]
pub async fn import_sessions(json: String) -> Result<Vec<SshSession>, AppError> {
    let imported: Vec<SshSession> = serde_json::from_str(&json)?;
    let mut sessions = get_sessions().await?;

    for incoming in imported {
        if let Some(pos) = sessions.iter().position(|s| s.id == incoming.id) {
            sessions[pos] = incoming;
        } else {
            sessions.push(incoming);
        }
    }

    let path = sessions_file()?;
    let out = serde_json::to_string_pretty(&sessions)?;
    fs::write(&path, out)?;
    Ok(sessions)
}
