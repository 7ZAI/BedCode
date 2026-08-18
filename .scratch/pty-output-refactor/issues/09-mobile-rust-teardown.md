# 09 — 移动端输出链路拆除 + 终端 WS 信息命令

**What to build:** 移动端 Rust 删除 PTY 输出转发链路：`handler/terminal.rs` 的 Output 分支、`MobileEvent::Output`、`EventForwarder` 的 `ws_output` emit、`commands/session.rs` 的 `ws_subscribe_session`/`ws_leave_session`、输入经 Rust WS 路径（`Message::input` 调用点）；新增 `get_terminal_ws_info()` 命令 → `{ url, token }`（由 03 的 get_ws_url/get_ws_token 组装，D3）；新增监听前端事件 `terminal_output_activity` `{session_id}` → 插件管理器 `TerminalOutput` 通知（保持仅传 session_id 语义，异步分发，D6）。

**Spec:** §6.1、§6.4（验收 1/6）

**Blocked by:** 04, 06

**Status:** done

- [x] 输出 Handler/EventForwarder/命令删除（ws_output 事件不再产生）
- [x] `get_terminal_ws_info` 命令 + 前端调用点（useMobileCommands）
- [x] `terminal_output_activity` 监听 → 插件通知（OCR 等插件回归验证）
- [x] cargo test + vitest 删除对应用例/适配

## Comments

- 2026-08-19 完成，提交 `feat(mobile): P2 ticket09 输出链路拆除 + get_terminal_ws_info`
- Rust 删除面：
  - `handler/terminal.rs`：删 TerminalAction::Output 分支（SubscribeResponse/UnsubscribeResponse 日志保留；ctx 参数改 _ctx）
  - `router/event.rs`：删 MobileEvent::Output 变体 + forward_event 的 ws_output emit 分支 + 插件 TerminalOutput 通知（D6 语义迁移）
  - `router/context.rs`：删 Output debug 日志分支
  - `commands/session.rs`：删 ws_subscribe_session/ws_leave_session/SubscribeResult；ws_join_session 保留（订阅确认/日志）
  - `commands/terminal.rs`：删 convert_json_to_message 的 input/subscribe/unsubscribe 分支（Message::input 调用点）
  - `commands.rs`/`lib.rs`：注册表同步；lib.rs setup 挂 init_terminal_output_listener
  - 集成测试 ws_protocol_integration.rs：删场景 3（terminal_output_push）
- 新增：
  - `get_terminal_ws_info(session_id)` → TerminalWsInfo{url, token}（conn.get_target() + get_global_token 组装，constants 新增 WS_TERMINAL_SESSION_PATH）
  - `router/event.rs::init_terminal_output_listener`：监听前端 `terminal_output_activity` {session_id} → PluginLifecycleEvent::TerminalOutput（异步分发不 await，保留插件通知链路）
  - 前端 useMobileCommands.ts：getTerminalWsInfo()
- **验证**：cargo check 无 error；`cargo test -j 2` 373 lib + 30 集成全绿；vitest 20 文件 206 测试全绿
- #lesson：替换含相邻重复函数的大块时（ws_join_session 在 oldText 和 newText 各出现一次）导致重复定义——大块替换前先 grep 目标函数出现次数；tauri 2 listen 回调 event.payload() 直接返回 &str（JSON 字符串），非 Value 对象
- 前端 ws_subscribe_session/ws_leave_session 调用删除、terminalBuffer 重写、输入迁移属 ticket 10（Rust 命令已删，同版本发布）