# 02 — 桌面端 WS 认证收敛：首消息 JWT 认证 + 事件通道 /ws/event

**What to build:** 所有 WS 路由统一为「首消息 JWT 认证」模型：连接建立后仅接受 `auth` 消息（携带 JWT，`AuthStage::Reauthenticate` 语义），`verify_token_with_expiry` 通过 → `set_authenticated` → 回 `authenticated`；10s 未完成认证 → 服务端关闭；认证前收到非 auth 消息 → 拒绝。`handle_auth` 收敛为纯 JWT 验证分支，RequestPairing/VerifyCode/QrConnect/Biometric* 分支标注 `compat` 保留（旧客户端，见 D2）。新增 **`GET /ws/event` 常驻事件通道**（`TerminalWs::new_event()` 构造，channel_type=Event）——解除 ticket 01 的过渡性回归（broadcast 已 Event-only，旧 Terminal 通道收不到 SyncData，事件通道建立后恢复）。`TerminalWs::stopping()` 按 channel_type 拆分：**仅事件 WS 断开且该设备无其他事件 WS 时**触发 `DEVICE_DISCONNECTED` + 连接历史 `close_open_connection_event` 回填（键 device_id=claims.sub）；终端通道断开不触发。stopping() 的 challenge 清理改 fingerprint 键（当前 `clear(&socket_addr.to_string())` 在指纹键控下是空操作）。

**Spec:** §4.3、§4.4、§4.6（验收 5/6/7/8）、§2.1

**Blocked by:** 01（已完成 7112f2b1）

**Status:** ready-for-agent

- [ ] `terminal_ws.rs` recv 循环：Unverified 状态只放行 auth 消息 + 10s 超时关闭 + 非 auth 首消息拒绝
- [ ] `auth_service::handle_auth` 收敛（新分支仅 JWT；旧分支标 compat）
- [ ] `/ws/event` 路由 + `TerminalWs::new_event()`（channel_type=Event 注册），broadcast_shutdown.rs 断言恢复（换 Event 客户端，重加 `wait_for_sync_data`）
- [ ] `stopping()` 按 channel_type 拆分：最后一条事件 WS 断开 → DEVICE_DISCONNECTED + close_open_connection_event（device_id 键）；终端断开不触发
- [ ] stopping() challenge 清理改 fingerprint 键
- [ ] 连接级认证测试：首条非 auth 被拒、无效 token 关闭、有效 token 认证后业务消息免 token、10s 超时

## Comments

- 2026-08-18 修订：原描述「stopping() 移除 DEVICE_DISCONNECTED」基于 presence 心跳方案（已取消）；定稿为事件 WS 语义——按 channel_type 拆分触发，见 spec §4.2/§8 D8。
- 2026-08-18：`/ws/event` 路由纳入本 ticket（ticket 01 衔接点 ②③）。