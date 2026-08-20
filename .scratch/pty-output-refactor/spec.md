# Spec: 认证机制重构（P0）+ PTY 输出链路改造（P1-P3）

> 状态：**已实施（P0/P1/P2/P3 全部完工，2026-08-19 归档）** · 关联：`.scratch/pty-output-refactor/proposal.md`（思考过程）、`docs/knowledge/pty-output-pipeline.md`（已按新架构重写）
> 范围：bedcode-desktop（主机）+ bedcode-mobile（远程终端），双端同发
> **实施记录**：ticket 01~11 全部 done（见 issues/）；提交 7112f2b1 / 6269e520 / 3e62d6b4 / 4aa0a0b4（桌面 P1）/ e1e28256（桌面 P3 死代码清理）/ 853ca8fd（移动 Rust 拆除）/ ticket10（移动前端直连）。双端全量测试绿：桌面 cargo 549 lib + 8 集成、vitest 46 文件 415；移动 cargo 373 lib + 30 集成、vitest 20 文件 203。
> 遗留项：真机弱网回归清单（spec §10）、旧路由 compat 拆除（观察期后）、移动端 wsGetTerminalIncremental 死代码清理。
> 本文档为实现规范：协议、状态机、验收标准以本文为准。

---

## 1. 概述

### 1.1 目标

1. **认证与连接状态完全 HTTP 化**：WS 不再参与认证握手与设备在线语义；配对/验码/QR/生物认证/重认证全部走 HTTP；JWT 成为唯一凭证。
2. **WS 按需建立**：移动端无常驻 WS；连接按需发生（终端会话、事件同步），每个连接以**首条消息携带 JWT** 完成认证，服务端将该连接标记为已认证后放行正常通信。
3. **移动端前端直连终端 WS 取二进制输出**：PTY 输出不再经移动端 Rust 中转，前端 WebSocket 直取原始字节（二进制帧）。
4. **服务端每会话独立输出连接**：废除广播式输出分发；每个会话的输出走其专属连接。
5. **废除字节偏移量机制**：改为「订阅即快照」——服务端在订阅时刻原子快照当前输出序号，历史 = `[min_seq .. snapshot_seq]` 全量传输；前端同时缓存历史与实时，消费完历史后对接实时。

### 1.2 非目标

- 不改变 PTY 读取、环形队列容量策略（25000 条 / 128MB）等既有行为
- 不改变桌面端本地终端（TerminalPreview）的写入管线（rAF 合并 + DEC 2026）——其传输层随 P1 统一迁移到快照模型
- 不引入新传输通道（无 SSE、无 UDP）

### 1.3 术语

| 词 | 含义 |
|----|------|
| seq | 输出事件序号（现有 `OutputEvent.index` 复用，全局单调递增） |
| snapshot_seq | 订阅时刻服务端最大已产出序号 |
| history_end | 控制帧，标记历史段结束、实时段开始 |
| 事件通道 | 移动端按需建立的同步/推送 WS（P0） |
| 终端通道 | 每会话一条的输出/输入 WS（P1） |

---

## 2. 现状（已核实）

### 2.1 认证现状

- **桌面端 HTTP 认证端点已存在且完整**（`server/controllers/auth_controller.rs`，全部走 `AppContext::global()`）：
  | 端点 | 行为 | 响应 |
  |------|------|------|
  | `POST /api/auth/pairing` `{device_name}` | 生成配对码 + emit `pairing-code-generated` | `{pairing_code, expires_in}` |
  | `POST /api/auth/verify` `{pairing_code, device_id, device_name, fingerprint, address}` | 单次消费验码 → 签发 JWT → `add_pairing` + 连接历史 + `DEVICE_CONNECTED` | `{token, expires_in}` |
  | `POST /api/auth/qr-connect` `{qr_token, device_id, device_name, fingerprint, address}` | `qr_manager.verify` → 同 verify 流程 | `{token, expires_in}` |
  | `POST /api/auth/reauth` `{session_token, fingerprint}` | `verify_token_with_expiry` → 换发新 JWT + last_seen 更新 | `{token, expires_in}` |
