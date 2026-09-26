# 移动端适配桌面端（桌面不动 / 只改移动端）— 排查报告

> 状态：**排查完成，待用户裁决开放点** ｜ 日期：2026-09-26 ｜ 分支：dev
> 口径：**桌面端零改动**；适配集中在「移动端调用桌面 API 的方式」。
> 依据：`.scratch/2026-09-25-websocket-business-downsink/spec.md`（WS 硬切，done）、
> `.scratch/2026-09-25-http-route-registration-downsink/spec.md`（HTTP 动态注册，done）、
> `.scratch/2026-09-23-session-engine-downsink/`（会话真源下沉，done）、
> `.scratch/2026-09-25-peer-transfer-orchestration-downsink/mobile-impact.md`（对等网络面已裁定无实际受损）。

---

## 0. 结论摘要（TL;DR）

| 面 | 桌面终态 | 移动端影响 | 结论 |
| --- | --- | --- | --- |
| HTTP 业务面（`/api/*`、`/api/plugin/*`、`/api/auth/*`、`/api/health`） | ABI v29 路由改插件代码注册，**对外 URL 与响应形状逐字不变** | 调用方式可零改动 | ✅ 基本零改动（仅 2 处走 WS 的入口要迁到 HTTP，见 A2） |
| WS 业务面 | `/ws/event`、`/ws/terminal/session/{id}` **已删除（404）**；只剩 `/ws/plugin/{plugin-id}/{path}` | 终端流与会话控制协议**全量重写** | ❌ 阻塞级，必须改（A1/A2） |
| WS 业务事件推送 | 宿主同步广播面已删（票 08）；**广播原语齐全但 wasm 应用零调用**——`broadcast-text/binary`、`list-clients` 在 SDK/宿实现/连接注册表三层都在，插件只用了单播回包 | `ws_sync_*` 全部失源（会话/配置/任务/队列/模式） | ⚠️ 阻塞级但**低成本可补**：插件侧收口点加广播即可（A3），需裁决 |
| WS 帧级链路加密 | `TrafficChannel::WsPlugin => false`；`derive_ws_session_ciphers / ws_register_ciphers` **生产已零调用者** | 移动端默认开启的 `ws-event/ws-terminal` 加密必须退役 | ❌ 阻塞级，必须改（A4） |
| 对等网络（peer/文件传输） | v31 传输编排下沉插件，wire 数据面零改动 | 无实际受损（已裁定） | ✅ 不动 |
| 认证（配对/QR/reauth/生物） | `/api/auth/*` 七条 URL 与信封不变 | 移动端已全走 HTTP | ✅ 不动 |

**一句话**：HTTP 面不用大动，**坏的是 WS 面（终端流 + 会话控制）**；推送面是「宿主能力全在、插件没调用」的**空档**（非设计禁用），补上只需在插件既有事件收口点加几行广播调用——但这一步在桌面插件里，是否算「桌面改动」需用户裁决。

---

## 1. 桌面端终态契约（移动端必须对齐的事实）

### 1.1 WS 面：只剩插件端点

```
/ws/plugin/com.bedcode.terminal-session/session-control   auth: jwt
/ws/plugin/com.bedcode.terminal-session/terminal          auth: jwt
```

- 握手：首消息固定帧 `{"type":"auth","token":"<jwt>"}`（宿主安全边界，非业务 `Message` 信封）；未认证 10s 超时 / 坏 token → close 4001。
- 宿主不解析帧：text/binary 原样转交插件（`server/websocket/channel/plugin.rs`）。
- 端点声明：`wasm-apps/terminal-session/plugin.json` → `contributes.wsEndpoints`。

**`terminal` 端点帧协议**（唯一事实源：`wasm-apps/terminal-session/rust/src/ws_terminal.rs` 文件头注释）：

