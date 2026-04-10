# CLAUDE.md

## Project Overview

RxTerm is a lightweight desktop terminal and remote session management application targeting Windows. It supports SSH, VNC, and RDP connections from a single interface with session saving, split-screen layouts, and offline operation.

**Tech stack:** Tauri 2.0 (Rust backend) + React + TypeScript (frontend)

**License:** GPL-3.0

## Repository Structure

```
RxTerm/
├── src/                          # Frontend (React + TypeScript)
│   ├── main.tsx                  # React entry point
│   ├── App.tsx                   # Root component — navigation, state, IPC coordination
│   ├── api.ts                    # Thin wrappers around Tauri invoke() calls
│   ├── types.ts                  # Shared TS types mirroring Rust data model
│   ├── styles.css                # All application styles (single CSS file)
│   ├── novnc.d.ts                # Type declarations for noVNC
│   └── components/
│       ├── SessionList.tsx       # Sidebar session list
│       ├── SshSessionForm.tsx    # New/edit session form
│       ├── TerminalPane.tsx      # xterm.js SSH terminal
│       ├── TerminalTabs.tsx      # Connection tab bar
│       ├── VncPane.tsx           # noVNC viewer pane
│       ├── RdpPane.tsx           # IronRDP canvas pane
│       └── HostKeyDialog.tsx     # SSH host key verification dialog
├── src-tauri/                    # Backend (Rust / Tauri)
│   ├── Cargo.toml                # Rust dependencies
│   ├── tauri.conf.json           # Tauri app config (window, plugins, updater)
│   ├── build.rs                  # Tauri build script
│   ├── capabilities/default.json # Tauri IPC permissions
│   └── src/
│       ├── main.rs               # Binary entry point (calls lib::run)
│       ├── lib.rs                # Tauri app builder — registers all IPC commands
│       ├── commands.rs           # All #[tauri::command] handlers + AppError type
│       ├── session.rs            # SshSession, Protocol, AuthMethod data models
│       ├── ssh.rs                # SshConnectionManager — russh client lifecycle
│       ├── vnc.rs                # VncConnectionManager — WebSocket-to-TCP proxy
│       ├── rdp.rs                # RdpConnectionManager — IronRDP session handling
│       └── known_hosts.rs        # SSH known_hosts file management
├── index.html                    # Vite HTML entry
├── package.json                  # Node dependencies and scripts
├── tsconfig.json                 # TypeScript config (strict mode)
├── vite.config.ts                # Vite config (port 25326, React plugin)
├── build-release.ps1             # Windows release build script (PowerShell)
└── .github/
    ├── copilot-instructions.md   # AI coding assistant guidelines
    └── workflows/release.yml     # CI release workflow (Windows, macOS, Linux)
```

## Build & Development

### Prerequisites

- Node.js 20+
- Rust stable toolchain
- On Windows: WebView2 runtime (bundled with Windows 10+)
- On Linux: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`

### Commands

```bash
# Install frontend dependencies
npm ci

# Start dev server (Vite + Tauri hot-reload)
npm run dev          # starts Vite on http://127.0.0.1:25326
npx tauri dev        # starts full Tauri dev environment

# Build frontend only
npm run build        # tsc && vite build → dist/

# Build release bundle (MSI + EXE on Windows)
npx tauri build

# PowerShell release script (Windows)
.\build-release.ps1 -Version 0.2.0
```

### Vite Dev Server

The dev server is hardcoded to `127.0.0.1:25326` (IPv4 only) to avoid Windows IPv6 bind issues. Tauri expects this exact port via `tauri.conf.json`.

## Architecture

### IPC Pattern

Every backend operation is a `#[tauri::command]` in `commands.rs`, invoked from the frontend via `@tauri-apps/api/core`'s `invoke()`. The `api.ts` file provides typed wrappers — each function maps 1:1 to a Rust command.

When adding a new command:
1. Add the `#[tauri::command]` function in `commands.rs`
2. Register it in `lib.rs` → `invoke_handler` macro
3. Add a typed wrapper in `src/api.ts`

### Connection Managers

Each protocol has a dedicated manager registered as Tauri managed state:
- `SshConnectionManager` — manages `russh` sessions, PTY channels, and reader tasks
- `VncConnectionManager` — runs a localhost WebSocket-to-TCP proxy per connection; frontend noVNC connects to the proxy
- `RdpConnectionManager` — manages IronRDP sessions, emits `rdp-frame` events with RGBA pixel data

### Event System

Backend-to-frontend communication uses Tauri events:
- `ssh-output-{connection_id}` — SSH terminal data
- `ssh-closed-{connection_id}` — SSH disconnection
- `rdp-frame` — RDP frame update (dirty rectangles)
- `rdp-disconnected` — RDP session ended

