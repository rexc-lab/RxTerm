use std::fs;
use std::path::PathBuf;
use std::sync::Mutex as StdMutex;

use once_cell::sync::Lazy;
use tauri::{AppHandle, State};

use crate::known_hosts::KnownHostsStore;
use crate::rdp::{RdpConnectionManager, RdpKeyEvent, RdpMouseEvent};
use crate::session::{self, Protocol, SshSession};
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
    #[error("RDP error: {0}")]
    Rdp(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Validation error: {0}")]
    Validation(String),
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

/// Global mutex that serialises all read-modify-write operations on
/// `sessions.json`, preventing concurrent calls from losing updates (RES-4).
static SESSIONS_FILE_LOCK: Lazy<StdMutex<()>> = Lazy::new(|| StdMutex::new(()));

/// Returns the directory used to persist session data.
///
/// On Windows this resolves to `%APPDATA%\RxTerm\`.
/// The directory is created on first access if it does not exist.
fn data_dir() -> Result<PathBuf, AppError> {
    let base = dirs::data_dir().ok_or_else(|| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "could not determine platform data directory",
        ))
    })?;
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
    let _guard = SESSIONS_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = sessions_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path)?;
    let sessions: Vec<SshSession> = serde_json::from_str(&data)?;
    Ok(sessions)
}

/// Internal helper: read sessions while the lock is already held.
fn read_sessions_locked() -> Result<Vec<SshSession>, AppError> {
    let path = sessions_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path)?;
    let sessions: Vec<SshSession> = serde_json::from_str(&data)?;
    Ok(sessions)
}

/// Internal helper: write sessions while the lock is already held.
fn write_sessions_locked(sessions: &[SshSession]) -> Result<(), AppError> {
    let path = sessions_file()?;
    let json = serde_json::to_string_pretty(sessions)?;
    fs::write(&path, json)?;
    Ok(())
}

/// Save a new or updated SSH session.
///
/// If a session with the same `id` already exists it is replaced;
/// otherwise the new session is appended.
#[tauri::command]
pub async fn save_session(session: SshSession) -> Result<Vec<SshSession>, AppError> {
    // SEC-7: validate before persisting
    session::validate_session(&session)
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let _guard = SESSIONS_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut sessions = read_sessions_locked()?;

    if let Some(pos) = sessions.iter().position(|s| s.id == session.id) {
        sessions[pos] = session;
    } else {
        sessions.push(session);
    }

    write_sessions_locked(&sessions)?;
    Ok(sessions)
}

/// Delete an SSH session by its `id`.
#[tauri::command]
pub async fn delete_session(id: String) -> Result<Vec<SshSession>, AppError> {
    let _guard = SESSIONS_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut sessions = read_sessions_locked()?;
    sessions.retain(|s| s.id != id);

    write_sessions_locked(&sessions)?;
    Ok(sessions)
}

/// Export all sessions to a JSON string (for file-save dialog on the frontend).
#[tauri::command]
pub async fn export_sessions() -> Result<String, AppError> {
    let _guard = SESSIONS_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sessions = read_sessions_locked()?;
    let json = serde_json::to_string_pretty(&sessions)?;
    Ok(json)
}

/// Import sessions from a JSON string, merging with existing sessions.
///
/// Sessions with duplicate `id` values are overwritten by the import.
#[tauri::command]
pub async fn import_sessions(json: String) -> Result<Vec<SshSession>, AppError> {
    let imported: Vec<SshSession> = serde_json::from_str(&json)?;

    // SEC-7: validate every imported session
    for s in &imported {
        session::validate_session(s)
            .map_err(|e| AppError::Validation(format!("session '{}': {}", s.id, e)))?;
    }

    let _guard = SESSIONS_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut sessions = read_sessions_locked()?;

    for incoming in imported {
        if let Some(pos) = sessions.iter().position(|s| s.id == incoming.id) {
            sessions[pos] = incoming;
        } else {
            sessions.push(incoming);
        }
    }

    write_sessions_locked(&sessions)?;
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

    // SEC-4: validate SSH host before connecting (consistent with VNC/RDP)
    if !session::is_valid_host(&session.host) {
        return Err(AppError::Validation(format!(
            "Invalid SSH host: {}",
            session.host
        )));
    }

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
        Err(crate::ssh::ConnectError::HostKeyUnknown(info)) => {
            // SEC-3: Key info captured from the first connection attempt
            Err(AppError::HostKeyUnknown(HostKeyInfo {
                fingerprint: info.fingerprint,
                key_data: info.key_data,
                algorithm: info.algorithm,
            }))
        }
        Err(e) => Err(AppError::Ssh(e.to_string())),
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

// ─── RDP connection commands ────────────────────────────────────────────────

/// Response from a successful RDP connection attempt.
#[derive(serde::Serialize)]
pub struct RdpConnectResult {
    connection_id: String,
}

/// Start an RDP session for the session specified by `session_id`.
///
/// Spawns a background Tauri task that:
///  1. Establishes a TCP connection to the RDP server.
///  2. Performs TLS and CredSSP negotiation (MS-RDPBCGR §1.3).
///  3. Enters the active-stage loop, emitting `rdp-frame` events for each
///     graphics update and `rdp-disconnected` when the session ends.
#[tauri::command]
pub async fn rdp_connect(
    app: AppHandle,
    rdp_manager: State<'_, RdpConnectionManager>,
    session_id: String,
    password: Option<String>,
) -> Result<RdpConnectResult, AppError> {
    let sessions = get_sessions().await?;
    let session = sessions
        .iter()
        .find(|s| s.id == session_id)
        .ok_or_else(|| AppError::NotFound(format!("Session {} not found", session_id)))?
        .clone();

    if session.protocol != Protocol::Rdp {
        return Err(AppError::Rdp("Session is not an RDP session".to_string()));
    }

    let pw = password
        .as_deref()
        .or(session.password.as_deref())
        .unwrap_or("");

    let connection_id = uuid::Uuid::new_v4().to_string();

    rdp_manager
        .connect(
            app,
            &connection_id,
            &session.host,
            session.port,
            &session.username,
            pw,
            session.domain.as_deref(),
        )
        .await
        .map_err(|e| AppError::Rdp(e.to_string()))?;

    Ok(RdpConnectResult { connection_id })
}

/// Disconnect an active RDP session and clean up resources.
#[tauri::command]
pub async fn rdp_disconnect(
    rdp_manager: State<'_, RdpConnectionManager>,
    connection_id: String,
) -> Result<(), AppError> {
    rdp_manager
        .disconnect(&connection_id)
        .await
        .map_err(|e| AppError::Rdp(e.to_string()))
}

/// Send a mouse event to an active RDP session.
///
/// The frontend should call this for every `mousemove`, `mousedown`, and
/// `mouseup` event captured on the RDP canvas.
#[tauri::command]
pub async fn rdp_mouse_event(
    rdp_manager: State<'_, RdpConnectionManager>,
    connection_id: String,
    event: RdpMouseEvent,
) -> Result<(), AppError> {
    rdp_manager
        .send_mouse(&connection_id, event)
        .await
        .map_err(|e| AppError::Rdp(e.to_string()))
}

/// Send a keyboard event to an active RDP session.
///
/// The frontend should call this for every `keydown` and `keyup` event.
#[tauri::command]
pub async fn rdp_key_event(
    rdp_manager: State<'_, RdpConnectionManager>,
    connection_id: String,
    event: RdpKeyEvent,
) -> Result<(), AppError> {
    rdp_manager
        .send_key(&connection_id, event)
        .await
        .map_err(|e| AppError::Rdp(e.to_string()))
}
