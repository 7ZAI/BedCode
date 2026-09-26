# 移动端适配桌面端 WS 硬切（wasm 应用最小补广播 + 移动端调用面重写）

Status: **done（2026-09-26 全部实现票交付；集成测试与全量门禁在票 07 统一运行通过；真机联调缺设备环境待补）**
Date: 2026-09-26
范围: `bedcode-desktop/wasm-apps/terminal-session/`（最小补广播）+ `bedcode-mobile/`（主体适配）；
**桌面宿主 `bedcode-desktop/src-tauri/src/**`、WIT/SDK/ABI/权限面零改动**
决策依据: 用户 2026-09-26 指令（「不算，可以改 wasm app 应用，但是尽量不要改动桌面端宿主」）+
`.scratch/2026-09-26-mobile-desktop-adaptation/survey.md`（排查报告）+ 相关专项 spec（见 §11）
承接: 桌面端 2026-09-25 WS 业务硬切（ABI v28）与 HTTP 路由注册下沉（ABI v29）已落地；本专项是移动端的对齐侧

---

## 0. 用户裁决与硬约束

1. **宿主零改动**：`bedcode-desktop/src-tauri/src/**`、`packages/plugin-sdk-desktop/**`（WIT / SDK / ABI / 权限词汇）
   一律不改；判据 = 收尾 `git diff --stat` 中这些路径零变更（随包产物 `src-tauri/resources/plugins/desktop/**` 除外，需重建）。
2. **wasm 应用可改但最小**：只在 `terminal-session` 的**既有事件收口点**追加广播调用 + 新增一个广播模块；
   **不新增 WS 端点、不改 `plugin.json`、不加权限位、不动 SDK 常量**。
3. **移动端是主体**：WS 调用面（终端流 / 事件通道）重写、会话控制与服务调用迁 HTTP、WS 链路加密退役、死代码清理。
4. **不兼容旧版移动端**：无 fallback、无双读；破坏性变更如实登记（CHANGELOG / code-map / 知识库文档）。
5. **事件不重放**：插件无事件缓冲；移动端重连后必须**主动对账**（HTTP 拉取）补齐状态。
6. **载荷纪律**：广播载荷 snake_case（与移动端既有消费形状一致）、自足、不含凭据（不落 token/密钥/公钥）；
   失败不伪造（无客户端＝0 不计错，投递失败显性留痕）。

---

## 1. Problem Statement

桌面端 2026-09-25 完成 WS 业务硬切（`.scratch/2026-09-25-websocket-business-downsink/`）：

- `/ws/event`、`/ws/terminal/session/{id}` **已删除**（404），只剩 `/ws/plugin/{plugin-id}/{path}`；
- 终端协议、会话控制协议归 `com.bedcode.terminal-session` 插件（自定义帧）；
- 宿主同步广播面（`broadcast-sync` → `Message::SyncData`）整体删除。

排查（`survey.md`）确认三项移动端受损，且定位到成因：

| # | 受损 | 成因 |
| --- | --- | --- |
| 1 | 终端流不可用 | 旧 URL + TB v3 协议（16B 头 / per-frame offset / `history_end`）在新端点无对应 |
| 2 | 会话控制与插件 API 不可用 | 移动端走 `/ws/event` 的 `Message::SessionControl` / `SessionConfig` 信封；端点已删 |
| 3 | 业务事件失源（`ws_sync_*` 全灭） | 旧广播由宿主实现，下沉时**漏接**「向移动端 WS 广播」一环——`ws_broadcast_text/binary`、`list-clients` 在 SDK / 宿实现 / 连接注册表三层齐备且已实现，但 `wasm-apps/**` 内**零调用**（WS 服务端原语仅 4 个单播点） |

附带：WS 帧级链路加密在桌面端**实为退役**（`TrafficChannel::WsPlugin => false`；
`derive_ws_session_ciphers` / `ws_register_ciphers` 生产零调用），而移动端 `encryptWsEvent/encryptWsTerminal`
默认开启——不退役会把加密帧直送插件端点导致协议解析失败。

---

## 2. 目标、非目标与边界

### 2.1 目标

1. 桌面插件在既有事件收口点向 `session-control` 端点客户端广播业务事件（帧壳 `{"type":"event",…}`），
   恢复移动端 `ws_sync_*` 事件面（体验与旧版等价）。
2. 移动端终端流对齐插件新协议（订阅 / 输入 / 流控 / 重锚 / 停止帧），端到端可用。
3. 移动端会话控制与插件 API 调用迁移到 HTTP（URL 与形状已由桌面保证不变）。
4. 移动端 WS 链路加密退役，HTTP 信封加密保留。
5. 移动端旧信封协议（`Message` 枚举 / request_response / SyncData 分发）随消费者清零退役。

