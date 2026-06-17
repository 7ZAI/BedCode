<div align="center">

# BedCode

**Use your phone to control Claude Code on your desktop**

[![Version](https://img.shields.io/badge/version-0.1.0-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Android-lightgrey.svg)](https://github.com/7ZAI/BedCode)

English | [简体中文](README_zh.md)

</div>

---

BedCode is a cross-platform application that lets you remotely control [Claude Code](https://claude.ai/code) from your mobile device within the same local network. The desktop app (Tauri + Vue 3) acts as the host running terminal sessions, while your phone becomes a powerful remote terminal with an optimized touch interface. While designed as a Claude Code remote control app, it also works as a general-purpose remote terminal.

Typical use cases: as the name suggests — coding from bed; handling other tasks at home while programming, such as bathroom breaks, cooking, childcare, or just before sleep.

> Currently only supports desktop and mobile on the same WiFi network.

Internet connectivity interface or NAT traversal protocol will be reserved in the future (requires a server).

## Features

### Desktop (Host)
- **Session Management** - Create, configure, and manage multiple Claude Code sessions
- **Terminal Preview** - Real-time xterm.js terminal output preview
- **Device Pairing** - QR code + 6-digit code authentication for secure device pairing
- **System Tray** - Quick actions from the system tray
- **WSL2 Support** - Run sessions inside Windows Subsystem for Linux

### Mobile (Remote)
- **Device Discovery & Pairing** - Scan QR code or enter pairing code to connect
- **Terminal Output** - Enhanced mode (parsed ANSI/Markdown) and raw mode toggle
- **Smart Input Bar** - Special keys (Tab, Ctrl+C, Esc, arrows) and input assistant
- **Quick Actions** - Customizable command shortcuts grid
- **Auto-Reconnect** - Automatic reconnection on unexpected disconnects
- **Foreground Service** - Keep connection alive in background (Android)
- **Edge-to-Edge Display** - Modern full-screen mobile experience

### Security
- JWT-based session authentication (HS256, 7-day expiry)
- QR token with one-time use and configurable TTL
- Pairing codes expire after 60 seconds
- Device fingerprint verification on connection

> **Note:** End-to-end encryption (X25519 key exchange + AES-GCM) is planned but not yet implemented. Current WebSocket communication is unencrypted (ws://). See [Roadmap](#roadmap).

## Architecture

```
┌─────────────────┐       WebSocket        ┌─────────────────┐
│   Desktop App    │◄──────────────────────►│   Mobile App     │
│  (Tauri + Vue)   │     WebSocket (WS)      │  (Tauri + Vue)   │
│                  │                        │                  │
│  ┌────────────┐  │                        │  ┌────────────┐  │
│  │ PTY Manager│  │                        │  │ WS Client  │  │
│  │ (Claude)   │  │                        │  │            │  │
│  └────────────┘  │                        │  └────────────┘  │
│  ┌────────────┐  │                        │  ┌────────────┐  │
│  │ WS Server  │  │                        │  │ UI (Touch) │  │
│  └────────────┘  │                        │  └────────────┘  │
└─────────────────┘                        └─────────────────┘
```

The project uses a **shared + platform-specific** architecture:

| Layer | Frontend (Vue 3) | Backend (Rust) |
|-------|-------------------|-----------------|
| **Shared** | Components, composables, stores, utils | Auth, DB, WebSocket, parser, models |
| **Desktop** | Session manager, terminal preview, sidebar | PTY, WS server, session management |
| **Mobile** | Terminal view, quick actions, pairing | WS client, remote connection |

## Tech Stack

| Category | Technology |
|----------|------------|
| Framework | Tauri 2.0 |
| Frontend | Vue 3 + TypeScript |
| Styling | TailwindCSS |
| State | Pinia |
| Backend | Rust (Tokio async runtime) |
| Database | SQLite (rusqlite) |
| Communication | WebSocket
| Terminal | xterm.js |
| Testing | Vitest, Playwright, Rust test |

## Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) >= 18
- [Rust](https://www.rust-lang.org/tools/install) >= 1.70
- [Tauri 2.0 CLI](https://v2.tauri.app/start/prerequisites/) dependencies for your platform
- [Claude Code CLI](https://claude.ai/code) installed and configured

### Install Dependencies

```bash
# Install frontend dependencies
npm install

# Rust dependencies are fetched automatically by Cargo
```

### Development

```bash
# Start desktop app in dev mode
npm run tauri:dev
```

### Build

```bash
# Build desktop app
npm run tauri:build

# Build Android APK
npm run tauri:android:build
```

### Testing

```bash
# Frontend unit tests
npm run test

# Frontend tests with coverage
npm run test:coverage

# E2E tests
npm run test:e2e

# Rust tests
cargo test
```

### Linting & Formatting

```bash
# Lint
npm run lint

# Format
npm run format
```

## Configuration

BedCode uses a `config.json` file (bundled as a Tauri resource) for runtime configuration:

| Category | Key | Default | Description |
|----------|-----|---------|-------------|
| Network | `network.port` | `8765` | WebSocket server port |
| Network | `network.heartbeat_interval_secs` | `30` | Heartbeat interval |
| Session | `session.default_command` | `"claude"` | Default terminal command |
| UI | `ui.theme` | `"system"` | Theme (system/light/dark) |
| Terminal | `terminal.default_cols` | `120` | Default terminal columns |
| Terminal | `terminal.flush_interval_ms` | `30` | Output flush interval |

## How It Works

1. **Start Desktop App** - Launch BedCode on your desktop, which starts the WebSocket server and mDNS discovery service
2. **Pair Your Phone** - Open BedCode on your phone, scan the QR code or enter the 6-digit pairing code
3. **Control Remotely** - Once paired, select a session and start sending commands from your phone
4. **Real-time Output** - Terminal output is streamed to your phone in real-time with ANSI rendering

## Project Structure

```
bedcode/
├── src/                          # Vue 3 frontend
│   └── modules/
│       ├── desktop/              # Desktop UI (sessions, terminal, devices)
│       ├── mobile/               # Mobile UI (terminal, pairing, quick actions)
│       └── shared/               # Shared components, stores, composables
├── src-tauri/
│   └── src/
│       ├── shared/               # Shared Rust modules (auth, db, websocket, parser)
│       ├── desktop/              # Desktop-only (PTY, WS server, session mgmt)
│       └── mobile/               # Mobile-only (WS client, remote connection)
├── docs/                         # Documentation
└── e2e/                          # E2E tests
```

See [docs/code-map.md](docs/code-map.md) for the complete module index.

## Roadmap

- [ ] End-to-end encryption (X25519 + AES-GCM)
- [ ] Mobile file browser and code viewer
- [ ] Linux desktop support
- [ ] Multi-language support (i18n)
- [ ] Internet connectivity interface
- [ ] Plugin system for custom commands
- [ ] FCM push notifications
- [ ] Virtual scrolling for terminal history

## Contributing 

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feat/my-feature`)
3. Commit your changes (`git commit -m 'feat: add my feature'`)
4. Push to the branch (`git push origin feat/my-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
