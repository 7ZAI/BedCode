# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.11] - 2026-07-07

### Added

#### Mobile
- AI 提示词标准 4 要素模板插入按钮（目标/上下文/约束/完成条件）
- 快捷栏 RTL 布局，最常用项渲染在右侧方便拇指操作
- 快捷键排序改为频次降序，配合 RTL 布局
- 测试终端会话（DEV 模式）用于开发调试

### Changed

- 项目描述从强调 Claude Code 远程控制调整为本地远程终端应用
- 插件页面入口、路由、组件初始化均已注释（暂未上线）
- Token 校验/生成逻辑已禁用（ensure_token 直接跳过，不再弹出 toast）
- 项目 hooks 配置不再比对 token 一致性

### Fixed

- 键盘避让与认证后订阅失败问题
- 连接状态不一致导致重连失败
- 终端键盘避让空白
- 工具栏 i18n 缺失
- 快捷键面板布局与自适应能力优化

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
| 1.1.11 | 2026-07-07 | 移动端 AI 模板与快捷栏优化，插件入口注释，token 校验禁用 |
| 1.1.0 | 2026-07-05 | Plugin system, mobile terminal refactor, i18n, Actix Web server |
| 1.0.0 | 2026-06-30 | Multi-project monorepo, stable desktop + mobile release |
| 0.1.0 | 2026-04-30 | Initial release with core features |
