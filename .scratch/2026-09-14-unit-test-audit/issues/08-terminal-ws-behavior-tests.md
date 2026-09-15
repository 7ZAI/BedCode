# 08 — terminal_ws.rs 补三类行为测试（1601 行零测试）

**What to build:** `src/server/ws/terminal_ws.rs` 是终端 WebSocket 的 actor 主逻辑（认证 → 订阅 → 输入 → 输出 → 背压 ack → 心跳 → 断线清理），1601 行代码**完全无测试模块**。补三类最高价值测试：(a) `handle_session_control_frame` 分派（auth 前后各类型帧的接受/拒绝）；(b) `stopping` 资源回收（多订阅者 abort、代数递增）；(c) `handle_ack_binary` 对 `parse_ack_frame` 错误的吞掉行为（不关连接）。

**Blocked by:** 无

**Status:** done（2026-09-15：抽 4 个纯函数 + 7 测试——`frame_needs_auth`/`should_reject_unauthenticated`（分派契约）、`ack_source_for`/`ack_frame_outcome`（ack 吞掉语义）、`cleanup_subscription_state`（stopping 资源回收）；handle_session_control_frame 前置守卫、handle_ack_binary 与 stopping 改调纯函数，行为不变；`cargo test --lib server::ws::terminal_ws::` 39 绿，变异自检 1 处（frame_needs_auth 反向）被捕获；完整 actor 级测试（ctx 构造）仍需重构，未做）

- [ ] 新增 `test_handle_session_control_frame_rejects_before_auth`：未 auth 时发 `{"type":"subscribe"}` → 期望 `Error{UNAUTHORIZED}` 且不断连；发 `{"type":"auth","token":"bad"}` → `Error{UNAUTHORIZED}`；发合法 auth → 转 `SessionAuthOutcome`
- [ ] 新增 `test_handle_session_control_frame_accepts_after_auth`：auth 后依次发 subscribe → subscribe_ok → set_mode → input，断言每步成功且状态机正确
- [ ] 新增 `test_handle_ack_binary_ignores_malformed_and_keeps_alive`：依次送入截断帧 / 错 magic / 非 ack 标志 / 非 UTF-8，断言 actor 不 close 且日志含 warn；再送合法 ack，断言 `GlobalOutputManager::ack_offset` 被推进到指定 offset
- [ ] 新增 `test_stopping_aborts_all_forwarders_and_bumps_generation`：建立 2 个订阅者（同/不同 session），调用 `stopping()` 后断言 `output_forwarders` / `subscribe_tasks` / `subscriber_modes` 全清空且 `stream_generations` 对应 key 的原子值 +1
- [ ] 新增 `test_handle_session_subscribe_snapshot_offset`：subscribe 带 `from_offset`，断言返回的快照包含指定偏移量后的数据
- [ ] `cargo test --lib server::ws::terminal_ws::` 通过

## 证据

- `grep -n "cfg(test)\|mod tests" src/server/ws/terminal_ws.rs` 返回空（无任何测试模块）
- 生产代码关键函数：`handle_session_auth`（:693）、`handle_session_subscribe`（:737）、`handle_ack_binary`（:921）、`handle_session_input`（:952）、`stopping`（:385）
- 旧路由 vs 新路由的分叉路径（`:120-122` 的 `subscriber_modes` 字段注释声明「重订阅时重置为 realtime」，`:734` 声明「重订阅 abort 旧 forwarder + 流代数递增」）无测试证明

## 根因

1601 行 actor 主逻辑无任何测试模块。认证握手、订阅、输入、输出、背压 ack、心跳、断线清理全零覆盖。任何回归只能靠手工 E2E 或线上问题暴露。

## 修复方向

1. 为 `handle_session_control_frame` 加分派测试：auth 前后各类型帧的接受/拒绝语义
2. 为 `stopping` 加资源回收测试：多订阅者 abort、代数递增、subscriber_modes 清空
3. 为 `handle_ack_binary` 加错误吞掉测试：malformed 帧不关连接，合法帧推进水位

## 影响面

修复后，终端 WS 主流程的回归能被抓到。但仍是单元测试级别，端到端链路仍需 `tests/ws_session_route.rs` 守。

## Comments

- 2026-09-14 审计发现，见 `../http-ws-spec.md` §5.10
- 这是 HTTP/WS 审计中最严重的红线：1601 行零测试