| 方向 | 形态 | 帧 |
| --- | --- | --- |
| C→P | text JSON | `{"type":"subscribe","sessionId":"...","mode":"live"\|"poll"}`（mode 缺省 live） |
| C→P | text JSON | `{"type":"unsubscribe"}` / `{"type":"ack","offset":N}` / `{"type":"resync","offset":N}` / `{"type":"poll"}` |
| C→P | text JSON | `{"type":"input","data":"<UTF-8 文本，无控制字符>"}` |
| C→P | **binary** | 原始输入字节（含控制字符，Ctrl-C 等） |
| P→C | **binary** | 输出裸字节（`ring-fetch` 原始数据，**无 TB v3 16B 帧头、无 per-frame offset**） |
| P→C | text JSON | `{"type":"subscribed","sessionId","mode"}` / `{"type":"unsubscribed"}` / `{"type":"ring_resync","offset":N}` / `{"type":"session_stopped","sessionId","reason","exitCode"?}` / `{"type":"error","message"}` |

关键语义（与移动端现状的差异点）：
1. **订阅按 sessionId**，不再按 URL 路径绑定；无 `from_offset`——插件固定从游标 0 拉环形窗口（历史 = 环驻留字节），环已淘汰时先发 `ring_resync` 重锚。
2. **输出无偏移量**：客户端拿不到 `start_offset/end_offset`；只有 `ring_resync.offset`（环头）一个锚点。
3. **无 `history_end`**：订阅后即连续排空，历史与实时是同一条流。
4. 流控是 pull 模型：任何入站帧后做一轮有界 drain（≤8×16KiB），另有 1s tick 兜底（live 模式）；`ack`/`poll` 只是「再拉一轮」的触发信号，offset 仅需存在。
5. 输入：文本帧走 UTF-8 正文，**控制字符/特殊键必须走 binary 帧**（插件不做 base64、不认 `special_key` 字段）。

**`session-control` 端点帧协议**（`ws_control.rs` 文件头）：

- C→P text：动作 JSON（与旧 `Message::SessionControl.payload.action` 同形）：
  `{"type":"list_sessions"}`、`{"type":"start_session","config_id"}`、`{"type":"stop_session","session_id"}`、
  `{"type":"remove_session","session_id"}`、`{"type":"resize_session","session_id","cols","rows","force"?}`
- P→C text：响应动作 JSON（`session_list` 的 `sessions` 为 snake_case `SessionSummary`）；`resize_session` 无回包；
  失败 `{"type":"error","message"}`。**无 `Message` 信封、无 `message_id`**（请求关联丢失）。

### 1.2 HTTP 面：URL 与形状不变（移动端零改动的部分）

`http_routes.rs::ROUTES` 共 24 条 `/api` 别名 + `/static/terminal-bg`，其中移动端在用的：

- 会话：`GET /api/sessions`、`POST /api/sessions/start`、`POST /api/sessions/{id}/stop`、`POST /api/sessions/{id}/resize`、
  `POST /api/sessions/{id}/input`、`GET /api/sessions/{id}/history?from=`、`DELETE /api/sessions/{id}/remove`
  —— 响应形状按旧控制器逐字节复刻（`sessions_http.rs` 头注释），错误口径 `HTTP 200 + {code:1002}`。
- 配置/快捷指令：`GET /api/configs`、`GET /api/quick-actions`。
- 认证：`/api/auth/{pairing,verify,qr-connect,reauth,biometric-*}`（`auth` 档 `none`，公开入口）。
- 图/探测：`GET /api/health`（形状不变）、`GET /static/terminal-bg`。
- 任务域内部路径（移动端按旧 id 访问，**靠桌面兜底别名**）：`/api/plugin/com.bedcode.auto-task/{task-queue/*,session-mode,session-settings,task-history/*,supported-agents,scheduled-jobs/*}`
  —— `plugin_controller.rs::LEGACY_HTTP_PLUGIN_ALIASES` 把 `com.bedcode.auto-task` → `com.bedcode.terminal-session`（仅当旧插件未激活时生效；桌面已无 auto-task 插件）。
- 历史快照形状：`{code,message,data:{minOffset,snapshotOffset,historyBytes,dataBase64}}`。

