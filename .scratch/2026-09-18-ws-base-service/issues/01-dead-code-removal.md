# 01 — 终端的死代码清理（前置瘦身）

**What to build:** 删掉事件通道下已无生产触发者的终端业务分支，让 1800+ 行的终端 WS actor 先瘦身，为后续骨架抽取留出干净面。**行为零变化**：移动端正在用的两条链路（`/ws/event` 事件通道、`/ws/terminal/session/{id}` 终端通道）对外表现完全不变。

明确判定（spec §1.2「死代码判定」）：

- **删除**：`bound_session=None` 分支下由 `Message::Terminal` 驱动的订阅/取消订阅/输入处理、由 `Message::SessionControl` 驱动的会话控制处理，以及会话模式 / 会话输入两处「None 即早退」分支（旧 `/ws/terminal` 兼容路由已删，无客户端会向事件通道发这些消息）。
- **必须保留**：`bound_session=None` 下的首消息 JWT 认证分支——它是事件通道认证的活路径，删掉会直接打断移动端事件通道（spec §3.2 A4 的防误删项）。

**Blocked by:** None — can start immediately.

**Status:** closed（前置检查否决删除；2026-09-18 用户裁决「跳过删除，直接进票 02」）

- [x] 前置检查完成并留证：确认事件通道消息流不含 Terminal / SessionControl 类消息（全仓发送点 grep + 现有测试覆盖说明，结论写进本票 Comments）
- [ ] ~~上述死分支与两处 None 早退分支删除~~ —— **取消**：前置检查结论为「非死代码」（见 Comments），删除会打断移动端 TUI 滚动输入与插件 `session.list`
- [x] ~~首消息 JWT 认证分支保留~~ —— 该分支保持不变（未做删除，无回归风险）
- [x] 既有 `cargo test` 全绿（未改动任何代码，基线保持）
- [x] wire 协议零变更（终端 wire 定义文件未改动）

## Comments

### 2026-09-18 前置检查结论（🔴 与 spec §1.2 判定不符，删除项暂停）

**结论：`Message::Terminal` 与 `Message::SessionControl` 在生产链路上仍有触发者，「无生产触发者」的判定不成立。**

证据链（全仓 grep，两端）：

| 桌面 handler | 消息 | 移动端生产发送点 | 结论 |
| --- | --- | --- | --- |
| `handle_terminal` → `handle_input` | `Message::Terminal{action: Input}` | `bedcode-mobile/src/composables/useTuiCompat.ts:27,13`（ADR-0013 TUI 滚动兼容）`wsSendInput` → `invoke('ws_send_input_async')` → `commands/terminal.rs:34 TerminalRequest::input` → `ConnectionManager::send_and_wait` → **事件 WS `/ws/event`**（`connection/manager.rs:294` 用 `WS_EVENT_PATH`） | **活路径** |
| `handle_session_control` | `Message::SessionControl{ListSessions}` | 移动端插件 `SessionAPI.list` → `src/plugin/context.ts:141 wsLoadSessions()` → `invoke('ws_load_sessions')` → `commands/session.rs:27 SessionRequest::list_sessions()` → 同上事件 WS | **活路径** |
| `handle_terminal` → `handle_subscribe`/`handle_unsubscribe` | `Terminal{Subscribe/Unsubscribe}` | `commands/session.rs:45 ws_join_session`（Rust 命令存在，但移动端前端无 `wsJoinSession` 调用者；`useMobileCommands.ts` 未导出） | 未发现触发者 |
| `handle_session_control` | `SessionControl{Resize/Start/Stop/Remove}` | `wsResizeTerminal`（前端无调用者）；Start/Stop/Remove 走 HTTP | 未发现触发者 |

关键点：移动端常驻连接就是**事件通道 `/ws/event`**（`ConnectionManager::client` 仅由 `establish_ws_client` 以 `WS_EVENT_PATH` 赋值，`manager.rs:231/294`），`bound_session=None` 分支并非「仅理论可达」——`Message::Terminal` / `Message::SessionControl` 由移动端经该连接实发。

**影响**：按本票删除 `handle_terminal`（含 Input 分支）与 `handle_session_control` 会直接打断
① 移动端 TUI（alt-screen）滚动输入；② 移动端插件 `session.list`。违反本票与 spec §7 的「行为零变化 / 移动端零感知」硬约束。

**处置**：已停手待用户裁决（保留 / 先迁移移动端路径 / 强行删除）。检查项留证 = 本段。

- 现有测试覆盖说明：桌面侧无任何测试发送 `Message::Terminal`/`Message::SessionControl`（`grep Message::Terminal` 在 `src-tauri` 仅命中 `message.rs` 自身定义与测试夹具），即「仅测试可达」的直觉来源；但移动端**集成测试** `bedcode-mobile/src-tauri/tests/ws_protocol_integration.rs:127/265` 也在发 `session_control(ListSessions)`，说明该走线是两端共同维护的契约面。
- 反向核实：`handle_session_mode` / `handle_session_input` 的 None 早退守卫确实不可达（仅由 `handle_session_control_frame` 调用，而该入口以 `bound_session.is_some()` 为前置），但这两处属防御性守卫，删除收益 ~7 行，不足以支撑本票目标。
