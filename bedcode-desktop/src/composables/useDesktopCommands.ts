//! Desktop Commands - Rust 后端命令封装（聚合层）
//!
//! 宿主页面仍直接调用的命令封装，按领域拆分在 `src/composables/commands/` 下，
//! 本文件只做聚合 re-export（历史 `useDesktopCommands()` composable 已无调用方，
//! 随 2026-09-21 命令面收敛一并删除）：
//! - settingsCommands：设置 / 系统
//!
//! 已删除（业务域随插件下沉）：
//! - `commands/sessionCommands`（票 08 会话命令面注销）：`list_sessions` /
//!   `get_session` / `resize_session` / `write_to_session` / `send_special_key`
//!   五条宿主命令连壳删除，会话数据与输入改走插件命令通道
//!   （`session.list` / `session.get` / `session.action.resize` / `session.input`）；
//! - `commands/deviceCommands`（配对 / QR / 连接历史 / 快捷指令）与
//!   `commands/eventListeners`（设备连接事件监听）——插件经宿主 api 面与自身命令面
//!   承载这些能力，宿主前端不再持有封装；WSL 探测（`listWslDistributions` /
//!   `isWslAvailable`）与 `getLocalIpAddresses` 随宿主命令面注销一并删除（产品面归
//!   `com.bedcode.terminal-session` 的 `session.environment.wsl-distros` /
//!   `session.network.info`）。

export * from './commands/settingsCommands'
