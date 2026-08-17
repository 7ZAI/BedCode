# 09 — 移动端输出链路拆除 + 终端 WS 信息命令

**What to build:** 移动端 Rust 删除 PTY 输出转发链路：`handler/terminal.rs` 的 Output 分支、`MobileEvent::Output`、`EventForwarder` 的 `ws_output` emit、`commands/session.rs` 的 `ws_subscribe_session`/`ws_leave_session`、输入经 Rust WS 路径（`Message::input` 调用点）；新增 `get_terminal_ws_info()` 命令 → `{ url, token }`（由 03 的 get_ws_url/get_ws_token 组装，D3）；新增监听前端事件 `terminal_output_activity` `{session_id}` → 插件管理器 `TerminalOutput` 通知（保持仅传 session_id 语义，异步分发，D6）。

**Spec:** §6.1、§6.4（验收 1/6）

**Blocked by:** 04, 06

**Status:** ready-for-agent

- [ ] 输出 Handler/EventForwarder/命令删除（ws_output 事件不再产生）
- [ ] `get_terminal_ws_info` 命令 + 前端调用点（useMobileCommands）
- [ ] `terminal_output_activity` 监听 → 插件通知（OCR 等插件回归验证）
- [ ] cargo test + vitest 删除对应用例/适配

## Comments