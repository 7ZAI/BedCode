# PTY 输出全链路：桌面端 → 移动端/桌面端本地终端显示

本文档描述从桌面端 PTY 进程产生输出到终端（移动端 xterm / 桌面端 xterm）渲染的完整数据链路与订阅协议。

> **当前架构（pty-output-refactor v2，02~10 完工）**：服务端真源 + **seq 序号 + 快照订阅 + TB v2 二进制帧**。
> 移动端前端**直连**桌面端每会话终端 WS 路由（无 Rust 中转）；本地终端走同一 `/ws/terminal` 协议族。
> 兼容通道（`output_broadcast` / `FrontendOutputHandler` / `OutputCache` / `ws_output` 事件）已全部删除（ticket 08/09）。

---

## 全链路概览

```
PTY 进程输出
    ↓ (os pipe)
PtyReader (std::thread) ─── 读取原始字节
    ↓ (单通道分发)
GlobalOutputManager.on_output(OutputEvent) ─── 统一真源
    └→ SessionOutputManager.on_output()
          ├→ UnifiedOutputQueue.push(event) ─── 环形队列（seq 分配 + 历史回放源）
          └→ 遍历 active subscribers → send_queue.try_send(OutputFrame)   (mpsc, 容量 32768)
                ↓
           forward.rs（terminal_ws）── TB v2 二进制帧编码（16B 帧头；支持合并）
                ↓ (WebSocket)
    ├→ 移动端: useTerminalSocket（前端直连 /ws/terminal/session/{id}）
    │            → terminalBuffer Store（seq 状态机 + 历史缓存）→ writeCoalescer → xterm
    └→ 桌面端: useTerminalOutputStream（本地 /ws/terminal/local）→ 同样直连 → xterm
```

两个消费出口共享**同一真源**，走**同一套 TB v2 二进制协议**：

| 消费出口 | 通道 | 路由 | 协议 |
|---------|------|------|------|
| 移动端终端 | 远程 WebSocket（前端直连） | `ws://host:port/ws/terminal/session/{id}` | TB v2 帧 + JSON 控制帧 |
| 桌面端本地终端 | 本地 WebSocket（环回） | `ws://127.0.0.1:port/ws/terminal/local` | TB v2 帧 + 相邻标记控制消息 |

> 已拆除：旧 `/ws/terminal` 兼容路由（多会话订阅 + base64 JSON 文本帧 + 旧 WS 配对认证
> RequestPairing/VerifyCode/QrConnect）及其 `RemoteLegacy` 转发格式已随旧 v2.0.0 客户端下线删除；
> 配对统一走 HTTP `/api/auth/*`，WS 首消息仅接受 JWT。`/ws/terminal/local` 与 `/ws/event` 仍共用
> 「相邻标记 Message 文本线」控制帧（非旧 v2.0.0 兼容，而是本地/事件通道的现行协议）。

---

## 1. 服务端（桌面端）

### 1.1 输出读取与入队

- `pty/pty_reader.rs`：`PtyReader::start(reader, lifecycle_tx, session_id, running)` 在独立线程读取 PTY，每批字节构造 `OutputEvent`（原始 bytes + `next_output_index()` 全局递增序号）→ `tauri::async_runtime::spawn` 桥接 `GlobalOutputManager.on_output()`
  - ticket 08 后已**无 broadcast 通道**（`output_broadcast` 参数删除），分发仅经 `GlobalOutputManager`
- `session/session_output.rs`：`UnifiedOutputQueue.push()` 分配 `index`（seq）并登记 `min_seq/max_seq`；超容量（条目/字节双上限）丢最旧事件并用 `min_seq` 推进表示头部截断
- `SessionOutputManager::subscribe()`：快照订阅协议——占位 → `snapshot_seq` → 历史 `[min_seq..snapshot_seq]` → **`OutputFrame::HistoryEnd`** → 写锁内排空 pending（天然全 > snapshot_seq）→ 原子激活。历史/实时拼接由服务端保证无重无漏

### 1.2 转发与编码