### 2.2 非目标

- 不改宿主内核、WIT/SDK/ABI、权限词汇、`plugin.json`、HTTP 面（已零改动）。
- 不补移动端不需要的推送：**配置增删改**（无桌面事件 → 移动端改按需/对账）、
  **设备上下线** `device:connected/disconnected`（移动端代码零消费，仅 i18n 残留）、
  **`session:restarted`**（移动端未消费，restart 同 id 场景由 `session:created` 覆盖）、
  **`task:preset-changed`**（移动端无对应 `ws_sync_*`）。
- 不做事件重放 / 断点补投（重连对账即可）。
- 不新增 WS 端点（复用 `session-control`）；不改对等网络面（v31 已裁定无跨端受损）。

### 2.3 边界口径

- 「桌面端改动」限缩为：`wasm-apps/terminal-session/rust/src/**` 追加广播（约 1 模块 + 8 个调用点）+ 产物重建。
- 移动端可自由改：`bedcode-mobile/src/**`（前端）与 `src-tauri/src/**`（Rust）。

---

## 3. 目标接口设计

### 3.1 桌面插件：事件广播出口（新增 `rust/src/ws_events.rs`）

```rust
/// 向 session-control 端点全体客户端广播业务事件（唯一广播出口）
/// 帧：{"type":"event","event":"<name>","payload":{...}}
pub fn broadcast_event(host: &WasmHost, event_name: &str, payload: &serde_json::Value);
```

实现要点：
- **端点反查**：新增 `endpoint_id_for_path("session-control")`，经 `WasmHost.ws_list_endpoints()`
  （返回 `[{endpointId, path, clientCount}]`，仅本人端点）建双向缓存；已有 `lib.rs::resolve_endpoint_path`
  是 id→path 单向缓存，本模块自建 path→id 缓存（或把它提为双向，二选一，改动最小者优先）。
- **零客户端早退**：`clientCount == 0` 直接返回（避免每事件一次宿主调用）；非零才 `ws_broadcast_text`。
- **失败口径**：`ws_broadcast_text` 返回成功数（0 合法）；`Err`（端点缺失/权限）→ `log_debug`/`log_warn` 留痕，
  **绝不影响业务返回值**（事件是旁路，业务真源在插件库/HTTP 回包）。
- **不新增权限/端点**：`ws:server` 已声明、端点已注册（`plugin.json` 零 diff）。

**接入点（既有收口函数，逐事件）**

| 事件名（SDK 常量） | 接入位置 | 广播载荷（snake_case，自足） | 移动端消费 |
| --- | --- | --- | --- |
| `session:created` | `session/events.rs::publish()` | `{session:<snake_case SessionSummary>, source_device}` | `ws_sync_session_created` |
| `session:stopped` | 同上 | `{session_id, session_name, source_device}` | `ws_sync_session_stopped` |
| `session:removed` | 同上 | `{session_id, session_name, source_device}` | `ws_sync_session_removed` |
| `task:status-changed` | `task/state.rs` 三个 emit 点（`:337` interrupted / `:372` dispatched / `:916` hook 状态） | `{session_id, task_status, task_reason?, task_questions?}`（bus 形 + 从同点局部量补 reason/questions） | `ws_sync_task_status_changed` |
| `session:mode-changed` | `task/state.rs::set_auto_mode`（`:1621`）与 `task/scheduled.rs`（`:527`） | `{session_id, auto_approve, auto_execute}`（bus 形） | `ws_sync_session_mode_changed` |
| `task:queue-changed` | `task/queue.rs::broadcast_queue_changed()` | 现有 bus 形 `{session_id, queue_count, action, task_id, status}` | `ws_sync_task_queue_changed` |
| `task:scheduled-changed` | `task/scheduled.rs::broadcast_scheduled_changed()` | 现有形 `{job_id, status, action}` | `ws_sync_task_scheduled_changed` |

- `task:status-changed` 是唯一需要「合并 bus 与 emit 字段」的事件（bus 无 reason/questions，emit 无 snake_case）；
  在 `task/state.rs` 内收口为一个 `broadcast_task_status(host, session_id, status, reason, questions)` 私有函数，
  三个点调用（保证载荷形状单点）。
- `publish()` / `broadcast_queue_changed()` / `broadcast_scheduled_changed()` 已有「bus + emit」双通道，
  广播是第三通道，**载荷取 bus 形**（snake_case），不引入第三套键名。

### 3.2 移动端：事件通道（`session-control` 常驻）