- **移动端 Rust 无 HTTP 客户端**（无 reqwest/hyper 依赖）：配对/认证 100% 走 WS 握手（`connect_and_pair` → `handler/auth.rs` → WS `Auth` 消息，`AuthStage::{RequestPairing, VerifyCode, QrConnect, Reauthenticate, Biometric*}`）。
- 桌面端 WS 连接已按连接维护认证态：`WsSessionRegistry`（client_id → `{actor_addr, socket_addr, device_name, fingerprint, authenticated, connected_at}`），`set_authenticated(addr...)` 由 `auth_service::handle_auth` 调用；未认证连接的消息被 gate。
- 每个 `Message` 携带 `token` 字段（逐消息校验时代）。
- 生物认证（`BiometricRequest/Challenge/Verify`）仅存在于 WS 路径，challenge 存于 `AppContext.biometric_challenges()`（按 addr 键控，WS 断连时清理）。
- 设备在线语义 = WS 已认证连接存在；`DEVICE_DISCONNECTED` + 连接历史 `close_open_connection_event` 在 `TerminalWs::stopping()` 触发。
- JWT：`JwtService.generate_token(device_id, name, fingerprint)`；`verify_token_with_expiry`；`DEFAULT_TOKEN_EXPIRY_SECS`。

### 2.2 PTY 输出现状

见 `docs/knowledge/pty-output-pipeline.md`（服务端真源 + 字节游标 + 服务端裁决 incremental/reset）。要点：

- 移动端链路：桌面 Rust（base64 JSON 文本帧）→ 移动 Rust（`ws_client` → `TerminalHandler` → `MobileEvent::Output` → `EventForwarder` → `emit("ws_output")`）→ 前端（`terminalBuffer` store：字节游标连续性校验 + 指数退避自愈）→ xterm
- 服务端：`PtyReader` 双通道分发（`GlobalOutputManager` + `output_broadcast` 兼容通道）；`UnifiedOutputQueue` 字节偏移（min/max/snapshot_offset）+ `get_range(cursor)` 字节裁剪；订阅裁决（incremental/reset）+ 占位/pending/原子激活
- 桌面本地通道：`/ws/terminal/local` TB 二进制帧（20B 帧头含 offset）+ 游标连续性
- 输入：移动端前端 → invoke → 移动 Rust `Message::input` → 桌面 WS `TerminalAction::Input`

---

## 3. 目标架构

```
桌面端                                                   移动端
┌─────────────────────────────────────────┐              ┌──────────────────────────┐
│ HTTP API（认证/会话/配置/文件/git）        │◄──HTTP──────│ Rust（reqwest 新增）       │
│   /api/auth/*（已有，补生物认证）          │              │  配对/QR/生物/reauth       │
│   /api/sessions/*（已有）                 │◄──HTTP──────│  JWT 持有（安全存储）       │
│                                          │              │  get_ws_token 命令 ──→ 前端│
│ WS（按需，首消息 JWT 认证 → 标记已认证）    │              │                          │
│   /ws/event（新增，事件/同步通道，常驻）     │◄──WS(常驻)──│ Rust：HTTP 认证成功后建立，  │
│                                          │              │ 意外中断按现有规则自动重连   │
│   /ws/terminal/session/{id}（P1 新增）    │◄──WS(前端)──│ 移动前端（P2）直接连        │
│     TB v2 二进制帧 + JSON 控制帧          │              │  useTerminalSocket        │
│   /ws/terminal（旧路由，P1 保留兼容）      │              │  旧客户端兼容              │
│ PTY: PtyReader → 环形队列(seq) → 快照订阅  │              │                          │
└─────────────────────────────────────────┘              └──────────────────────────┘
```

---

