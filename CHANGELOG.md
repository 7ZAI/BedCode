# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.1.1] - 2026-09-18

> Features · Platform & Infrastructure · Improvements · Fixes · Security · Tests & Quality · Documentation

### Features

#### Terminal Output Pipeline — TB v3 Byte Stream & Ring Buffer (desktop + mobile)
- Desktop PTY output rewritten to a byte-continuous pipeline (TB v3): bytes-block queue, v3 frames, byte cursor with dual-speed propagation; slow consumers drain from a ring buffer with per-subscriber pull cursors, so backpressure never blocks the producer; `session_output` link debug statistics (produce/ack throttling instrumentation)
- Desktop: one-shot history endpoint `GET /api/sessions/{id}/history`; frontend terminal output stream adapted to the v3 byte cursor over both WS and Channel paths
- Mobile: terminal output link moved into the Rust backend (`terminal_link` + frontend wiring); legacy TB v2 frames and the old `ws_event` channel terminal code removed
- Mobile: segment-2 backpressure reworked to ack-driven resend; output frames moved to a page-level Channel
- Desktop: legacy WS loopback terminal link removed (`local_token` / loopback WS / old output stream)

#### Desktop Plugin Management — Zip Install & Uninstall
- Uninstall is available on every plugin detail page regardless of source (built-in / file scan / zip install); it requires the plugin to be **disabled** (the button stays disabled with a "deactivate first" hint while the plugin runs) and clears everything the plugin owns: its install directory (taken from `extension_path`, including the private `plugin.db` next to it), key-value storage, persisted filesystem grants (`fs_granted_paths` / `preauth_paths`), the persisted approval record (`__system__`-scoped `plugin_approvals` entry: approved permissions + content hash pinning), persisted activation state, cached DB connection and runtime throttling records. Built-in plugins live in the resource directory shipped with the app: removal fails loudly on a read-only install and the bundled copy reappears after the next build/update
- Install plugins from a local zip package (unpacked into the user plugin directory, source `user-installed`)
- Plugin list layout: the load-plugin button (primary color) moved to the right of the "Disabled" section title and stays reachable when no plugin is disabled; refresh moved to the far right of the toolbar
- Uninstall integrity: the persisted approval record is revoked on uninstall and the plugin entry is located via its own loader path; orphan residual directories are cleaned up so reinstall after uninstall no longer stalls on disk dedup
- `fs_auth` grant granularity refined to a three-state model (directory / file / parent directory)

#### Mobile HTTP — Rust Proxy & Fail-Closed Egress Policy
- Mobile HTTP consolidated into a single Rust proxy with a fail-closed three-layer Egress policy; all frontend HTTP (`useHttpApi` / UpdateChecker / LinkEncryption) now goes through it and the `@tauri-apps/plugin-http` JS dependency was removed
- Egress authorization dialog plus an authorization viewer/revoker in settings
- Redirect revalidation guards against SSRF on both platforms (mobile Egress and desktop plugin HTTP)
- SDK: `link-crypto` gained HTTP key derivation; SDK manifest `preauthUrls` declarations

#### file-transfer Plugin — Explicit Pause/Resume & Concurrency
- Host-side concurrency gate plus explicit pause/resume, identical on both platforms; plugin `paused` semantics with frontend pause/continue/resume-all controls and concurrency settings
- Explicit pause/resume wire protocol with data-plane gating (dual-platform, built on `peer-net`)
- Desktop receive queue: per-card transfer rate / ETA, clear-history double-confirmation, panel aggregate rate
- Mobile task cards show a theme-colored active progress bar (paused/queued no longer misleading gray)
- Pause/resume/cancel state kept in sync across platforms (wire + data-plane gating + single-transaction persist)

#### Mobile Terminal Experience
- Terminal UX bundle: QR scan integration, first-run guide, keyboard avoidance, input bar, themes, help docs
- Font-size range widened; session count limit added
- TUI mouse-report sniffing supports multi-parameter DECSET and real report switches
- TerminalView decomposed into an orchestration layer with domain modules (row-tail static clipping, grid write funnel)
- `peer_pick_folder` command removed (SAF tree URIs now unified for folder picking)