```
ws://<host>:<port>/ws/plugin/com.bedcode.terminal-session/session-control
  C→P 首帧（认证）：{"type":"auth","token":"<jwt>"}
  P→C 事件帧：{"type":"event","event":"<name>","payload":{...}}
  P→C 动作回包（本专项不使用）：{"type":"session_list"|"start_session"|…|"error"}
```

- 移动端在该连接上**只读事件**（不发动作；动作走 HTTP，见 3.4）——避免与无 `message_id` 的回包关联问题。
- 认证：极简帧（**不再发 `Message::Auth` 信封、不携带链路加密提案**）。
- 事件 → 既有管道映射（**Rust `router/event.rs` 的 `MobileEvent` / 前端 `ws_sync_*` 事件名保持不变**）：

| 帧 `event` | `MobileEvent` 变体 | 前端事件（不变） |
| --- | --- | --- |
| `session:created` | `SyncSessionCreated` | `ws_sync_session_created` |
| `session:stopped` | `SyncSessionStopped` | `ws_sync_session_stopped` |
| `session:removed` | `SyncSessionRemoved` | `ws_sync_session_removed` |
| `task:status-changed` | `SyncTaskStatusChanged` | `ws_sync_task_status_changed` |
| `task:queue-changed` | `SyncTaskQueueChanged` | `ws_sync_task_queue_changed` |
| `task:scheduled-changed` | `SyncTaskScheduledChanged` | `ws_sync_task_scheduled_changed` |
| `session:mode-changed` | `SyncSessionModeChanged` | `ws_sync_session_mode_changed` |

- 未知 `event` 名 → `debug` 留痕丢弃（前进式演进，老端忽略未知字段/事件）。
- **不做发送端过滤**：事件带 `source_device`，移动端自身动作的回声照收（消费端幂等：列表/状态收敛）；
  若未来需要「排除发起端」，在移动端按本机设备名过滤（插件侧不做）。
- **重连对账（强制）**：`session-control` 连接认证成功（或自愈重建）后，前端触发一次
  对账 = `loadActiveSessions()`（HTTP `/api/sessions`，视图已含 `taskStatus/taskReason`）+ 活跃会话的
  `task-queue/list`（按需）。事件只保证「在线期间的变化可达」，不保证重连期间不漏。

### 3.3 移动端：终端流通道（`terminal` 端点，新协议）

| 项 | 现状（移动端） | 终态 |
| --- | --- | --- |
| URL | `/ws/terminal/session/{id}` | `/ws/plugin/com.bedcode.terminal-session/terminal` |
| 握手 | `{"type":"auth","token"}` | **不变** |
| 订阅 | `{"type":"subscribe","from_offset":N}` | `{"type":"subscribe","sessionId":"<id>","mode":"live"}`（离开终端页/批量态 → 重订阅 `mode:"poll"`；回包 `subscribed`） |
| 退订 | 连接关闭 | `{"type":"unsubscribe"}`（或关闭连接，二者皆可，实现择一并在契约测试锁定） |
| 输入（可打印） | `{"type":"input","data":"<base64>","special_key"}` | `{"type":"input","data":"<UTF-8 文本，无控制字符>"}` |
| 输入（控制字符/特殊键） | 同上（`special_key` 由桌面翻译） | **binary 帧**：`KeyCombo::to_pty_bytes()`（`enums/special_key.rs` 已有）→ 原始字节 |
| 流控 ack | TB v3 二进制 ACK 帧（偏移锚定桌面环） | `{"type":"ack","offset":<本地已渲染字节数>}`（阈值/空闲兜底节流保留） |
| 追加拉取 | 无 | `{"type":"poll"}`（批量态客户端驱动） |
| 输出帧 | TB v3：16B 头 + `[start,end)` | **裸字节**（本地字节计数；无 per-frame offset） |
| 重锚 | `resync` 控制帧（桌面重播） | 收 `{"type":"ring_resync","offset":N}` → 清缓存/清屏 → 以 N 为新基准 |
| 结束 | `SessionStopped` 控制帧 | `{"type":"session_stopped","sessionId","reason","exitCode"?}` → phase=stopped（尾帧在前） |
| 错误 | — | `{"type":"error","message"}` → 留痕 + 状态事件；`会话不存在` 类按退避重试（会话启动竞态） |
| 历史门控 | 等 `history_end`（phase→live） | 收 `subscribed` 即进入 live（历史与实时同一条流）；8s 超时兜底保留 |
| 历史回补 | WS 重播 + HTTP 回退 | 订阅即回放环形窗口；HTTP 回退 `GET /api/sessions/{id}/history?from=`（`from` 以 `ring_resync` 报的环偏移为基准） |
| 重连 | 退避 + `from_offset` 续传 | 退避保留；重连后**重订阅**（无续传语义；窗口淘汰由 `ring_resync` 如实告知） |