### Session Storage

Sessions are stored as JSON in `%APPDATA%/RxTerm/sessions.json` (via `dirs::data_dir()`). Known SSH host keys go in `%APPDATA%/RxTerm/known_hosts` using the format `[host]:port algorithm base64-key`.

### Frontend State

`App.tsx` is the single root component managing all application state: session list, active connections, sidebar view, host key prompts, and password prompts. There is no external state management library.

## Coding Conventions

### Rust (Backend)

- Use `async/await` with Tokio; all commands are async
- Use `thiserror` for error types; errors implement `serde::Serialize` as strings for Tauri IPC
- No `unwrap()` in production paths — use `?` with proper error context
- All public APIs have `///` doc comments
- Connection managers use `Arc<Mutex<HashMap<String, ...>>>` for thread-safe state
- Naming: `snake_case` for functions/variables, `PascalCase` for types

### TypeScript (Frontend)

- Strict mode enabled (`noUnusedLocals`, `noUnusedParameters`)
- Functional components only — no class components
- No `any` types
- Types mirror Rust data model in `types.ts`
- JSDoc comments on public functions
- Naming: `camelCase` for functions/variables, `PascalCase` for types/components

### General

- When generating Tauri commands, always implement both the Rust `#[tauri::command]` and the corresponding TypeScript `invoke()` wrapper together
- No secrets or credentials hardcoded
- SSH known_hosts verification must not be bypassed silently
- All user-supplied paths must be sanitized before filesystem or shell use
- IPC between frontend and Rust backend must validate all inputs
- Treat remote VNC/RDP endpoints as untrusted

## Key Dependencies

### Rust
- `tauri` 2.x — desktop app framework
- `tokio` — async runtime (full features)
- `russh` / `russh-keys` 0.46 — SSH client
- `ironrdp-*` — RDP client (connector, session, graphics, input, TLS)
- `tokio-tungstenite` — WebSocket for VNC proxy
- `serde` / `serde_json` — serialization
- `thiserror` — error derive macros
- `dirs` — platform data directory paths
- `uuid` — connection IDs

### Frontend
- `react` / `react-dom` 19.x
- `@xterm/xterm` — terminal emulator
- `@novnc/novnc` — VNC viewer
- `@tauri-apps/api` — Tauri IPC
- `@tauri-apps/plugin-updater` — auto-update support
- `vite` 8.x + `@vitejs/plugin-react`

## CI/CD

The release workflow (`.github/workflows/release.yml`) triggers on version tags (`v*`) and builds for Windows, macOS (universal binary), and Linux. It uses `tauri-apps/tauri-action` and generates SHA-256 checksums. Code signing is scaffolded but currently disabled.

## Known Issues

### Security

| ID | Severity | Location | Description |
|----|----------|----------|-------------|
| SEC-1 | High | `session.rs:54`, `commands.rs` | **Plaintext password storage.** The `password` field in `SshSession` is serialized directly to `sessions.json` as plaintext. Any process with read access to `%APPDATA%/RxTerm/` can harvest all stored passwords. Should migrate to a secure credential store (e.g. system keyring or Tauri secure store). |
| SEC-2 | Medium | `known_hosts.rs:75-100` | **Known-hosts injection.** `accept()` writes `host`, `algorithm`, and `key_data` to the file without sanitizing newlines or whitespace. A crafted `algorithm` value like `"ssh-rsa\n[evil.host]:22 ssh-ed25519 AAAA..."` can inject a trusted entry for an arbitrary host. |
| SEC-3 | Medium | `commands.rs:208-245` | **Host key TOCTOU.** When a host key is unknown, a second TCP connection with an all-accepting handler captures the key. An attacker performing MITM could present different keys on each connection, causing the user to approve the wrong key. Key info should be captured from the first attempt. |
| SEC-4 | Medium | `commands.rs:158-205` | **No SSH host validation.** VNC and RDP both validate hostnames before connecting; SSH does not, creating an inconsistency in input sanitization at the protocol boundary. |
| SEC-5 | Medium | `App.tsx:201,278-279` | **Passwords in React state.** VNC passwords and SSH host-key-prompt passwords persist in the React component tree for the connection's lifetime and are visible via React DevTools. |
| SEC-6 | Medium | `rdp.rs`, `ssh.rs` | **Passwords not zeroed in memory.** `password.to_string()` creates heap-allocated Strings that are freed but never zeroed. Credentials persist in process memory until the allocator reuses the page. Should use `secrecy::Secret<String>` or explicit zeroing. |
| SEC-7 | Medium | `session.rs:35-65` | **No validation on deserialized sessions.** `SshSession` fields (`id`, `host`, `port`, `private_key_path`) are deserialized from user JSON with no validation. Empty IDs, path traversal in `private_key_path`, or port 0 are all accepted silently. |

