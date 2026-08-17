# PTY 输出链路重构提案：前端直连 WS + 每会话连接 + 快照订阅（废除偏移量）

> 状态：提案待评审 · 关联文档：`docs/knowledge/pty-output-pipeline.md`
> 目标版本：桌面端与移动端同发（协议不兼容旧版，见「版本兼容」决策点）
> 实施顺序：**P0 认证机制重构（本节前）→ P1 桌面端 → P2 移动端 → P3 清理文档**

---

## P0. 认证机制重构（前置阶段）

> 目标：**WS 不再参与认证与连接状态**。配对/登录/JWT 签发全部走 HTTP；移动端建立连接不再需要 WS；WS 按需建立，连接认证依赖 JWT——首条消息完成认证后将该连接标记为已认证，之后才能正常通信。

### P0.1 现状（已确认事实）

- 桌面端**已有** HTTP API 层（`server/controllers/`）与四个认证端点：`POST /api/auth/pairing`、`/auth/verify`、`/auth/qr-connect`、`/auth/reauth`（`auth_controller.rs`）；另有 sessions/configs/file/git/plugin 等 HTTP 端点
- 但**移动端 Rust 无 HTTP 客户端**（无 reqwest/hyper）：认证完全走 WS 握手（`connect_and_pair` → RequestPairing → VerifyCode → Authenticated / QrConnect / Reauthenticate / Biometric* 阶段，`auth_service.rs::handle_auth` 实现）
- 桌面端已按连接维护认证状态：`WsSessionRegistry` 的 `authenticated` 标志 + `set_authenticated(addr...)`，未认证连接的消息被 gate；`Message` 每消息携带 `token` 字段
- JWT：`JwtService.generate_token(device_id, name, fingerprint)` / `verify_token_with_expiry`
- 移动端单个常驻 WS 同时承载：认证握手、同步事件推送（SyncData）、终端 I/O、文件服务消息

### P0.2 目标架构

```
移动端 Rust（新增 HTTP 客户端）                      桌面端
  ├─ 配对:  POST /api/auth/pairing ──────────────→ 生成配对码 + emit pairing-code-generated
  ├─ 验码:  POST /api/auth/verify ───────────────→ 验码(单次消费) + 签发 JWT + DB 记录 + DEVICE_CONNECTED
  ├─ QR:    POST /api/auth/qr-connect ───────────→ 验证 QR 令牌 + 签发 JWT
  ├─ 生物:  POST /api/auth/biometric-*（新增）────→ 一次性 challenge + 签名验证（替代 WS Biometric 阶段）
  ├─ 重认证: POST /api/auth/reauth ──────────────→ 校验旧 JWT + 换发/续期
  ├─ 存在性: POST /api/device/presence（新增）────→ 滑动 TTL 心跳，替代「WS 连接 = 设备在线」语义
  └─ 按需 WS（各自首消息 JWT 认证 → 连接标记已认证）
       ├─ 事件/同步通道（app 活跃时开启；决策点：或 HTTP 轮询替代）
       └─ 终端会话通道（P1：每会话一条，见 §2 新路由）
```

### P0.3 WS 认证规则（新）

1. 连接建立后，客户端首条业务消息必须是 `auth`（携带 JWT，即 `Reauthenticate` 阶段语义）；服务端 `verify_token_with_expiry` 通过 → `set_authenticated` → 回 `authenticated`
2. 已认证前：仅处理 auth 消息，其余一律拒绝（沿用现有 gate，auth 分支收敛为纯 JWT 验证）
3. 已认证后：**后续消息不再携带 token**（连接级信任）——`Message` 的 token 字段删除或标记 deprecated
4. 认证失败/过期：服务端关闭该连接；客户端走 HTTP 重认证后按需重开
5. 僵尸连接防护：规定时间内（如 10s）未完成首消息认证 → 服务端主动关闭

### P0.4 服务端改动清单（桌面端）