前端跟演：
- `stores/terminalBuffer.ts`：删除 TB v3 头解析与 `startOffset/endOffset` 缺口/重拼逻辑，改为
  「本地接收字节计数 + `ring_resync` 断点（清屏重锚）」；历史拼接完成后消费实时帧的次序不变。
- `composables/terminal/useTerminalSubscription.ts`：live 门控信号从「phase 到达 live（history_end）」
  改为「收到 `subscribed`」；超时兜底保留。
- `useTerminalResize` / HTTP 输入路径不变（resize 已走 HTTP）。

### 3.4 移动端：会话控制与插件 API 迁 HTTP

| 移动端现状（WS 信封） | 终态调用 |
| --- | --- |
| `session.rs::SessionManager::{start,stop,remove}_session` | `POST /api/sessions/start`、`POST /api/sessions/{id}/stop`、`DELETE /api/sessions/{id}/remove` |
| `commands/session.rs::ws_load_sessions` | `GET /api/sessions`（经 `commands/http_proxy` 既有通道，前端可直用 `useHttpApi`） |
| `commands/session.rs::ws_load_session_configs` | `GET /api/configs`（或直接删除该命令，前端已用 `httpListConfigs`） |
| `commands/terminal.rs::ws_send_input_async` | `POST /api/sessions/{id}/input`（`{data, specialKey}`，桌面已支持 specialKey 翻译）；终端页内输入走 3.3 的终端 WS 二进制帧 |
| `plugin/context.ts::session.list` | HTTP 列表（保持 `session.list` 权限判定不变） |
| `plugin/context.ts::terminal.sendInput` | HTTP `/api/sessions/{id}/input`（保持 `terminal.sendInput` 权限判定不变） |
| `commands/session.rs::get_terminal_ws_info` | 删除（旧前端直连路径无消费者）或返回 3.3 新 URL（实现择一） |

### 3.5 移动端：WS 链路加密退役

- `useLinkEncryption` 的 `ws-terminal` / `ws-event` 两个子开关**退役**（推荐直接删除开关与 i18n 文案；
  若保留 UI 则必须置灰 + 文案说明「桌面端已退役插件端点帧加密」）。
- `services/linkCrypto.ts` 保留 HTTP 通道实现与测试；`install_event_crypto` 路径随事件连接重构删除。
- HTTP 信封加密（`LinkEncryption` 的 `http` 子开关）行为不变。
- 依据：桌面 `TrafficChannel::WsPlugin => false`（插件端点帧永不加解密）+ WS 握手注册无生产调用者。

---

## 4. 文件级迁移矩阵

### 4.1 桌面（wasm 应用，最小）

| 文件 | 动作 | 说明 |
| --- | --- | --- |
| `wasm-apps/terminal-session/rust/src/ws_events.rs` | **新增** | 广播唯一出口（端点反查 + 帧壳 + 零客户端早退 + 失败留痕） |
| `rust/src/lib.rs` | 改 | `mod ws_events;`；`resolve_endpoint_path` 若提为双向则在此（否则不动） |
| `rust/src/session/events.rs` | 改 | `publish()` 内追加一次 `ws_events::broadcast_event` |
| `rust/src/task/queue.rs` | 改 | `broadcast_queue_changed()` 内追加一次 |
| `rust/src/task/scheduled.rs` | 改 | `broadcast_scheduled_changed()` 内追加一次；`set_auto_mode` 调用点（`:527`）追加 mode 事件 |
| `rust/src/task/state.rs` | 改 | 新增私有 `broadcast_task_status(...)`；三个 emit 点接入；`set_auto_mode` 追加 mode 事件 |
| `wasm-apps/terminal-session/rust/src/**`（其余） | 不动 | 终端协议 / 会话控制 / HTTP 注册表零改动 |
| `plugin.json`、WIT、SDK、宿主 `src-tauri/src/**` | **零 diff** | 硬约束 |
| `src-tauri/resources/plugins/desktop/terminal-session/**` | 重建 | 随包产物 + `wasmHash`（构建步骤，非源码改动） |

### 4.2 移动端（主体）

