use std::fs;
use std::path::PathBuf;

use tauri::{AppHandle, State};

use crate::known_hosts::{self, KnownHostsStore};
use crate::session::{Protocol, SshSession};
use crate::ssh::SshConnectionManager;
use crate::vnc::VncConnectionManager;

/// Application-level errors surfaced to the frontend via Tauri commands.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("SSH error: {0}")]
    Ssh(String),
    #[error("VNC error: {0}")]
    Vnc(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("HOST_KEY_UNKNOWN:{}", serde_json::to_string(.0).unwrap_or_default())]
    HostKeyUnknown(HostKeyInfo),
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

// ─── SSH connection commands ────────────────────────────────────────────────

/// Response from a successful SSH connection attempt.
#[derive(serde::Serialize)]
pub struct SshConnectResult {
    connection_id: String,
}

/// Response when the host key is not yet trusted.
#[derive(Debug, serde::Serialize)]
pub struct HostKeyInfo {
    fingerprint: String,
    key_data: String,
    algorithm: String,
}

/// Initiate an SSH connection to the server specified by `session_id`.
///
/// If the host key is unknown, returns an error whose message starts with
/// `HOST_KEY_UNKNOWN:` followed by a JSON-encoded `HostKeyInfo`. The
/// frontend should prompt the user and call `ssh_accept_host_key` before
/// retrying.
#[tauri::command]
pub async fn ssh_connect(
    app: AppHandle,
    manager: State<'_, SshConnectionManager>,
    session_id: String,
    password: Option<String>,
) -> Result<SshConnectResult, AppError> {
    // Look up the session config
    let sessions = get_sessions().await?;
    let session = sessions
        .iter()
        .find(|s| s.id == session_id)
        .ok_or_else(|| AppError::NotFound(format!("Session {} not found", session_id)))?
        .clone();

    // Pre-check host key before connecting
    // We need to attempt connection to get the server key; if rejected,
    // we'll detect it via the connection error and surface the key info.

    let connection_id = uuid::Uuid::new_v4().to_string();

    let pw = password.as_deref().or(session.password.as_deref());
    let key_path = if session.auth_method == crate::session::AuthMethod::Key {
        session.private_key_path.as_deref()
    } else {
        None
    };

    match manager
        .connect(
            &app,
            &connection_id,
            &session.host,
            session.port,
            &session.username,
            pw,
            key_path,
        )
        .await
    {
        Ok(()) => Ok(SshConnectResult { connection_id }),
        Err(crate::ssh::ConnectError::Ssh(russh::Error::UnknownKey)) => {
            // Host key was rejected by our handler — get the key info for the prompt
            let host_key_info = get_host_key_info(&session.host, session.port).await?;
            Err(AppError::HostKeyUnknown(host_key_info))
        }
        Err(e) => Err(AppError::Ssh(e.to_string())),
    }
}

/// Helper: connect briefly just to capture the server's public key info.
async fn get_host_key_info(host: &str, port: u16) -> Result<HostKeyInfo, AppError> {
    let key_capture = std::sync::Arc::new(tokio::sync::Mutex::new(None::<(String, String, String)>));
    let capture_clone = key_capture.clone();

    // We already know the key is unknown/changed, so construct the info
    // by doing a fresh check. Since the connection failed, we need to
    // try connecting with an accepting handler to capture the key.
    let handler = KeyCaptureHandler {
        captured: capture_clone,
    };

    let config = std::sync::Arc::new(russh::client::Config::default());
    let addr = format!("{}:{}", host, port);

    // Try to connect — we only care about capturing the key
    match russh::client::connect(config, &addr, handler).await {
        Ok(session) => {
            // Connected = key was captured; disconnect immediately
            let _ = session
                .disconnect(russh::Disconnect::ByApplication, "key capture", "en")
                .await;
        }
        Err(_) => {
            // Connection may have failed but key could still be captured
        }
    }

    let captured = key_capture.lock().await;
    if let Some((fingerprint, key_data, algorithm)) = captured.as_ref() {
        Ok(HostKeyInfo {
            fingerprint: fingerprint.clone(),
            key_data: key_data.clone(),
            algorithm: algorithm.clone(),
        })
    } else {
        Err(AppError::Ssh("Could not retrieve server host key".to_string()))
    }
}

