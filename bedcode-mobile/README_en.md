<div align="center">

# BedCode Mobile

**The mobile remote terminal of BedCode** — turn your phone into an optimized touch terminal and take over the Agent CLI / terminal sessions (Claude Code, opencode, etc.) running on your desktop, from anywhere on the same WiFi. Code from bed.

[![Version](https://img.shields.io/badge/version-2.0.0-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)
[![Platform](https://img.shields.io/badge/platform-Android-lightgrey.svg)](https://github.com/7ZAI/BedCode)

English | [简体中文](README.md)

</div>

This repository is the mobile project of the BedCode monorepo (Tauri 2.0 + Vue 3 + Rust, Android). The desktop host project lives in [`bedcode-desktop/`](../bedcode-desktop/); see the root [README](../README.md) for the full overview.

## Features

- **Device Discovery & Pairing** — mDNS-based auto discovery of the desktop, connect by QR code scan or pairing code
- **Terminal Output** — enhanced mode (parsed ANSI / Markdown) and raw mode toggle, TUI-compatible scrolling
- **Smart Input Bar** — special keys (Tab, Ctrl+C, Esc, arrows), input assistant, shortcut configuration
- **Code Browser** — browse remote project files, syntax highlighting (shiki), Git diff rendering
- **Preset Tasks** — task cards with type tags, editing, and one-tap execution
- **Toolbox** — quick-action panel with custom commands
- **Task Notifications** — system notifications per session task status; foreground service keeps the app alive and the screen awake
- **Auto-Reconnect** — automatic reconnect on unexpected disconnects, edge-to-edge display with safe-area (notch / gesture bar) adaptation
- **Biometric Auth** — biometric credential bound to a public key, challenge-response signature before session credential issuance (replay-proof)
- **Plugin System** — same plugin architecture as the desktop, plus mobile-only capabilities (SAF storage access, system back key, etc.)
- **Internationalization** — full vue-i18n support (zh-CN / en)

## Tech Stack

| Category | Technology |
|----------|------------|
| Framework | Tauri 2.0 (Android), Node.js + Rust |
| Frontend | Vue 3 + TypeScript + Vite + TailwindCSS |
| State | Pinia + vue-router |
| Backend | Rust (Tokio), tokio-tungstenite (WS client) |
| Terminal | @xterm/xterm + addon-fit / unicode11 / web-links / webgl |
| Auth | JWT (HS256), ECDSA biometric credential (p256), device fingerprint |
| Files | SAF (Storage Access Framework) tree traversal & relay copy |
| Plugins | WASM components (wasmtime) runtime |
| Other | shiki, html5-qrcode, marked, vue-i18n@9, tracing logging (logcat) |

## Directory Structure

```
bedcode-mobile/
├── src/                    # Vue 3 frontend
│   ├── components/         # UI components (TerminalView, MobileNav, InputBar, etc.)
│   ├── composables/        # Business logic (useMobileConnection, usePresetTasks, useFileTree, etc.)
│   ├── stores/             # Pinia stores (settings, terminalBuffer, codeViewer, etc.)
│   ├── views/              # Pages (discovery, scan, sessions, terminal, code browser, toolbox, presets, etc.)
│   ├── plugin/             # Frontend plugin loader & permission mapping
│   └── locales/            # i18n (zh-CN / en)
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── connection/     # WebSocket connection & routing
│   │   ├── auth/           # Pairing & biometric auth
│   │   ├── file_service/   # File service & SAF access
│   │   ├── plugin/         # Plugin host: wasmtime runtime
│   │   ├── mdns/           # mDNS service discovery
│   │   ├── session/        # Session model
│   │   └── commands/       # Tauri commands
│   └── gen/android/        # Android project (custom Kotlin plugins: foreground service, SAF, biometrics, etc.)
├── packages/
│   └── plugin-sdk-mobile/  # Plugin SDK (TS + Rust), see its README
├── plugins/                # Official plugins (ai-chatbox / auto-task / file-transfer)
├── scripts/                # Dev & build scripts
└── docs/                   # Project docs (code-map.md, etc.)
```

## Quick Start

### Prerequisites

- [Node.js](https://nodejs.org/) >= 18, [Rust](https://www.rust-lang.org/tools/install) >= 1.70
- [Tauri 2.0 CLI](https://v2.tauri.app/start/prerequisites/) with Android SDK / NDK
- A computer running [BedCode Desktop](../bedcode-desktop/) as the host

### Install & Run

```bash
npm install

# Dev mode: build & install to an Android device (physical or emulator)
npm run tauri:android:dev

# Dev mode with host-side log capture (logcat also written to .dev-logs/ for debugging)
npm run tauri:android:dev:log

# Production build (aarch64)
npm run tauri:android:build
```

> [!NOTE]
> After regenerating `gen/android`, the custom Kotlin files (foreground service, SAF, biometrics, downloads-dir plugins, etc.) plus AndroidManifest.xml, signing keys must be restored — see the Android section of the root [AGENTS.md](../AGENTS.md).

### Tests

```bash
npm run test:run            # Frontend unit tests (vitest run — don't use `npm run test`, it's watch mode)
cd src-tauri && cargo test  # Rust tests
```

> After modifying custom Kotlin plugins under `src-tauri/gen/android/`, you must additionally run
> `./gradlew :app:compileUniversalDebugKotlin` (in `src-tauri/gen/android/`) to verify compilation —
> `cargo test` / frontend tests cannot cover Kotlin code.

### Other Scripts

| Command | Description |
|---------|-------------|
| `npm run build` / `build:fast` | Frontend type-check + build / build only |
| `npm run tauri:android:build:fast` | Quick debug build without type-check |
| `npm run plugins:build` | Build official plugins (WASM artifacts) |
| `npm run target:size` | Check `src-tauri/target` size (run `target:clean` if over 15GB) |

## Plugin System

Mobile plugins share the same architecture as the desktop (WASM Component Model + permission control), with mobile-only capabilities added: **SAF storage access**, dialogs / system notifications, **dynamic routing**, lifecycle hooks, Android system back-key interception, and a dev-shell mock protocol (browser HMR dev environment).

To build your own plugin, use [`@bedcode/plugin-sdk-mobile`](packages/plugin-sdk-mobile/README_en.md) (TS SDK + Rust `bedcode-plugin-api-mobile` crate); full guide in [plugin-dev-mobile.md](plugin-dev-mobile.md).

### Official Plugins

| Plugin | Version | Description |
|--------|---------|-------------|
| **AI Chatbox** | 1.0.0-beta | AI chat: any OpenAI-compatible provider, streaming conversations, multi-session management |
| **Auto Task** | 1.0.0-beta | Agent task queue & auto-approval: task status sync, queue scheduling, preset tasks, scheduled jobs |
| **File Transfer** | 1.0.0-beta | LAN file transfer: online peer discovery, remote directory browsing, concurrent transfers (resume / retry) |

## Related Docs

- Root [README](../README.md) — project overview & security model
- [plugin-dev-mobile.md](plugin-dev-mobile.md) — mobile plugin development guide
- [docs/code-map.md](docs/code-map.md) — code structure index

## License

MIT — see [LICENSE](../LICENSE).