| 文件/目录 | 动作 | 说明 |
| --- | --- | --- |
| `src-tauri/src/system/constants/connection.rs` | 改 | 新增插件端点常量（base + session-control + terminal）；`WS_EVENT_PATH`/`WS_TERMINAL_SESSION_PATH` 退役 |
| `src-tauri/src/connection/event_ws.rs` | 重写 | 目标路径改 session-control；极简认证帧；去掉加密协商；退避自愈保留；认证成功触发对账 |
| `src-tauri/src/connection/manager.rs` | 改 | `establish_event_ws` 简化（无信封、无 crypto）；`connect()` 路径语义跟演 |
| `src-tauri/src/handler/sync.rs` | 改 | 输入从 `Message::SyncData` 改为 `{"type":"event"}` 帧；其余映射不变 |
| `src-tauri/src/terminal_link.rs` | **重写协议层** | URL / subscribe / input / ack / poll / ring_resync / session_stopped；裸字节缓存与本地计数 |
| `src-tauri/src/session.rs` | 改 | `SessionManager` 起停删 → HTTP |
| `src-tauri/src/commands/session.rs` | 改 | `ws_load_sessions`/`ws_load_session_configs` → HTTP；`get_terminal_ws_info` 删或改 URL |
| `src-tauri/src/commands/terminal.rs` | 改 | `ws_send_input_async` → HTTP（或删除，改由前端 `httpSendSessionInput`） |
| `src-tauri/src/model/message.rs`、`connection/{codec,request,request_response}.rs`、`router/{registry,*.rs}`、`handler/{auth,system}.rs` | 退役 | 消费者清零后按「无消费者即删」处理（保留 `router/event.rs` 事件转发） |
| `src/stores/terminalBuffer.ts`、`composables/useTerminalBuffer.ts`、`composables/terminal/useTerminalSubscription.ts` | 改 | TB v3 解析/offset 缺口模型 → 本地计数 + resync 断点；live 门控改 `subscribed` |
| `src/composables/useLinkEncryption.ts`、`src/services/linkCrypto.ts` | 改 | WS 子开关退役；HTTP 通道保留 |
| `src/composables/useMobileCommands.ts`、`src/composables/useMobileConnection.ts` | 改 | 命令面收敛；`ws_sync_*` 监听不变；补对账入口 |
| `src/plugin/context.ts` | 改 | `session.list` / `terminal.sendInput` 改 HTTP |
| `src/composables/useHttpApi.ts` | 改（仅前缀） | 任务面 `com.bedcode.auto-task` → `com.bedcode.terminal-session`（B1） |
| `src/locales/{zh-CN,en}/**` | 改 | 加密开关文案退役；双文件同步 |
| `bedcode-mobile/docs/code-map.md`、`docs/knowledge/mobile-desktop-auth.md` | 改 | 调用口径跟演（§B6） |

---

## 5. 实施阶段与依赖

### P0 基线与夹具

- 记录 `git status` / 在途改动；冻结宿主零改动判据（§0.1 命令）。
- 移动端建**假插件端点夹具**（本地 WS server 模拟 `session-control` 事件帧与 `terminal` 文本/二进制协议），
  供 Rust / 前端单测与集成使用（不依赖桌面）。

### P1 桌面补广播（D1，与 P2 可并行）

1. `ws_events.rs`（端点反查 + 帧壳 + 零客户端早退 + 失败留痕）；
2. 7 个事件接入（§3.1 表）；`task:status-changed` 合并载荷收口；
3. 结构锁：广播出口唯一（`ws_broadcast_text(` 在实现段仅允许出现在 `ws_events.rs`）、载荷 snake_case、
   调用点数钉死（session 1 / queue 1 / scheduled 2 / state 4）；
4. 单测：帧壳形状、字段形状、零客户端早退、未知路径（无端点）不 panic；端点反查为纯函数可测；
5. 重建插件产物（`node scripts/plugin-build.js --plugin com.bedcode.terminal-session`）+ `wasmHash` 核对。

### P2 移动端事件通道（M1 + M2，依赖 D1 的帧壳定稿）

1. 端点常量 + 极简认证 + session-control 常驻（替代 `/ws/event` supervisor）；
2. `{"type":"event"}` → `MobileEvent` → `ws_sync_*`（前端监听零改动）；
3. 重连对账（`loadActiveSessions` + 活动会话队列拉取）；
4. 单测：帧路由（7 事件 + 未知事件丢弃 + 畸形帧留痕）、断线自愈、对账触发点。

### P3 移动端控制面迁 HTTP（M3，独立）

1. `SessionManager` / 命令面 / 插件 API 三处迁 HTTP（§3.4 表）；
2. 旧信封协议消费者清零 → 死代码退役（保留事件转发链）；
3. 单测：命令面返回形状、失败语义（`code!=0` 映射 `AppError`）。