#### Plugin SDK
- `host-peer` transfer-control primitives (pause / resume / resume-all) added to the ABI contract; `plugin-component-test` fixture ABI bumped to 9
- `MarkdownEditor` component: `marked` rendering with raw-HTML escaping and syntax highlighting
- Both-platform SDKs packaged as GitHub Release attachments (npm tarball / crate / aggregated zip + SHA256SUMS)

### Platform & Infrastructure

- Adaptive build wrapper `adaptive-run` + `build-profile`: samples CPU load / available memory / swap pressure and injects compile parallelism (`CARGO_BUILD_JOBS`, Gradle `workers.max`, `NODE_OPTIONS` heap); usage in `docs/commands.md`
- CI release pipeline packages every plugin into zip dists and generates a bilingual release body
- Mobile dev log defaults to verbose so logcat includes Rust `debug!` output
- App name unified to **BedCode** on mobile; static splash animation disabled, Android launch screen switched to a solid color
- pi session archive script (archive threshold default 15 → 10 days); doc-tracking policy: `docs/` tracked on every branch, protected paths reduced to protected config files

### Improvements

#### Desktop
- Giant frontend components split into domain modules: TerminalPreview → orchestration layer + terminal composables, SettingsView → grouped settings sub-components, useDesktopCommands → domain command modules (session / device / settings / events)

#### Mobile
- Dev logging filters non-business noise with identical rules for console and file output

### Fixes

- **Terminal**: output-manager registration order (registered before PTY start, rolled back on start failure); subscription activation race; pty history/realtime splice race and re-entry cursor semantics; duplicate terminal entry now invalidates the previous segment-2 push channel; terminal display area / input bar spacing; `terminal_link` silent-error points now log; non-UTF-8 special-key bytes discarded with a warning
- **Desktop plugins**: user-installed rust-ts plugins run the full guest lifecycle; plugin concurrency-gate pulse failures no longer silently swallowed (warn log); right-click native context menu disabled in release builds; DEB rename regex escaping fixed in `tauri-build.js`
- **file-transfer**: pause/resume/cancel state desync between platforms; issue 16/17 defects (pause-resume/rate calculation, silent frontend failure)
- **Mobile**: `terminalRowClip` non-null assertion narrowing fix (vue-tsc); `http_auth_flow` global-token test serialization gate (parallel flake); touch-scroll cell-height fallback recalculation; running-state broadcast no longer resets a live session's buffer

### Security

- Redirect revalidation on both platforms (desktop plugin HTTP `redirect_decision`, mobile Egress) closes SSRF paths
- Mobile HTTP now fail-closed under the three-layer Egress policy

### Tests & Quality

- Desktop unit-test audit: 32 tickets landed (lib baseline 615 → 784)
- Mobile test audit: 3 P0 lanes fully landed, P1 partially
- New tests: segment-2 channel frame parsing, file-transfer clear-history double-confirmation dialog (driven + failure/cancel negatives), task-card active-state theming
- `unit-test-discipline` skill added to AGENTS.md task routing

### Documentation

- `docs/commands.md`: adaptive-build section (`adaptive-run` usage) and build-profile dynamic override guidance
- pty output pipeline TB v3 architecture docs (`docs/knowledge/pty-output-pipeline.md`) + pty-byte-history task records
- Mobile terminal link code-map additions with TB v3 annotations; mobile-ws-rust design/review/leftover records
- Architecture diagrams migrated to `docs/diagrams` (archify deliverables, README link)
- Feature-branch isolation spec (task-scheduler / OCR / code-viewer) and scratch records for file-transfer concurrency & pause/resume
- Obsolete implementation-plans and skills-course learning docs removed

## [2.1.0] - 2026-09-11