| 文件 | 改动 |
|------|------|
| `server/services/auth_service.rs` | 把 `handle_auth` 的核心逻辑（配对码生成/单次消费、JWT 签发、DB 配对记录/连接历史、DEVICE_CONNECTED 事件）抽取为**无连接依赖**的纯函数，WS 与 HTTP 共用；WS 版保留 `set_authenticated`，HTTP 版返回签发结果；新增 biometric challenge/verify 的 HTTP 逻辑 |
| `server/controllers/auth_controller.rs` | 核对/补全四个端点（**文件近期在编辑，实现时读最新**），复用抽取后的逻辑；新增 `/api/auth/biometric-*`、`/api/device/presence` |
| `server/ws/terminal_ws.rs` | auth 分支收敛：仅保留 JWT 首消息认证；删除 RequestPairing/VerifyCode/QrConnect/Biometric* 分支（或保留兼容，见决策点 D5） |
| `server/ws/message.rs` / `enums/auth.rs` | AuthStage 收敛（WS 只留 jwt/authenticated/failed）；token 字段处理 |
| `server/ws/registry.rs` | `WsSessionEntry` 增加 `channel_type`（event/terminal）字段——**广播去重**：同设备多按需连接时 SyncData 只发事件通道（或按 fingerprint 去重） |
| `server/app.rs` | HTTP 路由加 JWT 校验中间件（pairing/verify/qr 除外）；新增 presence 端点 |
| 设备在线语义 | DEVICE_DISCONNECTED / 连接历史 `close_open_connection_event` 的触发从「WS 断连」改为「presence 心跳超时」 |

### P0.5 移动端改动清单

| 文件 | 改动 |
|------|------|
| `Cargo.toml` | 新增 reqwest（HTTP 客户端） |
| `connection/manager.rs` + `ws_connection.rs` + `lifecycle.rs` | 连接生命周期重构：HTTP 认证成为唯一入口（`connect_and_pair` 删除或改 HTTP 实现）；WS 改为按需创建（事件通道 + 终端通道），各自完成首消息认证；presence 心跳任务（~10s 间隔，Rust 侧） |
| `handler/auth.rs` | 收敛为纯 JWT 首消息处理；配对/验码/QR/生物逻辑移入 HTTP 客户端 |
| `auth/manager.rs` | JWT 存取与刷新逻辑（`get_global_token` 保留：供 WS 首消息 + 前端 invoke） |
| `commands` | 新增 `get_ws_token` / `get_ws_url`（前端按需开 WS 用）；删除 WS 认证相关命令 |
| `router/event.rs` | 事件通道与终端通道分离 |
| 前端 `useMobileConnection` | 状态机适配 HTTP 认证（配对 UI 流程命令不变或微调） |

### P0.6 可行性评估

**结论：可行，桌面端 HTTP 认证端点已存在大半**，核心工作是逻辑抽取 + WS 收敛 + 移动端补 HTTP 客户端 + presence。

| # | 风险 | 影响 | 缓解 |
|---|------|------|------|
| R1 | JWT 持有者：Rust 还是前端 | 安全与可用性 | **Rust 持有**（设备身份/指纹/安全存储都在 Rust），前端按需 invoke 获取（对应 P1 的 `get_terminal_ws_info`） |
| R2 | 生物认证迁移 HTTP：challenge 清理时机从「WS 断连」变「设备生命周期」 | 挑战值残留 | `AppContext.biometric_challenges()` 按 device_id 管理，presence 超时或设备移除时清理 |
| R3 | presence 语义变更：设备「在线」从 WS 连接变为 HTTP 心跳 | 桌面端设备列表/连接历史 | 心跳间隔 ~10s / TTL ~30s；低功耗可降频；UI 在线状态展示对齐新语义 |
| R4 | 事件推送通道取舍 | 同步延迟/复杂度 | 决策点 D4：保留轻量事件 WS（推荐，延迟不劣化）vs 纯 HTTP 轮询（无 WS 常驻，延迟=轮询间隔，需事件游标） |
| R5 | 僵尸连接 | 资源占用 | 首消息认证超时（10s）主动关闭 |
| R6 | 旧移动端（v2.0.0）WS 认证对新桌面端 | 老客户端无法配对 | 决策点 D5：WS 保留 RequestPairing 等兼容分支（低成本）或与 P1 同版本强制升级 |
| R7 | 同设备多 WS 连接时广播重复 | 重复同步事件 | registry 按 channel_type / fingerprint 去重（P0.4 已含） |

> 注意：本提案撰写期间多个认证相关文件（auth_controller/auth_service/terminal_ws/message/registry 等）正在被编辑，实施前必须重新读取最新内容。

---

## 1. 背景与动机

现状（详见 `docs/knowledge/pty-output-pipeline.md`）：