### P4 移动端终端流重写（M4，最大票，独立）

1. 协议层（3.3 表）+ 本地计数缓存 + `ring_resync` 重锚 + `session_stopped`；
2. 输入：文本 vs binary（`KeyCombo::to_pty_bytes()`）；特殊键既有用例跟演；
3. 流控：ack 阈值/空闲兜底（保留既有节流参数语义：`ACK_BYTES_THRESHOLD` / `ACK_MAX_IDLE_MS`）；
4. 前端 TB v3 解析退役 + live 门控改 `subscribed`；
5. 单测/集成：夹具协议闭环（订阅→回放→实时→resync→stopped）、输入双形态、重连重订阅。

### P5 加密退役与清理（M5 + M6，收尾）

1. WS 子开关退役 + i18n + 设置页测试；
2. 任务面前缀迁移（`com.bedcode.terminal-session`）；
3. 文档跟演（code-map / auth 文档 / AGENTS §9 口径）；
4. 会话状态字面量核对：`waitingInput`（桌面 wire）vs 移动端比较用的 `waiting_input`
   （`useMobileConnection.ts:321/914` 等）——**存量漂移，本票内统一**（统一到桌面 wire 字面量）。

### P6 全量门禁与真机联调（收尾，一次跑全量）

见 §8。

---

## 6. 行为契约

### 6.1 广播（桌面插件）

- 连接生命周期：`client-connect` 早于该连接首个业务帧；未认证连接不收任何广播（宿主投递前已过认证门）。
- 事件不缓冲、不重放；无客户端（`clientCount==0`）静默跳过（不计错、不打 warn）。
- 投递失败（队列满/端点回收）→ `debug`/`warn` 留痕；**业务返回不受影响**（事件是旁路）。
- 顺序：同一客户端的广播按 `publish()` 调用序投递（宿主同连接帧序保证）。
- 载荷：snake_case、自足、无凭据；`task:status-changed` 可选字段（`task_reason`/`task_questions`）
  缺席即不出现键（不伪造空值）。
- 插件停用 / 端点注销 → 宿主回收连接（4005）；移动端自愈重建后对账补齐。

### 6.2 事件通道（移动端）

- 首帧必须认证；认证前收到的任何帧丢弃（防御畸形服务端）。
- 未知 `event` / 未知字段 / 畸形载荷 → 丢弃 + 留痕；**不 panic、不断连**。
- 消费幂等：同一事件重复到达（重连对账 + 回声）不得造成状态抖动（列表去重/收敛）。
- 事件面断开 ≠ 连接面断开：`ws_unexpected_disconnect` 语义保留（对端失联提示 + 自愈）。

### 6.3 终端流（移动端 ↔ 插件）

- 输出：裸字节按到达序入缓存；`ring_resync` 是**唯一**重锚信号（清屏 + 基准重置）；
  本地计数仅用于 ack 水位，不再作为绝对偏移发送给桌面。
- 尾帧与停止帧：`session_stopped` 到达前必须先消费已到达的输出帧（帧序保证，前端按序渲染）。
- 慢客户端：插件侧发送失败只停本人（游标不前进，下轮续拉）；移动端不得假设「未收到即丢失」。
- 会话不存在（启动竞态）→ `error` 帧 + 退避重试（沿用现重试上限语义）。
- 批量态：`mode:"poll"` 或 `{"type":"poll"}` 驱动；离开终端页必须退订/关闭，不得后台常拉。
- 历史：`subscribed` 后先渲染回放窗口；HTTP 回退仅在需要补齐窗口外数据时使用（`from` 用 ring 偏移）。

### 6.4 HTTP 控制面

- 错误语义沿用桌面口径：HTTP 200 + `{code:1002,message}` → 移动端映射为业务错误（`AppError::Auth`/`Internal` 按既有 `parse_envelope` 规则）。
- resize 裁决（`NeedsConfirmation` → force 覆盖）路径不变。

### 6.5 加密

- 插件端点（session-control / terminal）帧**永不加密**；移动端不得对这两条连接上装任何 WS codec。
- HTTP 信封加密保持现状（开关、strict 语义、pinning 不变）。

---

## 7. 依赖方向与源码锁

桌面（插件内）：
- `ws_events` 只依赖 `WasmHost`（ws 原语）+ `serde_json`；**不得** import 宿主业务类型 / `Message` / `SyncPayload`。
- 广播调用只允许经 `ws_events::broadcast_event`（结构锁：`ws_broadcast_text(` 实现段唯一出处 = `ws_events.rs`）。
- 载荷形状锁：广播 payload 的键为 snake_case（`session_id` / `task_status` / `auto_approve` …），
  拒绝 camelCase 混入（防「第三套键名」）。

