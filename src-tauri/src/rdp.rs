use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine as _;
use zeroize::Zeroize;
use ironrdp_async::{connect_begin, connect_finalize, mark_as_upgraded, FramedWrite, NetworkClient};
use ironrdp_connector::{
    ClientConnector, Config, Credentials, DesktopSize, ServerName,
    ConnectorResult, ConnectorError, ConnectorErrorKind,
};
use ironrdp_graphics::image_processing::PixelFormat;
use ironrdp_input::{Database as InputDatabase, MouseButton, MousePosition, Operation, Scancode, WheelRotations};
use ironrdp_pdu::geometry::Rectangle as _;
use ironrdp_pdu::rdp::client_info::{PerformanceFlags, TimezoneInfo};
use ironrdp_session::{image::DecodedImage, ActiveStage, ActiveStageOutput};
use ironrdp_tokio::{split_tokio_framed, TokioFramed};
use tauri::{AppHandle, Emitter};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

/// Maximum number of concurrent RDP sessions allowed.
const MAX_RDP_CONNECTIONS: usize = 8;

/// Default RDP desktop dimensions.
const DEFAULT_WIDTH: u16 = 1280;
const DEFAULT_HEIGHT: u16 = 800;

/// Event name emitted when a frame region is updated.
pub const RDP_FRAME_EVENT: &str = "rdp-frame";

/// Event name emitted when an RDP session disconnects.
pub const RDP_DISCONNECTED_EVENT: &str = "rdp-disconnected";

// ─── Input event types (sent from frontend) ─────────────────────────────────

/// A mouse input event from the frontend.
#[derive(Debug, serde::Deserialize)]
pub struct RdpMouseEvent {
    /// X coordinate in the remote desktop.
    pub x: u16,
    /// Y coordinate in the remote desktop.
    pub y: u16,
    /// Mouse button (0=left, 1=middle, 2=right). None = move only.
    pub button: Option<u8>,
    /// Whether the button is pressed (true) or released (false).
    pub pressed: bool,
    /// Scroll delta (positive = up, negative = down). None = no scroll.
    pub scroll_delta: Option<i16>,
}

/// A keyboard input event from the frontend.
#[derive(Debug, serde::Deserialize)]
pub struct RdpKeyEvent {
    /// Windows virtual-key / scan code (from browser `KeyboardEvent.code`).
    ///
    /// This should be a packed u16 where the high bit indicates an extended key.
    /// See [`Scancode::from_u16`].
    pub scancode: u16,
    /// Whether the key is pressed (true) or released (false).
    pub pressed: bool,
}

// ─── Frame update event (emitted to frontend) ────────────────────────────────

/// Payload for the `rdp-frame` Tauri event.
#[derive(Clone, serde::Serialize)]
pub struct RdpFramePayload {
    /// Connection identifier so the frontend can route to the right pane.
    pub connection_id: String,
    /// Full desktop width (pixels).
    pub full_width: u16,
    /// Full desktop height (pixels).
    pub full_height: u16,
    /// Updated region top-left X.
    pub x: u16,
    /// Updated region top-left Y.
    pub y: u16,
    /// Updated region width.
    pub width: u16,
    /// Updated region height.
    pub height: u16,
    /// Base64-encoded RGBA pixel data for the updated region (row-major, 4 bytes/pixel).
    pub data: String,
}

/// Payload for the `rdp-disconnected` Tauri event.
#[derive(Clone, serde::Serialize)]
pub struct RdpDisconnectedPayload {
    pub connection_id: String,
    pub reason: String,
}

// ─── Internal input channel message ──────────────────────────────────────────

enum RdpInput {
    Mouse(RdpMouseEvent),
    Key(RdpKeyEvent),
    Disconnect,
}

// ─── Session and manager ─────────────────────────────────────────────────────

/// A running RDP session.
struct RdpSession {
    /// Handle to the background session task.
    task: JoinHandle<()>,
    /// Sender for input events from the frontend.
    input_tx: mpsc::Sender<RdpInput>,
}

/// Thread-safe manager for all active RDP sessions.
pub struct RdpConnectionManager {
    sessions: Arc<Mutex<HashMap<String, RdpSession>>>,
}