### 1.3 推送面：**能力齐全，但 wasm 应用零调用**（四层证据）

**① 能力层（都在，且已实现）**
- SDK：`HostWebsocket` trait 有 `ws_broadcast_text` / `ws_broadcast_binary` / `ws_list_clients` / `ws_close_client`
  （`packages/plugin-sdk-desktop/rust/src/host/ws.rs:98-113`；WASM import 后端 `wasm_host.rs:720-746`）。
- 宿实现：`wasm_core/host_api/ws.rs:496 ws_broadcast_text`、`:520 ws_broadcast_binary`、`:549 ws_close_client`
  —— 权限门 `ws:server` + 属主仲裁 + 未注册端点 fail-visible。
- 连接注册表：`server/websocket/registry.rs:374 broadcast_binary_to_endpoint`（端点全体客户端；部分失败不回滚，返回成功数）。
- 权限位：terminal-session 的 `plugin.json` 已声明 `ws:server`。

**② 调用层（零调用）**
- 全量扫 `wasm-apps/**/*.rs`：WS 服务端原语只有 4 个**单播**调用点 ——
  `ws_control.rs:227`（控制动作回包给请求方）、`ws_terminal.rs:327`（输出二进制给订阅连接）、
  `:398`（终态尾帧给订阅连接）、`:433`（控制文本帧给指定客户端，含 subscribed/session_stopped/ring_resync）。
- `ws_broadcast_*` / `ws_list_clients` / `ws_close_client` **零命中**（全仓 wasm 应用）。

**③ 端点层**
- 四个 wasm 应用只有 `terminal-session` 声明 `contributes.wsEndpoints`（`session-control` / `terminal`）；
  `agent-hub` / `ai-chatbox` / `file-transfer` **无任何 WS 面**。

**④ 宿主桥层（也没有兜底路径）**
- `server/websocket/**` 零 bus 引用——不存在「插件发总线 → 宿主自动转发给 WS 客户端」的桥。
- `host-events.emit_event` = Tauri 事件（桌面 WebView，`host_api/events.rs:8`），不出 WS。
- 唯一「未被请求就推送」的路径是终端输出 drain（帧驱动 + 1s tick 兜底 `drain_all_on_tick`），推的是 PTY 字节，不是业务事件。

**结论**：旧 `/ws/event` 的业务广播原本由**宿主**实现（`broadcast-sync` → `Message::SyncData`，票 08 删除）；
下沉时插件接管了「事件生产」（`bus.publish` + `events.emit` 双通道），但**没有接管「向移动端 WS 广播」这一步**。
所以移动端 `ws_sync_*` 失源是**下沉漏项/能力空档**，不是宿主明确禁止——补齐属「插件自身实现」层面，**不需要动宿主内核、WIT/ABI 或新权限**。

**已有的插件事件收口点（补广播的落点，均已收口）**
| 域 | 收口函数 | 载荷与旧移动端 wire 的关系 |
| --- | --- | --- |
| 会话生命周期（created/stopped/removed） | `session/events.rs:99 publish()`（bus+emit 同形，5 个发布点喂它：created×1 / stopped×1 / removed×3） | `session` 字段 = snake_case `SessionSummary`、`session_id/session_name/source_device` —— **与旧 `SyncPayload::Session*` 同形** |
| 任务队列 | `task/queue.rs:1297 broadcast_queue_changed()` | `{session_id,queue_count,action,task_id,status}` = 旧 `ws_sync_task_queue_changed` 逐字 |
| 任务状态/会话模式 | `task/state.rs:330/365/501/908/1637`（bus）+ `:337/372/509/916/1621`（emit） | 与旧 `TaskStatusChanged` / `SessionModeChanged` 同字段族 |
| 定时任务 | `task/scheduled.rs:519/697`（bus）+ `:527/705`（emit） | 旧 `TaskScheduledChanged` |
| 预设任务 | `task/preset.rs:142`（bus）+ `:146`（emit） | 旧 preset-changed |
| 设备上下线 | `devices_events.rs:141/177`（emit） | 旧 `device-connected/disconnected`（移动端 `ClientDisconnected`） |

