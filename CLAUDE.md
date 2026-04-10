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

### Robustness & Error Handling

| ID | Severity | Location | Description |
|----|----------|----------|-------------|
| ROB-1 | High | `rdp.rs:555-573` | **RDP frame panic on untrusted input.** `extract_rect_rgba` indexes into `image.data()` without bounds checking. A malicious or buggy RDP server can crash the app with an index-out-of-bounds panic. Must validate `left + width <= image.width()` and `top + height <= image.height()`. |
| ROB-2 | Medium | `commands.rs`, `known_hosts.rs` | **Blocking I/O on Tokio runtime.** Synchronous `std::fs` operations (`read_to_string`, `write`, `create_dir_all`) called from async functions block the executor thread. Should use `tokio::fs` or `spawn_blocking`. |
| ROB-4 | Medium | `vnc.rs:167`, `rdp.rs:335` | **No TCP connect timeout.** `TcpStream::connect()` for VNC and RDP has no timeout. A firewalled port hangs for the OS TCP timeout (60-120 seconds) with no feedback to the user. |
| ROB-5 | Medium | `ssh.rs` | **Passphrase-protected SSH keys unsupported.** `load_secret_key(key_path, None)` always passes `None` for the passphrase. Keys with passphrases fail with a confusing "Failed to load key" error instead of prompting. |
| ROB-6 | Medium | `commands.rs` | **Fragile HOST_KEY_UNKNOWN error protocol.** The `HostKeyUnknown` variant serializes key info as a JSON string embedded in the error message. The frontend parses it by string prefix matching. A proper typed IPC response would be more reliable. |
| ROB-8 | Low | `lib.rs:48` | **Uninformative panic message.** `.expect("error while running tauri application")` doesn't include the actual error. |
| ROB-9 | Low | `vnc.rs:77-83` | **No failure event to frontend.** If the VNC proxy fails (timeout, server unreachable), the error is logged and the entry is auto-removed but no event is emitted to the frontend. The user sees no feedback. |

### Frontend State & React Patterns

| ID | Severity | Location | Description |
|----|----------|----------|-------------|
| FE-1 | Medium | `App.tsx` | **Nested state setter calls.** `setActiveConnectionId` is called inside the `setConnections` updater function. Calling one state setter inside another's updater is not a documented React pattern and may break in future React versions. Should use `useReducer` or compute both values together. |
| FE-2 | Medium | `App.tsx` | **Ref in className never triggers re-render.** `isResizing.current` is used in a className expression, but ref mutations don't cause re-renders. The `resizing` CSS class (for cursor styling during drag) is never reactively applied. |
| FE-3 | Medium | `App.tsx` | **`handleDisconnect` re-creates on every connection change.** Memoized with `[connections]` dependency, so its identity changes on every connection update, causing all tab components to re-render. Using a ref or `useReducer` would avoid this. |
| FE-5 | Medium | `SshSessionForm.tsx` | **Form state ignores prop updates.** `useState` initializer runs only on first render. If `initial` prop changes without unmounting, stale data is shown. Currently masked by the navigation flow but is a latent bug. |
| FE-6 | Medium | `SessionList.tsx` | **No duplicate connection guard.** Double-clicking Connect or rapid clicks can initiate multiple parallel connections to the same host. `handleConnect` doesn't check if a connection to that session already exists. |

### Performance

| ID | Severity | Location | Description |
|----|----------|----------|-------------|
| PERF-1 | Medium | `RdpPane.tsx` | **Inefficient base64 frame decoding.** `atob()` + character-by-character loop to build `Uint8Array`. For a full 1920x1080 RGBA frame (~8MB), this is a significant CPU bottleneck. |
| PERF-2 | Medium | `rdp.rs` | **Large frame payloads over Tauri events.** Each RDP graphics update is base64-encoded and sent as a Tauri event. Full-screen updates can be ~5.5MB base64, causing memory pressure and IPC overhead. |

### Accessibility & UX

| ID | Severity | Location | Description |
|----|----------|----------|-------------|
| UX-1 | Medium | `SshSessionForm.tsx` | **Duplicate HTML `id` attributes.** `id="password"` and `id="username"` are reused across SSH, VNC, and RDP input groups. `<label htmlFor>` associations may break in assistive technology during protocol switches. |
| UX-2 | Low | `TerminalTabs.tsx` | **Tabs not keyboard-accessible.** Tab elements are `<div>` with `onClick` but no `role`, `tabIndex`, or keyboard handlers. Cannot be reached or activated via keyboard. |
| UX-3 | Low | `HostKeyDialog.tsx` | **Dialog does not trap focus.** No `role="dialog"`, `aria-modal`, or focus trapping. Keyboard users can Tab past the dialog. Escape key does not dismiss it. |
| UX-4 | Low | `SessionList.tsx`, `App.tsx` | **No delete confirmation.** Session deletion is immediate on single click with no confirmation dialog. Prone to accidental data loss. |

## Things to Avoid

- Do NOT suggest Electron — Tauri is the chosen framework
- Do NOT use `tokio::spawn` without proper error handling and task lifecycle management
- Do NOT generate placeholder/stub code without marking it `// TODO:` with a description
- Do NOT bypass SSH host key verification
- Do NOT add unrequested features or refactors — one feature per change