impl RdpConnectionManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start an RDP session for the given host/port/credentials.
    ///
    /// Returns the connection ID that the frontend uses to route events.
    ///
    /// # Arguments
    ///
    /// * `app`          – Tauri app handle, used to emit events to the frontend.
    /// * `connection_id` – Unique ID for this connection (UUID v4).
    /// * `host`         – RDP server hostname or IP address.
    /// * `port`         – RDP server TCP port (default 3389).
    /// * `username`     – Windows username.
    /// * `password`     – Windows password.
    /// * `domain`       – Optional Windows domain name.
    pub async fn connect(
        &self,
        app: AppHandle,
        connection_id: &str,
        host: &str,
        port: u16,
        username: &str,
        password: &str,
        domain: Option<&str>,
    ) -> Result<(), RdpError> {
        // Validate host before acquiring the lock
        validate_rdp_host(host)?;

        let (input_tx, input_rx) = mpsc::channel::<RdpInput>(64);

        let cid = connection_id.to_string();
        let host = host.to_string();
        let username = username.to_string();
        let mut password = password.to_string();
        let domain = domain.map(str::to_string);
        let sessions = self.sessions.clone();

        // RES-6: Spawn the task first, then atomically check the limit and
        // insert the real entry in one lock acquisition.  The task waits on
        // a oneshot signal before doing any real work, so if the limit is
        // exceeded we drop the sender which causes the task to exit.
        let (start_tx, start_rx) = tokio::sync::oneshot::channel::<()>();

        let task = tokio::spawn(async move {
            // Wait for the go-ahead; if the sender is dropped (limit exceeded)
            // the task exits immediately.
            if start_rx.await.is_err() {
                return;
            }

            // SEC-6: zeroize the password after building credentials
            let reason = match run_session(&app, &cid, &host, port, &username, &password, domain.as_deref(), input_rx).await {
                Ok(reason) => reason,
                Err(e) => {
                    log::error!("[RDP {}] session error: {}", cid, e);
                    e.to_string()
                }
            };
            password.zeroize();

            // Notify the frontend that this session ended
            let payload = RdpDisconnectedPayload {
                connection_id: cid.clone(),
                reason,
            };
            let _ = app.emit(RDP_DISCONNECTED_EVENT, payload);

            // Clean up the session entry
            sessions.lock().await.remove(&cid);
        });

        // Atomically check limit and insert the real session entry.
        {
            let mut sessions_guard = self.sessions.lock().await;
            if sessions_guard.len() >= MAX_RDP_CONNECTIONS {
                // Drop start_tx so the task exits; then abort for good measure.
                drop(start_tx);
                task.abort();
                return Err(RdpError::TooManyConnections);
            }
            sessions_guard.insert(
                connection_id.to_string(),
                RdpSession {
                    task,
                    input_tx: input_tx.clone(),
                },
            );
        }

        // Signal the task to begin.
        let _ = start_tx.send(());

        Ok(())
    }

    /// Send a mouse event to an active RDP session.
    pub async fn send_mouse(&self, connection_id: &str, event: RdpMouseEvent) -> Result<(), RdpError> {
        // Clone the sender while holding the lock, then drop the lock before
        // awaiting the send to avoid holding the mutex across an `.await`.
        let tx = {
            let sessions = self.sessions.lock().await;
            sessions
                .get(connection_id)
                .ok_or_else(|| RdpError::NotFound(connection_id.to_string()))?
                .input_tx
                .clone()
        };
        tx.send(RdpInput::Mouse(event))
            .await
            .map_err(|_| RdpError::SessionClosed)
    }

    /// Send a keyboard event to an active RDP session.
    pub async fn send_key(&self, connection_id: &str, event: RdpKeyEvent) -> Result<(), RdpError> {
        // Clone the sender while holding the lock, then drop the lock before
        // awaiting the send to avoid holding the mutex across an `.await`.
        let tx = {
            let sessions = self.sessions.lock().await;
            sessions
                .get(connection_id)
                .ok_or_else(|| RdpError::NotFound(connection_id.to_string()))?
                .input_tx
                .clone()
        };
        tx.send(RdpInput::Key(event))
            .await
            .map_err(|_| RdpError::SessionClosed)
    }

    /// Disconnect and clean up an RDP session.
    pub async fn disconnect(&self, connection_id: &str) -> Result<(), RdpError> {
        // Remove the session from the map while holding the lock, then drop
        // the lock before doing any I/O (send + abort) to avoid holding the
        // mutex across an `.await`.
        let session = {
            let mut sessions = self.sessions.lock().await;
            sessions.remove(connection_id)
        };
        if let Some(session) = session {
            // Signal the session task to exit gracefully, then abort it in
            // case it is blocked in a long-running async operation.
            let _ = session.input_tx.send(RdpInput::Disconnect).await;
            session.task.abort();
        }
        Ok(())
    }
}