### Resource Leaks & Concurrency

| ID | Severity | Location | Description |
|----|----------|----------|-------------|
| RES-1 | High | `ssh.rs:204-260` | **SSH connection HashMap leak.** When `channel_reader_task` exits (EOF, close, connection lost), the `SshConnection` entry stays in the HashMap permanently. Unlike VNC and RDP which auto-cleanup, SSH has none. Dead entries accumulate holding russh `Handle` and TCP resources. |
| RES-2 | High | `TerminalPane.tsx:64-98` | **SSH event listener leak.** `setupListeners()` is async but called without `await`. If the component unmounts before `listen()` Promises resolve, cleanup runs against undefined unlisten functions, leaving listeners that fire indefinitely against a disposed xterm instance. |
| RES-3 | High | `RdpPane.tsx:63-80` | **RDP event listener leak.** Same pattern as TerminalPane. Frame listeners leak on fast unmount, continuously decoding base64 and painting to a non-existent canvas. |
| RES-4 | High | `commands.rs:63-133` | **Session file race condition.** `save_session`, `delete_session`, and `import_sessions` all do read-modify-write on `sessions.json` with no file lock or mutex. Concurrent calls can silently lose updates. |
| RES-5 | Medium | `ssh.rs:157-169` | **SSH mutex held across async I/O.** `send()` holds the entire connections HashMap lock during `write_all` + `flush`. If one connection's TCP buffer is full, all SSH operations (including to other connections) block. RDP correctly uses an mpsc pattern instead. |
| RES-6 | Medium | `rdp.rs:177-213` | **RDP placeholder task race.** Between inserting a placeholder `RdpSession` and replacing it with the real task handle, `disconnect()` could remove the placeholder, leaving the real spawned task running as a zombie with no way to stop it. |
| RES-7 | Low | `known_hosts.rs:104-114` | **Known-hosts file race condition.** Same read-modify-write pattern as sessions. Two concurrent `accept()` calls could lose one entry. Low likelihood since host key acceptance is user-initiated. |

### Robustness & Error Handling

| ID | Severity | Location | Description |
|----|----------|----------|-------------|
| ROB-1 | High | `rdp.rs:555-573` | **RDP frame panic on untrusted input.** `extract_rect_rgba` indexes into `image.data()` without bounds checking. A malicious or buggy RDP server can crash the app with an index-out-of-bounds panic. Must validate `left + width <= image.width()` and `top + height <= image.height()`. |
| ROB-2 | Medium | `commands.rs:68,89,101,109,131`, `known_hosts.rs:105,113` | **Blocking I/O on Tokio runtime.** Synchronous `std::fs` operations (`read_to_string`, `write`, `create_dir_all`) called from async functions block the executor thread. Should use `tokio::fs` or `spawn_blocking`. |
| ROB-3 | Medium | `commands.rs:46` | **Silent data directory fallback.** If `dirs::data_dir()` returns `None`, sessions are written to the current working directory (`.`) with no user indication. Should return an error. |
| ROB-4 | Medium | `vnc.rs:167`, `rdp.rs:335` | **No TCP connect timeout.** `TcpStream::connect()` for VNC and RDP has no timeout. A firewalled port hangs for the OS TCP timeout (60-120 seconds) with no feedback to the user. |
| ROB-5 | Medium | `ssh.rs:87` | **Passphrase-protected SSH keys unsupported.** `load_secret_key(key_path, None)` always passes `None` for the passphrase. Keys with passphrases fail with a confusing "Failed to load key" error instead of prompting. |
| ROB-6 | Medium | `commands.rs:27` | **Fragile HOST_KEY_UNKNOWN error protocol.** The `HostKeyUnknown` variant serializes key info as a JSON string embedded in the error message. The frontend parses it by string prefix matching. A proper typed IPC response would be more reliable. |
| ROB-7 | Low | `ssh.rs:185` | **Misused error variant.** `resize()` returns `ConnectError::Auth("Channel reader task ended")` when the control channel fails. This is semantically wrong — it's not an authentication error. |
| ROB-8 | Low | `lib.rs:48` | **Uninformative panic message.** `.expect("error while running tauri application")` doesn't include the actual error. |
| ROB-9 | Low | `vnc.rs:77-83` | **No failure event to frontend.** If the VNC proxy fails (timeout, server unreachable), the error is logged and the entry is auto-removed but no event is emitted to the frontend. The user sees no feedback. |