（注：载荷中 `source_device` 现已随事件下发；「排除发起端」语义由消费方自行按该字段过滤——移动端需按自身设备名去重。）

### 1.4 链路加密面

- `server/core/link_crypto.rs`：`TrafficChannel::WsPlugin => false`（插件端点帧不进链路加密，注释：「对端是第三方客户端，无成对密钥协商语义」）。
- `derive_ws_session_ciphers` / `ws_register_ciphers` 的生产调用者随旧 `/ws/event`·`terminal` 通道一并消失，**只剩测试引用**。
- HTTP 信封加密（`Http` 通道）不受影响，仍在生产链路中。

---

## 2. 移动端现状（需要改的调用点清单）

### 2.1 Rust 侧

| 位置 | 现状 | 桌面终态下 |
| --- | --- | --- |
| `src-tauri/src/system/constants/connection.rs:55,52` | `WS_EVENT_PATH = "/ws/event"`、`WS_TERMINAL_SESSION_PATH = "/ws/terminal/session"` | 两条路径均已 404 |
| `src-tauri/src/connection/event_ws.rs` | 认证成功后自动建常驻 `/ws/event`，断线退避自愈 | 建连必失败 → 反复重连与告警 |
| `src-tauri/src/connection/manager.rs:284 establish_event_ws` | 首消息发完整 `Message::Auth{Reauthenticate}` 信封；携链路加密提案并等回执 | 端点不存在；且新端点只认 `{"type":"auth","token"}` 极简帧 |
| `src-tauri/src/terminal_link.rs:1240` | `ws://host:port/ws/terminal/session/{id}` | 端点已删；新为 `/ws/plugin/com.bedcode.terminal-session/terminal` |
| 同上 `:1262` | 首帧 `{"type":"auth","token"}` | ✅ 形态一致（唯一无需改的握手） |
| 同上 `:1331` | `{"type":"subscribe","from_offset":N}` | 需改 `{"type":"subscribe","sessionId","mode"}`；`from_offset` 语义消失 |
| 同上 `:1340` | `{"type":"input","data":"<base64>","special_key"}` | 需改文本 `data`（UTF-8）或 binary 原始字节；插件不认 base64 / `special_key` |
| 同上 `:1361` | `{"type":"mode","mode":"realtime"\|"batch"}` | 无 mode 帧；模式在 subscribe 时定，或整连接重订阅 |
| 同上 ACK（`:1268-1306` `build_ack_frame`） | TB v3 二进制 ACK 帧 | 需改 `{"type":"ack","offset":N}` 文本帧 |
| 同上 TB v3 常量与解析（`:108-115`、收帧处理段） | 16B 头 + `[start_offset,end_offset)` 校验、缺口重拼、截断清屏、`history_end` 门控 | 新协议无 per-frame offset → 缓存/游标/缺口模型需重构 |
| `src-tauri/src/session.rs`（SessionManager start/stop/remove） | 全走 WS `Message::SessionControl` + `send_and_wait` | 通道不存在；应改 HTTP 或新 session-control 端点 |
| `src-tauri/src/commands/session.rs:23,122,70` | `ws_load_sessions` / `ws_load_session_configs` / `get_terminal_ws_info`（返回旧 URL） | 同上；`get_terminal_ws_info` 返回的 URL 已失效 |
| `src-tauri/src/commands/terminal.rs:16` | `ws_send_input_async` → WS `Terminal::Input` | 通道不存在 |
| `src-tauri/src/connection/request.rs`、`request_response.rs`、`codec.rs`、`model/message.rs`、`router/`、`handler/` | 整套 `Message` 信封协议（11 变体）+ message_id 请求关联 + SyncData 分发 | 桌面已无对应 wire；可整体退役或大幅收缩 |
| `src-tauri/src/enums/special_key.rs` | 已有 `KeyCombo::to_pty_bytes()`（`:256`、测试 `:690`） | ✅ 可直接复用为「特殊键 → binary 帧」翻译 |

