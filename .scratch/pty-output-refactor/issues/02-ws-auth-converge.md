# 02 — 桌面端 WS 认证收敛：首消息 JWT 认证 + 事件通道 /ws/event

**Status:** done（2389d08b）

- [x] `terminal_ws.rs` recv 循环：Unverified 状态只放行 auth 消息 + 10s 超时关闭 + 非 auth 首消息拒绝
- [x] `auth_service::handle_auth` 收敛（新分支仅 JWT；旧分支标 compat）
- [x] `/ws/event` 路由 + `TerminalWs::new_event()`（channel_type=Event 注册），broadcast_shutdown.rs 断言恢复（换 Event 客户端，重加 `wait_for_sync_data`）
- [x] `stopping()` 按 channel_type 拆分：最后一条事件 WS 断开 → DEVICE_DISCONNECTED + close_open_connection_event（device_id 键）；终端断开不触发
- [x] stopping() challenge 清理改 fingerprint 键
- [x] 连接级认证测试：首条非 auth 被拒、无效 token 关闭、有效 token 认证后业务消息免 token、10s 超时

## Comments

- 2026-08-18 修订：原描述「stopping() 移除 DEVICE_DISCONNECTED」基于 presence 心跳方案（已取消）；定稿为事件 WS 语义——按 channel_type 拆分触发，见 spec §4.2/§8 D8。
- 2026-08-18：`/ws/event` 路由纳入本 ticket（ticket 01 衔接点 ②③）。