- `server/ws/terminal_ws.rs`：每会话 actor。新路由 `/ws/terminal/session/{id}` 连接创建即绑定会话（`TerminalWs::new_for_session`），认证后不存在 → `error(SESSION_NOT_FOUND)` + 关闭
- 控制帧协议（`server/ws/terminal_ws/control_frame.rs`）：
  - 客户端：`auth{token}`（首消息 JWT）→ `subscribe` → `input{data, special_key}`（data 为 Base64）
  - 服务端：`auth_ok` → `subscribe_ok{snapshot_seq, min_seq, history_count}` → 历史帧 → `history_end{snapshot_seq}` → 实时帧；`error{code,message}` / `session_stopped{session_id}`
- TB v2 帧（`forward.rs` OutputFormat::RemoteV2，本地与远程统一）：

```
16 字节帧头 + payload
magic "TB"(2) + version=2(1) + flags(1) + seq(8 LE) + len(4 LE)
  flags: bit0 = is_waiting；高 7 位 = 事件数-1（合并帧，1..=128，超限拆帧）
  seq  = 帧内首事件 index（会话内连续：SessionOutputManager::on_output 按队列
         max_seq+1 分配，替代跨会话全局计数器——多会话并发时全局 seq 空洞会令
         客户端缺口检测（seq > last_rendered+1 → 重订阅）误触发成重订阅风暴）
  帧末 seq = seq + eventCount - 1（前端游标推进/缺口检测基准）
```

- forward 每订阅 mpsc 32768（远程背压余量）；合并按 `merge_output` 开关

### 1.3 认证（§4）

- 新路由与旧路由共享 `authenticate_jwt` 核心：验签 + 有效期 → 设置会话认证状态 → 注册 `WsSessionRegistry`（`set_authenticated`/`last_seen`/DEVICE_CONNECTED）→ 更新配对 last_seen
- 未认证发业务帧 → `error(AUTH_REQUIRED)` + `ctx.close(None)` + `ctx.stop()`（12 处错误关闭路径统一显式关闭，见 ticket 07 教训）
- HTTP 认证补全（ticket 01~04）：verify/QR/biometric + presence + 常驻事件 WS

---

## 2. 前端

### 2.1 桌面端本地终端（`composables/useTerminalOutputStream.ts`）

- 连接 `ws://127.0.0.1:{port}/ws/terminal/local?token=...`（一次性短期令牌每次重新签发），握手即订阅
- 控制消息为旧路由相邻标记格式：`subscribe_response{min_seq, max_seq=snapshot_seq, history_count}`
- `last_rendered_seq` 游标：快照重播/重连后帧 `lastSeq ≤ last_rendered_seq` **整帧跳过**（不双写）；`frame.seq > last_rendered_seq+1` → 重订阅（快照）
- `subscribe_response` 时 `min_seq > last_rendered_seq+1`（已渲染区被环形淘汰）→ `onReset` 清屏全量重播 + `onTruncated(min_seq)`
- 订阅确认前回放帧缓冲（防御）；`SESSION_NOT_FOUND` 有限重试 3 次后停止

### 2.2 移动端终端（`composables/useTerminalSocket.ts` + `stores/terminalBuffer.ts`）

状态机（spec §6.3）：

```
IDLE → (get_terminal_ws_info) → CONNECTING → OPEN → auth → subscribe
  → subscribe_ok{snapshot_seq} → HISTORY：
       frame.lastSeq ≤ snapshot_seq → 写 xterm + 入历史缓存（16MB LRU）
       frame.lastSeq >  snapshot_seq → 入实时缓冲
  → history_end → FLUSH 实时缓冲（按 seq 写入）→ LIVE：帧直写 + 入缓存
  seq 缺口（> last_rendered_seq+1）→ 重发 subscribe（快照重播跳过 ≤ 游标）
  重连（WS 断开）→ back to CONNECTING（退避 500ms→8s 封顶；JWT 有效期可复用）
  subscribe_ok 时 min_seq > last_rendered_seq+1 → 清屏 + onTruncated + 锚定（每会话提示一次）
  页面重进（xterm 新实例）→ registerRealtimeHandler 无条件回放历史缓存 → 服务端帧按 seq 跳过
```

