use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine as _;
use tauri::{AppHandle, Emitter};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use vnc::{PixelFormat, VncConnector, VncEncoding, VncEvent};
use vnc::event::{ClientKeyEvent, ClientMouseEvent, Rect, X11Event};
use zeroize::Zeroize;

/// Maximum number of concurrent VNC sessions allowed.
const MAX_VNC_CONNECTIONS: usize = 8;

/// Default VNC desktop dimensions.
const DEFAULT_WIDTH: u16 = 1920;
const DEFAULT_HEIGHT: u16 = 1080;

/// Event name emitted when a frame region is updated.
pub const VNC_FRAME_EVENT: &str = "vnc-frame";

/// Event name emitted when a VNC session disconnects.
pub const VNC_DISCONNECTED_EVENT: &str = "vnc-disconnected";

/// Event name emitted when clipboard text is received from the VNC server.
pub const VNC_CLIPBOARD_EVENT: &str = "vnc-clipboard";

// ─── Input event types (sent from frontend) ─────────────────────────────────

/// A mouse input event from the frontend.
#[derive(Debug, serde::Deserialize)]
pub struct VncMouseEvent {
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
pub struct VncKeyEvent {
    /// X11 keysym value from the frontend.
    pub keysym: u32,
    /// Whether the key is pressed (true) or released (false).
    pub pressed: bool,
}

// ─── Frame update event (emitted to frontend) ────────────────────────────────

/// Payload for the `vnc-frame` Tauri event.
#[derive(Clone, serde::Serialize)]
pub struct VncFramePayload {
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

/// Payload for the `vnc-disconnected` Tauri event.
#[derive(Clone, serde::Serialize)]
pub struct VncDisconnectedPayload {
    pub connection_id: String,
    pub reason: String,
}

/// Payload for the `vnc-clipboard` Tauri event.
#[derive(Clone, serde::Serialize)]
pub struct VncClipboardPayload {
    pub connection_id: String,
    pub text: String,
}

// ─── Internal input channel message ──────────────────────────────────────────

enum VncInput {
    Mouse(VncMouseEvent),
    Key(VncKeyEvent),
    Clipboard(String),
    Disconnect,
}

// ─── Session and manager ─────────────────────────────────────────────────────

/// A running VNC session.
struct VncSession {
    /// Handle to the background session task.
    task: JoinHandle<()>,
    /// Sender for input events from the frontend.
    input_tx: mpsc::Sender<VncInput>,
}

/// Thread-safe manager for all active VNC sessions.
pub struct VncConnectionManager {
    sessions: Arc<Mutex<HashMap<String, VncSession>>>,
}

impl VncConnectionManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start a VNC session for the given host/port/credentials.
    pub async fn connect(
        &self,
        app: AppHandle,
        connection_id: &str,
        host: &str,
        port: u16,
        password: &str,
        username: Option<&str>,
    ) -> Result<(), VncError> {
        if !crate::session::is_valid_host(host) {
            return Err(VncError::InvalidHost(format!(
                "Host contains invalid characters: {}",
                host
            )));
        }

        let (input_tx, input_rx) = mpsc::channel::<VncInput>(64);

        let cid = connection_id.to_string();
        let host = host.to_string();
        let mut password = password.to_string();
        // TODO: vnc-rs does not currently support username-based auth (e.g. Apple ARD).
        // The username parameter is accepted here for forward compatibility but is not sent.
        let _username = username.map(str::to_string);
        let sessions = self.sessions.clone();

        let (start_tx, start_rx) = tokio::sync::oneshot::channel::<()>();

        let task = tokio::spawn(async move {
            if start_rx.await.is_err() {
                return;
            }

            let reason = match run_session(&app, &cid, &host, port, &password, input_rx).await {
                Ok(reason) => reason,
                Err(e) => {
                    log::error!("[VNC {}] session error: {}", cid, e);
                    e.to_string()
                }
            };
            password.zeroize();

            let payload = VncDisconnectedPayload {
                connection_id: cid.clone(),
                reason,
            };
            let _ = app.emit(VNC_DISCONNECTED_EVENT, payload);

            sessions.lock().await.remove(&cid);
        });

        {
            let mut sessions_guard = self.sessions.lock().await;
            if sessions_guard.len() >= MAX_VNC_CONNECTIONS {
                drop(start_tx);
                task.abort();
                return Err(VncError::TooManyConnections);
            }
            sessions_guard.insert(
                connection_id.to_string(),
                VncSession {
                    task,
                    input_tx: input_tx.clone(),
                },
            );
        }

        let _ = start_tx.send(());

        Ok(())
    }