## 4. P0 认证机制重构

### 4.1 HTTP 认证 API（规范）

沿用现有 4 端点（行为已确认，见 §2.1），新增：

| 端点 | 请求 | 行为 | 响应 |
|------|------|------|------|
| `POST /api/auth/biometric-challenge` | `{device_id, device_fingerprint}` | 为该设备生成一次性 challenge（替代 `BiometricChallenge` 阶段；存储从按 addr 改为按 device_fingerprint） | `{challenge_nonce, expires_in}` |
| `POST /api/auth/biometric-verify` | `{device_id, device_fingerprint, challenge_nonce, signature}` | `verify_biometric_signature` → 签发 JWT + DB 记录 + `DEVICE_CONNECTED` | `{token, expires_in}` |

> 注：无 HTTP presence 端点——**设备在线语义由常驻事件 WS 承担**（见 §4.2），WS 本来就不参与认证，与用户要求一致。

统一响应包裹：`ApiResponse<T>`（已有）。

### 4.2 设备在线语义（常驻事件 WS）

- 定义：**设备在线 ⇔ 该设备至少一条已认证的常驻事件 WS 连接存活**（指纹匹配 `WsSessionRegistry`）。
- 判定：桌面端查询 registry 中该 fingerprint 是否存在 channel_type=Event 且 authenticated 的连接。
- 离线通知：设备最后一条事件 WS 断开 → 触发 `DEVICE_DISCONNECTED` + 连接历史 `close_open_connection_event` 回填。
- 终端通道断开**不**触发离线（瞬态通道）；事件 WS 断开即离线（app 退后台被 OS 挂断 → 桌面端自然显示离线，前台恢复重连后自动回在线）。

### 4.3 WS 认证规则（所有 WS 路由通用）

状态机（连接级，复用 `WsSessionRegistry.authenticated`）：

```
连接建立(Unverified)
  ├─ 10s 内未收到有效 auth → 服务端关闭（防僵尸）
  ├─ 首条消息 = auth { token: JWT } → verify_token_with_expiry
  │     ├─ 通过 → set_authenticated(claims.sub, claims.device_name, claims.fingerprint)
  │     │          → 回复 authenticated → 进入 Verified
  │     └─ 失败 → 回复 error(INVALID_TOKEN) + 关闭
  └─ 首条消息非 auth → 拒绝（error 或直接关闭）
Verified
  ├─ 正常处理业务消息（不再校验 token）
  └─ 连接断开 → 清理该连接（注册、订阅、输出流）
```

协议变更：
- WS `AuthStage` 收敛为：`Reauthenticate`（JWT 首消息）、`Authenticated`（回复）、`Failed`（回复）。其余阶段（RequestPairing/VerifyCode/QrConnect/Biometric*）**从 WS 删除**（兼容策略见 §7）。
- `Message` 的 `token` 字段废弃（可保留字段但不再使用/校验，避免全量改构造点；最终清理）。
- 移动端 Rust：主 WS 的认证握手路径删除；WS 连接建立后首消息直接发 JWT。

### 4.4 服务端改动清单（桌面端）

| 文件 | 改动 |
|------|------|
| `server/controllers/auth_controller.rs` | 新增 `biometric-challenge` / `biometric-verify` 两端点（无 presence 端点，在线语义见 §4.2） |
| `server/services/auth_service.rs` | 生物认证逻辑（challenge 生成/签名验证）从 WS 上下文抽取为可复用函数；challenge 存储键控改 fingerprint |
| `server/ws/terminal_ws.rs` | `handle_auth` 收敛为纯 JWT 验证；未认证 gate 保持；`stopping()` 改为按连接类型处理——仅**事件 WS 断开且该设备无其他事件 WS** 时触发 `DEVICE_DISCONNECTED`（终端通道断开不触发） |
| `server/ws/registry.rs` | `WsSessionEntry` 增加 `channel_type: Event \| Terminal`；`broadcast` 过滤默认仅 Event 通道、按 fingerprint 去重；新增设备在线查询（fingerprint 维） |
| `server/app.rs` | biometric 路由注册；HTTP JWT 校验中间件（pairing/verify/qr-connect 除外） |
| `system/app_context.rs` | `biometric_challenges` 键控调整（addr → fingerprint） |
| `server/connection_types.rs` / `db` | `DEVICE_DISCONNECTED` 事件字段兼容（携带 fingerprint 供 UI 定位设备）；连接历史回填时机对齐事件 WS 断开 |