> Features · Platform & Infrastructure · Improvements · Fixes · Security · Tests & Quality · Documentation

### Features

#### Peer Network — Direct P2P Link (`packages/peer-net`)
- New shared Rust crate used by both hosts: node identity, self-signed certificate with public-key binding, TLS 1.3 mTLS direct connection, trust store, dedicated mDNS discovery, shared-directory browsing and batch-resumable transfer engine
- Peer node power bus: central node lifecycle with plugin lifecycle wiring and inbound-connection event bridging on both hosts
- Dedicated `_bedcode-peer` mDNS service with TTL-based online cache and an injection seam for testable discovery
- TCP keepalive liveness probe, periodic mDNS re-announce/requery for reliable mutual discovery, first-frame timeout removed
- First-connect confirmation gate, revoke-and-re-confirm flow, node lifecycle gate eliminating TOCTOU races

#### Peer Network — Crypto Core (`packages/link-crypto`)
- New shared crypto crate: outbound-key miss observability, cache capacity guardrail, GET metric correction, query path normalization (cross-platform)
- Mobile pin write verification and fingerprint cleanup; HTTP GET/HEAD empty-body negotiation; `X-BedCode-Crypto` response marker
- HTTP-based biometric credential bind/unbind (migrated off WebSocket)

#### file-transfer Plugin — Peer Rewrite
- Old LAN file service fully retired; the peer stack takes over file transfer end to end
- V2 three-section main view (mobile), business logic self-held by the plugin (Phase 3, both platforms)
- Device discovery refresh, endpoint memory, history snapshot, first-connect confirmation timeout
- Shared-directory multi-select on both platforms; desktop settings shows full paths
- Endpoint cleanup/validation, device singleton, confirmation-timeout point settlement, root cache reset
- Transfer panel terminal-state archiving: progress / reason / open-in-folder / double-sided bookkeeping (both sides now record the same transfer)
- SAF `content://` URI intermediate copy fallback for partitioned storage (`EACCES`)

#### Auth & Transport Re-architecture
- HTTP biometric authentication plus event-channel primitives (desktop)
- WebSocket first-message JWT authentication and a dedicated `/ws/event` event channel
- Mobile persistent event WebSocket: connect-after-auth with automatic self-heal on disconnect
- Mobile HTTP auth client; direct terminal WebSocket (`useTerminalSocket` + state-machine store)

#### Terminal Output Pipeline
- Byte-stream PTY pipeline rewritten: sequence-based output queue with snapshot subscription (byte offset contract abolished)
- Per-session terminal routing with TB v2 binary frames on the remote channel; local channel migrated to TB v2 with snapshot re-subscription
- Legacy broadcast compatibility channel and dead code removed
- Mobile terminal write pipeline yields the main thread: 128 KB chunked yield with a `flushing` re-entrancy guard
- `get_terminal_ws_info` replaces the removed mobile output link; ack/yield/history caching on mobile

#### auto-task Plugin
- Task history status filter moved from chips to a dropdown Select (defaults to All)
- Terminal input submit character unified to the `\r` Enter byte across agents

#### ai-chatbox Plugin
- Vendor rate-limit automatic retry on both platforms

### Platform & Infrastructure

