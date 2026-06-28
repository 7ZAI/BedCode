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
- **Plugin System** - Auto-configure Claude Code hooks, task status tracking, auto-approve mode
- **HTTP + WebSocket Server** - Actix Web based HTTP API + WebSocket for terminal communication
- **System Tray** - Quick actions from the system tray
- **WSL2 Support** - Run sessions inside Windows Subsystem for Linux

### Mobile (Remote)
- **Device Discovery & Pairing** - Scan QR code or enter pairing code to connect
- **Terminal Output** - Enhanced mode (parsed ANSI/Markdown) and raw mode toggle
- **Smart Input Bar** - Special keys (Tab, Ctrl+C, Esc, arrows), input assistant, and shortcut config
- **Auto-Execute Engine** - Queue and auto-execute multiple tasks with auto/manual mode toggle
- **Code Explorer** - Browse project files, view code with syntax highlighting, and diff rendering
- **Preset Tasks** - Pre-configured task cards with type badges and action menus
- **Task Notifications** - Per-session task status notifications
- **Auto-Reconnect** - Automatic reconnection on unexpected disconnects
- **Foreground Service** - Keep connection alive in background with WakeLock (Android)
- **Edge-to-Edge Display** - Modern full-screen mobile experience

### Security
- JWT-based session authentication (HS256, 7-day expiry)
- QR token with one-time use and configurable TTL
- Plugin token for Claude Code hooks authentication
- Pairing codes expire after 60 seconds
- Device fingerprint verification on connection

> **Note:** End-to-end encryption (X25519 key exchange + AES-GCM) is planned but not yet implemented. Current WebSocket communication is unencrypted (ws://). See [Roadmap](#roadmap).

### Internationalization
- Full i18n support via vue-i18n (zh-CN / en)
- Language switcher in settings with persistent preference
- Error code mapping system for localized error messages

## Architecture

```
┌─────────────────────────────────┐                ┌─────────────────────────────────┐
│         Desktop App              │                │         Mobile App               │
│        (Tauri + Vue 3)           │                │        (Tauri + Vue 3)           │
│                                  │                │                                  │
│  ┌────────────┐  ┌────────────┐ │                │  ┌────────────┐  ┌────────────┐ │
│  │ PTY Manager│  │ WS Server  │ │   WebSocket    │  │ WS Client  │  │ Auto-Exec  │ │
│  │ (Claude)   │  │ (Actix)    │◄├───────────────►├►│            │  │ Engine     │ │
│  └────────────┘  └────────────┘ │   + HTTP API   │  └────────────┘  └────────────┘ │
│  ┌────────────┐  ┌────────────┐ │                │  ┌────────────┐  ┌────────────┐ │
│  │ Plugin Mgr │  │ HTTP API   │ │                │  │ Code       │  │ Touch UI   │ │
│  │ (Hooks)    │  │ (Actix)    │ │                │  │ Explorer   │  │            │ │
│  └────────────┘  └────────────┘ │                │  └────────────┘  └────────────┘ │
└─────────────────────────────────┘                └─────────────────────────────────┘
```

The project uses a **shared + platform-specific** architecture:

| Layer | Frontend (Vue 3) | Backend (Rust) |
|-------|-------------------|-----------------|
| **Shared** | Components, composables, stores, i18n, utils | Auth, DB, WebSocket, parser, models, error handling |
| **Desktop** | Session manager, terminal preview, sidebar | PTY, Actix Web server, session management, plugin system |
| **Mobile** | Terminal view, code explorer, auto-exec, toolbox | WS client, HTTP client, remote connection, routing |

## Tech Stack

| Category | Technology |
|----------|------------|
| Framework | Tauri 2.0 |
| Frontend | Vue 3 + TypeScript |
| Styling | TailwindCSS |
| State | Pinia |
| Backend | Rust (Tokio async runtime) |
| HTTP Server | Actix Web 4 |
| Database | SQLite (rusqlite) |
| Communication | WebSocket + HTTP REST API |
| Terminal | xterm.js |
| I18n | vue-i18n@9 |
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
| UI | `ui.language` | `"zh-CN"` | Language (zh-CN/en) |
| Terminal | `terminal.default_cols` | `120` | Default terminal columns |
| Terminal | `terminal.flush_interval_ms` | `30` | Output flush interval |

## How It Works

1. **Start Desktop App** - Launch BedCode on your desktop, which starts the Actix Web server (HTTP + WebSocket) and mDNS discovery service
2. **Pair Your Phone** - Open BedCode on your phone, scan the QR code or enter the 6-digit pairing code
3. **Control Remotely** - Once paired, select a session and start sending commands from your phone
4. **Real-time Output** - Terminal output is streamed to your phone in real-time with ANSI rendering
5. **Auto-Execute Tasks** - Queue multiple tasks on your phone; the auto-execute engine sends them one by one as Claude Code becomes idle
6. **Browse Code** - Use the code explorer to browse project files and view diffs with syntax highlighting

## Plugin System

BedCode integrates with Claude Code through a hook-based plugin system:

- **Auto-Configuration** - Project-scoped Claude Code hooks are automatically configured when a session starts
- **Task Status Tracking** - Claude Code hooks push task status (idle/in_progress/asking/completed/interrupted) to the desktop app via HTTP API
- **Auto-Approve Mode** - In auto mode, Claude Code tool-use permissions are automatically approved; in manual mode, the user operates Claude Code directly
- **Session ID Binding** - PTY sessions inject `BEDCODE_SESSION_ID` environment variable to bind Claude Code sessions with BedCode sessions

```
Claude Code Hook (Python)
    ↓ HTTP POST
Rust HTTP API (plugin_controller)
    ↓ DesktopSyncEvent
SyncEventHandler → WebSocket broadcast
    ↓ ws_sync_task_status_changed
Mobile Tauri Event → Auto-Execute Engine (state machine)
    ↓ sendInput / HTTP API
Claude Code (PTY)
```

## Project Structure

```
bedcode/
├── src/                          # Vue 3 frontend
│   ├── modules/
│   │   ├── desktop/              # Desktop UI (sessions, terminal, devices)
│   │   ├── mobile/               # Mobile UI (terminal, code explorer, toolbox, pairing)
│   │   └── shared/               # Shared components, stores, composables, i18n
│   └── locales/                  # i18n translations (zh-CN / en)
├── src-tauri/
│   └── src/
│       ├── shared/               # Shared Rust modules (auth, db, enums, models, system)
│       ├── desktop/              # Desktop-only (PTY, Actix server, session mgmt, plugin)
│       └── mobile/               # Mobile-only (WS client, HTTP client, routing, remote)
├── docs/                         # Documentation
└── e2e/                          # E2E tests
```

See [docs/code-map.md](docs/code-map.md) for the complete module index.

## Roadmap

- [x] Plugin system for Claude Code hooks and auto-execute
- [x] Mobile file browser and code viewer with diff rendering
- [x] Multi-language support (i18n: zh-CN / en)
- [x] Auto-execute task engine with auto/manual mode
- [ ] End-to-end encryption (X25519 + AES-GCM)
- [ ] Linux desktop support
- [ ] Internet connectivity interface
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