// ─── Core session runner ──────────────────────────────────────────────────────

/// Validate that the RDP host is a reasonable hostname or IP address.
fn validate_rdp_host(host: &str) -> Result<(), RdpError> {
    if host.is_empty() || host.len() > 253 {
        return Err(RdpError::InvalidHost(
            "Host must be between 1 and 253 characters".to_string(),
        ));
    }
    // Try parsing as IP first
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Ok(());
    }
    // Otherwise validate as a hostname
    if !host
        .chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        return Err(RdpError::InvalidHost(format!(
            "Host contains invalid characters: {}",
            host
        )));
    }
    Ok(())
}

/// Minimal `NetworkClient` implementation.
///
/// RDP's CredSSP/NTLM authentication usually doesn't require external HTTP
/// calls for local accounts. We return an error for any network request so
/// the connection fails gracefully if Kerberos (which does need KDC calls)
/// is selected instead.
struct NoNetworkClient;

impl NetworkClient for NoNetworkClient {
    async fn send(
        &mut self,
        _request: &ironrdp_connector::sspi::generator::NetworkRequest,
    ) -> ConnectorResult<Vec<u8>> {
        Err(ConnectorError::new(
            "NoNetworkClient",
            ConnectorErrorKind::General,
        ))
    }
}

/// Run a complete RDP session from connect to disconnect.
///
/// Returns the disconnect reason string on success, or an [`RdpError`] if
/// connection setup fails. Once the active-stage loop starts, all errors are
/// handled internally and reported via Tauri events.
async fn run_session(
    app: &AppHandle,
    connection_id: &str,
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    domain: Option<&str>,
    mut input_rx: mpsc::Receiver<RdpInput>,
) -> Result<String, RdpError> {
    let addr = format!("{}:{}", host, port);

    // ── 1. TCP connect (ROB-4: with 15-second timeout) ─────────────────
    let tcp_stream = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        TcpStream::connect(&addr),
    )
        .await
        .map_err(|_| RdpError::Io(format!("TCP connect to {} timed out after 15 seconds", addr)))?
        .map_err(|e| RdpError::Io(format!("TCP connect to {}: {}", addr, e)))?;

    let server_name = ServerName::new(host);

    // ── 2. Build connector config ─────────────────────────────────────────
    let credentials = Credentials::UsernamePassword {
        username: username.to_string(),
        password: password.to_string(),
    };

    let config = Config {
        desktop_size: DesktopSize {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
        },
        desktop_scale_factor: 0,
        // Enable both TLS and CredSSP; the server negotiates which to use.
        // RFC MS-RDPBCGR §1.3
        enable_tls: true,
        enable_credssp: true,
        credentials,
        domain: domain.map(str::to_string),
        client_build: 0,
        client_name: "RxTerm".to_string(),
        keyboard_type: ironrdp_pdu::gcc::KeyboardType::IbmEnhanced,
        keyboard_subtype: 0,
        keyboard_functional_keys_count: 12,
        keyboard_layout: 0x0409, // US English
        ime_file_name: String::new(),
        bitmap: None,
        dig_product_id: String::new(),
        client_dir: String::new(),
        platform: ironrdp_pdu::rdp::capability_sets::MajorPlatformType::UNSPECIFIED,
        hardware_id: None,
        request_data: None,
        autologon: true,
        enable_audio_playback: false,
        performance_flags: PerformanceFlags::default(),
        license_cache: None,
        timezone_info: TimezoneInfo::default(),
        enable_server_pointer: true,
        pointer_software_rendering: false,
    };

    // ── 3. Pre-TLS phase (MS-RDPBCGR §1.3.1.1) ───────────────────────────
    let client_addr = tcp_stream.local_addr().map_err(|e| RdpError::Io(e.to_string()))?;
    let mut connector = ClientConnector::new(config, client_addr);

    let mut pre_tls_framed = TokioFramed::<TcpStream>::new(tcp_stream);

    let should_upgrade = connect_begin(&mut pre_tls_framed, &mut connector)
        .await
        .map_err(|e| RdpError::Protocol(e.to_string()))?;

    // ── 4. TLS upgrade (MS-RDPBCGR §5.4) ────────────────────────────────
    let (tcp_stream, leftover) = pre_tls_framed.into_inner();
    let (tls_stream, tls_cert) = ironrdp_tls::upgrade(tcp_stream, server_name.as_str())
        .await
        .map_err(|e| RdpError::Tls(e.to_string()))?;

    let server_public_key = ironrdp_tls::extract_tls_server_public_key(&tls_cert)
        .ok_or_else(|| RdpError::Tls("server TLS certificate has no public key".to_string()))?
        .to_vec();

    let upgraded = mark_as_upgraded(should_upgrade, &mut connector);

    let mut tls_framed = TokioFramed::<tokio_native_tls::TlsStream<TcpStream>>::new_with_leftover(
        tls_stream,
        leftover,
    );

    // ── 5. Post-TLS (CredSSP / license / capability) phase ───────────────
    let mut network_client = NoNetworkClient;
    let connection_result = connect_finalize(
        upgraded,
        connector,
        &mut tls_framed,
        &mut network_client,
        server_name,
        server_public_key,
        None, // Kerberos config — not needed for NTLM
    )
    .await
    .map_err(|e| RdpError::Protocol(e.to_string()))?;

    // ── 6. Active stage setup ─────────────────────────────────────────────
    let mut active_stage = ActiveStage::new(connection_result);
    let mut image = DecodedImage::new(
        PixelFormat::RgbA32,
        DEFAULT_WIDTH,
        DEFAULT_HEIGHT,
    );
    let mut input_db = InputDatabase::new();

    // Split the TLS stream so we can read and write concurrently.
    let (mut framed_read, mut framed_write) = split_tokio_framed(tls_framed);

    let full_w = image.width();
    let full_h = image.height();

    // ── 7. Active-stage event loop ────────────────────────────────────────
    let disconnect_reason = 'session_loop: loop {
        tokio::select! {
            // ── Server → client ──────────────────────────────────────────
            read_result = framed_read.read_pdu() => {
                let (action, frame) = match read_result {
                    Ok(pdu) => pdu,
                    Err(e) => break format!("read error: {}", e),
                };

                let outputs = match active_stage.process(&mut image, action, &frame) {
                    Ok(o) => o,
                    Err(e) => break format!("process error: {}", e),
                };

                for output in outputs {
                    match output {
                        ActiveStageOutput::ResponseFrame(data) => {
                            if let Err(e) = framed_write.write_all(&data).await {
                                log::warn!("[RDP {}] write error: {}", connection_id, e);
                            }
                        }
                        ActiveStageOutput::GraphicsUpdate(rect) => {
                            let rect_w = rect.width();
                            let rect_h = rect.height();
                            if rect_w == 0 || rect_h == 0 {
                                continue;
                            }
                            // PERF-2: split large rectangles into tiles of max 256x256
                            // to avoid multi-megabyte base64 payloads in a single event.
                            const TILE_SIZE: u16 = 256;
                            let mut ty = rect.top;
                            while ty < rect.top + rect_h {
                                let th = TILE_SIZE.min(rect.top + rect_h - ty);
                                let mut tx = rect.left;
                                while tx < rect.left + rect_w {
                                    let tw = TILE_SIZE.min(rect.left + rect_w - tx);
                                    let rgba = match extract_rect_rgba(&image, tx, ty, tw, th) {
                                        Some(data) => data,
                                        None => { tx += TILE_SIZE; continue; }
                                    };
                                    let payload = RdpFramePayload {
                                        connection_id: connection_id.to_string(),
                                        full_width: full_w,
                                        full_height: full_h,
                                        x: tx,
                                        y: ty,
                                        width: tw,
                                        height: th,
                                        data: base64::engine::general_purpose::STANDARD.encode(&rgba),
                                    };
                                    let _ = app.emit(RDP_FRAME_EVENT, payload);
                                    tx += TILE_SIZE;
                                }
                                ty += TILE_SIZE;
                            }
                        }
                        ActiveStageOutput::Terminate(reason) => {
                            break 'session_loop reason.to_string();
                        }
                        ActiveStageOutput::DeactivateAll(_cas) => {
                            // Server-side deactivation (e.g. display change).
                            // Treated as a graceful disconnect for simplicity.
                            break 'session_loop "server deactivated session".to_string();
                        }
                        // Pointer events — ignored for now
                        _ => {}
                    }
                }
            }

            // ── Client → server (keyboard / mouse) ────────────────────────
            input = input_rx.recv() => {
                match input {
                    Some(RdpInput::Mouse(event)) => {
                        let ops = build_mouse_operations(&event);
                        let events = input_db.apply(ops);
                        if !events.is_empty() {
                            let outputs = match active_stage.process_fastpath_input(&mut image, &events) {
                                Ok(o) => o,
                                Err(e) => {
                                    log::warn!("[RDP {}] input error: {}", connection_id, e);
                                    continue;
                                }
                            };
                            for output in outputs {
                                if let ActiveStageOutput::ResponseFrame(data) = output {
                                    if let Err(e) = framed_write.write_all(&data).await {
                                        log::warn!("[RDP {}] write error: {}", connection_id, e);
                                    }
                                }
                            }
                        }
                    }
                    Some(RdpInput::Key(event)) => {
                        let scancode = Scancode::from_u16(event.scancode);
                        let op = if event.pressed {
                            Operation::KeyPressed(scancode)
                        } else {
                            Operation::KeyReleased(scancode)
                        };
                        let events = input_db.apply([op]);
                        if !events.is_empty() {
                            let outputs = match active_stage.process_fastpath_input(&mut image, &events) {
                                Ok(o) => o,
                                Err(e) => {
                                    log::warn!("[RDP {}] key input error: {}", connection_id, e);
                                    continue;
                                }
                            };
                            for output in outputs {
                                if let ActiveStageOutput::ResponseFrame(data) = output {
                                    if let Err(e) = framed_write.write_all(&data).await {
                                        log::warn!("[RDP {}] write error: {}", connection_id, e);
                                    }
                                }
                            }
                        }
                    }
                    Some(RdpInput::Disconnect) | None => {
                        break "disconnected by client".to_string();
                    }
                }
            }
        }
    };

    Ok(disconnect_reason)
}