- Version bumped to **2.1.0** across both `package.json` and Tauri manifests; `.deb` build support added
- CI: Linux build target added alongside Windows/macOS; cargo compilation resource limits
- CI: two-platform Rust + vitest regression gate as layered independent jobs, blocking on merge to `master` / `uat`
- CI: `release.yml` working-directory resolved relative to repo root; SDK build step switched from `pnpm filter` syntax to working-directory mode
- CI: verify-latest.json switched to draft-release asset queries; signing-key step now injects `TAURI_SIGNING_PRIVATE_KEY`
- **E2E infrastructure (desktop)**: WebdriverIO + `tauri-plugin-wdio` (debug-only isolation) + external `tauri-driver`; smoke assertions verify the execute/IPC chain. Legacy `@playwright/test` dependency removed
- **Plugin SDK renamed and published**: `@binblink/plugin-sdk-*` → `@binblink/bedcode-plugin-sdk-*` (desktop + mobile), v0.1.1 on npm and crates.io; SDK package files slimmed (dev-shell excludes `node_modules` and build artifacts) with `.npmignore`
- Shared Rust crates relocated to `packages/` (`peer-net`, `link-crypto`)
- Frontend package manager migrated npm → pnpm across the monorepo, each project keeping its own lockfile
- `dev-run` process-group recycling (signal to the whole group, not just the parent) plus plugin-watch grandchild self-cleanup
- `doc-tracking.sh` line-ending fix restoring pre-commit protection; `PROTECTED_PATHS` extended to cover both `bedcode-desktop/docs` and `bedcode-mobile/docs`
- AI tool configuration committed; unified `.agents/skills/` for pi / OpenCode / Codex / Claude Code
- Session artifacts excluded from version control (pi-lens backups, compactions)
- Rust MSRV 1.94 with wasmtime 47; plugin WASM built with `--release` keeping the names section

### Improvements

#### Desktop
- Terminal: xterm ghost-image convergence — backpressure hysteresis, Channel transport, renderer decision, ordered write queue
- Terminal Linux rendering optimization; WebKitGTK IME protection refactored to a single attach point
- Login PTY launched with `-lic` so it inherits the user PATH on Linux
- WebView2 black screen after Windows screen-saver / sleep wake-up now self-heals
- Startup splash footer version read from the single source of truth; window size and startup background color adjusted
- Plugin list description font size 12 px → 11 px

#### Mobile
- Settings split into `views/settings/` subviews: Appearance, Authentication, Connection, Notification, About (main view reduced from ~1130 to ~400 lines)
- Android launch theme simplified — system splash customization removed; light/dark splash follow with a unified startup window background
- SplashScreen temporarily offlined (kept commented, trivially reversible)
- Gesture swipe no longer fights the navigation-bar switch animation
- Keyboard collapse now exits the terminal input edit state
- Notification background semantics consolidated; dead code removed; navigation icons aligned

#### Logging & Observability
- **Frontend**: loglevel adopted as the unified frontend logging framework, replacing direct `console` calls; dev-only forwarding to `frontend.*.log` (desktop) and logcat (mobile), stripped in release
- **Desktop host**: full HTTP request-path logging, JSON span chain, startup bootstrap channel, structured-field migration of legacy log messages
- **Desktop plugins (WASM)**: `wasm_backtrace_max_frames(32)` enabled so traps carry a WASM call stack; trap logged at the host; debug mode (`BEDCODE_PLUGIN_DEBUG=1`) builds plugins with DWARF + line-number resolution and a 32× fuel budget; per-plugin levels via `BEDCODE_PLUGIN_LOG=id=level`
- **Mobile**: release log level converged, dev log retention governed (14-day rolling cleanup)
- Host error-handling hardened: self-describing `io` error wrapping, span instrumentation, spawn error boundaries

### Fixes

- **Peer network**: shutdown busy-loop, keepalive trace logging, directory `size` contract normalized across platforms; node lifecycle TOCTOU gate; connection-state resend now carries `deviceName`
- **File transfer**: shared-directory multi-select on mobile, single mDNS daemon restoring mutual discovery, connection awareness; desktop enable deadlock changed to "enable-first" with the pre-approval dialog raised before loading; plugin dynamic UI now strictly follows enabled state with symmetric lifecycle teardown and mDNS multicast-lock de-dupe
- **Terminal**: opencode TUI scroll ghost — rAF-merged writes enabled by default plus a stop-of-scroll repaint; mobile `touchmove` cancelable guard; zoom-compensation comments reconciled after the zoom removal
- **Mobile**: `SafPickerPlugin` constructor restored to the exact `android.app.Activity` (JNI signature lookup was returning `null`, causing an NPE); adb fd0 shim self-heals when platform-tools is upgraded; plugin auth dialog no longer co-appears with the loading overlay; real-device release-package integration defects
- **Desktop**: VerifyCode pairing success now includes `device_name`; splash caret residue cleaned up; global notification de-duplication
- **Build / dev**: `plugin-build` `JSON.parse` / `execSync` now carry error context; post-V2 file-transfer component import paths corrected