### 2.2 前端侧

| 位置 | 现状 | 影响 |
| --- | --- | --- |
| `src/stores/terminalBuffer.ts:317-360` | 解析页面 Channel 的 TB v3 16B 头；`:367 handleFrame(startOffset,endOffset)`；`:536` 缺口重拼；`:683` 历史拼接 | 新协议是裸字节流 → 帧解析器与游标/缺口模型重写 |
| `src/composables/terminal/useTerminalSubscription.ts` | 历史门控等 `phase === 'live'`（由 `history_end` 驱动），8s 兜底 | 无 `history_end` → phase 模型重定义 |
| `src/composables/useMobileConnection.ts:271-382` | 消费 `ws_sync_config/session/task_*` 更新列表与状态 | 失源（A3） |
| `src/composables/useNotification.ts`、`usePresetTasks.ts`、`presetTaskState.ts` | 任务通知与预设完成匹配依赖 `ws_sync_task_status_changed` / `ws_sync_task_queue_changed`（后者经 `bedcode:task_queue_changed` 自定义事件转发） | 失源（A3） |
| `src/plugin/context.ts:141` | 插件 API `session.list` → `wsLoadSessions()`（WS） | 需改 HTTP |
| 同上 `:124` | 插件 API `terminal.sendInput` → `wsSendInput()`（WS） | 需改 HTTP `/api/sessions/{id}/input` 或新终端端点 |
| `src/composables/useHttpApi.ts` | 会话/配置/文件/git/任务/认证全走 HTTP，URL 与形状均与桌面终态一致 | ✅ 无需改（除 `httpSetSessionMode` 的旧插件前缀，见 B1） |
| `src/services/linkCrypto.ts`、`src/composables/useLinkEncryption.ts:37-38` | `encryptWsTerminal` / `encryptWsEvent` **默认 true**，pin 存在时会上装 WS 加密 codec | 必须退役 WS 通道加密（A4） |
| `docs/knowledge/mobile-desktop-auth.md` §3.2、code-map 中 ws/终端链路描述 | 部分仍写旧 `/ws/terminal` 与 `Message` 信封 | 文档跟演（P2） |

---

## 3. 工作项分解

### A. 阻塞级（不改则功能不可用）

**A1. 终端流通道重写（最大工作量）**

- 端点常量：新增 `/ws/plugin/com.bedcode.terminal-session/terminal`（`system/constants/connection.rs`）。
- 握手：保持 `{"type":"auth","token"}`。
- 订阅：`{"type":"subscribe","sessionId":<链路会话 id>,"mode":"live"}`；重连后重订阅（不再有 `from_offset`，靠 `ring_resync` 重锚）。
- 输入：可打印文本走 `{"type":"input","data":...}`；控制字符/特殊键走 **binary 帧**（复用 `KeyCombo::to_pty_bytes()`）；`data` 不再 base64。
- 流控：渲染 ack 改为 `{"type":"ack","offset":<本地已渲染字节数>}`（触发下一轮 drain）；离页/批量态用重订阅 `mode:"poll"` 或 `{"type":"poll"}`（设计取舍）。
- 收帧：binary = 裸字节直接入缓存（本地字节计数，不再有 per-frame offset 校验）；text 控制帧处理 `subscribed`/`unsubscribed`/`ring_resync`/`session_stopped`/`error`。
- 缓存与缺口语义重构：`ring_resync` 是唯一重锚信号（清屏 + 以 offset 为新基准）；本地连续性只能靠「自计数 + 收到 resync 即视为断点」。
- 历史：主路径改为「订阅即回放环形窗口」；HTTP 回退 `GET /api/sessions/{id}/history?from=` 形状不变，`from` 需以 `ring_resync` 报的环偏移为基准对齐。
- 前端：`terminalBuffer.ts` 帧解析器/游标逻辑与 `useTerminalSubscription` 的 live 门控跟演。
- 单测/集成：现有用例全部基于 TB v3 与旧控制帧，需重写；建议补一个「假插件端点」协议夹具（text/binary 双形态）做契约测试。