移动端：
- 插件端点 URL 常量唯一出处（`system/constants/connection.rs`）；WS 路径字面量零散落。
- `Message::` 信封在 WS 生产路径**零使用**（结构锁：`terminal_link` / `event_ws` / 新事件路由实现段不得出现 `Message::`）。
- WS 加密：`install_event_crypto` 删除；`linkCrypto` 的 WS 通道在插件端点连接上零上装（结构锁 + 行为测试）。

---

## 8. 测试与验证

### 8.1 单测

桌面插件（`cd bedcode-desktop/wasm-apps/terminal-session/rust && cargo test`）：
- `ws_events`：帧壳形状、端点反查（含无端点 → None）、零客户端早退、字段 snake_case 锁、失败不 panic；
- 接入点：载荷形状（7 事件）、`task:status-changed` 合并字段（含/不含 reason、questions）；
- 结构锁：广播出口唯一、调用点数钉死。

移动端 Rust（`cd bedcode-mobile/src-tauri && cargo test`）：
- 事件帧路由（7 映射 + 未知/畸形）、对账触发、断线自愈；
- 终端协议：订阅帧、输入双形态、ack/poll、`ring_resync` 重锚、`session_stopped` 顺序、重连重订阅；
- 命令面 HTTP 迁移：成功/失败/形状。

移动端前端（`cd bedcode-mobile && pnpm run test:run`）：
- `terminalBuffer`：裸字节入缓存、resync 清屏重锚、缺口号不再误报、停止帧；
- `useTerminalSubscription`：`subscribed` 门控 + 超时兜底；
- `useMobileConnection`：事件驱动的会话/任务状态更新 + 对账幂等；
- 加密退役后的设置页行为。

### 8.2 集成

- 夹具闭环：假插件端点（session-control 事件 + terminal 二进制流）→ 移动端全链路（Rust → 前端事件/store）。
- 桌面侧（可选、按需）：插件广播在真实宿主上的投递（client-connect 后收事件、无客户端静默）——
  宿主零改动，优先用既有 `pty_session_chain` 类集成夹具扩一条推送断言。

### 8.3 真机联调（必须）

1. 终端：首订阅回放、大输出、Ctrl-C/特殊键、resize、停止帧、断线重连重锚；
2. 会话：列表/启动/停止/删除/历史；
3. 事件：桌面启动会话 → 移动端即时出现；移动端动作 → 桌面/移动端状态一致；任务状态/队列/模式/定时事件；
4. 断网重连 → 对账补齐（无事件丢失残留）；
5. 加密：HTTP 开启/关闭、插件端点连接无加密异常。

### 8.4 收尾命令

```bash
# 桌面插件（含产物重建）
cd bedcode-desktop/wasm-apps/terminal-session/rust && cargo test
node bedcode-desktop/scripts/plugin-build.js --plugin com.bedcode.terminal-session

# 移动端
cd bedcode-mobile/src-tauri && cargo test
cd bedcode-mobile && pnpm run test:run
pnpm exec eslint .          # 仓库根，0 error

# 宿主零改动判据
git diff --stat -- bedcode-desktop/src-tauri/src bedcode-desktop/packages/plugin-sdk-desktop
# 期望：空输出（随包产物 resources/plugins 除外）
```

- 测试两段式（AGENTS §3）：开发中只跑针对性单测；集成与全量回归留到 P6 一次跑。
- 跑测后清理残留进程/端口；cargo 用 rustup shim。

---

## 9. 风险与控制

| 风险 | 控制 |
| --- | --- |
| 广播成为新的「业务强耦合点」 | 唯一出口 + 结构锁 + 载荷 snake_case 锁；只接既有收口点，不新增事件语义 |
| 每事件一次宿主调用（`ws_list_endpoints`）开销 | path→id 缓存 + `clientCount==0` 早退；无客户端路径零宿主调用 |
| 事件丢失（断连期间）被误当「无变化」 | 重连对账强制（§6.2）；文档明示「事件不重放」 |
| 终端无 offset 后字节连续性不可自证 | 本地计数 + `ring_resync` 断点；契约测试锁定「resync 必清屏」；`error` 帧分类重试 |
| 现状 `waiting_input` 字面量漂移（既有 bug） | P5 统一到桌面 wire 字面量 + 用例锁 |
| 移动端旧信封协议残留消费者 | P3 清零后 `cargo check` + grep 兜底；结构锁（§7） |
| 加密退役后用户以为仍加密 | 设置页文案显式说明 + CHANGELOG；HTTP 加密保留并标注 |
| 桌面插件改动越界（动了宿主） | 收尾 `git diff --stat` 判据（§8.4）；评审时逐文件核对 |