### Security

- **Peer identity**: Ed25519 node identity generated on first run (purely random, atomically persisted), bound into an rcgen self-signed certificate and verified against `CertificateVerify` via ring — a forged identity cannot pass the handshake
- **Trust store**: first-connect confirmation gate with revoke-and-re-confirm, so an initially trusted peer can be revoked and challenged again
- **Transport**: first-message JWT authentication on the WebSocket plus a dedicated authenticated `/ws/event` channel; mobile biometric credential bind/unbind moved off WebSocket onto HTTP
- **Carried forward from 2.0.0**: plugin identity verification and permission approval, biometric auth chain hardening, JWT gateway with local bypass for agent hooks

### Tests & Quality

- **Desktop Rust**: 583 tests; integration coverage for HTTP contract, WS pairing auth, PTY session chain, multi-client broadcast + graceful shutdown, and contract-fixture drift alignment
- **Mobile Rust**: 251 tests; L1/L2 integration suite landed together with a disconnect-reconnect bug fix
- **peer-net**: 102 tests including a dual-node harness (discovery injection, shared directories, transfer session)
- **Frontend**: desktop 569 tests across 61 files; mobile 360 tests across 42 files (views / stores / composables / integration)
- **Plugin SDK**: desktop 5 → 85 contract tests; mobile 2 → 79
- E2E smoke suite validating the WebdriverIO → tauri-driver → IPC chain
- CI regression gate blocks merges to `master` / `uat` on any failing layer

### Documentation

- README rewritten for 2.1.0: version badges (incl. wasmtime 47), Linux platform added, Claude Code → Pi/Opencode wording, outdated screenshots removed; mirrored in `README_en.md`
- Per-platform code maps (`bedcode-desktop/docs/code-map.md`, `bedcode-mobile/docs/code-map.md`) as the directory-level lookup index
- Plugin architecture diagrams (Archify) for ai-chatbox, auto-task and file-transfer on both platforms
- Knowledge base: `pty-output-pipeline`, `mobile-terminal-optimization-reference`, `release-workflow`, `sdk-publish`, `github-actions-setup`, `build-process`, `feature-branch-isolation`
- ADRs for mobile file-service retirement (peer stack takeover) and code-viewer design
- auto-task DAG orchestration spec; xterm transparent-mode ghost convergence spec and tickets; desktop-e2e-webdriver spec and tickets; plugin-WASM logging spec
- Feature-branch isolation applied: desktop task-scheduler plugin → `feature/task-scheduler`, mobile OCR plugin → `feature/ocr-plugin`, code-viewer design → `feature/code-viewer`

---

## [2.0.0] - 2026-08-16

### Added

#### Plugin System — WASM Platform
- Migrated plugin runtime to WASM Component Model (wasmtime); cdylib dynamic loading removed, Component form is the only supported form
- ABI evolution v2 → v6: typed host API, param-bound SQL, memory reclaim, out_ptr, signature verification, plugin status reporting, InputSubmitted observation extension point
- Runtime hardening: epoch interruption + resource limits, fuel watchdog, trap auto-reload recovery, AOT cache, wasmtime 47
- Security: plugin identity verification and permission approval (anti-spoofing)
- Toolchain: bedcode-plugin CLI (create / build / dev / validate / doctor / manifest), Dev Shell browser dev environment (both platforms, `--host` for phone access), manifest-gen
- Lifecycle: dynamic activate/deactivate with state persistence, hot reload, install/uninstall, loading overlays
- Capabilities: per-plugin independent SQLite database, message bus for inter-plugin communication, host_notify, fs_auth batch directory authorization, file service mount/transfer, WSL filesystem bridge
- Shared UI component library in SDK (Rust + TS, both platforms)