### 4.5 移动端改动清单

| 文件 | 改动 |
|------|------|
| `Cargo.toml` | 新增 `reqwest`（+ rustls，保持无系统依赖） |
| `connection/manager.rs` | 连接生命周期重构：`connect_and_pair` → HTTP 认证流程（pairing/verify/QR/biometric/reauth）；删除 WS 认证握手；**认证成功后立即建立常驻事件 WS**（仅接收通知/同步事件，意外中断按现有重连规则自愈；终端 I/O 走前端终端 WS） |
| `connection/ws_connection.rs` / `lifecycle.rs` | 状态机适配（新增 `Authed` 状态；WS 层不再持有连接状态语义） |
| `handler/auth.rs` | 收敛：仅处理 WS `authenticated`/`failed` 回复；认证动作移入 HTTP 客户端 |
| `auth/manager.rs` | JWT 存取/刷新（`get_global_token` 保留）；生物认证调用改 HTTP |
| `commands/` | 新增 `get_ws_token()` / `get_ws_url()`（前端按需开 WS）；删除 WS 认证命令 |
| `router/event.rs` | 事件通道仅处理通知/同步事件（终端输出消息不再经此连接路由） |
| 前端 `useMobileConnection.ts` | 状态机适配（HTTP 认证的 loading/error 语义），配对 UI 命令保持 |

### 4.6 P0 验收标准

1. 全新设备：HTTP 配对码流程真机跑通（请求码 → 桌面端弹码 → 验码 → 拿 JWT），全程无 WS。
2. QR 连接、JWT reauth（重启 app 静默重连）走 HTTP 成功。
3. 生物认证：challenge/verify 走 HTTP 成功，挑战值单次有效。
4. 移动端无常驻主 WS（认证/连接状态走 HTTP）；HTTP 认证成功后事件 WS 常驻建立，首消息 JWT 认证后收到 SyncData；意外断开自动按现有重连规则恢复，恢复后重发认证。
5. 未认证 WS：首条非 auth 消息被拒；10s 无认证被服务端关闭。
6. 设备在线 = 事件 WS 存活：app 退后台/杀进程（事件 WS 断开）→ 桌面端设备列表显示离线 + 连接历史回填断开时间；前台恢复重连后自动回在线。
7. 同设备开多条 WS（事件 + 终端）时 SyncData 广播不重复。
8. 旧客户端（v2.0.0）WS 配对流程仍可用（若采用兼容策略 §7）。

---

## 5. P1 服务端：每会话连接 + 快照订阅

### 5.1 新路由

`GET /ws/terminal/session/{session_id}`（远程，JWT 首消息认证，复用 §4.3 规则）

- 连接创建即绑定 session_id（`TerminalWs::new_for_session(addr, session_id)`）
- 不存在的 session：认证通过后返回 `error(SESSION_NOT_FOUND)` 并关闭
- 旧 `/ws/terminal` 路由保留不动（旧客户端兼容）；`/ws/terminal/local` 在 P1 内迁移到快照协议（保留路由与本地令牌，帧格式升级）

### 5.2 快照订阅协议（废除偏移量）

```
客户端                         服务端
  │ subscribe ────────────────→│ ① 原子快照：snapshot_seq = 当前 max_seq（占位 subscriber）
  │ ←─ 历史帧（seq ∈ [min_seq .. snapshot_seq]，可合并）
  │ ←─ history_end {snapshot_seq, min_seq, history_count}
  │ ←─ 实时帧（seq > snapshot_seq，持续）
```

