# 03: 移动端事件通道（P2 / M1+M2）

**What to build:** 移动端把常驻事件连接从已删除的 `/ws/event`（404）迁到
`/ws/plugin/com.bedcode.terminal-session/session-control`：极简认证帧、事件帧路由到既有
`MobileEvent` / 前端 `ws_sync_*` 管道（**前端监听零改动**）、重连后主动对账补齐状态。

**Blocked by:** 02 — 帧壳（事件名与 payload 形状）定稿

**Status:** done（2026-09-26 实施完成；票 07 待运行全链路集成验证）

## 验收标准

- [x] **端点常量**：`src-tauri/src/system/constants/connection.rs` 新增插件端点常量
      （base `/ws/plugin/com.bedcode.terminal-session` + `session-control`）；
      `WS_EVENT_PATH`（`/ws/event`）退役；WS 路径字面量唯一出处在此文件。
- [x] **极简认证**：`connection/manager.rs::establish_event_ws` 首帧改为 `{"type":"auth","token":"<jwt>"}`
      （**不再发 `Message::Auth` 信封、不携带链路加密提案、不等待回执**）；`connection/event_ws.rs`
      目标路径改 session-control；退避自愈保留。
- [x] **事件帧路由**：`handler/sync.rs` 输入从 `Message::SyncData` 改为 `{"type":"event"}` 帧；
      映射到既有 `MobileEvent` 变体，前端 `ws_sync_*` 事件名**保持不变**：

      | 帧 `event` | `MobileEvent` 变体 | 前端事件 |
      | --- | --- | --- |
      | `session:created` | `SyncSessionCreated` | `ws_sync_session_created` |
      | `session:stopped` | `SyncSessionStopped` | `ws_sync_session_stopped` |
      | `session:removed` | `SyncSessionRemoved` | `ws_sync_session_removed` |
      | `task:status-changed` | `SyncTaskStatusChanged` | `ws_sync_task_status_changed` |
      | `task:queue-changed` | `SyncTaskQueueChanged` | `ws_sync_task_queue_changed` |
      | `task:scheduled-changed` | `SyncTaskScheduledChanged` | `ws_sync_task_scheduled_changed` |
      | `session:mode-changed` | `SyncSessionModeChanged` | `ws_sync_session_mode_changed` |

- [x] 移动端在该连接上**只读事件**（不发动作；动作走 HTTP，见票 04），避免无 `message_id` 的回包关联问题。
- [x] **未知 `event` / 未知字段 / 畸形载荷** → `debug` 留痕丢弃；**不 panic、不断连**（前进式演进）。
- [x] **消费幂等**：同一事件重复到达（重连对账 + 回声）不造成状态抖动（列表去重/收敛）；
      不做发送端过滤（`source_device` 照收，本机回声由消费端幂等吸收）。
- [x] **重连对账（强制）**：`session-control` 认证成功（或自愈重建）后触发一次对账 =
      `loadActiveSessions()`（HTTP `/api/sessions`，已含 `taskStatus/taskReason`）+ 活跃会话
      `task-queue/list`（按需）；文档明示「事件不重放」。
      → Rust 认证首帧后 emit `ws_event_channel_ready`；前端
      `useMobileConnection.ts` 监听该事件 → `loadActiveSessions()`（含单测）。
- [x] 事件面断开 ≠ 连接面断开：`ws_unexpected_disconnect` 语义保留（对端失联提示 + 自愈）。
- [x] 认证前收到的任何帧丢弃（防御畸形服务端）。
- [x] 单测：7 事件帧路由 + 未知事件丢弃 + 畸形帧留痕、断线自愈重建、对账触发点、
      认证前帧丢弃、幂等去重。
- [x] **结构锁**：`event_ws` / 新事件路由实现段不得出现 `Message::`（信封在 WS 生产路径零使用）。

## 边界与不做

- 不改前端 `ws_sync_*` 监听与 store 收敛逻辑（除幂等修正）。
- 不新增/改桌面端点；不改 HTTP 面。
- 加密退役不在本票（票 06）——本票仅核对其行为：事件通道**不再协商加密**。
- **本票不运行集成测试**（见 `README.md`）——实际：集成测试已迁移并随全量回归跑通，
  全链路（前端 store 断言）仍待票 07。

