<div align="center">

<img src="public/favicon.svg" width="96" alt="BedCode Desktop logo">

# BedCode Desktop

**The desktop host of BedCode** — run multiple Agent CLI / terminal sessions (Claude Code, pi, opencode, Codex, etc.) on Windows / Linux and let your phone take over remotely over the same WiFi.

[![Version](https://img.shields.io/badge/version-2.1.0-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)
[![Wasmtime](https://img.shields.io/badge/wasmtime-47-%232F6FED.svg)](https://wasmtime.dev/)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux-lightgrey.svg)](https://github.com/7ZAI/BedCode)

English | [简体中文](README.md)

</div>

This repository is the desktop project of the BedCode monorepo (Tauri 2.0 + Vue 3 + Rust). The mobile project lives in [`bedcode-mobile/`](../bedcode-mobile/); see the root [README](../README.md) for the full overview.

## Features

- **Session Management** — run multiple Agent CLI / terminal sessions concurrently, session configs persisted in SQLite, real-time xterm.js output preview
- **PTY Terminal** — native pseudo-terminals for any command-line program (including TUI apps), WSL2 distro support, `BEDCODE_SESSION_ID` injected to bind Agent sessions
- **Device Pairing & Discovery** — QR code + 6-digit code secure pairing, mDNS advertisement for automatic mobile discovery, biometric credentials bound to a public key
- **HTTP + WebSocket Server** — Actix Web powering terminal duplex streams and REST APIs (plugin hooks, file service), with advanced network configuration (worker threads, Keep-Alive, timeouts, frame size limits) and a dedicated metrics dashboard
- **Link Encryption** — X25519 identity keys + HKDF direction-separated derivation, HTTP envelope protocol and dual-ECDH WebSocket handshake frame encryption (opt-in, toggled in settings)
- **Peer Network (P2P)** — node identity (Ed25519) + self-signed certificates ("fingerprint as identity"), TLS 1.3 mTLS direct links, trust store with a first-connect confirmation gate, dedicated `_bedcode-peer` mDNS discovery; carries shared-directory browsing and batch resumable transfer
- **Plugin System** — WASM (wasmtime sandbox, WASM Component Model) + cdylib dynamic loading, permission control, hooks integration, session ID binding
- **System Tray** — background resident with quick actions
- **Auto Updates** — Tauri updater (release builds signed by GitHub Actions)
- **Internationalization** — full vue-i18n support (zh-CN / en)

## Tech Stack

| Category  | Technology                                                       |
| --------- | ---------------------------------------------------------------- |
| Framework | Tauri 2.0 (Windows / Linux), Node.js + Rust                      |
| Frontend  | Vue 3 + TypeScript + Vite + TailwindCSS                          |
| State     | Pinia + vue-router                                               |
| Backend   | Rust (Tokio), Actix Web 4 + tokio-tungstenite                    |
| Database  | SQLite (rusqlite)                                                |
| Terminal  | @xterm/xterm + addon-fit / unicode11 / web-links / webgl         |
| Auth      | JWT (HS256), ECDSA biometric credential (p256), device fingerprint |
| Crypto    | X25519 ECDH + AES-256-GCM (HKDF), ChaCha20-Poly1305, RSA-OAEP/PSS |
| Plugins   | wasmtime 47 (WASM component runtime) + cdylib dynamic loading    |
| Other     | shiki, ECharts, qrcode, vue-i18n@9, tracing logging              |

## Directory Structure

```
bedcode-desktop/
├── src/                    # Vue 3 frontend (flat layout)
│   ├── components/         # UI components (TitleBar, Sidebar, TerminalPreview, devices/sessions/plugins, etc.)
│   ├── composables/        # Business logic (useServer, usePairing, usePluginManager, useWsl, etc.)
│   ├── stores/             # Pinia stores (session, device, settings, wsl, inputAssistant, etc.)
│   ├── views/              # Pages (terminal window, session config, server, plugin list/detail/config, devices, settings, etc.)
│   ├── plugin/             # Frontend plugin system: loader, registry, permissions, shared-module runtime
│   ├── utils/  router/     # Tauri invoke wrappers / routing
│   ├── dev/                # Dev assets (terminal mocks, PTY output dumps)
│   ├── locales/            # i18n (zh-CN / en)
│   └── __tests__/          # Frontend tests
├── src-tauri/              # Rust backend
│   ├── resources/          # Bundled resources (config.properties + built-in plugin artifacts)
│   └── src/                # Flat per-domain layout, each domain with a same-named entry file
│       ├── commands/       # Tauri invoke layer (devices, mdns, plugin, pty_input, qr, etc.)
│       ├── pty/            # Pseudo-terminals & process management (output read/cache, WSL support)
│       ├── server/         # Actix Web HTTP + WS: controllers, dtos, middleware, ws,
│       │                   #   filter (traffic filter chain), link_crypto (link encryption), metrics,
│       │                   #   supervisor (server lifecycle)
│       ├── session/        # Session model & lifecycle, output management (cache/queue/subscription replay), input extension points
│       ├── db/             # SQLite persistence
│       ├── plugin/         # Plugin host: wasmtime runtime (host_impl split by capability domain) + cdylib loading,
│       │                   #   permission approval, message bus, three-layer fs_auth
│       ├── mdns/           # mDNS service advertisement
│       ├── events/         # Global event system: AppEvent + frontend forwarding + WebSocket sync broadcast
│       ├── system/         # App context (DI), config, error types, lifecycle hooks, grouped constants
│       ├── utils/          # auth (JWT/pairing/QR token), crypto, parser (ANSI/Markdown)
│       ├── enums/          # Enum types (auth, control, plugin, PTY state, session, special keys, etc.)
│       ├── peer_net.rs     # Peer network integration (node identity, trust gate, node lifecycle)
│       ├── peer_receive.rs # Inbound connection handling & event bridging
│       ├── peer_remote.rs  # Remote node access (shared-directory browsing, etc.)
│       ├── peer_transfer.rs# Batch resumable transfer over shared directories
│       └── peer_migration.rs# Migration from the old file service to the peer network
├── packages/
│   ├── plugin-sdk-desktop/ # Plugin SDK (TS + Rust, incl. dev-shell, templates, scaffolding CLI)
│   ├── plugin-test/        # Test WASM plugin covering every host call path
│   ├── plugin-sdk-test/    # SDK interface test plugin
│   └── plugin-wasi-test/   # WASI preopen test plugin (wasm32-wasip2)
├── plugins/                # Official plugins (ai-chatbox / auto-task / file-transfer)
├── e2e/                    # E2E tests (wdio)
├── scripts/                # Dev & build scripts (with README)
└── docs/                   # Project docs (code-map.md, linux-build.md, etc.)
```

## Quick Start

### Prerequisites

- [Node.js](https://nodejs.org/) ≥ 18, [Rust](https://www.rust-lang.org/tools/install) ≥ 1.94 (wasmtime 47 MSRV)
- [Tauri 2.0 CLI](https://v2.tauri.app/start/prerequisites/) and platform dependencies
  (Linux system dependencies: [docs/linux-build.md](docs/linux-build.md))
- An installed & configured Agent CLI (e.g. [Claude Code](https://claude.ai/code), pi, opencode, Codex)

### Install & Run

```bash
pnpm install

# Dev mode (builds plugins + launches Tauri dev)
pnpm run tauri:dev

# Production build (tauri-build.js resolves the updater signing key automatically;
# without a key, updater artifacts are disabled — no private key needed for local builds;
# artifacts: Windows NSIS + Linux DEB)
pnpm run tauri:build
```

### Tests

```bash
pnpm run test:run                  # Frontend unit tests (vitest run — don't use `pnpm run test`, it's watch mode)
pnpm run test:e2e                  # E2E tests (wdio)
cd src-tauri && cargo test         # Rust tests
```

### Other Scripts

| Command                         | Description                                                     |
| ------------------------------- | --------------------------------------------------------------- |
| `pnpm run build` / `build:fast` | Frontend type-check + build / build only                        |
| `pnpm run plugins:build`        | Build all official plugins (wasm + frontend artifacts)          |
| `pnpm run plugins:dev`          | Plugin dev hot-reload                                           |
| `pnpm run lint` / `format`      | ESLint / Prettier                                               |
| `pnpm run target:size`          | Check `src-tauri/target` size (run `target:clean` if over 15GB) |

## Plugin System

Desktop plugins are built on the **wasmtime 47 runtime (WASM Component Model)**: plugins compile from Rust / TypeScript into WASM components loaded in a host sandbox, with cdylib dynamic-library plugins also supported. Plugins can observe and extend host session behavior:

- **WASM sandbox runtime** — resource-bounded, memory-isolated; a crashing plugin never takes down the host
- **Dynamic loading** — scanned from `plugins/{plugin-id}/plugin.json` at runtime, no host recompile needed
- **Host capability bridge** — engine capabilities (process/network/storage/security/communication) accessed through the wit ABI and `host-*` primitives, implemented per capability domain under `host_impl/`, behind a unified permission gate
- **Plugin-to-plugin calls** — callable APIs must be declared in the manifest `api` field, invoked across plugins over JSON-RPC 2.0; a topic-based message bus provides publish/subscribe
- **Permission control** — plugins declare required permissions; the frontend fails fast and the host makes the final ruling. File system access goes through three layers (path whitelist → plugin whitelist → approval dialog)
- **Hooks integration** — project-level hooks auto-configured at session start, pushing task status (idle / in_progress / asking / completed / interrupted) via HTTP API
- **Session ID binding** — `BEDCODE_SESSION_ID` injected into the PTY, binding Agent CLI sessions to BedCode sessions
- **Lifecycle / input extension points** — plugins join session creation and input submission via `SessionLifecycleListener` and `SessionInputListener`

### Official Plugins

| Plugin            | Version    | Description                                                                                         |
| ----------------- | ---------- | --------------------------------------------------------------------------------------------------- |
| **AI Chatbox**    | 1.0.0-beta | AI chat: any OpenAI-compatible provider (OpenAI / Anthropic / DeepSeek / Qwen), streaming conversations, multi-session management, JSONL conversation logs |
| **Auto Task**     | 1.0.0-beta | Agent task queue & auto-approval: adapts Claude Code / pi / opencode / Codex; task status sync, queue scheduling, preset tasks, scheduled jobs, auto-response |
| **File Transfer** | 1.0.0-beta | LAN file transfer (over peer-network direct links): online peer discovery & switching, remote directory browsing, concurrent transfers (pause / resume / resume-after-break / retry), local directory mounting |

To build your own plugin, use [`@binblink/bedcode-plugin-sdk-desktop`](packages/plugin-sdk-desktop/README_en.md) (TS SDK + Rust `bedcode-plugin-api` crate); full guide in [plugin-dev-desktop.md](plugin-dev-desktop.md).

## Related Docs

- Root [README](../README.md) — project overview & security model
- [plugin-dev-desktop.md](plugin-dev-desktop.md) — desktop plugin development guide
- [docs/code-map.md](docs/code-map.md) — code structure index (incl. scripts)
- [docs/linux-build.md](docs/linux-build.md) — Linux build dependencies & cross-compilation
- [scripts/README.md](scripts/README.md) — build script docs

## License

MIT — see [LICENSE](../LICENSE).