**A2. 会话控制与服务调用迁移**

- 三选一（推荐 HTTP，理由：URL/形状已验证保持一致，且免去无 `message_id` 的请求关联难题）：
  1. `SessionManager` start/stop/remove、`ws_load_sessions`、`ws_load_session_configs` → 改走 HTTP `/api/sessions*`、`/api/configs`；
  2. 或接新 `session-control` WS 端点（动作 JSON + 回包 JSON；请求关联需自建单飞/顺序约定）；
  3. 混合：写操作 HTTP，订阅/推送（若将来补）WS。
- 插件 API 面（`session.list`、`terminal.sendInput`）同步迁移，保持插件权限判定不变。
- `get_terminal_ws_info` 要么删除（旧前端直连路径已无消费者），要么返回新端点 URL。

**A3. 事件/推送面（需用户裁决）**

- 事实（修正后，见 §1.3）：**广播能力齐全且已实现，但 wasm 应用零调用** → 移动端 `ws_sync_*` 失源是「下沉漏项」，不是设计禁止。
- 失源清单（移动端全部 `ws_sync_*`）：会话创建/状态/停止/删除、配置增删改、任务状态、任务队列、定时任务、会话模式；另 `ClientDisconnected`（设备离线）亦无来源。

**方案 a（严格「桌面零改动」）：移动端轮询 + 乐观更新**
- 会话列表：`GET /api/sessions` 定时对账（间隔/退避需定）+ 本地动作后立即拉取；
- 任务面：按会话 `GET /api/plugin/com.bedcode.terminal-session/task-status?session_id=`、`task-history/current`、`task-queue/list`（内部路径可用，JWT 档）；
- 代价：任务通知/预设任务完成匹配（`usePresetTasks`/`presetTaskState`）由「事件驱动」降级为「轮询近似」，实时性从亚秒降到轮询周期；需接受或在 UI 上弱化承诺。

**方案 b（推荐，插件侧补齐广播：不动宿主内核 / WIT / ABI / 权限）**
- 落点极为集中（§1.3 收口点表）：`session/events.rs::publish()` 一处覆盖会话生命周期；
  `task/queue.rs::broadcast_queue_changed()` 一处覆盖队列；`task/state.rs`、`scheduled.rs`、`preset.rs`、`devices_events.rs` 各 1–2 处。
- 实现形态：在这些收口函数里追加一次 `ws_broadcast_text(<endpoint_id>, frame)`；端点句柄可经插件已有
  `resolve_endpoint_path()`（`lib.rs:605`，按 `ws_list_endpoints` 缓存）反查——**无需新增权限**（`ws:server` 已声明）。
- 帧壳需约定（插件自定义，建议最小化）：
  `{"type":"event","event":"session:created","payload":{...}}`（payload 沿用现有自足载荷，与旧 `SyncPayload` 同形）。
- 移动端收益：可**复用现有消费链**（`handler/sync.rs` → `MobileEvent` → 前端 `ws_sync_*` 监听）——只换传输层
  （端点 + 极简认证帧 + 帧壳），A3 从「轮询降级」变为「小改动对齐」；旧 `/ws/event` 的整个事件管道可保留。
- 需一并定的细节：推在哪条端点（复用 `session-control`，或新增第三个端点 `events` 更干净）；是否排除发起端
  （载荷已带 `source_device`，可移动端自行过滤，避免自己动作的回声刷新）；慢客户端/无客户端时广播返回 0 不计错。
- 口径提示：改动发生在**桌面端插件工程**内，但不动宿主内核——是否计入「桌面端改动」请用户裁定；
  若不接受任何桌面改动，则只能走方案 a。

- 补充：即使走方案 b，`/ws/plugin/.../session-control` 也可用于「列表拉取」（替代轮询点查询）。