    /// Send a mouse event to an active VNC session.
    pub async fn send_mouse(&self, connection_id: &str, event: VncMouseEvent) -> Result<(), VncError> {
        let tx = {
            let sessions = self.sessions.lock().await;
            sessions
                .get(connection_id)
                .ok_or_else(|| VncError::NotFound(connection_id.to_string()))?
                .input_tx
                .clone()
        };
        tx.send(VncInput::Mouse(event))
            .await
            .map_err(|_| VncError::SessionClosed)
    }

    /// Send a keyboard event to an active VNC session.
    pub async fn send_key(&self, connection_id: &str, event: VncKeyEvent) -> Result<(), VncError> {
        let tx = {
            let sessions = self.sessions.lock().await;
            sessions
                .get(connection_id)
                .ok_or_else(|| VncError::NotFound(connection_id.to_string()))?
                .input_tx
                .clone()
        };
        tx.send(VncInput::Key(event))
            .await
            .map_err(|_| VncError::SessionClosed)
    }

    /// Send clipboard text to an active VNC session.
    pub async fn send_clipboard(&self, connection_id: &str, text: String) -> Result<(), VncError> {
        let tx = {
            let sessions = self.sessions.lock().await;
            sessions
                .get(connection_id)
                .ok_or_else(|| VncError::NotFound(connection_id.to_string()))?
                .input_tx
                .clone()
        };
        tx.send(VncInput::Clipboard(text))
            .await
            .map_err(|_| VncError::SessionClosed)
    }

    /// Disconnect and clean up a VNC session.
    pub async fn disconnect(&self, connection_id: &str) -> Result<(), VncError> {
        let session = {
            let mut sessions = self.sessions.lock().await;
            sessions.remove(connection_id)
        };
        if let Some(session) = session {
            let _ = session.input_tx.send(VncInput::Disconnect).await;
            session.task.abort();
        }
        Ok(())
    }
}

// ─── Core session runner ──────────────────────────────────────────────────────

/// Run a complete VNC session from connect to disconnect.
async fn run_session(
    app: &AppHandle,
    connection_id: &str,
    host: &str,
    port: u16,
    password: &str,
    mut input_rx: mpsc::Receiver<VncInput>,
) -> Result<String, VncError> {
    let addr = format!("{}:{}", host, port);

    // ── 1. TCP connect with 15-second timeout ─────────────────────
    let tcp_stream = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        TcpStream::connect(&addr),
    )
        .await
        .map_err(|_| VncError::Io(format!("TCP connect to {} timed out after 15 seconds", addr)))?
        .map_err(|e| VncError::Io(format!("TCP connect to {}: {}", addr, e)))?;

    // ── 2. VNC handshake via vnc-rs ───────────────────────────────
    let password = password.to_string();
    let vnc = VncConnector::new(tcp_stream)
        .set_auth_method(async move { Ok(password) })
        .add_encoding(VncEncoding::Tight)
        .add_encoding(VncEncoding::Zrle)
        .add_encoding(VncEncoding::CopyRect)
        .add_encoding(VncEncoding::Raw)
        .allow_shared(true)
        .set_pixel_format(PixelFormat::rgba())
        .build()
        .map_err(|e| VncError::Protocol(e.to_string()))?
        .try_start()
        .await
        .map_err(|e| VncError::Auth(e.to_string()))?
        .finish()
        .map_err(|e| VncError::Protocol(e.to_string()))?;

    // ── 3. Framebuffer init ───────────────────────────────────────
    let mut fb_width: u16 = DEFAULT_WIDTH;
    let mut fb_height: u16 = DEFAULT_HEIGHT;
    let mut framebuffer: Vec<u8> = vec![0u8; fb_width as usize * fb_height as usize * 4];

    // Track VNC button mask state (bits 0-7 per RFB spec)
    let mut button_mask: u8 = 0;