- `registerRealtimeHandler` 挂载时回放历史缓存并推进游标（覆盖重进与 preload 两个场景，不双写）
- `sendInput(data, specialKey)` → `input` 帧（UTF-8 → Base64）
- 每帧触发 `terminal_output_activity`（节流 200ms）→ Rust 监听 → 插件 `TerminalOutput` 通知（OCR 等插件保留，仅传 session_id）
- 会话停止（`session_stopped`）→ 停 socket 等外部恢复；`SESSION_NOT_FOUND` 3 次后停止
- writeCoalescer：默认直写 + 64KB 拆块；rAF 合并 / DEC 2026 包裹为 WebGL 渲染器调试开关

---

## 3. 关键协议知识

| 项 | 值 |
|----|-----|
| 帧头 | 16B：magic "TB" + version=2 + flags + seq(8 LE) + len(4 LE) |
| flags | bit0 waiting；高位 7 位 = 事件数-1（1..=128，超限拆帧） |
| seq  | 帧内首事件 index（会话内连续递增，on_output 按队列 max_seq+1 分配；跨会话不再产生空洞）；帧末 = seq + count - 1 |
| 新路由 | `GET /ws/terminal/session/{session_id}`（订阅即连接、无多路复用） |
| 本地路由 | `GET /ws/terminal/local`（环回，免 JWT） |
| 旧路由 | 已删除（旧 v2.0.0 兼容路由下线；其 base64 JSON 文本帧与 WS 配对认证一并移除） |
| 历史边界 | `subscribe_ok.snapshot_seq`；历史 [min_seq..snapshot_seq] + `history_end` |
| 输入 | `{"type":"input","data":Base64,"special_key":opt}` |
| 认证 | `{"type":"auth","token":JWT}` 首消息；未认证业务帧 → AUTH_REQUIRED + 关闭 |

> **输入帧 data 编码语义（必读）**：新路由 input 帧的 `data` 为 Base64（UTF-8 → `base64 STANDARD`），
> 移动端发送侧即为 `utf8ToBase64`（`useTerminalSocket.ts`，TextEncoder + btoa）。这是**防御性/对称约定**
> 而非硬性要求——编码后为纯 ASCII，保证任意字符（中文/emoji/控制符）无损穿越 WS 文本帧，且与输出
> legacy 通道 base64 口径一致。**解码责任在桌面端新路由**：`terminal_ws.rs::handle_session_input` 对 `data`
> 做 `base64::Engine::decode` 后再进 `handle_input → write_input`（该链路按**明文**透传，自身不负责解编码）；
> **解码失败按明文透传 + warn**，兼容误用此路由的明文客户端（勿在此处丢弃用户输入）。
>
> **历史歧义**：旧 `/ws/terminal` 兼容路由与旧移动端 Rust 通道（`request.rs::Message::input`）的输入 `data`
> 为**明文**（从未 base64）。改动/排查时不可按旧路由口径直接透传 base64 到 PTY（典型症状：CLI 把 `/new`
> 回显成 `L25ldw==`），也不可把解码逻辑放进 `handle_input`/`write_input`（会破坏旧路由明文路径）；
> 解码只归新路由。

---

## 4. 测试覆盖

- **Rust**：快照订阅顺序（占位期竞态）、seq 无重无漏、全局 seq 空洞、环形淘汰、TB v2 编解码（128 拆帧）、控制帧解析、forward 回归、`ws_session_route`/`ws_auth_rules`/`ws_pairing_auth` 集成（含 session_stopped/认证拒绝对称）
- **前端桌面端**：useTerminalOutputStream 单测（解析/去重/重订阅/截断）+ terminal-flow 集成
- **前端移动端**：terminalBuffer store 状态机 15 用例（完整流/实时缓冲/去重/缺口/截断/重连/session_stopped/ERROR 重试/缓存回放/LRU/sendInput/通知）+ useTerminalBuffer 11 + terminal-flow 集成 3
- **真机**：配对（码/QR/生物）→ 终端（进入/切页/重连/弱网）→ 设备离线展示（待双端联调 checklist 见 spec §10）

---

## 5. 相关文档

- 规格与单 ticket 记录：`.scratch/pty-output-refactor/`（spec.md + issues/01~11，Status 为真源）
- 认证：`docs/knowledge/mobile-desktop-auth.md`
- 双端测试命令：`AGENTS.md` Build & Run
