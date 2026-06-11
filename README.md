# RxTerm

A lightweight, modern terminal and remote session management program built for daily use on Windows. RxTerm provides a streamlined, minimal-setup experience for managing SSH, RDP, and VNC connections from a single tabbed interface, fully offline-capable.

Licensed under the [GNU General Public License v3.0](LICENSE).

---

## Features

### Remote Sessions
- **SSH terminal** — full xterm.js terminal over `russh`, with password and key-file authentication, host-key verification (distinct warning when a stored key changes), and PTY resize.
- **RDP remote desktop** — native IronRDP client rendering to an HTML5 canvas with keyboard and mouse input.
- **VNC remote desktop** — native Rust RFB client (`vnc-rs`) with canvas rendering, keyboard/mouse input, and bidirectional clipboard.

### Session Management
- **Save, edit, and organize** sessions (host, port, protocol — passwords are never persisted to disk).
- **Export / import** session lists as JSON for backup or sharing.
- **Tabbed layout** — run multiple sessions side by side in tabs, Windows Terminal-style.

### Design Goals
- **Windows-first** — native WebView2, no Electron, small footprint.
- **Minimal setup** — portable zip needs no installation at all.
- **Offline-capable** — full functionality without an internet connection.

---

## Installation

Download from the [latest release](https://github.com/rexc-lab/RxTerm/releases/latest):

- **Windows installer** — `RxTerm_<version>_x64_en-US.msi` or the `.exe` (NSIS) installer
- **Windows portable** — `RxTerm_<version>_windows_portable.zip`: unzip and run `rxterm.exe`, no installation required (settings live in `%APPDATA%\RxTerm\`)
- macOS (`.dmg`, universal) and Linux (`.deb` / `.rpm` / `.AppImage`) builds are published too, though Windows is the primary target

SHA-256 checksums (`checksums-sha256.txt`) accompany every release. Binaries are not yet code-signed, so Windows SmartScreen may prompt on first run.

### Building from source

```bash
npm ci
npx tauri dev     # development window with hot reload
npx tauri build   # release bundle
```

Requires Node 22+, Rust stable, and on Linux the WebKitGTK dev stack (see `.github/workflows/ci.yml`).

---

## Roadmap

Shipped:

- [x] SSH session management and connection handling
- [x] RDP and VNC session integration
- [x] Session save/load/export functionality
- [x] Tabbed session layout
- [x] Offline-capable packaging (portable zip)

Planned (not yet implemented):

- [ ] Split-screen panes
- [ ] SSH key generation and deployment
- [ ] SSH tunneling and port forwarding
- [ ] SFTP / SCP file transfer
- [ ] tmux detection and session attachment
- [ ] Server resource monitoring (CPU & memory)
- [ ] Auto-update
- [ ] Code-signed binaries

---

## Contributing

Contributions are welcome. Please open an issue to discuss proposed changes before submitting a pull request. Every PR is validated by CI (`tsc`, `cargo clippy -D warnings`, and the test suite on Windows, Linux, and macOS).

---

## License

This project is licensed under the **GNU General Public License v3.0**. See the [LICENSE](LICENSE) file for details.