```
桌面端 Rust                                   移动端 Rust                         移动端前端
PtyReader → SessionOutputManager.on_output     ws_client → router                 listen('ws_output')
  → 环形队列（字节偏移 min/max_offset）          → TerminalHandler                  → terminalBuffer store
  → 扇出 try_send → forward_loop                → MobileEvent::Output                → 字节游标连续性校验
  → base64 JSON 文本帧 → WS ←────────────────── → EventForwarder → emit ─────────→ → 自愈重订阅（指数退避）
  + output_broadcast（兼容通道，无监听）          + 插件 TerminalOutput 通知           → writeCoalescer → xterm
```

痛点：
1. **输出经移动端 Rust 中转两次**（Rust 解码 → Tauri event → JS 再 atob），base64 编解码 ×2，链路长、调试难；
2. **一个 WS 连接多路复用所有会话**：订阅/取消/流代数/终止旧流等复杂度集中在 TerminalWs actor；
3. **字节偏移 + 服务端裁决（incremental/reset）机制复杂**：游标、get_range 字节裁剪、pending 跳过快照、退避冷却自愈——大量防御性代码，只为服务"断点续传"，而真实场景绝大多数是整段重播。

---

## 2. 目标架构

```
桌面端 Rust                                   移动端前端（WebView 原生 WebSocket，直接连桌面端）
PtyReader → SessionOutputManager.on_output     useTerminalSocket（新 composable）
  → 环形队列（序号 min_seq/snapshot_seq）        → 建立连接 + JWT 认证
  → 每会话连接通道 try_send → forward_loop      → subscribe → 服务端快照
  → TB v2 二进制帧（seq + 原始字节）→ WS ←──────→ 历史帧（[min_seq..snapshot_seq]）
  → history_end → 实时帧（seq > snapshot_seq）    → 历史缓存 + 实时缓冲拼接
                                                → writeCoalescer → xterm
移动端 Rust：只保留主 WS（认证/同步/会话控制/文件服务）
  + 新增 get_terminal_ws_info 命令（给前端签发连接信息）
  + 监听前端上报的终端活动事件（维持插件通知）
```

三个要求的落点：

| 要求 | 落点 |
|------|------|
| 1. 前端直连 WS 取二进制，不走移动端 Rust | 新路由 `/ws/terminal/session/{session_id}`，输出为二进制帧（TB v2），移动端前端用浏览器 WebSocket 直连；移动端 Rust 输出路径（ws_client→router→EventForwarder→ws_output）整体删除 |
| 2. 服务端不广播，每会话一个连接 | 连接在握手时绑定单个会话；删除 output_broadcast 兼容通道（PtyOutputEvent broadcast + FrontendOutputHandler + pty-output-* emit）；扇出保留但仅为该会话的消费者，每个消费者独立通道 + 独立 forward_loop（现状已是此结构，去掉多路复用层） |
| 3. 废除偏移量，快照订阅 | subscribe 无参数；服务端原子快照 `snapshot_seq`；历史 = `[min_seq..snapshot_seq]` 按序传输 → `history_end` 控制帧 → 实时续传；前端历史/实时双缓冲拼接，消费完历史对接实时 |

---

## 3. 协议设计

### 3.1 连接与认证

- 前端 WS 地址：`ws://{host}:{port}/ws/terminal/session/{session_id}`（复用移动端 Rust 已知的 address/port）。
- 认证沿用现有 JWT 按消息校验模型：连接后发 `auth` 消息（token 由移动端 Rust `get_terminal_ws_info` 提供，即当前全局 JWT）。
- 绑定：actor 创建时即绑定 session_id，**不再有** `subscribed_sessions` 集合与 subscribe/unsubscribe 多路复用。
- 注册：以 `{device_id}:term:{session_id}` 作为 client_id 注册（与主 WS 的 client_id 区分），断开时按既有 stopping() 清理路径注销。

### 3.2 快照订阅时序（核心）

```
前端                          服务端
 │  connect + auth             │
 │  subscribe ────────────────→│ 原子快照：snapshot_seq = 当前最大序号
 │  ←─ 历史帧 [min_seq..snapshot_seq]（逐条/合并）
 │  ←─ history_end {snapshot_seq, min_seq, history_count}
 │  ←─ 实时帧（seq > snapshot_seq，持续）
```