    // ── 4. Event loop ─────────────────────────────────────────────
    let disconnect_reason = loop {
        tokio::select! {
            // ── Server → client ──────────────────────────────────
            event_result = vnc.poll_event() => {
                match event_result {
                    Ok(Some(event)) => {
                        match event {
                            VncEvent::SetResolution(screen) => {
                                fb_width = screen.width;
                                fb_height = screen.height;
                                framebuffer = vec![0u8; fb_width as usize * fb_height as usize * 4];
                            }
                            VncEvent::RawImage(rect, data) => {
                                update_framebuffer(
                                    &mut framebuffer,
                                    fb_width,
                                    &rect,
                                    &data,
                                );
                                emit_frame_tiles(
                                    app,
                                    connection_id,
                                    fb_width,
                                    fb_height,
                                    &rect,
                                    &data,
                                );
                            }
                            VncEvent::Copy(dst, src) => {
                                copy_framebuffer_rect(
                                    &mut framebuffer,
                                    fb_width,
                                    &src,
                                    &dst,
                                );
                                let dst_data = extract_rect_from_framebuffer(
                                    &framebuffer,
                                    fb_width,
                                    &dst,
                                );
                                emit_frame_tiles(
                                    app,
                                    connection_id,
                                    fb_width,
                                    fb_height,
                                    &dst,
                                    &dst_data,
                                );
                            }
                            VncEvent::Text(text) => {
                                let payload = VncClipboardPayload {
                                    connection_id: connection_id.to_string(),
                                    text,
                                };
                                let _ = app.emit(VNC_CLIPBOARD_EVENT, payload);
                            }
                            VncEvent::Bell => {
                                // Ignore bell for now
                            }
                            VncEvent::SetCursor(_rect, _data) => {
                                // Ignore custom cursor for now
                            }
                            VncEvent::SetPixelFormat(_) => {
                                // Acknowledged, no action needed
                            }
                            VncEvent::JpegImage(_rect, _data) => {
                                // TODO: decode JPEG and update framebuffer
                                // For now, request a full refresh to get raw data
                                let _ = vnc.input(X11Event::Refresh).await;
                            }
                            VncEvent::Error(msg) => {
                                break format!("VNC error: {}", msg);
                            }
                            _ => {
                                // Handle any future non_exhaustive variants
                            }
                        }
                    }
                    Ok(None) => {
                        // No event available, continue polling
                    }
                    Err(e) => {
                        break format!("VNC poll error: {}", e);
                    }
                }
            }

            // ── Client → server (keyboard / mouse / clipboard) ───
            input = input_rx.recv() => {
                match input {
                    Some(VncInput::Mouse(event)) => {
                        button_mask = update_button_mask(button_mask, &event);
                        let mouse_event = ClientMouseEvent {
                            position_x: event.x,
                            position_y: event.y,
                            bottons: button_mask,
                        };
                        if let Err(e) = vnc.input(X11Event::PointerEvent(mouse_event)).await {
                            log::warn!("[VNC {}] mouse input error: {}", connection_id, e);
                        }
                    }
                    Some(VncInput::Key(event)) => {
                        let key_event = ClientKeyEvent {
                            keycode: event.keysym,
                            down: event.pressed,
                        };
                        if let Err(e) = vnc.input(X11Event::KeyEvent(key_event)).await {
                            log::warn!("[VNC {}] key input error: {}", connection_id, e);
                        }
                    }
                    Some(VncInput::Clipboard(text)) => {
                        if let Err(e) = vnc.input(X11Event::CopyText(text)).await {
                            log::warn!("[VNC {}] clipboard send error: {}", connection_id, e);
                        }
                    }
                    Some(VncInput::Disconnect) | None => {
                        break "disconnected by client".to_string();
                    }
                }
            }
        }
    };

    let _ = vnc.close().await;

    Ok(disconnect_reason)
}

// ─── Framebuffer helpers ─────────────────────────────────────────────────────

/// Update the local framebuffer with incoming pixel data for a given rect.
fn update_framebuffer(
    framebuffer: &mut [u8],
    fb_width: u16,
    rect: &Rect,
    data: &[u8],
) {
    let bpp = 4usize; // RGBA
    let stride = fb_width as usize * bpp;
    let rect_w = rect.width as usize;
    let rect_h = rect.height as usize;
    let src_stride = rect_w * bpp;

    for row in 0..rect_h {
        let dst_offset = (rect.y as usize + row) * stride + rect.x as usize * bpp;
        let src_offset = row * src_stride;

        if dst_offset + src_stride > framebuffer.len() || src_offset + src_stride > data.len() {
            break;
        }

        framebuffer[dst_offset..dst_offset + src_stride]
            .copy_from_slice(&data[src_offset..src_offset + src_stride]);
    }
}

/// Copy a rectangular region within the framebuffer (for CopyRect encoding).
///
/// When source and destination overlap vertically, rows are iterated in reverse
/// order to prevent overwriting source data before it is read.
fn copy_framebuffer_rect(
    framebuffer: &mut [u8],
    fb_width: u16,
    src: &Rect,
    dst: &Rect,
) {
    let bpp = 4usize;
    let stride = fb_width as usize * bpp;
    let w = dst.width.min(src.width) as usize;
    let h = dst.height.min(src.height) as usize;
    let row_bytes = w * bpp;

    let mut row_buf = vec![0u8; row_bytes];

    // When dst is below src, iterate bottom-to-top to avoid overwriting
    // source rows before they are read (memmove-style overlap handling).
    let reverse = dst.y > src.y || (dst.y == src.y && dst.x > src.x);

    for i in 0..h {
        let row = if reverse { h - 1 - i } else { i };
        let src_offset = (src.y as usize + row) * stride + src.x as usize * bpp;
        let dst_offset = (dst.y as usize + row) * stride + dst.x as usize * bpp;

        if src_offset + row_bytes > framebuffer.len() || dst_offset + row_bytes > framebuffer.len() {
            continue;
        }

        row_buf.copy_from_slice(&framebuffer[src_offset..src_offset + row_bytes]);
        framebuffer[dst_offset..dst_offset + row_bytes].copy_from_slice(&row_buf);
    }
}

