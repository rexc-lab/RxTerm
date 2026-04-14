# VNC Host Support — Design Spec

**Date:** 2026-04-13
**Status:** Approved
**Approach:** Pure Rust VNC client using `vnc-rs` crate (Approach A)

## Summary

Add VNC client connection support to RxTerm, allowing users to connect to remote VNC servers. The implementation mirrors the existing RDP architecture: Rust owns the VNC session via the `vnc-rs` async library, emits frame events with RGBA pixel data, and the frontend renders on an HTML5 canvas.

**Scope — Implement:**
- Connect, view, send keyboard/mouse input, disconnect
- Clipboard sharing (bidirectional)

**Scope — Future (not implemented):**
- File transfer
- Encoding/quality preference UI

## Architecture

```
Frontend (React)                    Backend (Rust)
┌──────────────┐                   ┌──────────────────┐
│  VncPane     │◄─── vnc-frame ───│  VncConnection    │
│  (canvas)    │◄── vnc-disconn ──│  Manager          │
│              │◄── vnc-clipboard─│                    │
│              ├── vnc_connect ──►│  vnc.rs            │
│              ├── vnc_disconnect►│  (vnc-rs crate)    │
│              ├── vnc_mouse ────►│                    │
│              ├── vnc_key ──────►│                    │
│              ├── vnc_clipboard─►│                    │
└──────────────┘                   └──────────────────┘
```

Rust backend manages the VNC session lifecycle, polls for frame updates, and emits Tauri events. Frontend renders frames on `<canvas>` and forwards input via Tauri commands. Identical pattern to RDP.

## Backend: `src-tauri/src/vnc.rs`

### Constants

| Name | Value | Notes |
|------|-------|-------|
| `MAX_VNC_CONNECTIONS` | `8` | Same as RDP |
| `DEFAULT_WIDTH` | `1920` | Initial framebuffer width |
| `DEFAULT_HEIGHT` | `1080` | Initial framebuffer height |
| `VNC_FRAME_EVENT` | `"vnc-frame"` | Dirty rect pixel data |
| `VNC_DISCONNECTED_EVENT` | `"vnc-disconnected"` | Session ended |
| `VNC_CLIPBOARD_EVENT` | `"vnc-clipboard"` | Server clipboard text |

### Types

**Input events (from frontend):**
- `VncMouseEvent` — `{ x: u16, y: u16, button: Option<u8>, pressed: bool, scroll_delta: Option<i16> }` (same shape as `RdpMouseEvent`)
- `VncKeyEvent` — `{ keysym: u32, pressed: bool }` (X11 keysym, not scancode — VNC uses keysyms per RFB spec)

**Output events (to frontend):**
- `VncFramePayload` — `{ connection_id, full_width, full_height, x, y, width, height, data }` (base64-encoded RGBA, same shape as `RdpFramePayload`)
- `VncDisconnectedPayload` — `{ connection_id, reason }`
- `VncClipboardPayload` — `{ connection_id, text }`

**Internal:**
- `VncInput` enum — `Mouse(VncMouseEvent) | Key(VncKeyEvent) | Clipboard(String) | Disconnect`
- `VncSession` struct — `{ task: JoinHandle<()>, input_tx: mpsc::Sender<VncInput> }`

**Error type:**
- `VncError` — thiserror enum: `Io(String)`, `Protocol(String)`, `Auth(String)`, `InvalidHost(String)`, `TooManyConnections`, `NotFound(String)`, `SessionClosed`
- Implements `serde::Serialize` as string (same as `RdpError`)

### `VncConnectionManager`

Same pattern as `RdpConnectionManager`:
- `sessions: Arc<Mutex<HashMap<String, VncSession>>>`
- `new()` — constructor
- `connect(app, connection_id, host, port, password, username?)` — validates host, checks connection limit, spawns session task with oneshot go-ahead signal
- `send_mouse(connection_id, event)` — clone tx, send via mpsc
- `send_key(connection_id, event)` — clone tx, send via mpsc
- `send_clipboard(connection_id, text)` — clone tx, send via mpsc
- `disconnect(connection_id)` — remove from map, send Disconnect, abort task

### Session task (`run_session`)

1. **TCP connect** — `TcpStream::connect` with 15-second timeout
2. **VNC handshake** — use `vnc-rs` `VncConnector`:
   - Set credentials (password, optional username)
   - Add encodings: Tight, Zrle, Raw
   - Set pixel format to RGBA
   - Build and connect
3. **Framebuffer init** — allocate `Vec<u8>` of size `width * height * 4`
4. **Event loop** — `tokio::select!`:
   - `vnc.poll_event()` branch:
     - `RawImage` — update framebuffer, emit `vnc-frame` (tiled at 256x256, base64-encoded)
     - `Text` — emit `vnc-clipboard`
     - `Bell` — ignore for now
     - `SetCursor` — ignore for now
   - `input_rx.recv()` branch:
     - `Mouse` — translate button state to VNC button mask, send via vnc-rs
     - `Key` — forward X11 keysym to vnc-rs
     - `Clipboard` — send clipboard text via vnc-rs
     - `Disconnect` / `None` — break loop
