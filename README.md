<div align="center">

# BedCode

**Use your phone to control Claude Code on your desktop**

[![Version](https://img.shields.io/badge/version-1.1.0-blue.svg)](https://github.com/7ZAI/BedCode)
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
- **Session Management** - Create, configure, and manage multiple Claude Code sessions with SQLite persistence
- **Terminal Preview** - Real-time xterm.js terminal output preview
- **Device Pairing** - QR code + 6-digit code authentication for secure device pairing
- **Plugin System** - cdylib dynamic loading plugin architecture with host API bridge, permission control, and storage
- **HTTP + WebSocket Server** - Actix Web based HTTP API + WebSocket for terminal communication, with advanced network configuration (workers, keep-alive, timeouts, frame size limits)
- **Server Management** - Dedicated server view with status monitoring, metrics dashboard, and network config editor
- **System Tray** - Quick actions from the system tray
- **WSL2 Support** - Run sessions inside Windows Subsystem for Linux with distro selection
- **mDNS Discovery** - mDNS service advertisement for mobile device discovery

### Mobile (Remote)
- **Device Discovery & Pairing** - mDNS-based device discovery, QR code scanning, or pairing code input
- **Terminal Output** - Enhanced mode (parsed ANSI/Markdown) and raw mode toggle
- **Smart Input Bar** - Special keys (Tab, Ctrl+C, Esc, arrows), input assistant, and shortcut config
- **Code Explorer** - Browse project files, view code with syntax highlighting, and diff rendering
- **Preset Tasks** - Pre-configured task cards with type badges, edit dialog, and one-tap execution
- **Toolbox** - Quick action panel with customizable commands
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
│  │ PTY Manager│  │ WS Server  │ │   WebSocket    │  │ WS Client  │  │ Code       │ │
│  │ (Claude)   │  │ (Actix)    │◄├───────────────►├►│            │  │ Explorer   │ │
│  └────────────┘  └────────────┘ │   + HTTP API   │  └────────────┘  └────────────┘ │
│  ┌────────────┐  ┌────────────┐ │                │  ┌────────────┐  ┌────────────┐ │
│  │ Plugin Mgr │  │ HTTP API   │ │                │  │ Preset     │  │ Touch UI   │ │
│  │ (cdylib)   │  │ (Actix)    │ │                │  │ Tasks      │  │            │ │
│  └────────────┘  └────────────┘ │                │  └────────────┘  └────────────┘ │
└─────────────────────────────────┘                └─────────────────────────────────┘
```

The project uses a **monorepo with independent platform projects**:

| Layer | Frontend (Vue 3) | Backend (Rust) |
|-------|-------------------|-----------------|
| **Desktop** | Session manager, terminal preview, server view, plugin config, sidebar | PTY, Actix Web server, session management, cdylib plugin system, mDNS advertiser |
| **Mobile** | Terminal view, code explorer, preset tasks, toolbox, device discovery | WS client, HTTP client, remote connection, routing, mDNS discovery |

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
| Terminal | @xterm/xterm + @xterm/addon-fit + @xterm/addon-web-links + @xterm/addon-webgl |
| I18n | vue-i18n@9 |
| Testing | Vitest, Rust test |

## Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) >= 18
- [Rust](https://www.rust-lang.org/tools/install) >= 1.70
- [Tauri 2.0 CLI](https://v2.tauri.app/start/prerequisites/) dependencies for your platform
- [Claude Code CLI](https://claude.ai/code) installed and configured

### Install Dependencies

```bash
# Desktop
cd bedcode-desktop
npm install

# Mobile
cd bedcode-mobile
npm install

# Rust dependencies are fetched automatically by Cargo
```

### Development

```bash
# Start desktop app in dev mode
cd bedcode-desktop
npm run tauri:dev

# Start mobile app in dev mode
cd bedcode-mobile
npm run tauri:android:dev
```

### Build

```bash
# Build desktop app
cd bedcode-desktop
npm run tauri:build

# Build Android APK (release)
cd bedcode-mobile
npm run tauri:android:build

# Build Android APK (debug fast)
cd bedcode-mobile
npm run tauri:android:build:fast
```

### Testing

```bash
# Frontend unit tests
npm run test

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

BedCode uses a `config.properties` file (bundled as a Tauri resource) for runtime configuration. The file supports comments and is organized by category:

### Network

| Key | Default | Description |
|-----|---------|-------------|
| `network.port` | `8765` | WebSocket server port |
| `network.auto_start` | `true` | Auto-start server on app launch |
| `network.prevent_sleep` | `true` | Prevent system sleep while server is running |
| `network.workers` | `0` | Actix Web worker threads (0 = CPU core count) |
| `network.keep_alive_secs` | `5` | HTTP Keep-Alive timeout in seconds (0 = disabled) |
| `network.client_request_timeout_secs` | `5` | Client request header read timeout |
| `network.client_disconnect_timeout_secs` | `3` | Client disconnect wait timeout |
| `network.max_connections` | `256` | Max concurrent connections per worker |
| `network.backlog` | `2048` | TCP half-open connection queue limit |
| `network.tcp_nodelay` | `true` | Enable TCP_NODELAY (disable Nagle algorithm) |
| `network.shutdown_timeout_secs` | `30` | Graceful shutdown timeout |
| `network.ws_max_frame_size_kb` | `64` | WebSocket max frame size (KB) |
| `network.ws_max_message_size_mb` | `16` | WebSocket max message size (MB, across frames) |

### Session

| Key | Default | Description |
|-----|---------|-------------|
| `session.default_environment` | `windows` | Default execution environment (windows / wsl2) |
| `session.default_wsl_distro` | *(empty)* | Default WSL distro (only for wsl2, empty = default) |
| `session.default_working_dir` | *(empty)* | Default working directory (empty = user home) |
| `session.default_command` | `claude` | Default terminal command |
| `session.session_timeout` | `3600` | Session timeout in seconds (auto-close on inactivity) |

### UI

| Key | Default | Description |
|-----|---------|-------------|
| `ui.theme` | `system` | Theme (system/light/dark) |
| `ui.terminal_font_size` | `12` | Terminal font size |
| `ui.terminal_font_family` | `Consolas` | Terminal font family |
| `ui.terminal_theme` | `dracula` | Terminal color theme name |
| `ui.show_preview` | `true` | Show terminal preview |

### Terminal

| Key | Default | Description |
|-----|---------|-------------|
| `terminal.default_cols` | `120` | Default terminal columns |
| `terminal.default_rows` | `40` | Default terminal rows |
| `terminal.flush_interval_ms` | `50` | Output buffer flush interval (ms) |
| `terminal.max_buffer_size` | `65536` | Max output buffer size (bytes) |
| `terminal.read_buffer_size` | `4096` | PTY read buffer size (bytes) |

### Channels

| Key | Default | Description |
|-----|---------|-------------|
| `channels.output_broadcast_capacity` | `2048` | PTY output broadcast capacity |
| `channels.status_broadcast_capacity` | `64` | Session status broadcast capacity |
| `channels.restart_broadcast_capacity` | `64` | Session restart broadcast capacity |
| `channels.event_broadcast_capacity` | `256` | Unified event broadcast capacity |
| `channels.pty_subscription_capacity` | `1024` | PTY subscription broadcast capacity |
| `channels.global_queue_capacity` | `50000` | Global output queue capacity (for mobile replay) |
| `channels.ws_event_capacity` | `1024` | WebSocket event broadcast capacity |
| `channels.lifecycle_capacity` | `16` | Lifecycle event broadcast capacity |

### Plugin

| Key | Default | Description |
|-----|---------|-------------|
| `plugin.token` | *(empty)* | HTTP API auth token for plugin status push (empty = skip verification, dev mode) |

## How It Works

1. **Start Desktop App** - Launch BedCode on your desktop, which starts the Actix Web server (HTTP + WebSocket) and mDNS discovery service
2. **Pair Your Phone** - Open BedCode on your phone, discover the desktop via mDNS, scan the QR code or enter the 6-digit pairing code
3. **Control Remotely** - Once paired, select a session and start sending commands from your phone
4. **Real-time Output** - Terminal output is streamed to your phone in real-time with ANSI rendering
5. **Browse Code** - Use the code explorer to browse project files and view diffs with syntax highlighting
6. **Preset Tasks** - Configure common tasks as preset cards for one-tap execution

## Plugin System

BedCode features a cdylib-based dynamic plugin system on the desktop:

- **Dynamic Loading** - Plugins are compiled as `.cdylib` shared libraries and loaded at runtime
- **Host API Bridge** - Plugins access host functionality (send input, read output, session info) through a versioned API bridge
- **Permission Control** - Each plugin declares required permissions; the host enforces access boundaries
- **Persistent Storage** - Plugins can store key-value data through the host-provided storage interface
- **Auto-Configuration** - Project-scoped Claude Code hooks are automatically configured when a session starts
- **Task Status Tracking** - Claude Code hooks push task status (idle/in_progress/asking/completed/interrupted) to the desktop app via HTTP API
- **Session ID Binding** - PTY sessions inject `BEDCODE_SESSION_ID` environment variable to bind Claude Code sessions with BedCode sessions

```
Claude Code Hook (Python)
    ↓ HTTP POST
Rust HTTP API (plugin_controller)
    ↓ DesktopSyncEvent
SyncEventHandler → WebSocket broadcast
    ↓ ws_sync_task_status_changed
Mobile Tauri Event → Preset Tasks / UI
    ↓ sendInput / HTTP API
Claude Code (PTY)
```

## Project Structure

```
BedCode/
├── bedcode-desktop/               # Desktop app (Tauri + Vue 3)
│   ├── src/                       # Vue 3 frontend
│   │   ├── components/            # UI components
│   │   ├── composables/           # Business logic composables
│   │   ├── stores/                # Pinia state stores
│   │   ├── views/                 # Page views
│   │   ├── locales/               # i18n translations (zh-CN / en)
│   │   └── plugins/               # Frontend plugin loader
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── commands/          # Tauri invoke commands
│   │   │   ├── db/                # SQLite database layer
│   │   │   ├── enums/             # Enum types
│   │   │   ├── events/            # Global event system
│   │   │   ├── mdns/              # mDNS service advertisement
│   │   │   ├── plugin/            # cdylib plugin system
│   │   │   ├── pty/               # PTY management (Windows + WSL2)
│   │   │   ├── server/            # Actix Web HTTP/WS server
│   │   │   ├── session/           # Session management
│   │   │   ├── system/            # Config, error handling, app context
│   │   │   └── utils/             # Auth (JWT, pairing), parsers (ANSI, Markdown)
│   │   └── resources/
│   │       └── config.properties  # Runtime configuration
│   └── docs/
│       └── code-map.md            # Desktop module index
│
├── bedcode-mobile/                # Mobile app (Tauri + Vue 3)
│   ├── src/                       # Vue 3 frontend
│   │   ├── components/            # UI components
│   │   ├── composables/           # Business logic composables
│   │   ├── stores/                # Pinia state stores
│   │   ├── views/                 # Page views
│   │   └── locales/               # i18n translations (zh-CN / en)
│   ├── src-tauri/
│   │   └── src/
│   │       ├── auth/              # Authentication (manager, pairing)
│   │       ├── commands/          # Tauri invoke commands
│   │       ├── connection/        # Remote connection (WS client, heartbeat, reconnect)
│   │       ├── enums/             # Enum types
│   │       ├── handler/           # Message handlers
│   │       ├── mdns/              # mDNS service discovery
│   │       ├── model/             # Data models
│   │       ├── plugin/            # Android plugin bridge
│   │       ├── router/            # Message routing
│   │       ├── system/            # Config, error handling, settings
│   │       ├── session.rs         # Remote session management
│   │       └── state.rs           # Global state
│   └── docs/
│       └── code-map.md            # Mobile module index
│
├── docs/                          # Shared documentation
└── .github/                       # CI/CD workflows
```

See [bedcode-desktop/docs/code-map.md](bedcode-desktop/docs/code-map.md) and [bedcode-mobile/docs/code-map.md](bedcode-mobile/docs/code-map.md) for complete module indexes.

## Roadmap

- [x] Plugin system for Claude Code hooks and cdylib dynamic loading
- [x] Mobile file browser and code viewer with diff rendering
- [x] Multi-language support (i18n: zh-CN / en)
- [x] Preset task cards with one-tap execution
- [x] Advanced network configuration for Actix Web server
- [x] Server management view with metrics dashboard
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