服务端实现（`SessionOutputManager::subscribe` 重构，复用占位/pending/原子激活）：

1. 插入 `active=false` 占位 subscriber，快照 `snapshot_seq`；
2. 读取队列 `[min_seq .. snapshot_seq]` 经 send_queue 逐条发送（持读锁）；
3. 发送 `history_end` 控制帧；
4. 写锁内排空 pending（占位期间 `on_output` 缓存的实时帧，seq 必然 > snapshot_seq，**无需跳过逻辑**）+ 原子激活；
5. 激活后 `on_output` 直接 try_send 实时帧。

删除：`start_seq` 参数、`SubscribeMode`（incremental/reset）、`get_range(cursor)` 字节裁剪、min/max/snapshot **offset**（改为 min/max/snapshot **seq**）、订阅响应中的 mode/offset 字段。

### 5.3 帧格式（TB v2）

二进制输出帧（16B 帧头）：

```
magic(2)="TB" | version(1)=2 | flags(1) | seq(8 LE) | len(4 LE) | data
flags: 0x01 = is_waiting
```

JSON 控制帧（文本帧，同连接）：

```json
{ "type": "auth", "token": "<jwt>" }
{ "type": "auth_ok" }
{ "type": "subscribe" }
{ "type": "subscribe_ok", "snapshot_seq": 42, "min_seq": 0, "history_count": 42 }
{ "type": "history_end", "snapshot_seq": 42 }
{ "type": "output_frame" /* 二进制帧，见上 */ }
{ "type": "input", "data": "<base64>", "special_key": null }
{ "type": "session_stopped", "session_id": "..." }
{ "type": "error", "code": "SESSION_NOT_FOUND", "message": "..." }
```

> 控制帧是简化版协议：无 message_id/expect_response 机制（连接级状态机替代请求-响应）。

### 5.4 输出传输

- 每会话连接一条独立 mpsc（容量 8192 → 远程提升至 32768）+ 独立 forward_loop；
- `on_output` 保持 try_send 背压丢弃（内存有界），丢帧由前端 seq 缺口检测 → 重发 subscribe（新快照）自愈；
- 远程通道输出帧直接二进制（**删除 base64 JSON 形态**），合并策略保留（30ms / 64KB）；
- 桌面本地通道（`/ws/terminal/local`）帧头同步升级 TB v2，游标逻辑改为 last_rendered_seq。

### 5.5 服务端改动清单（桌面端）

| 文件 | 改动 |
|------|------|
| `session/session_output.rs` | `OutputEvent` 删 offset；队列 offset→seq；`subscribe()` 快照化；删裁决/裁剪 |
| `server/ws/terminal_ws.rs` | per-session actor；删 subscribed_sessions 多路复用；auth 收敛（§4.3） |
| `server/ws/terminal_ws/forward.rs` | TB v2 编码（远程二进制 + 本地二进制）；OutputBuffer 删 offset |
| `server/ws/message.rs` / `enums/control.rs` | 协议类型收敛（Subscribe 无参、SubscribeMode 删除） |
| `server/app.rs` | 新路由注册 |
| `pty/pty_reader.rs` / `pty/pty_process.rs` / `pty/frontend_output_handler.rs` | 删除 output_broadcast + FrontendOutputHandler 兼容通道 |
| `commands/session.rs` / `session.rs` | 删 `get_session_output_history` + OutputCache（无调用方） |
| `composables/useTerminalOutputStream.ts`（前端） | TB v2 解析 + last_rendered_seq + 快照重订阅 |

### 5.6 P1 验收标准