5. **Cleanup** — emit `vnc-disconnected`, remove from session map, zeroize password

### Security

- Host validation via `session::is_valid_host()` before connect
- Password zeroized after building VNC credentials
- Connection limit enforced (MAX_VNC_CONNECTIONS = 8)

## Backend: `src-tauri/src/commands.rs`

### New error variant

Add `AppError::Vnc(String)`.

### New commands

| Command | Signature | Returns |
|---------|-----------|---------|
| `vnc_connect` | `(app, vnc_manager, session_id, password?, username?)` | `VncConnectResult { connection_id }` |
| `vnc_disconnect` | `(vnc_manager, connection_id)` | `()` |
| `vnc_mouse_event` | `(vnc_manager, connection_id, event: VncMouseEvent)` | `()` |
| `vnc_key_event` | `(vnc_manager, connection_id, event: VncKeyEvent)` | `()` |
| `vnc_send_clipboard` | `(vnc_manager, connection_id, text: String)` | `()` |

The `vnc_connect` command follows the same pattern as `rdp_connect`: look up session by ID, validate protocol is VNC, extract password, generate connection UUID, call manager.

## Backend: `src-tauri/src/session.rs`

Add `Vnc` variant to `Protocol` enum. No other changes needed — existing validation, `skip_serializing` on password, and host validation all apply.

## Backend: `src-tauri/src/lib.rs`

- Add `pub mod vnc;`
- Register `VncConnectionManager::new()` with `.manage()`
- Add all 5 VNC commands to `invoke_handler`

## Backend: `src-tauri/Cargo.toml`

Add dependency: `vnc-rs` (latest version).

## Frontend: `src/types.ts`

- `Protocol` type: `"ssh" | "rdp" | "vnc"`
- Add `VncFramePayload` interface (same shape as `RdpFramePayload`)
- Add `VncDisconnectedPayload` interface (same shape as `RdpDisconnectedPayload`)
- Add `VncClipboardPayload` interface: `{ connection_id: string, text: string }`
- Add `emptyVncDraft()` function — defaults: protocol `"vnc"`, port `5900`, username `""`, auth_method `"password"`

## Frontend: `src/api.ts`

New functions (1:1 with Rust commands):
- `vncConnect(sessionId, password?, username?)` → `{ connection_id: string }`
- `vncDisconnect(connectionId)` → `void`
- `vncMouseEvent(connectionId, x, y, button, pressed, scrollDelta)` → `void`
- `vncKeyEvent(connectionId, keysym, pressed)` → `void`
- `vncSendClipboard(connectionId, text)` → `void`

## Frontend: `src/components/VncPane.tsx`

Modeled directly on `RdpPane.tsx`:
- Canvas-based rendering
- Listens to `vnc-frame` events, calls `blitFrame` (base64 → ImageData → putImageData)
- Listens to `vnc-disconnected` events, shows error overlay with Reconnect/Close
- Listens to `vnc-clipboard` events, copies received text to local clipboard
- Mouse event handlers: same coordinate mapping as RDP (canvas scale)
- Keyboard event handlers: same scancode map as RDP
- Error overlay with Reconnect/Close buttons (same as RDP)

## Frontend: `src/components/SshSessionForm.tsx`

- Protocol dropdown: add `<option value="vnc">VNC</option>`
- VNC-specific fields block (similar to RDP):
  - Password field (always shown)
  - Username field (optional, for servers supporting username+password)
- `handleProtocolChange`: handle `"vnc"` → call `emptyVncDraft()`
- Validation: VNC requires no username (password-only is valid)

## Frontend: `src/App.tsx`

- Import `vncConnect`, `vncDisconnect` from api
- Import `VncPane` component
- Connection handler: add VNC branch (mirrors RDP flow)
  - Look up session, call `vncConnect`, create Connection entry
  - Password prompt if needed (same pattern as RDP)
- Tab content routing: render `<VncPane>` for VNC connections
- Disconnect handler: call `vncDisconnect` for VNC connections

## Files Changed Summary

| File | Change |
|------|--------|
| `src-tauri/Cargo.toml` | Add `vnc-rs` dependency |
| `src-tauri/src/vnc.rs` | **New file** — VncConnectionManager, run_session, types |
| `src-tauri/src/session.rs` | Add `Protocol::Vnc` variant |
| `src-tauri/src/commands.rs` | Add VNC commands + AppError::Vnc |
| `src-tauri/src/lib.rs` | Register VNC module, manager, commands |
| `src/types.ts` | Add VNC to Protocol, VNC payload types, emptyVncDraft |
| `src/api.ts` | Add VNC API wrappers |
| `src/components/VncPane.tsx` | **New file** — canvas-based VNC viewer |
| `src/components/SshSessionForm.tsx` | Add VNC protocol option + fields |
| `src/App.tsx` | VNC connection flow + tab routing |