/// Extract the RGBA pixel data for a rectangular sub-region of the decoded image.
///
/// The image data is stored row-major. For a partial-width rectangle we must
/// copy each row individually to avoid including pixels from adjacent columns.
///
/// ROB-1: validates that the rectangle fits within the image bounds to prevent
/// panics from untrusted RDP server data.
fn extract_rect_rgba(
    image: &DecodedImage,
    left: u16,
    top: u16,
    width: u16,
    height: u16,
) -> Option<Vec<u8>> {
    let bpp = image.bytes_per_pixel();
    let stride = image.stride();
    let data = image.data();
    let w = width as usize;
    let h = height as usize;

    // ROB-1: bounds check — reject rectangles that extend beyond the image
    if (left as usize + w) > image.width() as usize
        || (top as usize + h) > image.height() as usize
    {
        log::warn!(
            "RDP frame rect ({},{} {}x{}) exceeds image bounds ({}x{})",
            left, top, width, height, image.width(), image.height()
        );
        return None;
    }

    let mut out = Vec::with_capacity(w * h * bpp);
    for row in top as usize..(top as usize + h) {
        let start = row * stride + left as usize * bpp;
        let end = start + w * bpp;
        if end > data.len() {
            return None;
        }
        out.extend_from_slice(&data[start..end]);
    }
    Some(out)
}