/// Extract pixel data for a rect from the framebuffer.
fn extract_rect_from_framebuffer(
    framebuffer: &[u8],
    fb_width: u16,
    rect: &Rect,
) -> Vec<u8> {
    let bpp = 4usize;
    let stride = fb_width as usize * bpp;
    let rect_w = rect.width as usize;
    let rect_h = rect.height as usize;
    let row_bytes = rect_w * bpp;
    let mut out = Vec::with_capacity(rect_w * rect_h * bpp);

    for row in 0..rect_h {
        let offset = (rect.y as usize + row) * stride + rect.x as usize * bpp;
        if offset + row_bytes > framebuffer.len() {
            break;
        }
        out.extend_from_slice(&framebuffer[offset..offset + row_bytes]);
    }
    out
}

/// Emit frame data as tiled vnc-frame events (max 256x256 per tile).
fn emit_frame_tiles(
    app: &AppHandle,
    connection_id: &str,
    fb_width: u16,
    fb_height: u16,
    rect: &Rect,
    data: &[u8],
) {
    const TILE_SIZE: u16 = 256;
    let bpp = 4usize;
    let src_stride = rect.width as usize * bpp;

    let mut ty: u16 = 0;
    while ty < rect.height {
        let th = TILE_SIZE.min(rect.height - ty);
        let mut tx: u16 = 0;
        while tx < rect.width {
            let tw = TILE_SIZE.min(rect.width - tx);

            let mut tile_data = Vec::with_capacity(tw as usize * th as usize * bpp);
            for row in 0..th as usize {
                let src_row = (ty as usize + row) * src_stride + tx as usize * bpp;
                let end = src_row + tw as usize * bpp;
                if end > data.len() {
                    break;
                }
                tile_data.extend_from_slice(&data[src_row..end]);
            }

            let payload = VncFramePayload {
                connection_id: connection_id.to_string(),
                full_width: fb_width,
                full_height: fb_height,
                x: rect.x + tx,
                y: rect.y + ty,
                width: tw,
                height: th,
                data: base64::engine::general_purpose::STANDARD.encode(&tile_data),
            };
            let _ = app.emit(VNC_FRAME_EVENT, payload);

            tx += TILE_SIZE;
        }
        ty += TILE_SIZE;
    }
}

/// Update the VNC button mask based on a mouse event from the frontend.
///
/// VNC button mask bits (per RFB spec):
/// - Bit 0: left button
/// - Bit 1: middle button
/// - Bit 2: right button
/// - Bit 3: scroll up
/// - Bit 4: scroll down
fn update_button_mask(mut mask: u8, event: &VncMouseEvent) -> u8 {
    // Clear scroll bits — they are momentary
    mask &= 0b0000_0111;

    // Handle button press/release
    if let Some(button) = event.button {
        let bit = match button {
            0 => 0, // left
            1 => 1, // middle
            2 => 2, // right
            _ => return mask,
        };
        if event.pressed {
            mask |= 1 << bit;
        } else {
            mask &= !(1 << bit);
        }
    }

    // Handle scroll
    if let Some(delta) = event.scroll_delta {
        if delta > 0 {
            mask |= 1 << 3; // scroll up
        } else if delta < 0 {
            mask |= 1 << 4; // scroll down
        }
    }

    mask
}

// ─── Error type ───────────────────────────────────────────────────────────────

/// Errors that can occur during the VNC session lifecycle.
#[derive(Debug, thiserror::Error)]
pub enum VncError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("VNC protocol error: {0}")]
    Protocol(String),
    #[error("VNC authentication error: {0}")]
    Auth(String),
    #[error("Invalid VNC host: {0}")]
    InvalidHost(String),
    #[error("Too many VNC connections (max {MAX_VNC_CONNECTIONS})")]
    TooManyConnections,
    #[error("Session not found: {0}")]
    NotFound(String),
    #[error("Session has been closed")]
    SessionClosed,
}

impl serde::Serialize for VncError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