- **原子性**：复用现有「占位 subscriber → 读历史 → 排空 pending → 写锁原子激活」机制（`session_output.rs`）。占位期间 `on_output` 把事件缓存到 pending（seq 必然 > snapshot_seq），排空在写锁内完成 → 顺序严格为 [历史][pending][实时]，无重复无遗漏。
- **废除**：`start_seq` 参数、`SubscribeMode`（incremental/reset）、`get_range(cursor)` 字节裁剪、pending 跳过快照逻辑（历史现在天然包含到 snapshot 为止的全部事件，pending 无需跳过）。
- **history_end 必须显式发送**（历史为空时也要发，前端靠它切实时模式）；也可在末帧加 flag 位，但空历史场景仍需要控制帧，故统一用控制帧。

### 3.3 帧格式

二进制输出帧（TB v2，替换 20B 帧头）：

```
magic(2) "TB" | version(1)=2 | flags(1) | seq(8 LE) | len(4 LE) | data
flags: 0x01 = is_waiting
```

- 废除 start_offset/end_offset（8+8=16B → seq 8B + len 4B，帧头 20B→16B）。
- 序号即连续性原语：`OutputEvent` 已有 `index` 字段，直接充当 seq（无新概念）。

JSON 控制帧（文本帧，同连接）：

```json
{ "type": "auth", "token": "..." }
{ "type": "subscribe_ok", "snapshot_seq": 42, "min_seq": 0, "history_count": 42 }
{ "type": "history_end", "snapshot_seq": 42 }
{ "type": "error", "code": "SESSION_NOT_FOUND", "message": "..." }
{ "type": "input", "data": "<base64>", "special_key": null }
{ "type": "session_stopped", "session_id": "..." }
```

- 输入也走此连接（满足"不走 Rust 后端"的完整闭环）；`Message::input` 的移动端路径删除。

### 3.4 前端拼接模型（要求 3 核心）

- 订阅确认前到达的帧 → 现有 `pending` 缓冲（保留）。
- **历史阶段**：`seq ≤ snapshot_seq` 的帧 → 立即写 xterm；`seq > snapshot_seq` 的帧（实时缓存）→ 按 seq 入缓冲。
- **history_end 到达**：按序 flush 实时缓存 → 进入实时模式（后续帧直写）。
- **前端历史缓存**：以 seq 为键保留本会话收到的帧（上限 ~16MB，LRU 淘汰），用途：
  - 页面重进（xterm 已销毁）：本地缓存立即回放 → 订阅后按 seq 跳过 ≤ last_rendered 的服务端帧（去重，不双写）；
  - 断线重连：跳过 ≤ last_rendered，只消费缺口（`min_seq > last_rendered + 1` 视为历史截断，toast 提示 + 清屏全量重播）。
- **连续性**：实时阶段收到 `seq > last_rendered + 1` → 帧丢失（背压 drop）→ 重发 subscribe（新快照，跳过 ≤ last_rendered）。这是唯一的自愈路径，替代旧 cursor + 退避冷却机制。

---

## 4. 文件级改动清单

### 桌面端 Rust（`bedcode-desktop/src-tauri/src/`）

| 文件 | 改动 |
|------|------|
| `session/session_output.rs` | `OutputEvent` 删除 start_offset/end_offset；`UnifiedOutputQueue` 的 min/max_offset 改 min_seq/max_seq；`get_range(cursor)` 改按 seq 取整段（无字节裁剪）；`subscribe()` 删除 start_seq/mode，改为快照订阅（占位→[min..snap]→pending→激活，删 skip 逻辑）；`snapshot_offset`（2J 点）保留为可选历史起点优化 |
| `server/ws/terminal_ws.rs` | 新增 per-session 构造（绑定 session_id）；删除 subscribed_sessions 集合/订阅取消多路复用；handle_subscribe 简化（无 start_seq）；stopping() 清理按 client_id 注销 |
| `server/ws/terminal_ws/forward.rs` | 远程通道改 TB v2 二进制帧（删 base64 JSON 形态）；OutputBuffer 删 offset 字段、保留合并（30ms/64KB）；本地通道帧头同步升级 |
| `server/ws/message.rs` / `enums/control.rs` | 协议类型：删 Subscribe.start_seq、SubscribeMode；新增 snapshot 语义字段 |
| `server/app.rs` | 新增路由 `/ws/terminal/session/{session_id}`（JWT 认证，同现有远程路由） |
| `pty/pty_reader.rs` + `pty/frontend_output_handler.rs` + `pty/pty_process.rs` | 删除 output_broadcast 通道 + FrontendOutputHandler + PtyOutputEvent broadcast 路径（兼容通道无监听，一并清理） |
| `commands/session.rs` | 删 `get_session_output_history`（无调用方）与 OutputCache（`session.rs` 中的 legacy 引用一并清理） |
| `session/session_manager.rs` | 删 FrontendOutputHandler::spawn 调用 |