**A4. WS 链路加密退役**

- `useLinkEncryption` 的 `ws-terminal` / `ws-event` 两个子开关改为不再上装 codec（或直接删开关 + i18n 文案），保留 HTTP 信封加密。
- `establish_event_ws` 的协商路径（提案 + 回执 + `install_event_crypto`）随事件 WS 处理一并退役，避免「加密帧直送插件端点 → 插件解析失败」。
- 需同步移动端设置页（外观/连接/认证之外的「链路加密」分组）与相关单测/集成。

### B. 需核对/小改

- **B1** 任务面旧前缀：`/api/plugin/com.bedcode.auto-task/*` 依赖桌面兜底别名；建议移动端直连 `com.bedcode.terminal-session` 前缀，去掉对兜底表的隐式依赖。
- **B2** `httpSetSessionMode` body 形状与桌面一致（snake_case `session_id` + `auto_execute`/`auto_answer`），已核对通过；`auth:none` 匿名可达，无需 JWT —— 保持现状即可。
- **B3** 认证 HTTP 链路（`auth/http.rs`）与桌面 `/api/auth/*` 形状对称（含 `kdPublicB64`）→ 零改动；但需删除/停用「WS 首消息 `Message::Auth`」的残留调用面（`AuthRequest::reauthenticate*`）。
- **B4** `httpProbe`（`/api/health`）与 `/static/terminal-bg` 不变。
- **B5** 死代码清理：`model/message.rs`、`connection/{codec,request,request_response}.rs`、`router/{registry,event}.rs`、`handler/{sync,auth}.rs` 的 SyncData/SessionConfig/SessionEvent 分支——迁移后按「无消费者即删」处理（注意与移动插件契约的 re-export 垫片区分）。
- **B6** 文档跟演：`docs/knowledge/mobile-desktop-auth.md`（§3.2 WS 连接、§关键源码索引的 `utils/auth/*` 与 `terminal_ws/**`）、`bedcode-mobile/docs/code-map.md`（connection/event_ws、terminal_link、handler/sync 描述）、根 `AGENTS.md` §9 协议节（补「移动端 WS 面硬切后的调用口径」）。

### C. 移动端→桌面端调用映射速查（终态）

| 移动端需求 | 终态调用方式 |
| --- | --- |
| 探测连通性 | `GET /api/health`（不变） |
| 配对 / 验码 / QR / reauth / 生物 | `POST /api/auth/*`（不变，HTTP） |
| 会话列表 | `GET /api/sessions`（HTTP，形状不变） |
| 启停/删除/resize/输入/历史 | `/api/sessions*`（HTTP，形状不变） |
| 会话配置 / 快捷指令 | `GET /api/configs`、`GET /api/quick-actions` |
| 文件树/内容/差异/git | `/api/file-*`、`/api/diff-tree`、`/api/git/*`（不变） |
| 任务队列/状态/历史/定时 | `/api/plugin/com.bedcode.terminal-session/*`（旧 id 前缀靠兜底，建议改新 id） |
| 终端输出/输入流 | `/ws/plugin/com.bedcode.terminal-session/terminal`（**新协议**） |
| 会话控制（如需 WS） | `/ws/plugin/com.bedcode.terminal-session/session-control`（**新协议**） |
| 业务事件推送 | **当前无推送实现**；方案 a = 移动端轮询，方案 b = 插件收口点补 `ws_broadcast_text`（推荐） |

---

## 4. 待用户裁决的开放点

1. **A3 推送口径**：接受「轮询 + 乐观更新」（方案 a，严格桌面零改动），还是允许「terminal-session 插件收口点补 `ws_broadcast_text`」（方案 b：不动宿主内核/WIT/ABI/权限，但改的是桌面端插件工程源码）？方案 b 可让移动端复用既有 `ws_sync_*` 事件管道，体验与旧版等价。
2. **A2 迁移形态**：会话控制统一走 HTTP（推荐），还是并行接 `session-control` WS 端点（为将来推送留管道）？
3. **A1 模式语义**：移动端「实时 / 批量」双速如何映射到 `live`/`poll`（重订阅 vs poll 帧）——涉及重连与游标重锚路径的复杂度。
4. **A4 加密开关**：直接删除 `ws-terminal`/`ws-event` 子开关，还是保留 UI 但置灰并注明「已随桌面端退役」？（涉及 i18n 与设置页测试）