1. 双移动端 + 桌面本地终端同看一会话：输出一致、互不阻塞（慢消费者不冻结他人）。
2. 订阅顺序严格 `[历史][history_end][实时]`，无重复无遗漏（单元测试覆盖占位期输出竞态）。
3. 快照语义：订阅期间新输出不出现在历史段；历史段含订阅时刻全部已产出事件。
4. 长会话重订阅（环形淘汰后）：min_seq > 0 且历史截断提示可用。
5. 桌面本地终端回归：渲染/滚动/输入/重连与改造前一致。
6. 输出帧为二进制（无 base64），移动端可直写 xterm。

---

## 6. P2 移动端：前端直连 + 快照拼接

### 6.1 移动端 Rust

| 文件 | 改动 |
|------|------|
| `handler/terminal.rs` / `router/event.rs` | 删除 Output → `MobileEvent::Output` → `emit("ws_output")` 链路 |
| `commands/session.rs` | 删 `ws_subscribe_session` / `ws_leave_session`；新增 `get_terminal_ws_info()` → `{ url, token }`（P0 的 get_ws_token/get_ws_url 的终端版） |
| `router/event.rs` | 新增监听前端事件 `terminal_output_activity` `{session_id}` → 插件管理器 `TerminalOutput` 通知（保持仅传 session_id 语义） |
| `model/message.rs` | 输入消息路径删除（输入改走前端终端 WS） |

### 6.2 移动端前端

| 文件 | 改动 |
|------|------|
| `composables/useTerminalSocket.ts`（新） | 每会话终端 WS：连接 → 首消息 auth（JWT 经 invoke 获取）→ subscribe → 快照拼接 → 重连退避（500ms→8s 封顶）→ seq 去重 |
| `stores/terminalBuffer.ts` | 重写：删 cursor/offset/连续性校验/指数退避自愈；新增 `last_rendered_seq`、`snapshot_seq`、实时缓冲、历史缓存（上限 16MB，LRU）、pending（保留：handler 注册前缓冲） |
| `composables/useTerminalBuffer.ts` | subscribe/unsubscribe/forceReplay/prepareSession/handleReconnect 改为 socket 驱动 |
| `views/TerminalView.vue` / `components/TerminalInputBar.vue` | 输入改经 socket（JSON input 帧） |
| `composables/useMobileCommands.ts` | 删 ws_subscribe_session/ws_leave_session；加 get_terminal_ws_info |

### 6.3 前端拼接状态机（要求 3 核心）

```
IDLE → (invoke get_terminal_ws_info) → CONNECTING → OPEN → AUTH_SENT
  → auth_ok → SUBSCRIBED（缓冲到达帧到 pending）
  → subscribe_ok {snapshot_seq} → HISTORY（帧按 seq 分发）：
       seq ≤ snapshot_seq → 历史路径：写 xterm + 入历史缓存
       seq > snapshot_seq → 实时缓冲（按 seq 入队）
  → history_end → FLUSH 实时缓冲（按序写 xterm）→ LIVE
LIVE：帧直写 xterm + 入历史缓存
  seq 缺口（> last_rendered_seq + 1）→ 重发 subscribe（新快照，跳过 ≤ last_rendered_seq）
重连（WS 断开）→ 回到 CONNECTING；快照重播时跳过 ≤ last_rendered_seq；
  min_seq > last_rendered_seq + 1 → 历史截断 toast + 清屏全量重播
页面重进（xterm 已销毁）→ 本地历史缓存立即回放 → subscribe → 服务端帧按 seq 跳过（不双写）
```

### 6.4 P2 验收标准

1. 移动端终端页无 Rust 中转：`ws_output` 事件不再产生（代码删除），输出为二进制帧直入 xterm。
2. 历史/实时拼接无缝隙：订阅瞬间的输出既不重复也不丢失（快照语义）。
3. 终端页进出：本地缓存回放即时可见，无全量重播等待。
4. 断网重连：快照恢复、去重正确、无黑屏/闪烁。
5. 输入经新通道到达 PTY（含特殊键）。
6. 插件 TerminalOutput 通知仍触发（OCR 等插件回归）。