### 桌面端前端（`bedcode-desktop/src/`）

| 文件 | 改动 |
|------|------|
| `composables/useTerminalOutputStream.ts` | 本地通道同步迁移：帧头解析 TB v2（seq），cursor→last_rendered_seq，forceResubscribe→快照重订阅 |
| `components/TerminalPreview.vue` | 配合 composable 接口微调（onTruncated 语义不变） |

> 决策：桌面端本地通道一并迁移，否则服务端删 offset 后本地通道失去依赖；且迁移成本低（一个小 composable）。两端统一为同一模型。

### 移动端 Rust（`bedcode-mobile/src-tauri/src/`）

| 文件 | 改动 |
|------|------|
| `handler/terminal.rs` + `router/event.rs` | 删除 Output → MobileEvent::Output → EventForwarder `ws_output` emit 链路（含广播通道订阅） |
| `commands/session.rs` | 删 `ws_subscribe_session` / `ws_leave_session`（保留 ws_join_session？前端已不用，一并删）；**新增 `get_terminal_ws_info` → `{ url, token }`**（从 ConnectionManager 状态 + 全局 JWT 组装；token 刷新时前端 WS 由其失效重取） |
| `router/event.rs` | 新增前端→Rust 活动事件监听（`terminal_output_activity` { session_id }）→ 插件管理器 TerminalOutput 通知（保持只传 session_id 的语义） |
| `connection/*` | 主 WS 不动（认证/同步/文件服务仍走它） |

### 移动端前端（`bedcode-mobile/src/`）

| 文件 | 改动 |
|------|------|
| `composables/useTerminalSocket.ts`（新） | 每会话 WS 生命周期：连接/认证/订阅/二进制帧解析/快照拼接/重连退避/序号去重；`binaryType = 'arraybuffer'` |
| `stores/terminalBuffer.ts` | 重写：删 cursor/start_offset/end_offset/连续性自愈/退避冷却；改为 socket 状态 + last_rendered_seq + snapshot_seq + 实时缓冲 + 历史缓存（16MB 上限）+ pending（保留） |
| `composables/useTerminalBuffer.ts` | subscribeSession/unsubscribeSession/forceReplay/prepareSession 语义改为 socket 驱动；handleReconnect 重开活跃会话 socket |
| `composables/useMobileCommands.ts` | 删 ws_subscribe_session/ws_leave_session；加 get_terminal_ws_info |
| `views/TerminalView.vue` / `components/TerminalInputBar.vue` | 输入改走 socket（JSON input 帧） |
| `composables/writeCoalescer.ts` / `useTerminalScroll.ts` | 不变（写入管线与滚动与传输无关） |

---

## 5. 可行性评估

### 5.1 结论：可行，无技术障碍

- WebView 原生 WebSocket + 二进制帧（`binaryType='arraybuffer'`）在 Android/iOS WebView 均为标准能力；
- 服务端所需全部机制已存在（环形队列、占位/pending/原子激活、每消费者独立通道）；本次是**删减 + 重组**，非新增能力；
- 序号（index）字段已存在，不需要引入新概念；
- 移动端 Rust 只需新增一个取连接信息的命令 + 删链路。

### 5.2 风险表（按严重度排序）