#### auto-task Plugin
- Multi-agent support: Claude Code / pi / opencode / Codex (registry-driven agent adaptation architecture)
- Task queue with scheduling, auto-execution, auto-answer, preset tasks (one-shot), scheduled jobs state machine, task history with stats, filtering and retry
- Mobile toolbox: task history / scheduled jobs panels
- TUI-agent first-dispatch fallback (15s grace) and per-agent terminal hooks

#### file-transfer Plugin
- LAN file transfer plugin (WASM core + desktop/mobile UI + bundled distribution)
- Bidirectional transfer: send to phone (upload direction), receive with policy approval, async batch approval, transfer history, resume re-queue, dedicated downloads_dir
- Android SAF streaming: shared directories, SAF picker with pfd strong reference, full-file-access authorization guidance

#### ai-chatbox Plugin
- Pure AI chat rewrite (both platforms): multi-vendor providers, streaming SSE parsing, thinking mode, Shiki highlighting, code rendering config, JSONL persistence

#### Mobile
- Plugin system enabled: dynamic routes, plugin management page, toolbox entries, plugin nav tabs
- Android SAF file/directory pickers (startActivityForResult)
- Biometric authentication: keys, auth settings page, challenge-response, device identity persistence
- Terminal: session preload, cursor-based output subscription, TUI scroll compatibility (SGR), Agent CLI command presets, 16-key default shortcuts
- Accent color palette (shared source with desktop)

#### Desktop
- Terminal PTY replay and history playback (Rust-side recovery of output lost while window closed)
- Byte-stream PTY output pipeline: byte offset contract, cursor incremental re-subscription
- Four theme palettes (forest / ocean / sunset / violet)
- Biometric challenge-response and connection history
- SystemInfo collection and device name broadcast, generic crypto utility module

### Changed

- Plugin backend is now fully WASM-based (cdylib removed); Rust MSRV raised to 1.94 (wasmtime 47)
- Toast migrated to vue-sonner (both platforms)
- Desktop UI rebuilt on Warm Workbench design; mobile UI unified to group-card style; font-size tokenized
- Terminal size control is remote-first with mobile pause/resume subscription
- Output pipeline migrated to local WS single channel (desktop), cursor-based subscription replaces 2MB frontend ring buffer (mobile)
- Build: rust-lld linker, thin LTO, version bump script, installer release-suffix rename, CI builds plugin artifacts with wasm32 target + Windows signature thumbprint injection
- Skills unified under `.agents/skills/` for pi / OpenCode / Codex / Claude Code sharing

### Fixed

- Terminal: output continuity (replay storms fixed by cursor incremental re-subscription), long-run page crash, frame loss (async plugin callbacks, drain/reset), reconnection size sync
- File transfer: name conflicts (409 + rejection reason), notification storms, task races, Windows path separators, explorer reveal, .part residue cleanup
- Plugins: multi-plugin PluginContext corruption breaking i18n, WASM trap recovery, fuel exhaustion traps, loader handle release, WSL subprocess timeout
- Mobile: heartbeat blocking_write panic, Activity-recreation picker failure (EBADF), subscription leaks, reconnect state inconsistency
- Desktop: settings save loop (content snapshot compare), port input, session naming regression after delete

### Security

- Plugin identity verification and permission approval (anti-spoofing)
- Biometric auth chain hardening: IPC serialization, DER parsing, binding guard self-check
- JWT gateway with local bypass for agent hooks (token removed from hook scripts)

### Tests

- Frontend +175, desktop Rust +204, mobile +116, SDK contract tests (desktop 5→85, mobile 2→79), file-transfer host unit tests

---

## [1.1.0] - 2026-07-05

### Added

