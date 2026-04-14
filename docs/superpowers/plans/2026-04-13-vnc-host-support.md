# VNC Host Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add VNC client connection support using `vnc-rs`, mirroring the existing RDP architecture (Rust manages session, emits frame events, frontend renders on canvas).

**Architecture:** Rust backend creates a `VncConnectionManager` (same pattern as `RdpConnectionManager`) that spawns per-connection tokio tasks. Each task uses `vnc-rs` to handle the RFB protocol, polls for frame events, and emits RGBA pixel data via Tauri events. The frontend renders frames on an HTML5 canvas in a new `VncPane` component.

**Tech Stack:** vnc-rs (Rust async VNC client), Tauri 2.0 events/commands, React + TypeScript + HTML5 Canvas

**Spec:** `docs/superpowers/specs/2026-04-13-vnc-host-support-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src-tauri/Cargo.toml` | Modify | Add `vnc-rs` dependency |
| `src-tauri/src/vnc.rs` | Create | VncConnectionManager, run_session, types, error |
| `src-tauri/src/session.rs` | Modify | Add `Protocol::Vnc` variant |
| `src-tauri/src/commands.rs` | Modify | Add 5 VNC commands + `AppError::Vnc` |
| `src-tauri/src/lib.rs` | Modify | Register VNC module, manager, commands |
| `src/types.ts` | Modify | Add VNC to Protocol, VNC payload types, `emptyVncDraft()` |
| `src/api.ts` | Modify | Add 5 VNC API wrappers |
| `src/components/VncPane.tsx` | Create | Canvas-based VNC viewer component |
| `src/components/SshSessionForm.tsx` | Modify | Add VNC protocol option + form fields |
| `src/App.tsx` | Modify | VNC connection flow + tab routing |

---

