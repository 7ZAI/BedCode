# 08: 宿主业务硬切与旧协议删除

**What to build:** 完成一次不可回退的宿主 WS 硬切：业务只通过插件端点进入，宿主只保留通用传输核心；所有旧路由、旧消息枚举、终端业务服务、同步桥和 PTY 会话映射一次性删除。

**Blocked by:** 03 — 会话控制直连端点；04 — 终端输入输出流端点；05 — 会话事件归插件；06 — 任务事件归插件；07 — 设备派生与认证记录归插件

**Status:** done（2026-09-25，commit `<FILL>`）

- [x] 旧事件通道和旧终端会话通道删除，请求只得到通用 404，不提供 alias 或 fallback。
      `routes.rs` 只剩 `/ws/plugin/{plugin_id}/{path}`；`/ws/event` 与 `/ws/terminal/session/{id}`
      已删（结构锁：路由字符串只在迁移说明/测试反例中出现）。
- [x] 宿主业务消息枚举、会话/终端服务、宿主订阅器和终端协议模块全部删除。
      `message.rs` / `services/` / `subscription.rs` / `terminal_ws/` / `session.rs` 整删。
- [x] 宿主 WS 核心不再硬编码插件、端点、动作、会话、设备或任务类型。
      `conn.rs` / `registry.rs` / `websocket_manager.rs` 收敛为通用 transport；
      `ChannelKind` 枚举删除（只剩插件端点一类连接）；宿主业务 `Message` 发送/广播 API 删除。
- [x] PTY 不再声明或映射宿主广播会话，插件只经通用 ring-fetch/write 原语工作。
      `hostBroadcastSessionId` / `broadcast_handle_for_session` / `BroadcastHandle` 删除；
      PTY 引擎不再知道 session id；HTTP 历史快照改经插件互调 `session-history`
      （插件用自己 `session record.pty_id` 调 `ring-fetch` 分片续拉）。
- [x] 宿主同步事件桥和产品连接 DTO 删除，插件事件/连接事实成为唯一来源。
      `src/events/` 整目录删除（AppEvent/publish/matcher/HostSyncEvent/sync_handler）；
      `host-events.broadcast-sync` / SDK `SyncEvent` / `SyncPayload` / `AppContext.sync_tx` 删除；
      插件事件走 `host-bus.publish` + `host-events.emit`。
- [x] desktop ABI、SDK、插件 manifest、随包产物和旧产物拒绝提示同步完成。
      ABI v28（含票 02 的 connection-context）；SDK `wire/{sync,control}.rs` 删除、
      `PtySpawnConfig.host_broadcast_session_id` 删除、`broadcast.sync` 权限子项退役
      （`broadcast` 位保留——前端 events.on/emit 仍映射）、权限词汇重出；四插件产物重建。
- [x] 全仓无旧路由、旧协议和旧生产调用残留；旧客户端不在兼容范围内。
      `rg` 扫宿主生产源码：`/ws/event` 路由串、`broadcast_sync`、`HostSyncEvent`、
      `SyncPayload`、`hostBroadcastSessionId`、`broadcast_handle_for_session` 零命中。
- [x] 端到端插件端点与真实 PTY 闭环通过，宿主通用 transport 回归全绿。
      `pty_session_chain`（session-control start/list/stop + terminal subscribe/input/输出/
      session_stopped + HTTP history）与 `ws_auth_rules`（插件端点认证门禁）/ `broadcast_shutdown`
      （连接清理 + 停机鲁棒性）重写为插件端点协议；desktop lib 927 + 集成 target 全绿。

## 执行记录（2026-09-25）

- 删除文件（git rm）：`server/websocket/message.rs`、`channel/{event,terminal}.rs`、
  `services/*`、`subscription.rs`、`terminal_ws/*`、`session.rs`、`events/{app_event,
  host_sync_event,matcher,sync_handler}.rs` + `events.rs`、`enums/{control,sync,summary}.rs`、
  SDK `wire/{sync,control}.rs`。
- 收敛：`conn.rs`（去 ChannelKind/bound_session/subscriptions/new_for_session/new_event/
  authenticate_jwt/pending_ws_crypto，WsSession 收缩为连接认证态）、`registry.rs`（去
  ChannelKind/broadcast/broadcast_targets/设备名字段与广播过滤，测试重写）、
  `websocket_manager.rs`（去 BusinessMessage API 与重复 ClientSummary）、`routes.rs`、
  `host_api/pty.rs`（去广播映射）、`host_api/events.rs`（去 broadcast_sync）、
  `component.rs`、`lib.rs`、`app_context.rs`、`constants.rs`、`enums.rs`。
- 新增插件互调 `session-history`（output.rs `history_via_host` + SessionApi trait/impl +
  plugin.json api +1；宿主 `session_gateway::history_snapshot` 改签名、`session_controller`
  `get_session_history` 跟演）。两处 api 计数断言 32→33（宿主 session_e2e / 插件 lib）。
- 集成测试重写（插件端点协议）：`ws_auth_rules`（未认证业务帧丢弃 + 10s 超时 close4001 /
  坏 JWT close4001 / JWT+list_sessions / 10s 静默超时）、`pty_session_chain`（真实 PTY
  闭环 + 未注册端点 404）、`broadcast_shutdown`（连接清理 + 停机鲁棒性，SyncData 断言删除）；
  `session_e2e` 删旧 relay 测试、`ws_e2e` 去 sc import；`http_auth_biometric` 去 sync_tx 装配。
- SDK：WIT 去 broadcast-sync + hostBroadcastSessionId 文档；`host/events.rs` 去 broadcast_sync；
  `wasm_host.rs` 去 broadcast_sync；`events.rs` 去 SyncEvent（留 ProcessDoneEvent/PluginQuestion
  + 补 shape 测试）；`lib.rs` 去 SyncEvent re-export；`permission.rs` PERMISSION_BROADCAST
  apiMap 条目删除（空清单省略，测试约束）；`host/pty.rs` 去 host_broadcast_session_id；
  重跑 `pnpm run gen:permissions`。
- 插件：`launch.rs` 去 host_broadcast_session_id；`plugin.json` api +session-history；
  前端 contract 测试 api 计数 27→28 跟演。
- 文档：code-map.md（websocket/events/pty/树/链路图/quick-nav）、插件检查清单（v28 +
  broadcast-sync 退役）、CHANGELOG（hunk 级 staging，票 08 条目入库）。
- 验证：desktop `cargo test` 全绿（lib 927 + 集成 target）、插件 326、SDK 146、
  前端 vitest 807、eslint 0 error、`cargo fmt --check` 我的文件零漂移、clippy 我引入的
  lint 已清（剩余为既有漂移）。

## 遗留

- 票 09（终态全量门禁：文档复核 + lens）+ 移动端明确不兼容的登记（spec 已写，ADR 0022
  修订记录未补——见票 09）。
- 共享 worktree 下 gitignored 的 WASM 产物会被并发 agent 重建覆盖（实测 terminal-session
  resources plugin.json 曾被并发构建换成无 session-history 的旧清单导致测试假红）——
  跑测试前若 api 断言失败先核对 `resources/plugins/desktop/*/plugin.json` 与源一致。