#### Plugin System
- Rust plugin API crate with cdylib dynamic loading
- Plugin manifest types and permission system
- PluginHost and API bridge Tauri commands
- Extension point registry for UI slots
- Plugin loader, storage, and AppError::Plugin variant
- Complete frontend plugin system with PluginRegistry
- PluginConfigView page with auto-generated config form
- PluginsView page with list, toggle, and expandable detail
- usePluginManager composable
- PluginTerminalToolbar and PluginTitleBarItems rendering components
- registerTerminalToolbarItem and registerTitleBarItem proxy APIs
- AI chatbox plugin rewritten as independent cdylib plugin
- Resource-dir plugin loading and API security
- Plugin sidebar/toolbox view routes and navigation
- Plugin page i18n keys

#### Mobile
- Buffer-Only terminal architecture for performance
- mDNS service discovery and advertisement
- Per-session task notification system
- Auto-execute task engine and terminal integration
- WebSocket heartbeat keepalive and improved reconnection
- CodeExplorerView with sidebar + code display layout
- Diff rendering support with line-level coloring
- FileViewerModal with diff mode
- PresetTaskCard component with type badge, status, and action menu
- usePresetTasks composable with localStorage persistence
- Shortcut config modal and infinite carousel for terminal input bar
- Loading overlays and UX improvements
- Quick bar button colors consistent with shortcut panel
- Smooth open/close animations for all modal popups
- tauri-plugin-http integration with wildcard scope permissions
- WakeLock in ForegroundService

#### Desktop
- Advanced network config for Actix Web server
- Server management page with config migration to properties format
- Fingerprint tracking for device identification
- Port availability check on startup
- Git branch switcher in FileSidebar header
- Power management features
- Claude Code hooks moved from global to project-scoped configuration
- Globalized hooks with session ID binding

#### Server / Backend
- Actix Web HTTP server alongside existing WsServer
- Actix Web HTTP controllers, DTOs, and middleware
- Actix WS actor for terminal I/O
- WS metrics and configuration endpoints
- HTTP+WS dual protocol support
- File content/diff-tree HTTP API
- Terminal output buffer to reduce WebSocket message count
- Current line input tracking and plugin event response

#### i18n
- vue-i18n infrastructure with language persistence
- i18n for all views, components, composables + error code system
- i18n settings pages with language switcher UI
- i18n navigation, layout, and shared components
- i18n terminal view and input bar
- i18n BottomSheet and PairingInput components
- i18n desktop SessionManager, SessionsConfig, and component files

#### Code Viewer
- Multi-theme support in useCodeHighlight
- useCodeViewerStore for code viewer settings
- CodeViewerSettingsModal component
- Code viewer settings integration in FileViewerModal and CodeExplorerView

### Changed

- Refactored plugin to task-status manager with KeyCombo system and auto-approve mode
- Replaced IPC subprocess with in-process Actix Web
- Merged event/ into events/, fixed IPC runtime
- Removed desktop/ and shared/ layers, flattened Rust modules by domain
- Mobile module structure flattened and Android package name migrated
- Mobile: removed auto-executor, extracted FileExplorer, added light code themes
- Mobile: TerminalView refactored and preset task simplified
- Desktop: reorganized Rust modules, added mDNS, redesigned UI with design tokens
- Desktop: server reset defaults + UI polish
- Mobile notification migration
- Task picker refactored

### Fixed

- Mobile connection error handling and state consistency
- Path separators normalized to forward slash
- Sidebar animation improvements
- Reconnection handling and special key modifiers
- Mobile terminal swipe-back issue after returning to session list
- Plugin state type handling and table header
- PluginViewHost props routing
- IPC reader implementation and sysinfo metrics
- Button symbol cleanup

---

## [1.0.0] - 2026-06-30

### Added

#### Core Architecture
- Multi-project monorepo: bedcode-desktop + bedcode-mobile as independent projects
- WebSocket + HTTP dual-protocol communication between desktop and mobile
- X25519 key exchange for device pairing
- AES-GCM encryption for all communication
- Secure storage using system keychain/secret service
- 6-digit pairing code authentication with 60-second expiry