---

## 7. 兼容策略

| 项 | 策略 |
|----|------|
| 旧移动端（v2.0.0）↔ 新桌面端 | **已删除**：旧 `/ws/terminal` 路由、WS 配对认证（RequestPairing/VerifyCode）与 `RemoteLegacy` base64 JSON 已随 v2.0.0 客户端下线拆除（2026-08-20）；配对统一走 HTTP `/api/auth/*` + 首消息 JWT | 删除前曾有“保留 compat、观察期后删”的 D2 决策；已按删除计划执行并回写 |
| 新移动端 ↔ 旧桌面端 | 不支持（版本同发）；前端按 HTTP 端点探测降级提示 |
| 桌面本地终端 | 随 P1 迁移（同一协议，无兼容负担） |
| 协议字段 | `start_seq`/offset 字段在新路由/新消息中删除；旧路由消息结构不变 |

---

## 8. 实施计划与验收

| 阶段 | 内容 | 验证 | 工作量参考 |
|------|------|------|-----------|
| **P0** | §4 全部（HTTP 认证补全 + presence + WS 收敛 + 移动端 HTTP 客户端 + 事件通道） | §4.6 + cargo test + 真机配对 | 桌面 1 会话 + 移动 1 会话 |
| **P1** | §5 全部（新路由 + 快照订阅 + TB v2 + 删广播/偏移 + 本地通道迁移） | §5.6 + cargo test + 桌面回归 | 1.5-2 会话 |
| **P2** | §6 全部（移动端 Rust 删链路 + 前端 socket/store 重写 + 输入迁移） | §6.4 + vitest + 真机 | 1-1.5 会话 |
| **P3** | 死代码清理（OutputCache/FrontendOutputHandler/PtyOutputEvent 广播）、旧协议标记、重写 `pty-output-pipeline.md`、spec 归档 | 全量测试 + 双端联调 | 0.5 会话 |

阶段顺序严格（P0 → P1 → P2 → P3）；P0 独立可发布（不动输出链路），P1 与 P2 逻辑耦合（协议同版）。

### 测试计划

- **Rust 单元**：① auth：HTTP verify 成功/失败/码消耗、biometric challenge 单次有效、presence TTL、WS 首消息 gate（非 auth 拒、超时关）；② 输出：快照订阅顺序（占位期竞态）、seq 范围正确性、TB v2 编解码、背压丢帧 + 缺口恢复
- **Rust 集成**：`ws_pairing_auth`、`pty_session_chain` 等既有测试适配新协议
- **前端 vitest**：useTerminalSocket（拼接/缺口/重连/去重/退避）、terminalBuffer store、输入帧构造
- **真机**：配对（码/QR/生物）→ 终端（进入/切页/重连/弱网）→ 设备离线展示

---

## 9. 风险与开放决策

| # | 决策/风险 | 默认方案 | 备注 |
|---|-----------|----------|------|
| D1 | 事件推送通道形态 | 保留轻量事件 WS（按需开启，首消息 JWT 认证） | 纯轮询延迟不可控；事件 WS 关闭时（后台）可降级轮询（可选） |
| D2 | 旧客户端兼容 | ~~旧 `/ws/terminal` 路由 + WS 认证各阶段保留~~ → **已拆除**（2026-08-20，随 v2.0.0 客户端下线） | 成本低；删除需双端确认（已确认移动端全 HTTP 化） |
| D3 | JWT 持有者 | 移动端 Rust（安全存储/指纹同侧），前端 invoke 获取 | 禁止前端 localStorage 长期存 token |
| D4 | 历史起点 | 默认 `min_seq`（用户要求）；长会话可退化为最近 2J 快照点（保留 `snapshot_offset` 机制为可选开关） | 需实测大历史重播延迟后定 |
| D5 | 前端历史缓存上限 | 16MB/会话 LRU | 与 scrollback 10000 行对齐；超限截断提示 |
| D6 | 插件 TerminalOutput 通知 | 前端 emit 活动事件 → Rust 转发（仅 session_id） | 多一跳延迟，可接受 |
| D7 | 背压丢帧自愈成本 | 远程通道容量 32768 + seq 缺口 → 快照重订阅 | 长会话缺口修复较重（D4 的 2J 起点缓解） |
| D8 | presence 心跳频率 | 10s 心跳 / 30s TTL | 低功耗可调；桌面 UI 在线状态展示对齐新语义 |
| D9 | WS 认证超时 | 10s | 可配 |