/// A temporary handler that accepts any key and captures its info.
struct KeyCaptureHandler {
    captured: std::sync::Arc<tokio::sync::Mutex<Option<(String, String, String)>>>,
}

#[async_trait::async_trait]
impl russh::client::Handler for KeyCaptureHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        let algorithm = known_hosts::key_algorithm(server_public_key);
        let key_data = known_hosts::key_to_base64(server_public_key);

        // Build a human-readable fingerprint
        let fp = format!("{} {}", algorithm, &key_data[..32.min(key_data.len())]);

        let mut captured = self.captured.lock().await;
        *captured = Some((fp, key_data, algorithm));
        Ok(true) // Accept so the connection completes
    }
}

/// Accept and persist a host key so future connections succeed.
#[tauri::command]
pub async fn ssh_accept_host_key(
    host: String,
    port: u16,
    key_data: String,
    algorithm: String,
) -> Result<(), AppError> {
    let store = KnownHostsStore::new()?;
    store.accept(&host, port, &key_data, &algorithm)?;
    Ok(())
}

/// Send raw bytes (user keystrokes) to an active SSH connection.
#[tauri::command]
pub async fn ssh_send(
    manager: State<'_, SshConnectionManager>,
    connection_id: String,
    data: Vec<u8>,
) -> Result<(), AppError> {
    manager
        .send(&connection_id, &data)
        .await
        .map_err(|e| AppError::Ssh(e.to_string()))
}

/// Notify the remote SSH server of a terminal resize.
#[tauri::command]
pub async fn ssh_resize(
    manager: State<'_, SshConnectionManager>,
    connection_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), AppError> {
    manager
        .resize(&connection_id, cols, rows)
        .await
        .map_err(|e| AppError::Ssh(e.to_string()))
}

/// Disconnect an active SSH connection and clean up resources.
#[tauri::command]
pub async fn ssh_disconnect(
    manager: State<'_, SshConnectionManager>,
    connection_id: String,
) -> Result<(), AppError> {
    manager
        .disconnect(&connection_id)
        .await
        .map_err(|e| AppError::Ssh(e.to_string()))
}

// ─── VNC connection commands ────────────────────────────────────────────────

/// Response from a successful VNC connection attempt.
#[derive(serde::Serialize)]
pub struct VncConnectResult {
    connection_id: String,
    ws_port: u16,
}

/// Start a VNC WebSocket proxy for the session specified by `session_id`.
///
/// Returns a `connection_id` and the local WebSocket port that the
/// frontend noVNC client should connect to.
#[tauri::command]
pub async fn vnc_connect(
    vnc_manager: State<'_, VncConnectionManager>,
    session_id: String,
    password: Option<String>,
) -> Result<VncConnectResult, AppError> {
    let sessions = get_sessions().await?;
    let session = sessions
        .iter()
        .find(|s| s.id == session_id)
        .ok_or_else(|| AppError::NotFound(format!("Session {} not found", session_id)))?
        .clone();

    if session.protocol != Protocol::Vnc {
        return Err(AppError::Vnc("Session is not a VNC session".to_string()));
    }

    let connection_id = uuid::Uuid::new_v4().to_string();

    // The VNC password will be sent by the noVNC client during the RFB
    // handshake.  We accept the parameter here so the frontend can
    // forward stored passwords in a future iteration.
    let _ = password;

    let ws_port = vnc_manager
        .start_proxy(&connection_id, &session.host, session.port)
        .await
        .map_err(|e| AppError::Vnc(e.to_string()))?;

    Ok(VncConnectResult {
        connection_id,
        ws_port,
    })
}

/// Stop a VNC proxy connection and clean up resources.
#[tauri::command]
pub async fn vnc_disconnect(
    vnc_manager: State<'_, VncConnectionManager>,
    connection_id: String,
) -> Result<(), AppError> {
    vnc_manager
        .stop_proxy(&connection_id)
        .await
        .map_err(|e| AppError::Vnc(e.to_string()))
}