#### Desktop
- Session management interface (create, edit, delete sessions)
- Device pairing interface with QR code display
- Terminal preview with xterm.js integration
- System tray with quick actions
- Settings page for network and appearance configuration
- PTY (Pseudo Terminal) management for Windows and WSL2
- Session configuration management with SQLite persistence
- WebSocket server for mobile communication
- mDNS device discovery service
- Tmux session integration

#### Mobile
- Device discovery and pairing flow
- Terminal output display with enhanced/raw mode toggle
- Input bar with special keys (Tab, Ctrl+C, Esc, etc.)
- Quick actions grid with customizable commands
- History records with search functionality
- Settings page with notification preferences

#### Backend (Rust)
- Database layer with SQLite (pairings, sessions, messages, quick actions)
- PTY process management with portable-pty
- WSL2 support with path conversion
- WebSocket message protocol
- ANSI escape sequence parser
- Markdown block extractor
- Output parser with waiting input detection
- Notification service with quiet hours support

### Security
- All WebSocket communication encrypted with WSS
- Pairing codes expire after 60 seconds
- Device fingerprints verified on connection

---

## [0.1.0] - 2026-04-30

### Added

#### Core Features
- Initial project structure with Tauri 2.0 + Vue 3 + TypeScript
- PTY (Pseudo Terminal) management for Windows and WSL2
- Session configuration management with SQLite persistence
- WebSocket server for mobile communication
- mDNS device discovery service
- 6-digit pairing code authentication

#### Desktop UI
- Session management interface (create, edit, delete sessions)
- Device pairing interface with QR code display
- Terminal preview with xterm.js integration
- System tray with quick actions
- Settings page for network and appearance configuration

#### Mobile UI
- Device discovery and pairing flow
- Terminal output display with enhanced/raw mode toggle
- Input bar with special keys (Tab, Ctrl+C, Esc, etc.)
- Quick actions grid with customizable commands
- History records with search functionality
- Settings page with notification preferences

#### Backend (Rust)
- Database layer with SQLite (pairings, sessions, messages, quick actions)
- PTY process management with portable-pty
- WSL2 support with path conversion
- Tmux session integration
- WebSocket message protocol
- ANSI escape sequence parser
- Markdown block extractor
- Output parser with waiting input detection
- Notification service with quiet hours support

#### Security
- X25519 key exchange for device pairing
- AES-GCM encryption for communication
- Secure storage using system keychain/secret service

### Changed
- N/A (Initial release)

### Fixed
- N/A (Initial release)

### Security
- All WebSocket communication encrypted with WSS
- Pairing codes expire after 60 seconds
- Device fingerprints verified on connection

---

## Version History

| Version | Date | Description |
|---------|------|-------------|
| 2.1.1 | 2026-09-18 | Terminal output pipeline TB v3 + ring buffer, zip plugin install/uninstall, mobile HTTP Rust proxy + fail-closed Egress, file-transfer pause/resume, mobile terminal UX, SDK host-peer primitives, adaptive build wrapper |
| 2.1.0 | 2026-09-11 | Peer network (`packages/peer-net` + `link-crypto`) with TLS 1.3 mTLS and trust store, file-transfer peer rewrite, JWT + persistent event WebSocket, terminal output pipeline rewrite, unified frontend logging, WASM trap logging, E2E + CI gate, SDK rename to `@binblink/bedcode-plugin-sdk-*` |
| 2.0.0 | 2026-08-16 | WASM Component Model plugin platform, auto-task / file-transfer / ai-chatbox plugins, mobile plugin system, biometric auth, UI redesign |
| 1.1.0 | 2026-07-05 | Plugin system, mobile terminal refactor, i18n, Actix Web server |
| 1.0.0 | 2026-06-30 | Multi-project monorepo, stable desktop + mobile release |
| 0.1.0 | 2026-04-30 | Initial release with core features |