### Task 1: Add vnc-rs dependency and Protocol::Vnc

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/session.rs`

- [ ] **Step 1: Add vnc-rs to Cargo.toml**

In `src-tauri/Cargo.toml`, add to the `[dependencies]` section after the `tokio-native-tls` line:

```toml
# VNC client — async RFB protocol implementation
vnc-rs = "0.5"
```

- [ ] **Step 2: Add Vnc variant to Protocol enum in session.rs**

In `src-tauri/src/session.rs`, change the `Protocol` enum from:

```rust
pub enum Protocol {
    /// SSH terminal session.
    Ssh,
    /// RDP remote desktop session.
    Rdp,
}
```

to:

```rust
pub enum Protocol {
    /// SSH terminal session.
    Ssh,
    /// RDP remote desktop session.
    Rdp,
    /// VNC remote desktop session.
    Vnc,
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully (may have warnings about unused `Vnc` variant — that's fine at this stage).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/session.rs
git commit -m "feat: add vnc-rs dependency and Protocol::Vnc variant"
```

---

### Task 2: Create vnc.rs — types and VncConnectionManager skeleton

**Files:**
- Create: `src-tauri/src/vnc.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create vnc.rs with types, error, and manager skeleton**

Create `src-tauri/src/vnc.rs`:

```rust
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
        validate_vnc_host(host)?;

        let (input_tx, input_rx) = mpsc::channel::<VncInput>(64);

        let cid = connection_id.to_string();
        let host = host.to_string();
        let mut password = password.to_string();
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

/// Validate that the VNC host is a reasonable hostname or IP address.
fn validate_vnc_host(host: &str) -> Result<(), VncError> {
    if host.is_empty() || host.len() > 253 {
        return Err(VncError::InvalidHost(
            "Host must be between 1 and 253 characters".to_string(),
        ));
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Ok(());
    }
    if !host
        .chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        return Err(VncError::InvalidHost(format!(
            "Host contains invalid characters: {}",
            host
        )));
    }
    Ok(())
}

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
                                // Update framebuffer
                                update_framebuffer(
                                    &mut framebuffer,
                                    fb_width,
                                    &rect,
                                    &data,
                                );
                                // Emit dirty rect as tiled vnc-frame events
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
                                // Copy pixels within the framebuffer
                                copy_framebuffer_rect(
                                    &mut framebuffer,
                                    fb_width,
                                    &src,
                                    &dst,
                                );
                                // Emit the destination rect as updated
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
                        // Update button mask based on press/release
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

    // Gracefully close the VNC connection
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

    // Copy row by row via a temporary buffer to handle overlapping regions
    let mut row_buf = vec![0u8; row_bytes];
    for row in 0..h {
        let src_offset = (src.y as usize + row) * stride + src.x as usize * bpp;
        let dst_offset = (dst.y as usize + row) * stride + dst.x as usize * bpp;

        if src_offset + row_bytes > framebuffer.len() || dst_offset + row_bytes > framebuffer.len() {
            break;
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

            // Extract tile pixels from the rect data
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
```

- [ ] **Step 2: Register the vnc module in lib.rs**

In `src-tauri/src/lib.rs`, add `pub mod vnc;` after the existing module declarations:

Change:
```rust
pub mod commands;
pub mod known_hosts;
pub mod rdp;
pub mod session;
pub mod ssh;
```

To:
```rust
pub mod commands;
pub mod known_hosts;
pub mod rdp;
pub mod session;
pub mod ssh;
pub mod vnc;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully. The `vnc` module is registered but its manager is not yet wired into commands.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/vnc.rs src-tauri/src/lib.rs
git commit -m "feat: add VncConnectionManager with session runner and framebuffer helpers"
```

---

### Task 3: Add VNC commands to commands.rs and register in lib.rs

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add VNC imports and AppError::Vnc to commands.rs**

In `src-tauri/src/commands.rs`, add the VNC import to the existing use block. Change:

```rust
use crate::rdp::{RdpConnectionManager, RdpKeyEvent, RdpMouseEvent};
```

To:

```rust
use crate::rdp::{RdpConnectionManager, RdpKeyEvent, RdpMouseEvent};
use crate::vnc::{VncConnectionManager, VncKeyEvent, VncMouseEvent};
```

Add the `Vnc` variant to the `AppError` enum. Change:

```rust
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("SSH error: {0}")]
    Ssh(String),
    #[error("RDP error: {0}")]
    Rdp(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Validation error: {0}")]
    Validation(String),
}
```

To:

```rust
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("SSH error: {0}")]
    Ssh(String),
    #[error("RDP error: {0}")]
    Rdp(String),
    #[error("VNC error: {0}")]
    Vnc(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Validation error: {0}")]
    Validation(String),
}
```

- [ ] **Step 2: Add VNC command functions to commands.rs**

Append the following after the RDP commands section at the end of `commands.rs`:

```rust
// ─── VNC connection commands ────────────────────────────────────────────────

/// Response from a successful VNC connection attempt.
#[derive(serde::Serialize)]
pub struct VncConnectResult {
    connection_id: String,
}

/// Start a VNC session for the session specified by `session_id`.
#[tauri::command]
pub async fn vnc_connect(
    app: AppHandle,
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

    let pw = password
        .as_deref()
        .or(session.password.as_deref())
        .unwrap_or("");

    let connection_id = uuid::Uuid::new_v4().to_string();

    let username = if session.username.is_empty() {
        None
    } else {
        Some(session.username.as_str())
    };

    vnc_manager
        .connect(
            app,
            &connection_id,
            &session.host,
            session.port,
            pw,
            username,
        )
        .await
        .map_err(|e| AppError::Vnc(e.to_string()))?;

    Ok(VncConnectResult { connection_id })
}

/// Disconnect an active VNC session and clean up resources.
#[tauri::command]
pub async fn vnc_disconnect(
    vnc_manager: State<'_, VncConnectionManager>,
    connection_id: String,
) -> Result<(), AppError> {
    vnc_manager
        .disconnect(&connection_id)
        .await
        .map_err(|e| AppError::Vnc(e.to_string()))
}

/// Send a mouse event to an active VNC session.
#[tauri::command]
pub async fn vnc_mouse_event(
    vnc_manager: State<'_, VncConnectionManager>,
    connection_id: String,
    event: VncMouseEvent,
) -> Result<(), AppError> {
    vnc_manager
        .send_mouse(&connection_id, event)
        .await
        .map_err(|e| AppError::Vnc(e.to_string()))
}

/// Send a keyboard event to an active VNC session.
#[tauri::command]
pub async fn vnc_key_event(
    vnc_manager: State<'_, VncConnectionManager>,
    connection_id: String,
    event: VncKeyEvent,
) -> Result<(), AppError> {
    vnc_manager
        .send_key(&connection_id, event)
        .await
        .map_err(|e| AppError::Vnc(e.to_string()))
}

/// Send clipboard text to an active VNC session.
#[tauri::command]
pub async fn vnc_send_clipboard(
    vnc_manager: State<'_, VncConnectionManager>,
    connection_id: String,
    text: String,
) -> Result<(), AppError> {
    vnc_manager
        .send_clipboard(&connection_id, text)
        .await
        .map_err(|e| AppError::Vnc(e.to_string()))
}
```

- [ ] **Step 3: Register VNC manager and commands in lib.rs**

In `src-tauri/src/lib.rs`, update the imports. Change:

```rust
use commands::{
    delete_session, export_sessions, get_sessions, import_sessions, save_session,
    ssh_accept_host_key, ssh_connect, ssh_disconnect, ssh_resize, ssh_send,
    rdp_connect, rdp_disconnect, rdp_mouse_event, rdp_key_event,
};
use rdp::RdpConnectionManager;
use ssh::SshConnectionManager;
```

To:

```rust
use commands::{
    delete_session, export_sessions, get_sessions, import_sessions, save_session,
    ssh_accept_host_key, ssh_connect, ssh_disconnect, ssh_resize, ssh_send,
    rdp_connect, rdp_disconnect, rdp_mouse_event, rdp_key_event,
    vnc_connect, vnc_disconnect, vnc_mouse_event, vnc_key_event, vnc_send_clipboard,
};
use rdp::RdpConnectionManager;
use ssh::SshConnectionManager;
use vnc::VncConnectionManager;
```

Add VNC manager registration. Change:

```rust
        .manage(SshConnectionManager::new())
        .manage(RdpConnectionManager::new())
```

To:

```rust
        .manage(SshConnectionManager::new())
        .manage(RdpConnectionManager::new())
        .manage(VncConnectionManager::new())
```

Add VNC commands to invoke_handler. Change:

```rust
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
```

To:

```rust
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
            vnc_connect,
            vnc_disconnect,
            vnc_mouse_event,
            vnc_key_event,
            vnc_send_clipboard,
        ])
```

- [ ] **Step 4: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully. All VNC commands are registered and wired up.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: add VNC Tauri commands and register manager"
```

---

### Task 4: Add VNC types and API wrappers to frontend

**Files:**
- Modify: `src/types.ts`
- Modify: `src/api.ts`

- [ ] **Step 1: Update Protocol type and add VNC types in types.ts**

In `src/types.ts`, change the Protocol type:

```typescript
export type Protocol = "ssh" | "rdp";
```

To:

```typescript
export type Protocol = "ssh" | "rdp" | "vnc";
```

Add VNC payload types after the `RdpDisconnectedPayload` interface:

```typescript
/** Payload of a VNC frame update event from the backend. */
export interface VncFramePayload {
  connection_id: string;
  full_width: number;
  full_height: number;
  x: number;
  y: number;
  width: number;
  height: number;
  /** Base64-encoded RGBA pixel data for the dirty rectangle. */
  data: string;
}

/** Payload of the vnc-disconnected event. */
export interface VncDisconnectedPayload {
  connection_id: string;
  reason: string;
}

/** Payload of the vnc-clipboard event. */
export interface VncClipboardPayload {
  connection_id: string;
  text: string;
}
```

Add `emptyVncDraft()` after the existing `emptyRdpDraft()`:

```typescript
/** Create an empty VNC draft with sensible defaults. */
export function emptyVncDraft(): SshSessionDraft {
  return {
    label: "",
    protocol: "vnc",
    host: "",
    port: 5900,
    username: "",
    auth_method: "password",
    password: "",
    notes: "",
  };
}
```

- [ ] **Step 2: Add VNC API wrappers in api.ts**

In `src/api.ts`, add the VNC section after the RDP commands. Also add `emptyVncDraft` to any needed imports if the file imports from types (it currently doesn't import draft helpers, so no import change needed).

Append after the RDP section:

```typescript
// ─── VNC connection commands ────────────────────────────────────────────────

/** Start a VNC session for the given session. */
export async function vncConnect(
  sessionId: string,
  password?: string,
): Promise<{ connection_id: string }> {
  return invoke<{ connection_id: string }>("vnc_connect", {
    sessionId,
    password: password ?? null,
  });
}

/** Disconnect an active VNC session. */
export async function vncDisconnect(connectionId: string): Promise<void> {
  return invoke("vnc_disconnect", { connectionId });
}

/** Send a mouse event to an active VNC session. */
export async function vncMouseEvent(
  connectionId: string,
  x: number,
  y: number,
  button: number | null,
  pressed: boolean,
  scrollDelta: number | null,
): Promise<void> {
  return invoke("vnc_mouse_event", {
    connectionId,
    event: { x, y, button, pressed, scroll_delta: scrollDelta },
  });
}

/** Send a keyboard event to an active VNC session. */
export async function vncKeyEvent(
  connectionId: string,
  keysym: number,
  pressed: boolean,
): Promise<void> {
  return invoke("vnc_key_event", {
    connectionId,
    event: { keysym, pressed },
  });
}

/** Send clipboard text to an active VNC session. */
export async function vncSendClipboard(
  connectionId: string,
  text: string,
): Promise<void> {
  return invoke("vnc_send_clipboard", { connectionId, text });
}
```

- [ ] **Step 3: Verify frontend compiles**

Run: `npm run build`
Expected: TypeScript compilation succeeds (the new types and functions are exported but not yet consumed by components).

- [ ] **Step 4: Commit**

```bash
git add src/types.ts src/api.ts
git commit -m "feat: add VNC types and API wrappers to frontend"
```

---

### Task 5: Create VncPane component

**Files:**
- Create: `src/components/VncPane.tsx`

- [ ] **Step 1: Create VncPane.tsx**

Create `src/components/VncPane.tsx`:

```tsx
import { useEffect, useRef, useCallback, useState } from "react";
import type { MouseEvent, WheelEvent, KeyboardEvent } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { VncFramePayload, VncDisconnectedPayload, VncClipboardPayload } from "../types";
import { vncMouseEvent, vncKeyEvent, vncSendClipboard } from "../api";

interface VncPaneProps {
  /** Unique connection identifier from the backend. */
  connectionId: string;
  /** Called when the user explicitly closes the tab. */
  onDisconnected: () => void;
  /** Called to re-establish the connection (reconnect). */
  onReconnect: () => void;
}

/**
 * X11 keysym map for browser KeyboardEvent.code values.
 *
 * VNC uses X11 keysyms (not scancodes like RDP). This map covers
 * the most common keys. Printable characters use their Unicode
 * code point directly via KeyboardEvent.key.
 */
const KEYSYM_MAP: Record<string, number> = {
  Backspace: 0xff08,
  Tab: 0xff09,
  Enter: 0xff0d,
  Escape: 0xff1b,
  Delete: 0xffff,
  Home: 0xff50,
  End: 0xff57,
  PageUp: 0xff55,
  PageDown: 0xff56,
  ArrowLeft: 0xff51,
  ArrowUp: 0xff52,
  ArrowRight: 0xff53,
  ArrowDown: 0xff54,
  Insert: 0xff63,
  F1: 0xffbe, F2: 0xffbf, F3: 0xffc0, F4: 0xffc1,
  F5: 0xffc2, F6: 0xffc3, F7: 0xffc4, F8: 0xffc5,
  F9: 0xffc6, F10: 0xffc7, F11: 0xffc8, F12: 0xffc9,
  ShiftLeft: 0xffe1, ShiftRight: 0xffe2,
  ControlLeft: 0xffe3, ControlRight: 0xffe4,
  AltLeft: 0xffe9, AltRight: 0xffea,
  MetaLeft: 0xffeb, MetaRight: 0xffec,
  CapsLock: 0xffe5,
  NumLock: 0xff7f,
  ScrollLock: 0xff14,
  // Numpad
  NumpadEnter: 0xff8d,
  NumpadMultiply: 0xffaa,
  NumpadAdd: 0xffab,
  NumpadSubtract: 0xffad,
  NumpadDecimal: 0xffae,
  NumpadDivide: 0xffaf,
  Numpad0: 0xffb0, Numpad1: 0xffb1, Numpad2: 0xffb2, Numpad3: 0xffb3,
  Numpad4: 0xffb4, Numpad5: 0xffb5, Numpad6: 0xffb6, Numpad7: 0xffb7,
  Numpad8: 0xffb8, Numpad9: 0xffb9,
  Space: 0x0020,
};

/**
 * Convert a browser KeyboardEvent to an X11 keysym.
 *
 * For printable characters, use the Unicode code point.
 * For special keys, use the KEYSYM_MAP lookup.
 */
function keyEventToKeysym(e: KeyboardEvent<HTMLDivElement>): number | null {
  // Check special keys first
  const mapped = KEYSYM_MAP[e.code];
  if (mapped !== undefined) return mapped;

  // For printable characters, use the key value's char code
  if (e.key.length === 1) {
    return e.key.charCodeAt(0);
  }

  return null;
}

/**
 * Renders a VNC remote-desktop session on an HTML5 canvas.
 *
 * Data flow:
 * - Rust backend emits `vnc-frame` events with RGBA pixel data for dirty rects
 * - This component paints each dirty rect onto the canvas using `putImageData`
 * - Keyboard / mouse events are forwarded to the backend via Tauri commands
 * - Clipboard events from the server are written to the local clipboard
 */
export default function VncPane({ connectionId, onDisconnected, onReconnect }: VncPaneProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const connectionIdRef = useRef(connectionId);
  const onDisconnectedRef = useRef(onDisconnected);
  const ctxRef = useRef<CanvasRenderingContext2D | null>(null);
  const [error, setError] = useState<string | null>(null);

  connectionIdRef.current = connectionId;
  onDisconnectedRef.current = onDisconnected;

  // ── Canvas helper: decode base64 RGBA and blit it to the canvas ───────────
  const blitFrame = useCallback((payload: VncFramePayload) => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    if (canvas.width !== payload.full_width || canvas.height !== payload.full_height) {
      canvas.width = payload.full_width;
      canvas.height = payload.full_height;
      ctxRef.current = canvas.getContext("2d");
    }

    if (!ctxRef.current) {
      ctxRef.current = canvas.getContext("2d");
    }
    const ctx = ctxRef.current;
    if (!ctx) return;

    const raw = atob(payload.data);
    const bytes = Uint8Array.from(raw, (c) => c.charCodeAt(0));

    const imageData = new ImageData(
      new Uint8ClampedArray(bytes.buffer),
      payload.width,
      payload.height,
    );
    ctx.putImageData(imageData, payload.x, payload.y);
  }, []);

  // ── Subscribe to backend events ───────────────────────────────────────────
  useEffect(() => {
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];

    listen<VncFramePayload>("vnc-frame", (event) => {
      if (event.payload.connection_id === connectionIdRef.current) {
        blitFrame(event.payload);
      }
    }).then((fn) => {
      if (cancelled) { fn(); } else { unlisteners.push(fn); }
    });

    listen<VncDisconnectedPayload>("vnc-disconnected", (event) => {
      if (event.payload.connection_id === connectionIdRef.current) {
        const reason = event.payload.reason || "Connection closed";
        setError(`VNC session ended: ${reason}`);
      }
    }).then((fn) => {
      if (cancelled) { fn(); } else { unlisteners.push(fn); }
    });

    listen<VncClipboardPayload>("vnc-clipboard", (event) => {
      if (event.payload.connection_id === connectionIdRef.current) {
        navigator.clipboard.writeText(event.payload.text).catch(() => {});
      }
    }).then((fn) => {
      if (cancelled) { fn(); } else { unlisteners.push(fn); }
    });

    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, [blitFrame]);

  // ── Send local clipboard to VNC server on paste ──────────────────────────
  useEffect(() => {
    const handlePaste = (e: ClipboardEvent) => {
      const text = e.clipboardData?.getData("text/plain");
      if (text) {
        vncSendClipboard(connectionIdRef.current, text).catch(() => {});
      }
    };

    const container = containerRef.current;
    if (container) {
      container.addEventListener("paste", handlePaste);
      return () => container.removeEventListener("paste", handlePaste);
    }
  }, []);

  // ── Mouse events ──────────────────────────────────────────────────────────
  const getCanvasCoords = (e: MouseEvent<HTMLCanvasElement>): [number, number] => {
    const canvas = canvasRef.current;
    if (!canvas) return [0, 0];
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    return [
      Math.round((e.clientX - rect.left) * scaleX),
      Math.round((e.clientY - rect.top) * scaleY),
    ];
  };

  const handleMouseMove = useCallback((e: MouseEvent<HTMLCanvasElement>) => {
    const [x, y] = getCanvasCoords(e);
    vncMouseEvent(connectionIdRef.current, x, y, null, false, null).catch(() => {});
  }, []);

  const handleMouseDown = useCallback((e: MouseEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const [x, y] = getCanvasCoords(e);
    vncMouseEvent(connectionIdRef.current, x, y, e.button, true, null).catch(() => {});
  }, []);

  const handleMouseUp = useCallback((e: MouseEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const [x, y] = getCanvasCoords(e);
    vncMouseEvent(connectionIdRef.current, x, y, e.button, false, null).catch(() => {});
  }, []);

  const handleWheel = useCallback((e: WheelEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    const x = Math.round((e.clientX - rect.left) * scaleX);
    const y = Math.round((e.clientY - rect.top) * scaleY);
    const delta = e.deltaY < 0 ? 120 : -120;
    vncMouseEvent(connectionIdRef.current, x, y, null, false, delta).catch(() => {});
  }, []);

  const handleContextMenu = useCallback((e: MouseEvent<HTMLCanvasElement>) => {
    e.preventDefault();
  }, []);

  // ── Keyboard events ───────────────────────────────────────────────────────
  const handleKeyDown = useCallback((e: KeyboardEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    const keysym = keyEventToKeysym(e);
    if (keysym !== null) {
      vncKeyEvent(connectionIdRef.current, keysym, true).catch(() => {});
    }
  }, []);

  const handleKeyUp = useCallback((e: KeyboardEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    const keysym = keyEventToKeysym(e);
    if (keysym !== null) {
      vncKeyEvent(connectionIdRef.current, keysym, false).catch(() => {});
    }
  }, []);

  if (error) {
    return (
      <div className="pane-error">
        <div className="pane-error-icon">&#x26A0;</div>
        <div className="pane-error-title">Connection Failed</div>
        <div className="pane-error-message">{error}</div>
        <div className="pane-error-actions">
          <button className="btn-primary" onClick={onReconnect}>
            Reconnect
          </button>
          <button className="btn-secondary" onClick={onDisconnected}>
            Close Tab
          </button>
        </div>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="vnc-container"
      tabIndex={0}
      onKeyDown={handleKeyDown}
      onKeyUp={handleKeyUp}
      style={{ width: "100%", height: "100%", outline: "none", overflow: "auto" }}
    >
      <canvas
        ref={canvasRef}
        onMouseMove={handleMouseMove}
        onMouseDown={handleMouseDown}
        onMouseUp={handleMouseUp}
        onWheel={handleWheel}
        onContextMenu={handleContextMenu}
        style={{ display: "block", maxWidth: "100%", cursor: "default" }}
      />
    </div>
  );
}
```

- [ ] **Step 2: Verify frontend compiles**

Run: `npm run build`
Expected: TypeScript compilation succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/components/VncPane.tsx
git commit -m "feat: add VncPane canvas-based VNC viewer component"
```

---

### Task 6: Update SshSessionForm for VNC protocol

**Files:**
- Modify: `src/components/SshSessionForm.tsx`

- [ ] **Step 1: Add VNC import and protocol option**

In `src/components/SshSessionForm.tsx`, update the import to include `emptyVncDraft`:

```typescript
import { emptySshDraft, emptyRdpDraft, emptyVncDraft } from "../types";
```

- [ ] **Step 2: Add VNC to handleProtocolChange**

Change the `handleProtocolChange` function from:

```typescript
  const handleProtocolChange = (protocol: Protocol) => {
    if (protocol === draft.protocol) return;
    const base =
      protocol === "rdp"
        ? emptyRdpDraft()
        : emptySshDraft();
    // Preserve label, host, and notes across protocol switches
    setDraft({
      ...base,
      label: draft.label,
      host: draft.host,
      notes: draft.notes,
    });
    setErrors({});
  };
```

To:

```typescript
  const handleProtocolChange = (protocol: Protocol) => {
    if (protocol === draft.protocol) return;
    const base =
      protocol === "rdp"
        ? emptyRdpDraft()
        : protocol === "vnc"
          ? emptyVncDraft()
          : emptySshDraft();
    // Preserve label, host, and notes across protocol switches
    setDraft({
      ...base,
      label: draft.label,
      host: draft.host,
      notes: draft.notes,
    });
    setErrors({});
  };
```

- [ ] **Step 3: Add VNC option to protocol dropdown**

Change the protocol select from:

```tsx
        <select
          id="protocol"
          value={draft.protocol}
          onChange={(e) => handleProtocolChange(e.target.value as Protocol)}
        >
          <option value="ssh">SSH</option>
          <option value="rdp">RDP</option>
        </select>
```

To:

```tsx
        <select
          id="protocol"
          value={draft.protocol}
          onChange={(e) => handleProtocolChange(e.target.value as Protocol)}
        >
          <option value="ssh">SSH</option>
          <option value="rdp">RDP</option>
          <option value="vnc">VNC</option>
        </select>
```

- [ ] **Step 4: Add isVnc variable and VNC-specific form fields**

After the existing `isRdp` variable:

```typescript
  const isSsh = draft.protocol === "ssh";
  const isRdp = draft.protocol === "rdp";
```

Change to:

```typescript
  const isSsh = draft.protocol === "ssh";
  const isRdp = draft.protocol === "rdp";
  const isVnc = draft.protocol === "vnc";
```

Add a VNC fields block after the RDP fields block (after the closing `</>` of `{isRdp && (...)}`):

```tsx
      {/* VNC-specific fields */}
      {isVnc && (
        <>
          {/* Username (optional — for servers supporting user+password) */}
          <div className="form-group">
            <label htmlFor={`${prefix}-username`}>Username (optional)</label>
            <input
              id={`${prefix}-username`}
              type="text"
              placeholder="Leave empty for classic VNC auth"
              value={draft.username}
              onChange={(e) => set("username", e.target.value)}
            />
          </div>

          {/* Password */}
          <div className="form-group">
            <label htmlFor={`${prefix}-password`}>Password</label>
            <input
              id={`${prefix}-password`}
              type="password"
              placeholder="VNC password"
              value={draft.password ?? ""}
              onChange={(e) => set("password", e.target.value)}
            />
          </div>
        </>
      )}
```

- [ ] **Step 5: Verify frontend compiles**

Run: `npm run build`
Expected: TypeScript compilation succeeds.

- [ ] **Step 6: Commit**

```bash
git add src/components/SshSessionForm.tsx
git commit -m "feat: add VNC protocol option to session form"
```

---

### Task 7: Update SessionList for VNC protocol badge

**Files:**
- Modify: `src/components/SessionList.tsx`

- [ ] **Step 1: Update protocol badge logic**

In `src/components/SessionList.tsx`, change the `protoBadge` assignment from:

```typescript
        const protoBadge = proto === "rdp" ? "RDP" : "SSH";
```

To:

```typescript
        const protoBadge = proto === "rdp" ? "RDP" : proto === "vnc" ? "VNC" : "SSH";
```

- [ ] **Step 2: Update tooltip**

Change the tooltip from:

```typescript
        const tooltip =
          proto === "rdp"
            ? `${s.username}@${s.host}:${s.port} (RDP) — Double-click to connect`
            : `${s.username}@${s.host}:${s.port} — Double-click to connect`;
```

To:

```typescript
        const tooltip =
          proto === "rdp"
            ? `${s.username}@${s.host}:${s.port} (RDP) — Double-click to connect`
            : proto === "vnc"
              ? `${s.host}:${s.port} (VNC) — Double-click to connect`
              : `${s.username}@${s.host}:${s.port} — Double-click to connect`;
```

- [ ] **Step 3: Update detail display for VNC (no username required)**

Change the detail from:

```typescript
        const detail =
          proto === "rdp"
            ? `${s.username}@${s.host}:${s.port}`
            : `${s.username}@${s.host}:${s.port}`;
```

To:

```typescript
        const detail =
          proto === "vnc"
            ? `${s.host}:${s.port}`
            : `${s.username}@${s.host}:${s.port}`;
```

- [ ] **Step 4: Verify frontend compiles**

Run: `npm run build`
Expected: TypeScript compilation succeeds.

- [ ] **Step 5: Commit**

```bash
git add src/components/SessionList.tsx
git commit -m "feat: add VNC protocol badge to session list"
```

---

### Task 8: Wire VNC into App.tsx connection flow

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Add VNC imports**

Update the component import at the top of `App.tsx`. Change:

```typescript
import RdpPane from "./components/RdpPane";
```

To:

```typescript
import RdpPane from "./components/RdpPane";
import VncPane from "./components/VncPane";
```

Update the API import. Change:

```typescript
import {
  getSessions,
  saveSession,
  deleteSession,
  exportSessions,
  importSessions,
  sshConnect,
  sshAcceptHostKey,
  sshDisconnect,
  rdpConnect,
  rdpDisconnect,
} from "./api";
```

To:

```typescript
import {
  getSessions,
  saveSession,
  deleteSession,
  exportSessions,
  importSessions,
  sshConnect,
  sshAcceptHostKey,
  sshDisconnect,
  rdpConnect,
  rdpDisconnect,
  vncConnect,
  vncDisconnect,
} from "./api";
```

- [ ] **Step 2: Add VNC connection flow to handleConnect**

In the `handleConnect` function, add a VNC branch before the SSH flow. Change:

```typescript
      if (proto === "rdp") {
        // RDP connection flow
        try {
          setStatus({ type: "success", text: `Connecting to ${session.label}…` });
          const result = await rdpConnect(session.id, overridePassword ?? session.password);

          const conn: Connection = {
            id: result.connection_id,
            sessionId: session.id,
            label: session.label,
            protocol: "rdp",
          };
          setConnections((prev) => [...prev, conn]);
          setActiveConnectionId(result.connection_id);
          setStatus({ type: "success", text: `Connected to ${session.label}` });
        } catch (err: unknown) {
          setStatus({ type: "error", text: String(err) });
        }
        return;
      }
```

To:

```typescript
      if (proto === "rdp") {
        // RDP connection flow
        try {
          setStatus({ type: "success", text: `Connecting to ${session.label}…` });
          const result = await rdpConnect(session.id, overridePassword ?? session.password);

          const conn: Connection = {
            id: result.connection_id,
            sessionId: session.id,
            label: session.label,
            protocol: "rdp",
          };
          setConnections((prev) => [...prev, conn]);
          setActiveConnectionId(result.connection_id);
          setStatus({ type: "success", text: `Connected to ${session.label}` });
        } catch (err: unknown) {
          setStatus({ type: "error", text: String(err) });
        }
        return;
      }

      if (proto === "vnc") {
        // VNC connection flow
        try {
          setStatus({ type: "success", text: `Connecting to ${session.label}…` });
          const result = await vncConnect(session.id, overridePassword ?? session.password);

          const conn: Connection = {
            id: result.connection_id,
            sessionId: session.id,
            label: session.label,
            protocol: "vnc",
          };
          setConnections((prev) => [...prev, conn]);
          setActiveConnectionId(result.connection_id);
          setStatus({ type: "success", text: `Connected to ${session.label}` });
        } catch (err: unknown) {
          setStatus({ type: "error", text: String(err) });
        }
        return;
      }
```

- [ ] **Step 3: Add VNC disconnect to handleDisconnect**

Update the `handleDisconnect` function. Change:

```typescript
      if (conn?.protocol === "rdp") {
        await rdpDisconnect(connectionId);
      } else {
        await sshDisconnect(connectionId);
      }
```

To:

```typescript
      if (conn?.protocol === "rdp") {
        await rdpDisconnect(connectionId);
      } else if (conn?.protocol === "vnc") {
        await vncDisconnect(connectionId);
      } else {
        await sshDisconnect(connectionId);
      }
```

- [ ] **Step 4: Add VncPane to tab content rendering**

Update the tab content area where panes are rendered. Change:

```tsx
                  {conn.protocol === "rdp" ? (
                    <RdpPane
                      connectionId={conn.id}
                      onDisconnected={() => handleRemoteDisconnect(conn.id)}
                      onReconnect={() => {
                        const session = sessions.find((s) => s.id === conn.sessionId);
                        removeConnection(conn.id);
                        if (session) handleConnect(session);
                      }}
                    />
                  ) : (
                    <TerminalPane
                      connectionId={conn.id}
                      onDisconnected={() => handleRemoteDisconnect(conn.id)}
                    />
                  )}
```

To:

```tsx
                  {conn.protocol === "rdp" ? (
                    <RdpPane
                      connectionId={conn.id}
                      onDisconnected={() => handleRemoteDisconnect(conn.id)}
                      onReconnect={() => {
                        const session = sessions.find((s) => s.id === conn.sessionId);
                        removeConnection(conn.id);
                        if (session) handleConnect(session);
                      }}
                    />
                  ) : conn.protocol === "vnc" ? (
                    <VncPane
                      connectionId={conn.id}
                      onDisconnected={() => handleRemoteDisconnect(conn.id)}
                      onReconnect={() => {
                        const session = sessions.find((s) => s.id === conn.sessionId);
                        removeConnection(conn.id);
                        if (session) handleConnect(session);
                      }}
                    />
                  ) : (
                    <TerminalPane
                      connectionId={conn.id}
                      onDisconnected={() => handleRemoteDisconnect(conn.id)}
                    />
                  )}
```

- [ ] **Step 5: Verify frontend compiles**

Run: `npm run build`
Expected: TypeScript compilation succeeds with no errors.

- [ ] **Step 6: Commit**

```bash
git add src/App.tsx
git commit -m "feat: wire VNC connection flow and VncPane into App"
```

---

### Task 9: Full build verification

**Files:** None (verification only)

- [ ] **Step 1: Run Rust checks**

Run: `cd src-tauri && cargo clippy`
Expected: No errors. Fix any clippy warnings.

- [ ] **Step 2: Run TypeScript checks**

Run: `npm run build`
Expected: Clean build with no errors.

- [ ] **Step 3: Run Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All existing tests pass (session validation, host validation, password serialization).

- [ ] **Step 4: Start dev server and verify UI**

Run: `npx tauri dev`
Expected:
- App launches successfully
- Protocol dropdown in "New Host" form shows SSH, RDP, VNC options
- Selecting VNC shows password field and optional username field
- VNC session can be saved and appears in the session list with "VNC" badge
- Session list shows `host:port` format for VNC (no username prefix)

- [ ] **Step 5: Final commit if any fixes were needed**

```bash
git add -A
git commit -m "fix: address clippy warnings and build issues"
```