---

## 10. 完成定义（Definition of Done）

- [ ] 桌面插件：`ws_events` 唯一广播出口；7 事件接入；单测（形状/出口/点数）通过；产物重建 + `wasmHash` 更新。
- [ ] 宿主零改动：`bedcode-desktop/src-tauri/src/**`、`packages/plugin-sdk-desktop/**`、`plugin.json`、WIT 零 diff。
- [ ] 移动端：终端流新协议端到端可用；会话控制/插件 API 全走 HTTP；事件通道恢复 `ws_sync_*`；重连对账生效。
- [ ] 移动端：WS 加密退役（含 i18n/设置页）；HTTP 信封加密回归通过。
- [ ] 旧信封协议消费者清零；结构锁落地并变异自检（旁路 → 转红 → 还原）。
- [ ] 移动端 `cargo test` 全量 + `pnpm run test:run` 全量 + 根 `eslint` 0 error；i18n zh/en 同步。
- [ ] 真机联调清单（§8.3）逐项通过；桌面插件产物已随包更新。
- [ ] 文档：`bedcode-mobile/docs/code-map.md`、`docs/knowledge/mobile-desktop-auth.md`、根 `AGENTS.md` §9（调用口径）
      与本 spec 状态同步；CHANGELOG（两端）如实登记破坏性变更与「配置推送退役/设备事件不推送」的收敛。

---

## 11. 参考

- `.scratch/2026-09-26-mobile-desktop-adaptation/survey.md`（本专项排查报告，含逐层证据）
- `.scratch/2026-09-25-websocket-business-downsink/spec.md` + `issues/08-host-business-hard-cut.md`（桌面 WS 硬切）
- `.scratch/2026-09-25-http-route-registration-downsink/spec.md`（HTTP 动态注册，移动端 URL 零改动锚点）
- `.scratch/2026-09-23-session-engine-downsink/`（会话真源下沉；`issues/06-mobile-output-history-pty-ring.md`）
- `.scratch/2026-09-25-peer-transfer-orchestration-downsink/mobile-impact.md`（对等面无跨端受损）
- 代码锚点：`wasm-apps/terminal-session/rust/src/{ws_control.rs,ws_terminal.rs,session/events.rs,task/{state,queue,scheduled}.rs}`
  （协议与事件收口）、`packages/plugin-sdk-desktop/rust/src/host/ws.rs`（广播原语）、
  `src-tauri/src/wasm_core/host_api/ws.rs`（宿实现）、`src-tauri/src/server/websocket/registry.rs`（端点广播）

---

## 12. 票据索引与测试节奏

本 spec 已拆分为 `issues/` 下 7 张票（索引见 [`issues/README.md`](issues/README.md)）：

| 票 | 阶段 | 内容 | Blocked by |
| --- | --- | --- | --- |
| [01](issues/01-baseline-and-fixture.md) | P0 | 基线与假插件端点夹具 | 无 |
| [02](issues/02-plugin-broadcast-events.md) | P1 / D1 | 桌面插件补广播（唯一出口 + 7 事件） | 无（帧壳定稿解锁 03） |
| [03](issues/03-mobile-event-channel.md) | P2 / M1+M2 | 移动端事件通道（session-control 常驻 + 对账） | 02 |
| [04](issues/04-mobile-control-plane-http.md) | P3 / M3 | 控制面迁 HTTP + 旧信封退役 | 03 |
| [05](issues/05-mobile-terminal-stream.md) | P4 / M4 | 终端流新协议重写 + 前端 TB v3 退役 | 01, 03 |
| [06](issues/06-encryption-retire-and-cleanup.md) | P5 / M5+M6 | WS 加密退役 + 清理 + 文档 | 03, 04, 05 |
| [07](issues/07-integration-run-and-final-gates.md) | P6 | **集成测试统一运行** + 全量门禁 + 真机联调 | 01–06 |

**测试节奏（强制）**：

- 每个实现票（01–06）只跑**针对性单元测试**自验（§8.1；AGENTS §3 测试两段式）。
- **单个票不运行集成测试**；但**可以写**集成测试（Rust `bedcode-mobile/src-tauri/tests/`、
  前端 `bedcode-mobile/src/__tests__/integration/`），在该票末尾「集成测试（待票 07 运行）」
  列出文件与用例名，**不在本票执行**。
- 全部实现票完成后，由**票 07 一次统一运行**所有集成测试 + 全量回归（§8.2/§8.3/§8.4）。
