# RxTerm — Agent Learning Document

This document serves as a living reference for AI agents contributing to the RxTerm project. It captures key architectural decisions, project conventions, feature requirements, and context that an agent needs to work effectively on this codebase.

---

## Project Overview

**RxTerm** is a lightweight, offline-capable terminal and remote session management application targeting **Windows**. It is licensed under **GPLv3**.

The core mission is: *minimal setup, maximum capability* — give users a single tool to manage SSH, VNC, and RDP sessions with split-screen layouts, file transfers, tunneling, and server monitoring, all without requiring heavyweight dependencies or an internet connection.

---

## Key Requirements & Constraints

| # | Requirement | Notes |
|---|-------------|-------|
| 1 | **Windows-native** | Must run on Windows 10+. Avoid Unix-only dependencies. |
| 2 | **Split-screen sessions** | Users can tile multiple terminal/session panes in a single window. |
| 3 | **SSH, VNC, RDP protocols** | All three must be supported as first-class session types. |
| 4 | **Tunneling & port forwarding** | SSH local/remote/dynamic forwarding. Config-driven setup. |
| 5 | **Minimal setup** | No complex installers. Ideally a single executable or small portable package. |
| 6 | **Session save/export** | Persist session configs (host, port, protocol, credentials) to disk. Support import/export (e.g., JSON or encrypted file). |
| 7 | **SSH key generation & deployment** | Generate RSA/Ed25519 key pairs. Send public key to remote host via SSH. |
| 8 | **File transfers: SFTP, SCP, FTP** | Integrated file transfer within session context. |
| 9 | **tmux detection** | On SSH connect, check if tmux is available on the remote host. List and attach to existing tmux sessions. |
| 10 | **Server resource monitoring** | Display remote host CPU and memory usage in a dashboard or status bar. |
| 11 | **Lightweight** | Keep binary size and runtime memory low. Avoid bundling unnecessary runtimes. |
| 12 | **Offline-capable** | All features must work without an internet connection (except connecting to remote hosts, obviously). No telemetry or cloud dependency. |

---

## Architecture Guidance

### Technology Choices (TBD)
Technology stack has not been finalized yet. When choosing technologies, prioritize:
- Native Windows support without heavy runtimes (e.g., prefer compiled languages or lightweight runtimes).
- Libraries that support SSH, VNC, and RDP without pulling in large dependency trees.
- A UI framework that supports flexible pane/split layouts natively.
- Portable deployment — single binary or small folder with no installer required.

### Data & Configuration
- Session information should be stored in a human-readable format (JSON preferred).
- Support an optional encryption layer for stored credentials.
- Config files should reside alongside the executable or in a well-known user directory (e.g., `%APPDATA%\RxTerm\`).
- Export format should be self-contained and importable on another machine.

### Networking
- SSH connections should support password auth, key-based auth, and agent forwarding.
- Tunneling config should allow local, remote, and dynamic port forwarding.
- VNC and RDP should use native protocol clients or lightweight embedded implementations.
- File transfer (SFTP/SCP/FTP) should reuse the existing SSH connection when possible.

### tmux Integration
- On SSH session open, run a detection command (e.g., `which tmux` or `tmux ls`) to determine tmux availability.
- If tmux sessions exist, offer the user a list to attach to.
- Optionally allow creating new tmux sessions from the UI.

### Server Monitoring
- Gather CPU and memory stats via lightweight commands over SSH (e.g., parsing `/proc/stat`, `/proc/meminfo`, or `top -bn1`).
- Display in a non-intrusive status bar or small overlay panel.
- Polling interval should be configurable and default to something reasonable (e.g., 5 seconds).

---

## Project Structure

> The project is in early development. The structure below will evolve.

```
RxTerm/
├── LICENSE              # GPLv3
├── README.md            # Project overview and feature list
├── AGENT_LEARNING.md    # This file — agent context and conventions
└── src/                 # Source code (TBD)
```

---

## Conventions

- **Commit messages**: Use conventional commits (`feat:`, `fix:`, `docs:`, `refactor:`, etc.).
- **Branching**: Feature branches off `main`. Name them `feature/<short-description>`.
- **Documentation**: Keep README.md updated with any new features. Update this file when architectural decisions are made.
- **No telemetry**: Do not add any analytics, crash reporting, or network calls that are not explicitly user-initiated.

---

## Open Questions

- [ ] Which language/framework to use? (Candidates: Rust + egui, C# + WPF, C++ + Qt, Go + Fyne, etc.)
- [ ] How to embed VNC/RDP viewers — native libraries vs. launching external clients?
- [ ] Credential storage strategy — Windows Credential Manager, encrypted JSON, or both?
- [ ] Should the terminal emulator be custom-built or leverage an existing library (e.g., xterm.js via WebView, ConPTY)?
- [ ] Plugin/extension system — is it in scope?

---

## Change Log

| Date | Change |
|------|--------|
| 2026-03-16 | Initial project scaffolding. README and Agent Learning document created. |
