# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
| 2.0.0 | 2026-08-16 | WASM Component Model plugin platform, auto-task / file-transfer / ai-chatbox plugins, mobile plugin system, biometric auth, UI redesign |
| 1.1.0 | 2026-07-05 | Plugin system, mobile terminal refactor, i18n, Actix Web server |
| 1.0.0 | 2026-06-30 | Multi-project monorepo, stable desktop + mobile release |
| 0.1.0 | 2026-04-30 | Initial release with core features |