---

## 5. 验证与联调清单（任务收尾用）

- Rust：`cd bedcode-mobile/src-tauri && cargo test` 全量（协议单测重写 + 新夹具）。
- 前端：`cd bedcode-mobile && pnpm run test:run` 全量；根 `pnpm exec eslint .` 0 error；i18n zh-CN/en 同步。
- 真机联调（必须有，单测覆盖不了真实桌面插件）：
  1. 终端：首订阅回放环形窗口、大输出（含 128KiB/轮排空）、Ctrl-C 与特殊键、resize、会话停止帧、断线重连后重锚；
  2. 会话：列表/启动/停止/删除/历史（HTTP 路径）；
  3. 任务：队列增删改查、状态轮询、通知触发；
  4. 加密：HTTP 信封加密开启/关闭、WS 通道加密退役后无异常。
- 契约回归：桌面 `pty_session_chain` / `ws_auth_rules` 已按新协议锁死，移动端侧建议补一份对称协议契约测试（帧形状逐字段锁）。

---

## 6. 证据索引

- 桌面 WS 硬切：`.scratch/2026-09-25-websocket-business-downsink/spec.md`、`issues/08-host-business-hard-cut.md`
- 插件终端协议：`bedcode-desktop/wasm-apps/terminal-session/rust/src/ws_terminal.rs`（文件头帧协议）
- 插件会话控制协议：`.../ws_control.rs`（文件头 + `handle_action` 词表）
- HTTP 注册表与形状锚点：`.../http_routes.rs`、`.../sessions_http.rs`、`src-tauri/src/server/http/controllers/plugin_controller.rs`（`LEGACY_HTTP_PLUGIN_ALIASES`）
- 推送能力（都在）：`packages/plugin-sdk-desktop/rust/src/host/ws.rs:98-113`（trait）、`wasm_host.rs:720-746`（import 后端）、
  `src-tauri/src/wasm_core/host_api/ws.rs:496/520/549`（宿实现，`ws:server` 门禁）、`server/websocket/registry.rs:374`（端点广播）
- 推送零调用：`wasm-apps/**` 内 `ws_broadcast_*`/`ws_list_clients`/`ws_close_client` 零命中；WS 服务端原语仅 4 个单播点
  （`ws_control.rs:227`、`ws_terminal.rs:327/398/433`）；仅 terminal-session 声明 `wsEndpoints`
- 无宿主桥：`server/websocket/**` 零 bus 引用；`host_api/events.rs:8` 为 Tauri emit（桌面 WebView）
- 补广播落点（插件收口点）：`session/events.rs:99 publish()`、`task/queue.rs:1297 broadcast_queue_changed()`、
  `task/state.rs`（bus `:330/365/501/908/1637` + emit `:337/372/509/916/1621`）、`task/scheduled.rs:519/697/527/705`、
  `task/preset.rs:142/146`、`devices_events.rs:141/177`；端点句柄反查 `lib.rs:605 resolve_endpoint_path()`
- WS 加密退役：`src-tauri/src/server/core/link_crypto.rs:776-779`（`WsPlugin => false`）、`derive_ws_session_ciphers` 仅测试引用
- 移动端现状：`src-tauri/src/terminal_link.rs`、`connection/{event_ws,manager}.rs`、`commands/{session,terminal}.rs`、`session.rs`、`router/event.rs`、`handler/sync.rs`
- 移动端前端：`src/stores/terminalBuffer.ts`、`src/composables/useMobileConnection.ts`、`useMobileCommands.ts`、`useLinkEncryption.ts`、`src/plugin/context.ts`