> 风险监控：P0 期间关注 auth 相关文件并发编辑（本 spec 撰写时 `auth_controller`/`auth_service`/`terminal_ws` 等正被修改，实施前重读最新代码）。

---

## 10. 参考

- 现状文档：`docs/knowledge/pty-output-pipeline.md`
- 思考过程：`.scratch/pty-output-refactor/proposal.md`
- 相关模块索引见上述文档「模块文件索引」节

## 11. 决策定稿（D1-D9）

| # | 决策 | 决议 |
|---|------|------|
| D1 | 事件推送通道 | **常驻事件 WS**（用户定稿）：HTTP 认证成功后移动端 Rust 立即建立、一直保持，意外中断按现有重连规则自动重连（重连后重发认证；token 失效经 HTTP reauth 换发）；仅接收通知/同步事件，终端 I/O 走前端终端 WS |
| D2 | 旧客户端兼容 | ~~旧 `/ws/terminal` 路由 + WS 认证各阶段保留，标记 compat~~ → 已拆除（2026-08-20） | ticket 11 记录观察期；拆除后回写 |
| D3 | JWT 持有者 | 移动端 Rust 持有，前端经 `get_ws_token`/`get_terminal_ws_info` invoke 获取 |
| D4 | 历史起点 | 默认严格 `min_seq`；2J 快照点保留为配置 `history_start_mode`（默认 `min`，实测后再开） |
| D5 | 前端历史缓存 | 16MB/会话 LRU，与 scrollback 10000 行对齐 |
| D6 | 插件 TerminalOutput 通知 | 前端 emit `terminal_output_activity` → Rust 转发（仅 session_id） |
| D7 | 背压自愈 | 远程每会话通道容量 32768；seq 缺口 → 快照重订阅；2J 开关缓解长会话修复 |
| D8 | 设备在线语义 | **常驻事件 WS 存活 = 设备在线**（用户定稿）：指纹匹配 + channel_type=Event 且已验证；最后一条事件 WS 断开 → DEVICE_DISCONNECTED + 连接历史回填（取消 HTTP presence 心跳方案，终端通道断开不触发离线） |
| D9 | WS 认证超时 | 10s 未完成首消息认证 → 服务端关闭 |

## 12. Ticket 索引

详见 `issues/`（依赖：01 → 02 → 03 → 04；
05 → 06 → 07 → 08；09 ← 04,06；10 ← 09,03；11 ← 08,10）：

| Ticket | 内容 | 阶段 |
|--------|------|------|
| 01 | 桌面 HTTP 认证补全（生物/presence/去重） | P0 |
| 02 | 桌面 WS 认证收敛（首消息 JWT + 超时） | P0 |
| 03 | 移动 HTTP 认证客户端 + 心跳 | P0 |
| 04 | 移动常驻事件 WS（认证后建立+自动重连） | P0 |
| 05 | 输出队列 seq 化 + 快照订阅 | P1 |
| 06 | 每会话终端路由 + TB v2 帧 | P1 |
| 07 | 桌面本地通道迁移 | P1 |
| 08 | 删除广播/死代码 | P1 |
| 09 | 移动输出链路拆除 + 终端 WS 命令 | P2 |
| 10 | 前端直连终端 WS + 快照拼接 | P2 |
| 11 | 兼容标记、文档重写、全量回归 | P3 |