| # | 风险 | 影响 | 缓解 |
|---|------|------|------|
| R1 | **慢消费者丢帧的代价变重**：旧机制 drop 后 cursor 增量续传（便宜）；新机制 drop 后快照全量重播（长会话 128MB 环 → 分钟级） | 弱网移动端偶发卡顿重播 | ① 远程每连接通道容量 8192→32768（drop 极罕见）；② 历史起点可退化为最近 2J 快照点（已有机制，重播量=自上次清屏）；③ 监控 drop 计数日志 |
| R2 | **版本兼容**：旧移动端 APK（v2.0.0）连新版桌面端 → 旧协议（start_seq/offset/base64 JSON）不再被支持 | 旧客户端终端黑屏 | 推荐：新路由 `/ws/terminal/session/{id}` 与旧 `/ws/terminal` 并存，旧协议路径保留（低成本，改动独立）；移动端前端切换新路由。桌面本地通道迁移后旧远程路径仅作兼容 |
| R3 | JWT 过期/刷新时前端 WS 失效 | 终端断流 | 前端收到 401/关闭 → invoke `get_terminal_ws_info` 重取 token → 重连重订阅；实现时确认桌面端对过期 token 的关闭行为并复用主 WS 的重认证事件 |
| R4 | 多连接注册冲突：前端 WS 与主 WS 同 device_id | 注册表/清理错乱 | client_id 加后缀 `:term:{session_id}`；stopping() 清理路径验证 |
| R5 | WebView 发送 Origin 头可能被服务端校验拒绝 | 连接失败 | 实现时检查 `app.rs` 对远程路由的 Origin 处理（本地路由有环回校验先例，远程路由大概率不校验） |
| R6 | 插件 TerminalOutput 通知链路改道 | 插件行为回归 | 前端 socket 每收到输出帧 emit `terminal_output_activity`（仅 session_id，与现状一致），移动端 Rust 监听后转发插件管理器；验证 ocr 等插件 |
| R7 | 前端历史缓存内存 | 移动端内存压力 | 16MB/会话上限 + LRU；与 xterm scrollback 10000 行对齐 |
| R8 | 每页进入全量历史 [min..snapshot] 的带宽 | 长会话进入终端页慢 | 前端缓存立即回放（瞬时可见）+ 服务端帧按 seq 跳过；2J 起点优化；实测后决定是否加「from_seq」协商（若加，注意与"废除偏移"目标的关系——序号游标非字节偏移，可作为后续优化项） |

### 5.3 收益

- 输出链路：移动端 Rust 中转 + base64×2 删除 → **二进制直通**，延迟/CPU/内存全降；
- TerminalWs actor 复杂度大减：无多路复用、无流代数、无终止旧流（每连接即一会话，断连即清理）；
- 前端自愈逻辑大减：无字节游标、无 continuity 校验、无指数退避冷却 → 换成 seq 去重 + 快照重订阅；
- 服务端队列简化：无字节裁剪、无裁决模式；
- 顺带删除死代码：OutputCache/get_session_output_history、output_broadcast 兼容通道。

---

## 6. 实施阶段（建议）

| 阶段 | 内容 | 验证 |
|------|------|------|
| **P0 认证重构** | 桌面端：抽取 auth 逻辑 + HTTP 端点核对/补全 + WS 收敛（首消息 JWT 认证）+ presence + 广播去重；移动端：HTTP 客户端 + 连接生命周期重构 + 按需 WS + 心跳 | cargo test（HTTP 认证流程、WS 首消息 gate、presence TTL）+ 真机配对回归 |
| P1 桌面端 | 新路由 + 快照订阅 + TB v2 帧 + 删 broadcast/offset；**保留旧 /ws/terminal 路由不动**；本地通道迁移 | cargo test（新增：快照订阅顺序、seq 范围、帧编码）+ 桌面端本地终端回归 |
| P2 移动端 | Rust：删输出链路 + get_terminal_ws_info + 插件活动事件；前端：useTerminalSocket + store 重写 + 输入迁移 | vitest（拼接/去重/缺口/重连）+ 移动端真机回归 |
| P3 清理与文档 | 删旧远程协议（或降级为兼容标记）、删死代码、重写 `pty-output-pipeline.md` | 全量测试 + 双端联调 |

## 7. 待决策点

1. 旧协议兼容：并存（推荐）还是直接破坏（两端同发）？
2. 历史起点：严格 `min_seq`（用户原话）还是长会话退化为最近 2J 点（性能优先）？
3. 前端历史缓存上限与淘汰策略（16MB？与 scrollback 对齐？）
4. 插件 TerminalOutput 通知改道是否可接受（延迟增加一跳）
5. **P0 事件推送通道**：保留轻量事件 WS（推荐）vs 纯 HTTP 轮询？
6. **P0 旧客户端兼容**：WS 保留 RequestPairing 等兼容分支（低成本）vs 同版本强制升级？
7. **P0 JWT 持有**：移动端 Rust 持有（推荐，安全存储/指纹同侧）vs 前端 localStorage？
