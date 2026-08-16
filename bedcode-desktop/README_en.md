<div align="center">

<img src="public/favicon.svg" width="96" alt="BedCode Desktop logo">

# BedCode Desktop

**The desktop host of BedCode** — run multiple Agent CLI / terminal sessions (Claude Code, opencode, pi, etc.) on Windows and let your phone take over remotely over the same WiFi.

[![Version](https://img.shields.io/badge/version-2.0.0-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)

English | [简体中文](README.md)

</div>

This repository is the desktop project of the BedCode monorepo (Tauri 2.0 + Vue 3 + Rust). The mobile project lives in [`bedcode-mobile/`](../bedcode-mobile/); see the root [README](../README.md) for the full overview.

## Features

- **Session Management** — run multiple Agent CLI / terminal sessions concurrently, session configs persisted in SQLite, real-time xterm.js output preview
- **PTY Terminal** — native pseudo-terminals for any command-line program (including TUI apps), with WSL2 distro support
- **Device Pairing & Discovery** — QR code + 6-digit code secure pairing, mDNS service advertisement for automatic mobile discovery
- **HTTP + WebSocket Server** — Actix Web powering terminal duplex streams and REST APIs (plugin hooks, file service), with advanced network configuration (worker threads, Keep-Alive, timeouts, frame size limits) and a dedicated metrics dashboard
- **Plugin System** — WASM (wasmtime sandbox, WASM Component Model) + cdylib dynamic loading, permission control, hooks integration, session ID binding
- **System Tray** — background resident with quick actions
- **Auto Updates** — Tauri updater (release builds signed by GitHub Actions)
- **Internationalization** — full vue-i18n support (zh-CN / en)

## Tech Stack

| Category | Technology |
|----------|------------|
| Framework | Tauri 2.0 (Windows), Node.js + Rust |
| Frontend | Vue 3 + TypeScript + Vite + TailwindCSS |
| State | Pinia + vue-router |
| Backend | Rust (Tokio), Actix Web 4 + tokio-tungstenite |
| Database | SQLite (rusqlite) |
| Terminal | @xterm/xterm + addon-fit / web-links / webgl |
| Auth | JWT (HS256), device fingerprint, 6-digit pairing code / QR token |
| Plugins | wasmtime (WASM component runtime) + cdylib dynamic loading |
| Other | shiki, ECharts, qrcode, vue-i18n@9, tracing logging |

## Directory Structure

```
bedcode-desktop/
├── src/                    # Vue 3 frontend
│   ├── components/         # UI components (TitleBar, Sidebar, TerminalPreview, etc.)
│   ├── composables/        # Business logic (useServer, usePairing, usePluginManager, etc.)
│   ├── stores/             # Pinia stores (session, device, settings, wsl, etc.)
│   ├── views/              # Pages (terminal window, session config, server, plugins, devices, etc.)
│   ├── plugin/             # Frontend plugin loader & permission mapping
│   └── locales/            # i18n (zh-CN / en)
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── pty/            # Pseudo-terminals & process management
│   │   ├── server/         # Actix Web HTTP + WebSocket server
│   │   ├── session/        # Session model & lifecycle
│   │   ├── db/             # SQLite persistence
│   │   ├── plugin/         # Plugin host: wasmtime runtime + cdylib loading
│   │   ├── mdns/           # mDNS service advertisement
│   │   ├── events/         # Event bus & WebSocket broadcast
│   │   └── commands/       # Tauri commands
├── packages/
│   └── plugin-sdk-desktop/ # Plugin SDK (TS + Rust), see its README
├── plugins/                # Official plugins (ai-chatbox / auto-task / file-transfer / scheduler)
├── scripts/                # Dev & build scripts (with README)
└── docs/                   # Project docs (code-map.md, etc.)
```

## Quick Start

### Prerequisites

- [Node.js](https://nodejs.org/) >= 18, [Rust](https://www.rust-lang.org/tools/install) >= 1.70
- [Tauri 2.0 CLI](https://v2.tauri.app/start/prerequisites/) and platform-specific dependencies
- An installed & configured Agent CLI (e.g. [Claude Code](https://claude.ai/code), opencode, pi)

### Install & Run

```bash
npm install

# Dev mode (builds plugins + launches Tauri dev)
npm run tauri:dev

# Production build (tauri-build.js resolves the updater signing key automatically;
# without a key, updater artifacts are disabled — no private key needed for local builds)
npm run tauri:build
```

### Tests

```bash
npm run test:run            # Frontend unit tests (vitest run — don't use `npm run test`, it's watch mode)
npx playwright test         # E2E tests
cd src-tauri && cargo test  # Rust tests
```

### Other Scripts

| Command | Description |
|---------|-------------|
| `npm run build` / `build:fast` | Frontend type-check + build / build only |
| `npm run plugins:build` | Build all official plugins (wasm + frontend artifacts) |
| `npm run plugins:dev` | Plugin dev hot-reload |
| `npm run lint` / `format` | ESLint / Prettier |
| `npm run target:size` | Check `src-tauri/target` size (run `target:clean` if over 15GB) |

## Plugin System

Desktop plugins are built on the **wasmtime runtime (WASM Component Model)**: plugins compile from Rust / TypeScript into WASM components loaded in a host sandbox, with cdylib dynamic-library plugins also supported. Plugins can observe and extend host session behavior:

- **WASM sandbox runtime** — resource-bounded, memory-isolated; a crashing plugin never takes down the host
- **Dynamic loading** — scanned from `plugins/desktop/{plugin-id}/plugin.json` at runtime, no host recompile needed
- **Host API bridge** — versioned API for host capabilities (send input, read output, session info) behind a unified permission gate
- **Permission control** — plugins declare required permissions; the host enforces access boundaries
- **Hooks integration** — project-level hooks auto-configured at session start, pushing task status (idle / in_progress / asking / completed / interrupted) via HTTP API
- **Session ID binding** — `BEDCODE_SESSION_ID` injected into the PTY, binding Agent CLI sessions to BedCode sessions

### Official Plugins

| Plugin | Version | Description |
|--------|---------|-------------|
| **AI Chatbox** | 1.0.0-beta | AI chat: any OpenAI-compatible provider, streaming conversations, multi-session management |
| **Auto Task** | 1.0.0-beta | Agent task queue & auto-approval: task status sync, queue scheduling, preset tasks, scheduled jobs |
| **File Transfer** | 1.0.0-beta | LAN file transfer: peer discovery, remote directory browsing, concurrent transfers (resume / retry) |
| **Scheduler** | 1.0.0-beta | Generic scheduling framework: cron-triggered shell scripts / inline commands with audit logs |

To build your own plugin, use [`@binblink/plugin-sdk-desktop`](packages/plugin-sdk-desktop/README_en.md) (TS SDK + Rust `bedcode-plugin-api` crate); full guide in [plugin-dev-desktop.md](plugin-dev-desktop.md).

## Related Docs

- Root [README](../README.md) — project overview & security model
- [plugin-dev-desktop.md](plugin-dev-desktop.md) — desktop plugin development guide
- [docs/code-map.md](docs/code-map.md) — code structure index (incl. scripts)
- [scripts/README.md](scripts/README.md) — build script docs

## License

MIT — see [LICENSE](../LICENSE).
