# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

RxTerm is a lightweight desktop terminal and remote session management application targeting Windows. It supports SSH, VNC, and RDP connections from a single interface with session saving, split-screen layouts, and offline operation.

**Tech stack:** Tauri 2.0 (Rust backend) + React + TypeScript (frontend), licensed GPL-3.0.

## Build & Development Commands

```bash
# Install frontend dependencies
npm ci

# Start dev (Vite hot-reload + Tauri window)
npx tauri dev        # runs Vite on http://127.0.0.1:25326, then launches Tauri

# Build frontend only (type-check + bundle)
npm run build        # tsc && vite build → dist/

# Build release bundle (MSI + EXE on Windows)
npx tauri build

# Rust-only checks (from repo root)
cd src-tauri && cargo check            # type-check Rust
cd src-tauri && cargo test             # run Rust unit tests
cd src-tauri && cargo clippy           # lint (no config file — uses defaults)

# Windows release script
.\build-release.ps1 -Version 0.5.1
```

The Vite dev server is hardcoded to `127.0.0.1:25326` (IPv4) to avoid Windows IPv6 bind issues. Tauri expects this exact port via `tauri.conf.json`.

There is no frontend test runner, no ESLint/Prettier config, and no Rust `rustfmt.toml`. The project relies on `tsc --strict` for frontend checks and `cargo clippy` for backend linting.

## Architecture

### IPC Pattern

Every backend operation is a `#[tauri::command]` in `commands.rs`, invoked from the frontend via `@tauri-apps/api/core`'s `invoke()`. The `api.ts` file provides typed wrappers — each function maps 1:1 to a Rust command.

When adding a new command:
1. Add the `#[tauri::command]` function in `commands.rs`
2. Register it in `lib.rs` → `invoke_handler` macro
3. Add a typed wrapper in `src/api.ts`
4. If the command needs IPC permissions, update `src-tauri/capabilities/default.json`

### Connection Managers

Each protocol has a dedicated manager registered as Tauri managed state (`tauri::manage()`):
- **`SshConnectionManager`** (`ssh.rs`) — russh sessions, PTY channels, and reader tasks
- **`VncConnectionManager`** (`vnc.rs`) — localhost WebSocket-to-TCP proxy per connection; frontend noVNC connects to the proxy
- **`RdpConnectionManager`** (`rdp.rs`) — IronRDP sessions, emits `rdp-frame` events with RGBA pixel data

All managers use `Arc<Mutex<HashMap<String, ...>>>` for thread-safe connection tracking keyed by UUID connection IDs.

### Event System (Backend → Frontend)

Tauri events for real-time data:
- `ssh-output-{connection_id}` / `ssh-closed-{connection_id}` — terminal data and disconnection
- `rdp-frame` / `rdp-disconnected` — RDP frame updates (dirty rectangles) and session end

### Frontend State

`App.tsx` is the single root component managing all state: session list, active connections, sidebar view, host key prompts, and password prompts. No external state management library. Types in `types.ts` mirror Rust data models in `session.rs`.

### Session Storage

Sessions persist as JSON in `%APPDATA%/RxTerm/sessions.json` (via `dirs::data_dir()`). A global `tokio::sync::Mutex` (`SESSIONS_FILE_LOCK` in `commands.rs`) serializes all read-modify-write operations to prevent data loss from concurrent IPC calls.

Known SSH host keys go in `%APPDATA%/RxTerm/known_hosts` using the format `[host]:port algorithm base64-key`.

## Coding Conventions

### Rust
- Async/await with Tokio; all commands are async
- `thiserror` for error types; `AppError` serializes as a string for Tauri IPC
- No `unwrap()` in production paths — use `?` with proper error context
- No `tokio::spawn` without proper error handling and task lifecycle management

### TypeScript
- Strict mode with `noUnusedLocals` and `noUnusedParameters` (enforced by `tsconfig.json`)
- Functional components only, no `any` types
- Types in `types.ts` must stay in sync with Rust `session.rs` data model

### General
- When adding Tauri commands, always implement both the Rust `#[tauri::command]` and the TypeScript `invoke()` wrapper together
- If a solution requires a new dependency, explain why it was chosen over alternatives
- Mark incomplete code with `// TODO:` and a description — no silent stubs
- One feature per change — do not add unrequested features or refactors

## Version Syncing

The version string lives in three files that must stay in sync:
- `package.json` → `version`
- `src-tauri/tauri.conf.json` → `version`
- `src-tauri/Cargo.toml` → `version`

The CI release workflow and `build-release.ps1` handle this automatically from a git tag. For local dev, keep them consistent manually.

## CI/CD

The release workflow (`.github/workflows/release.yml`) triggers on `v*` tags and builds for Windows, macOS (universal binary), and Linux using `tauri-apps/tauri-action`. It also produces a Windows portable zip and SHA-256 checksums. Code signing is scaffolded but currently disabled.

## Things to Avoid

- Do NOT suggest Electron — Tauri is the chosen framework
- Do NOT bypass SSH host key verification
- Treat remote VNC/RDP endpoints as untrusted
- IPC between frontend and Rust backend must validate all inputs