## 验证（单测）

```bash
cd bedcode-mobile/src-tauri && cargo test plugin_event   # 18 项（7 映射 + 反例 + 幂等 + 结构锁）
cd bedcode-mobile/src-tauri && cargo test mock_plugin_ws  # 票 01 夹具自检 14 项
# 全量：cargo test（lib 350 + 集成全绿）；前端 pnpm run test:run（49 文件 467 例）
```

## Comments

- **2026-09-26 实施**（handoff 03 → 全票落地）：
  - P0：`manager.rs::connect_without_emit` / `commands/connection.rs::get_ws_url` 的
    `WS_EVENT_PATH` 残留清理；`establish_event_ws` 旧 doc 注释更新。
  - P2：`tests/ws_protocol_integration.rs` 全部 5 个 `event_ws_*` / `supervisor_*` 用例迁移到
    票 01 夹具（`MockPluginWsServer` 的 session-control 端点）与新协议（极简认证帧 + 事件帧），
    已随全量回归跑绿。
  - P3：前端 `useMobileConnection.ts` 新增 `ws_event_channel_ready` 监听 → `loadActiveSessions()`
    （事件不重放，重连期间变化靠这次 HTTP 全量对账补齐）；新增集成单测
    「事件通道就绪：ws_event_channel_ready → 触发一次 HTTP 对账」（connection-flow.test.ts，10/10）。
  - P4 核对结论：`establish_event_ws` 只发极简认证帧，`is_event_encryption_active` 无调用者、
    `install_event_crypto`/`extract_crypto_echo`/`EVENT_WS_AUTH_TIMEOUT_MS` 已 `#[allow(dead_code)]`
    （票 06 随 WS 链路加密整体退役一并删除）；WS 连接零加密上装（`install_link_crypto` 无活调用）。
    前端 `encryptWsEvent/encryptWsTerminal` 设置项与 i18n 退役属票 06 范围，本票未动。
  - **关键修复（集成层缺口，本票暴露）**：`RequestResponseManager::try_match` 原返回
    `Option<Message>`，事件帧 `{"type":"event",...}` 无法解码为 `Message` → 被当作
    「已匹配」吞掉，永远到不了 `PluginEventRouter`。改为三态 `MatchOutcome::{Matched,Push,Unroutable}`：
    解析失败帧（含插件事件帧）裁决 `Unroutable` 交还调用方给 handler 侦察路由（见
    `request_response.rs` doc + ws_client receiver）；配套 14 项单测（含新增事件帧/畸形帧用例）。
  - **串行化**：本二进制内测试共享进程全局 token/凭据（`get_global_token`/`get_auth_manager`），
    事件通道要求非空 JWT 后并发必相互踩踏（实测 5/5 红）。放弃「静态 Mutex 跨 await 持锁」
    （规则引擎判 blocker），改为单入口 `ws_protocol_full_suite` 串行驱动 11 个场景。
- **集成测试（已迁移、随全量跑绿；票 07 全链路验证仍待跑）**：
  - `ws_protocol_full_suite`（ws_protocol_integration.rs）串行 11 场景：
    - `scenario_event_ws_first_message_is_jwt_auth`：首帧极简认证 + 无信封 payload 结构锁 +
      认证后事件帧路由
    - `scenario_event_ws_forwards_sync_data`：`task:scheduled-changed` 事件帧 → SyncTaskScheduledChanged
    - `scenario_supervisor_establishes_on_auth_success`：AuthSuccess 驱动建连，连接数恰 1
    - `scenario_supervisor_no_dup_connection_on_auth_echo`：重复 AuthSuccess 不双连
    - `scenario_supervisor_recovers_after_disconnect`：断开 → 空 token 快速失败 → 下一认证流重建
  - 票 07 增量（前端 store 断言）：认证成功后注入 7 类事件断言前端 `ws_sync_*` 触发、
    未知事件被丢弃、断开重建后 `ws_event_channel_ready` 对账补齐。
- 运行环境注意：`tests/support/mock_plugin_ws.rs` 同时被 lib 测试（lib.rs `#[path]`）与
  两个集成测试二进制编译，属预期（各自 crate root 独立实例）。