### Frontend State & React Patterns

| ID | Severity | Location | Description |
|----|----------|----------|-------------|
| FE-1 | Medium | `App.tsx:324-331,339-345` | **Nested state setter calls.** `setActiveConnectionId` is called inside the `setConnections` updater function. Calling one state setter inside another's updater is not a documented React pattern and may break in future React versions. Should use `useReducer` or compute both values together. |
| FE-2 | Medium | `App.tsx:352` | **Ref in className never triggers re-render.** `isResizing.current` is used in a className expression, but ref mutations don't cause re-renders. The `resizing` CSS class (for cursor styling during drag) is never reactively applied. |
| FE-3 | Medium | `App.tsx:310-334` | **`handleDisconnect` re-creates on every connection change.** Memoized with `[connections]` dependency, so its identity changes on every connection update, causing all tab components to re-render. Using a ref or `useReducer` would avoid this. |
| FE-4 | Medium | `TerminalPane.tsx:76-78,100-101` | **Stale `onDisconnected` closure.** `onDisconnected` is captured in the listener closure but the `useEffect` depends only on `[connectionId]`. If the callback identity changes, the stale reference is called. A ref-based approach (like VncPane uses) would be safer. |
| FE-5 | Medium | `SshSessionForm.tsx:24-26` | **Form state ignores prop updates.** `useState` initializer runs only on first render. If `initial` prop changes without unmounting, stale data is shown. Currently masked by the navigation flow but is a latent bug. |
| FE-6 | Medium | `SessionList.tsx:57,68` | **No duplicate connection guard.** Double-clicking Connect or rapid clicks can initiate multiple parallel connections to the same host. `handleConnect` doesn't check if a connection to that session already exists. |

### Performance

| ID | Severity | Location | Description |
|----|----------|----------|-------------|
| PERF-1 | Medium | `RdpPane.tsx:47-51` | **Inefficient base64 frame decoding.** `atob()` + character-by-character loop to build `Uint8Array`. For a full 1920x1080 RGBA frame (~8MB), this is a significant CPU bottleneck. |
| PERF-2 | Medium | `rdp.rs:474` | **Large frame payloads over Tauri events.** Each RDP graphics update is base64-encoded and sent as a Tauri event. Full-screen updates can be ~5.5MB base64, causing memory pressure and IPC overhead. |
| PERF-3 | Medium | `TerminalPane.tsx:85-88` | **No resize debounce.** `ResizeObserver` fires `fitAddon.fit()` and `sshResize()` IPC call dozens of times per second during window resize. Should be throttled. |
| PERF-4 | Low | `RdpPane.tsx:182-233` | **Scancode map re-allocated on every keypress.** `browserCodeToScancode` creates a new map object on every call. Should be a module-level constant. |
| PERF-5 | Low | `RdpPane.tsx:43` | **Canvas 2D context obtained on every frame.** `canvas.getContext("2d")` called inside `blitFrame` for each incoming frame. Should be stored in a ref after first creation. |

### Accessibility & UX

| ID | Severity | Location | Description |
|----|----------|----------|-------------|
| UX-1 | Medium | `SshSessionForm.tsx:199,236,280` | **Duplicate HTML `id` attributes.** `id="password"` and `id="username"` are reused across SSH, VNC, and RDP input groups. `<label htmlFor>` associations may break in assistive technology during protocol switches. |
| UX-2 | Low | `TerminalTabs.tsx:30-31` | **Tabs not keyboard-accessible.** Tab elements are `<div>` with `onClick` but no `role`, `tabIndex`, or keyboard handlers. Cannot be reached or activated via keyboard. |
| UX-3 | Low | `HostKeyDialog.tsx:25-48` | **Dialog does not trap focus.** No `role="dialog"`, `aria-modal`, or focus trapping. Keyboard users can Tab past the dialog. Escape key does not dismiss it. |
| UX-4 | Low | `SessionList.tsx:70`, `App.tsx:123-131` | **No delete confirmation.** Session deletion is immediate on single click with no confirmation dialog. Prone to accidental data loss. |

### Dead Code

| ID | Severity | Location | Description |
|----|----------|----------|-------------|
| DC-1 | Low | `known_hosts.rs:154-158` | **Unused `_key_fingerprint` function.** Prefixed with `_` to suppress the warning but never called. Should be removed or used. |

## Things to Avoid

- Do NOT suggest Electron — Tauri is the chosen framework
- Do NOT use `tokio::spawn` without proper error handling and task lifecycle management
- Do NOT generate placeholder/stub code without marking it `// TODO:` with a description
- Do NOT bypass SSH host key verification
- Do NOT add unrequested features or refactors — one feature per change
