<div align="center">

<img src="public/favicon.svg" width="96" alt="BedCode Desktop logo">

# BedCode Desktop

**The desktop host of BedCode** — run multiple Agent CLI / terminal sessions (Claude Code, pi, opencode, Codex, etc.) on Windows / Linux and let your phone take over remotely over the same WiFi.

[![Version](https://img.shields.io/badge/version-2.1.0-blue.svg)](https://github.com/7ZAI/BedCode)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](../LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://v2.tauri.app/)
[![Wasmtime](https://img.shields.io/badge/wasmtime-48-%232F6FED.svg)](https://wasmtime.dev/)
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
- **Plugin System** — WASM (wasmtime sandbox, WASM Component Model), permission control, hooks integration, session ID binding
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
| Plugins   | wasmtime 48 (WASM component runtime, LTS; WIT contract `bedcode:plugin@0.2.0`) |
| Other     | shiki, ECharts, qrcode, vue-i18n@9, tracing logging              |

## Directory Structure

```
bedcode-desktop/                        # Desktop project (Tauri 2 + Vue 3)
├── scripts/                            # Build/dev scripts (dev orchestration, plugin build & hot reload, artifact checks)
├── packages/                           # Plugin contract & fixture crates (mechanism/capability/engine crates moved to root packages/)
│   ├── plugin-sdk-desktop/             # Plugin SDK: Rust (bedcode-plugin-api + macros) + TS, templates, scaffolding, dev-shell
│   ├── plugin-sdk-fixtures/            # Host wasm closed-loop fixtures
│   ├── plugin-system-test/             # System-shape test plugin covering every host call path
│   ├── plugin-p3-async-host-import-test/ # WASI p3 async host-import test plugin
│   ├── plugin-bench-test/              # Benchmark fixture
│   └── plugin-wasi-test/               # WASI preopen test plugin (wasm32-wasip2)
├── wasm-apps/                          # WASM apps (business home ground; each an independent package: plugin.json + rust/ + src/ + its own build)
│   ├── agent-hub/                      # Agent CLI management console (env / skills / providers / usage)
│   ├── ai-chatbox/                     # Multi-provider OpenAI-compatible chat
│   ├── file-transfer/                  # File transfer (single artifact for both ends; business core in packages/bedcode-file-transfer-core)
│   └── terminal-session/               # Terminal session center (session source of truth / pairing & auth / HTTP endpoints / automated tasks)
├── src/                                # Vue 3 frontend
│   ├── components/ composables/        # UI components; business composables (desktop command aggregation / plugin management / window orchestration)
│   ├── stores/ views/                  # Pinia state; pages (terminal / devices / sessions / plugin list·detail·config / settings)
│   ├── plugin/                         # Frontend plugin system (loader / registry / permissions / context / events / shared-module runtime / UI host)
│   ├── shell/                          # Desktop shell layer
│   ├── utils/ locales/ router/ dev/    # Utilities / i18n (zh-CN + en) / routing / dev assets
│   └── __tests__/                      # Frontend tests
├── src-tauri/                          # Rust backend (Tokio)
│   ├── resources/                      # Bundled resources (config.properties + built-in WASM app artifacts)
│   └── src/                            # Flat per-domain layout; plugin/<domain>.rs = host plugin capability face (bindgen + per-domain Host impls)
│       ├── commands.rs                 # Tauri command layer (host direct-call shell; business surface belongs to plugin command faces)
│       ├── plugin/                     # End-specific WIT bindings + Host impls (fs / platform / notify / peer / ws / auth, etc.)
│       ├── server/                     # Server host shell (composition.rs composition root + port wiring + boundary locks)
│       ├── system/                     # DI / errors / lifecycle / logging; config/opener/process are pub use shims
│       ├── utils/                      # auth/ (adjudication bridge source of truth) + session_gateway.rs (single session funnel)
│       ├── lib.rs                      # Library entry (assembly; wasm_core/db/enums are pub use shims)
│       └── main.rs                     # Binary entry (panic hook)
├── e2e/                                # E2E tests (wdio)
└── docs/                               # Project docs (code-map.md, linux-build.md, etc.)
```

> Root `packages/bedcode-*`: mechanism kernel `bedcode-host-kit`, plugin mechanism whole-core `bedcode-wasm-core`
> (single crate, single form: WASM runtime / storage / database / authorization and the `core.wit` source of
> truth all live there), capability-domain and transport engines (server-* / crypto / discovery / pty /
> ws-client / notify / peer-net / link-crypto / host-api-core / file-transfer-core / headless-host-probe).
> Index: [`../packages/README.md`](../packages/README.md).

## Quick Start

### Prerequisites

- [Node.js](https://nodejs.org/) ≥ 18, [Rust](https://www.rust-lang.org/tools/install) ≥ 1.95 (wasmtime 48 MSRV)
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

Desktop plugins are built on the **wasmtime 48 runtime (WASM Component Model)**: apps compile from Rust / TypeScript into WASM components loaded in a host sandbox, with the contract defined by WIT (`bedcode:plugin@0.2.0`). Apps can observe and extend host session behavior:

- **WASM sandbox runtime** — resource-bounded, memory-isolated; a crashing app never takes down the host
- **Dynamic loading** — scanned from `resources/plugins/desktop/<id>/plugin.json` at runtime, no host recompile needed
- **Host capability bridge** — engine capabilities (process/network/storage/security/communication) accessed through the WIT ABI and `host-*` primitives, split per capability domain; the shared mechanism lives in root `packages/bedcode-*`, end-specific domains in each host's `src-tauri/src/plugin/`, behind a unified permission gate
- **Plugin-to-plugin calls** — callable APIs must be declared in the manifest `api` field, invoked across plugins over JSON-RPC 2.0; a topic-based message bus provides publish/subscribe
- **Permission control** — apps declare required permissions; the frontend fails fast and the host makes the final ruling. File system access goes through three layers (path whitelist → app whitelist → approval dialog)
- **Hooks integration** — project-level hooks auto-configured at session start, pushing task status (idle / in_progress / asking / completed / interrupted) via HTTP API
- **Session ID binding** — the terminal-session app injects `BEDCODE_SESSION_ID` via `host-pty.spawn` (argv / env are computed by the app; the engine never wraps them), binding Agent CLI sessions to BedCode sessions

### Built-in WASM Apps

| App                   | Version    | Description                                                                                         |
| --------------------- | ---------- | --------------------------------------------------------------------------------------------------- |
| **AI Chatbox**        | 1.0.0-beta | AI chat: any OpenAI-compatible provider (OpenAI / Anthropic / DeepSeek / Qwen), streaming conversations, multi-session management, JSONL conversation logs |
| **Terminal Session**  | 1.0.0-beta | Terminal session center: agent task queue & auto-approval (Claude Code / pi / opencode / Codex), task status sync, queue scheduling, preset tasks, scheduled jobs, pairing & trust, session lifecycle orchestration — the session source of truth |
| **File Transfer**     | 1.0.0-beta | LAN file transfer (over peer-network direct links): online peer discovery & switching, remote directory browsing, concurrent transfers (pause / resume / resume-after-break / retry), local directory mounting |
| **Agent Hub**         | 0.1.0      | Agent CLI management console: environment detection with one-click install / uninstall, skills management, unified provider config, usage stats & session log parsing |

To build your own WASM app, use [`@binblink/bedcode-plugin-sdk-desktop`](packages/plugin-sdk-desktop/README_en.md) (TS SDK + Rust `bedcode-plugin-api` crate); full guide in [wasm-app-dev-desktop.md](wasm-app-dev-desktop.md).

## Related Docs

- Root [README](../README.md) — project overview & security model
- [wasm-app-dev-desktop.md](wasm-app-dev-desktop.md) — desktop WASM app development guide (from scratch to on-device verification)
- [docs/knowledge/wasm-app-dev-guide.md](docs/knowledge/wasm-app-dev-guide.md) — manifest / PluginContext API / extension point reference
- [`../docs/knowledge/wasm-app-development-checklist.md`](../docs/knowledge/wasm-app-development-checklist.md) — development red lines (permissions / storage / events / auth center / HTTP surface)
- [docs/code-map.md](docs/code-map.md) — code structure index (incl. scripts)
- [docs/linux-build.md](docs/linux-build.md) — Linux build dependencies & cross-compilation
- [scripts/README.md](scripts/README.md) — build script docs

## License

MIT — see [LICENSE](../LICENSE).