/// Convert a frontend [`RdpMouseEvent`] into a list of IronRDP input operations.
fn build_mouse_operations(event: &RdpMouseEvent) -> Vec<Operation> {
    let mut ops = Vec::with_capacity(3);

    // Always report the current cursor position
    ops.push(Operation::MouseMove(MousePosition {
        x: event.x,
        y: event.y,
    }));

    // Scroll wheel — positive delta = scroll up, negative = scroll down
    if let Some(delta) = event.scroll_delta {
        ops.push(Operation::WheelRotations(WheelRotations {
            is_vertical: true,
            rotation_units: delta,
        }));
    }

    // Button press/release
    if let Some(btn) = event.button {
        let mouse_button = match btn {
            0 => Some(MouseButton::Left),
            1 => Some(MouseButton::Middle),
            2 => Some(MouseButton::Right),
            _ => None,
        };
        if let Some(button) = mouse_button {
            let op = if event.pressed {
                Operation::MouseButtonPressed(button)
            } else {
                Operation::MouseButtonReleased(button)
            };
            ops.push(op);
        }
    }

    ops
}

// ─── Error type ───────────────────────────────────────────────────────────────

/// Errors that can occur during the RDP session lifecycle.
#[derive(Debug, thiserror::Error)]
pub enum RdpError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("TLS error: {0}")]
    Tls(String),
    #[error("RDP protocol error: {0}")]
    Protocol(String),
    #[error("Invalid RDP host: {0}")]
    InvalidHost(String),
    #[error("Too many RDP connections (max {MAX_RDP_CONNECTIONS})")]
    TooManyConnections,
    #[error("Session not found: {0}")]
    NotFound(String),
    #[error("Session has been closed")]
    SessionClosed,
}

impl serde::Serialize for RdpError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
