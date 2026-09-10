<div align="center">

# BedCode Mobile

**The mobile remote terminal of BedCode** — turn your phone into an optimized touch terminal and take over the Agent CLI / terminal sessions (Claude Code, pi, opencode, Codex, etc.) running on your desktop, from anywhere on the same WiFi. Code from bed.

[![Version](https://img.shields.io/badge/version-2.1.0-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)
[![Wasmtime](https://img.shields.io/badge/wasmtime-47-%232F6FED.svg)](https://wasmtime.dev/)
[![Platform](https://img.shields.io/badge/platform-Android-lightgrey.svg)](https://github.com/7ZAI/BedCode)

English | [简体中文](README.md)

</div>

This repository is the mobile project of the BedCode monorepo (Tauri 2.0 + Vue 3 + Rust, Android). The desktop host project lives in [`bedcode-desktop/`](../bedcode-desktop/); see the root [README](../README.md) for the full overview.

## Features

- **Device Discovery & Pairing** — mDNS-based auto discovery of the desktop, connect by QR code scan or pairing code, one-tap reconnect from connection history
- **Terminal Output** — enhanced mode (parsed ANSI / Markdown) and raw mode toggle, TUI-compatible scrolling
- **Smart Input Bar** — special keys (Tab, Ctrl+C, Esc, arrows), input assistant, shortcut configuration
- **Code Browser** — browse remote project files, syntax highlighting (shiki), Git diff rendering and branch switching
- **Preset Tasks** — task cards with type tags, editing, and one-tap execution
- **Toolbox** — quick-action panel with custom commands
- **Task Notifications** — system notifications per session task status; foreground service keeps the app alive and the screen awake
- **Auto-Reconnect** — automatic reconnect on unexpected disconnects, edge-to-edge display with safe-area (notch / gesture bar) adaptation
- **Biometric Auth** — biometric credential bound to a public key, challenge-response signature before session credential issuance (replay-proof)
- **Link Encryption** — X25519 + HKDF encrypted link paired with the desktop (HTTP envelope + WebSocket frame encryption, opt-in)
- **Peer Network (P2P)** — node identity (Ed25519) + self-signed certificates, TLS 1.3 mTLS direct links, trust store with a first-connect confirmation gate, dedicated `_bedcode-peer` mDNS discovery; carries shared-directory browsing and batch resumable transfer
- **Plugin System** — same plugin architecture as the desktop, plus mobile-only capabilities (SAF storage access, dynamic routing, system back key, etc.)
- **Internationalization** — full vue-i18n support (zh-CN / en)

## Tech Stack

| Category  | Technology                                                         |
| --------- | ------------------------------------------------------------------ |
| Framework | Tauri 2.0 (Android), Node.js + Rust                                |
| Frontend  | Vue 3 + TypeScript + Vite + TailwindCSS                            |
| State     | Pinia + vue-router                                                 |
| Backend   | Rust (Tokio), tokio-tungstenite (WS client)                        |
| Terminal  | @xterm/xterm + addon-fit / unicode11 / web-links / webgl           |
| Auth      | JWT (HS256), ECDSA biometric credential (p256), device fingerprint |
| Crypto    | X25519 ECDH + AES-256-GCM (HKDF), ChaCha20-Poly1305                |
| Files     | SAF (Storage Access Framework) tree traversal & relay copy         |
| Plugins   | wasmtime 47 (WASM component runtime)                               |
| Other     | shiki, html5-qrcode, marked, vue-i18n@9, tracing logging (logcat)  |

## Directory Structure

```
bedcode-mobile/
├── src/                    # Vue 3 frontend (flat layout)
│   ├── components/         # UI components (TerminalView, MobileNav, InputBar, device cards, file browser, etc.)
│   ├── composables/        # Business logic (useMobileConnection, usePresetTasks, useFileTree,
│   │                       #   useTerminalBuffer, useEdgeToEdge, useForegroundService, etc.)
│   ├── stores/             # Pinia stores (settings, terminalBuffer, codeViewer, inputAssistant, etc.)
│   ├── views/              # Pages (terminal, code browser, devices, mDNS discovery, scan, sessions, toolbox, presets,
│   │                       #   plugins; views/settings/ split by domain: appearance/connection/auth/notifications/about)
│   ├── plugin/             # Frontend plugin system: loader, registry, permissions, shared-module runtime, dialog host
│   ├── services/           # Cross-cutting services (link encryption client)
│   ├── config/  styles/    # Terminal theme definitions / global styles (mobile.css + terminal.css)
│   ├── assets/  utils/  router/  locales/  # Static assets / utilities / routing / i18n (zh-CN / en)
│   └── __tests__/          # Frontend tests (incl. fixtures and integration)
├── src-tauri/              # Rust backend
│   ├── resources/          # Bundled resources (config.json + built-in plugin artifacts)
│   ├── src/                # Flat per-domain layout, each domain with a same-named entry file
│   │   ├── connection/     # WebSocket client: codec, request-response correlation, heartbeat, reconnect, message routing
│   │   ├── handler/        # WS message handlers (auth, sync, system, terminal)
│   │   ├── router/         # Message routing (route context, events, registry)
│   │   ├── auth/           # Auth state & pairing flow
│   │   ├── file_service/   # File service: SAF directory tree reading
│   │   ├── plugin/         # Plugin host: wasmtime runtime (host_impl split by capability domain),
│   │   │                   #   APK assets extraction, downloader, saf_io, android_plugins native bridge
│   │   ├── mdns/           # mDNS service discovery & advertisement
│   │   ├── model/  enums/  # Data models (API DTOs, WS messages) / enum types
│   │   ├── system/         # Shared commands, config management, grouped constants, unified error types, JSON settings persistence
│   │   ├── session.rs      # Remote session management
│   │   ├── state.rs        # Global state management (singleton managers + token storage)
│   │   ├── peer_net.rs     # Peer network integration (node identity, trust gate, node lifecycle)
│   │   ├── peer_receive.rs # Inbound connection handling & event bridging
│   │   ├── peer_remote.rs  # Remote node access (shared-directory browsing, etc.)
│   │   ├── peer_transfer.rs# Batch resumable transfer over shared directories
│   │   └── peer_migration.rs# Migration from the old file service to the peer network
│   └── gen/android/        # Android project (custom Kotlin plugins: foreground service, SAF, biometrics,
│                           #   downloads dir, all-files access, multicast lock, notifications, etc.)
├── packages/
│   ├── plugin-sdk-mobile/  # Plugin SDK (TS + Rust, incl. dev-shell and templates)
│   └── plugin-component-test/ # Test WASM plugin crate
├── plugins/                # Official plugins (ai-chatbox / auto-task / file-transfer)
├── scripts/                # Dev & build scripts (Android dev log capture, plugin build, etc.)
└── docs/                   # Project docs (code-map.md, etc.)
```

## Quick Start

### Prerequisites

- [Node.js](https://nodejs.org/) ≥ 18, [Rust](https://www.rust-lang.org/tools/install) ≥ 1.94 (wasmtime 47 MSRV)
- [Tauri 2.0 CLI](https://v2.tauri.app/start/prerequisites/) with Android SDK / NDK
- A computer running [BedCode Desktop](../bedcode-desktop/) as the host

### Install & Run

```bash
pnpm install

# Dev mode: build & install to an Android device (physical or emulator)
pnpm run tauri:android:dev

# Dev mode with host-side log capture (logcat also written to .dev-logs/ for debugging)
pnpm run tauri:android:dev:log

# Production build (aarch64)
pnpm run tauri:android:build

# Quick debug build (skips type-check) / emulator build
pnpm run tauri:android:build:fast
pnpm run tauri:android:build:emulator
```

> [!NOTE]
> After regenerating `gen/android`, the custom Kotlin files (foreground service, SAF, biometrics, downloads-dir plugins, etc.) plus AndroidManifest.xml and signing keys must be restored — see the Android section of the root [AGENTS.md](../AGENTS.md).

### Tests

```bash
pnpm run test:run                  # Frontend unit tests (vitest run — don't use `pnpm run test`, it's watch mode)
cd src-tauri && cargo test         # Rust tests
```

> After modifying custom Kotlin plugins under `src-tauri/gen/android/`, you must additionally run
> `./gradlew :app:compileUniversalDebugKotlin` (in `src-tauri/gen/android/`) to verify compilation —
> `cargo test` / frontend tests cannot cover Kotlin code.

### Other Scripts

| Command                                | Description                                                     |
| -------------------------------------- | --------------------------------------------------------------- |
| `pnpm run build` / `build:fast`        | Frontend type-check + build / build only                        |
| `pnpm run tauri:android:build:fast`    | Quick debug build without type-check                            |
| `pnpm run tauri:android:build:emulator` | Emulator (x86_64) debug build                                |
| `pnpm run plugins:build`               | Build official plugins (WASM artifacts)                         |
| `pnpm run build:all`                   | Build plugins first, then the frontend                          |
| `pnpm run target:size`                 | Check `src-tauri/target` size (run `target:clean` if over 15GB) |

## Plugin System

Mobile plugins share the same architecture as the desktop (wasmtime 47, WASM Component Model + permission control), with mobile-only capabilities added: **SAF storage access**, dialogs / system notifications, **dynamic routing**, lifecycle hooks, Android system back-key interception, and a dev-shell mock protocol (browser HMR dev environment). Host capability implementations are split by capability domain under `host_impl/` (storage/db/fs/http/terminal/event/bus/config/notify/peer/support). Built-in plugins are extracted from APK assets into the app data directory and scanned there; remote download with SHA256 verification is also supported.

To build your own plugin, use [`@binblink/bedcode-plugin-sdk-mobile`](packages/plugin-sdk-mobile/README_en.md) (TS SDK + Rust `bedcode-plugin-api-mobile` crate); full guide in [plugin-dev-mobile.md](plugin-dev-mobile.md).

### Official Plugins

| Plugin            | Version    | Description                                                                                                |
| ----------------- | ---------- | ---------------------------------------------------------------------------------------------------------- |
| **AI Chatbox**    | 1.0.0-beta | AI chat: any OpenAI-compatible provider (OpenAI / Anthropic / DeepSeek / Qwen), streaming conversations, multi-session management, JSONL conversation logs |
| **Auto Task**     | 1.0.0-beta | Auto task queue: queue & auto-execution, auto-response to agent questions, preset tasks, scheduled jobs, task history with retry (backend logic lives in the desktop plugin of the same name) |
| **File Transfer** | 1.0.0-beta | LAN file transfer (over peer-network direct links): browse peer shared directories, multi-select batch transfer, queue pause/resume/cancel/retry with explicit failure reasons, receive policy (batch-approve / auto-accept / auto-decline), SAF save-location picker, history |

## Related Docs

- Root [README](../README.md) — project overview & security model
- [plugin-dev-mobile.md](plugin-dev-mobile.md) — mobile plugin development guide
- [docs/code-map.md](docs/code-map.md) — code structure index
- [docs/terminal-output-pipeline-optimization.md](docs/terminal-output-pipeline-optimization.md) — terminal output pipeline optimization notes

## License

MIT — see [LICENSE](../LICENSE).
