# PTY 输出全链路：桌面端 → 移动端/桌面端本地终端显示

本文档描述从桌面端 PTY 进程产生输出到终端（移动端 xterm / 桌面端 xterm）渲染的完整数据链路与订阅协议。

> **当前架构（TB v3 字节连续 + 移动端 WS 迁入 Rust，2026-09-12）**：
> - 服务端真源 = **字节累计偏移**（`start_offset`/`end_offset`），订阅/游标/去重/截断/ack 全部收敛到 `[start, end)` 区间运算（TB v3，取代 v2 的 seq/事件数语义）
> - 桌面端维护两套缓存（实时 + 历史）+ **双速传播**（realtime 读即传 / batch 满 `batch_bytes` 才发）；历史一次性经 HTTP 快照接口给移动端
> - 移动端终端 WS **由 Rust 后端持有**（`bedcode-mobile/src-tauri/src/terminal_link.rs`）：每会话一连接、认证、缓存（真源）、ack 节流、退避重连；前端只触发订阅/模式/输入并消费 `terminal-*` 事件
> - 链路加密（ws-terminal 协商）为后续 ticket；当前 JWT 认证 + 明文帧（与 v2 时代明文终端 WS 同安全位）

---

## 全链路概览

> 📊 交互式流程图：[桌面端 PTY 输出数据流](../diagrams/pty-output-flow-desktop.html) · [移动端终端 PTY 输出链路](../diagrams/pty-output-flow-mobile.html)

```text
桌面端                                       移动端
PTY 进程输出
  ↓ (os pipe)
PtyReader (std::thread) ─── 原始字节
  ↓ 单通道分发
UnifiedOutputQueue（字节块队列，50MB）
  ├─ push：分配 start_offset、min_offset 推进、超限淘汰最旧
  └─ 订阅者 forward_loop（双速：realtime / batch）
        ↓ encode_output_frame_v3（start_offset(8LE)+len(4LE)）
        ↓ WebSocket TB v3 帧
   ├─→ 移动端: terminal_link.rs（Rust 持有）       ────────▶ HTTP GET /api/sessions/{id}/history?from=
   │      → 字节缓存（16MB LRU，真源）                       （一次性历史，快照字节截取）
   │      → ack 节流回发 → 桌面端释放背压
   │      → 事件 terminal-frame / terminal-state → 前端
   └─→ 桌面端: useTerminalOutputStreamChannel（Tauri Channel 原生 IPC；
         WS 环回链路 /ws/terminal/local 已下线删除）
```

两个消费出口共享**同一真源**，走**同一套 TB v3 二进制协议**；历史获取渠道不同（移动端 HTTP 一次性拉取、桌面本地终端经订阅快照）。

---

## 1. 服务端（桌面端）

### 1.1 输出读取与入队

- `pty/pty_reader.rs`：PTY 读取线程只构造 `OutputEvent`（不分配序号）；字节区间起点由 `session/session_output.rs::on_output` 在串行临界区内分配（`start_offset = 队列 max_offset`）
- `UnifiedOutputQueue`（字节块队列）：`VecDeque<OutputChunk{start_offset, bytes: Bytes, end_is_waiting}>`；`max_offset`（产出游标）/ `min_offset`（驻留最旧）/ `total_bytes` / `max_total_bytes`（50MB，可配置）/ `max_chunks`（65536 防极小块风暴）；push 时 while 淘汰最旧（min_offset 推进）
- `subscribe(client_id, send_queue, response_tx, from_offset)`：from_offset 起播（chunk 级跳过 + 半块 `Bytes::slice`）；`SubscribeResponse{snapshot_offset, min_offset, history_bytes}`（字节三件套）
- `snapshot_from(from_offset)` / `range(from, to)`：HTTP 历史 / 快照截取用

### 1.2 转发与编码（TB v3）

- `server/ws/terminal_ws/forward.rs`：
  - `encode_output_frame_v3(start_offset, is_waiting, data)`：`magic "TB"(2) + version=3(1) + flags(1) + start_offset(8 LE) + len(4 LE) + data`（16 字节头；flags bit0 = is_waiting；无事件数编码、无 128 上限）
  - `forward_loop` **双速模式**（订阅者级 `Arc<AtomicU8>`，0=realtime 1=batch）：
    - realtime（默认，进终端页）：时间窗 + 字节窗合并（读即传）
    - batch（退出终端页但会话未停）：**仅当缓冲 ≥ batch_bytes（默认 64KB，可配置）才发**，无时间窗——输出留在移动端 Rust 缓存
    - 模式切换即时生效，real→batch 切换时 flush 残留缓冲
  - HistoryEnd 透传字节三件套（严格保持在历史帧之后）
- `server/ws/terminal_ws/control_frame.rs` 控制帧（JSON）：
  - 客户端 → 服务端：`auth {token}`、`subscribe {from_offset?}`（缺省 = 服务端从 min_offset 全量回放，老客户端兼容）、`input {data(base64), special_key?}`、`mode {realtime|batch}`
  - 服务端 → 客户端：`auth_ok`、`subscribe_ok {protocol:3, snapshot_offset, min_offset, history_bytes}`、`history_end {snapshot_offset}`、`session_stopped {session_id}`、`error {code, message}`
- 背压 ack（客户端 → 服务端二进制）：TB 帧头 + flags ACK(0x02) + `acked_offset(8 LE)` + `len(4 LE)` + session_id 负载；服务端按 `end_offset ≤ acked_offset` 弹出 unacked 记账

### 1.3 HTTP 一次性历史（移动端首选路径）

- `GET /api/sessions/{id}/history?from=<u64>`（JWT 认证，见 app.rs 路由 + session_controller::get_session_history）
- 返回统一信封 `ApiResponse{code, message, data:{min_offset, snapshot_offset, history_bytes, data_base64}}`
  - `[from, snapshot_offset)` 字节（chunk 级跳过 + 半块 slice）；session 不存在 → code=1002
- 移动端 `terminal_get_history`：**缓存优先**（真源 = Rust 缓存）；缓存空/头被淘汰时回退此接口增量拉取

### 1.4 认证

- `ws/terminal/session/{id}` 连接后首消息 JWT 认证（`auth {token}`），成功回 `auth_ok` 后客户端才能 subscribe；随 WS 关闭会话资源释放。链路加密协商后续 ticket

---

## 2. 前端

### 2.1 桌面端本地终端（`composables/useTerminalOutputStream.ts` + Channel 变体）

- 挂载订阅 / 卸载断开（非会话即订阅）；TB v3 解析 + lastRenderedOffset 游标 + **跨帧裁剪**（`overlap = cursor - start_offset` → `data.subarray(overlap)` 零重复）；ack 带 acked_offset
- 桌面端不接线双速 UI（保持 realtime；双速能力给移动端页面进出用）

### 2.2 移动端终端（Rust 后端持有 + 事件驱动）

- **Rust 层** `terminal_link.rs`（`terminal_link_manager` 单例，lib.rs 已注册）：
  - 每会话一个 tokio-tungstenite WS；JWT 首消息认证；`subscribe from_offset = 已接收游标`（重连续传不重发已缓存区）
  - 会话级字节缓存 `SessionCache`（VecDeque 片段 + 字节区间，16MB LRU 淘汰头部）：WS 收帧即收即缓存（真源），历史段（end ≤ snapshot）静默入缓存、实时段（end > snapshot）经 `terminal-frame` 事件推送
  - ack：`acked` 水位 = max(缓存游标, 渲染游标)；节流回发（64KB 阈值 + 250ms 空闲兜底）
  - 重连：意外断开放弃 + 指数退避（500ms→8s 封顶）+ 重订阅（保留游标）；会话缺失（SESSION_NOT_FOUND）有限重试后停止；手动取消 → 关连接不再重连
  - `terminal-state` 事件携带 phase（idle/connecting/auth/history/live）/ cursor / snapshot_offset / min_offset / mode / detail
  - 命令：`terminal_subscribe` / `terminal_unsubscribe` / `terminal_unsubscribe_all` / `terminal_remove` / `terminal_send_input`（base64 + special_key）/ `terminal_set_mode`（realtime/batch）/ `terminal_ack_rendered` / `terminal_get_history` / `terminal_get_state`
  - **invoke 返回值使用 camelCase 键**（Tauri invoke 只转换请求参数、返回值原样传递——必须与前端 TS 接口逐字对齐）
- **前端**（`stores/terminalBuffer.ts` + `composables/useTerminalBuffer.ts` + `views/TerminalView.vue`）：
  - 订阅生命周期驱动：会话启动（startSession）→ `terminal_subscribe`；Stopped → `markSessionStopped`（取消订阅 + 游离标）；设备断开/恢复配对 → 全量取消 / 重建订阅；意外断开重连由 Rust 自动处理
  - 页面进出影响「是否消费事件」不影响订阅：进入 → `set_mode(realtime)` + 历史拼接；退出（会话未停）→ `set_mode(batch)`，事件照常入 Rust 缓存、前端不再渲染
  - 历史拼接：进入页面 → `terminalGetHistory(from=游标)`（缓存优先/HTTP 回退）→ 历史段经 writeParsed 写入 xterm（写解析完成才推进游标）→ 期间到达的实时帧缓冲（historyPreparing）→ 拼接完成 FLUSH——「拼完历史才通知前端消费」
  - 字节连续判定（与桌面端同构，spec §0.1 8 条对齐清单）：`lastRenderedOffset` 游标；去重 `endOffset ≤ cursor` 整帧跳过；缺口 `startOffset > cursor` → `forceReplay`（重拼接带 from_offset，3s 冷却）；跨帧裁剪 `overlap = cursor - startOffset`；截断 `min_offset > cursor` → 清屏 + truncated 提示
  - `sendInput` → `terminal_send_input`（Rust → WS → 桌面 PTY）；resize 保持既有路径；渲染背压 `onWriteParsed → terminal_ack_rendered`

---

## 3. 关键协议知识

| 项 | 值 |
| --- | --- |
| 输出帧头 | `magic "TB"(2) + version=3(1) + flags(1) + start_offset(8 LE) + len(4 LE)` = 16 字节 + data |
| flags | bit0 = is_waiting；bit1 = ACK（仅客户端→服务端 ack 帧用） |
| end_offset | `= start_offset + len`（帧内可导，无事件数编码） |
| 游标 | `lastRenderedOffset` = 已渲染帧末 endOffset（跨重连/重启保留语义） |
| 快照三件套 | `subscribe_ok{protocol:3, snapshot_offset, min_offset, history_bytes}` |
| 历史边界 | 历史段帧 `end_offset ≤ snapshot_offset`；实时帧 `end_offset > snapshot_offset` |
| 截断判定 | `min_offset > 客户端游标` → 历史头部被淘汰，清屏 + 提示 |
| ack | 二进制帧（ACK flag + acked_offset(8LE) + session_id 负载）；节流 64KB / 250ms 兜底 |
| 双速 | realtime 时间窗+字节窗合并；batch 仅满 batch_bytes 才发（默认 64KB，config `terminal.batch_bytes`） |
| HTTP 历史 | `GET /api/sessions/{id}/history?from=` → ApiResponse 信封 + data_base64（snake_case 内层，移动端映射 camelCase） |

---

## 4. 测试覆盖

- **桌面端**（cargo test）：session_output（字节块队列/淘汰/slice/连续性）、forward（v3 编码/双速/HistoryEnd 顺序）、control_frame（subscribe from_offset/mode/ack 解析 v3+v2）、ws_session_route（v3 断言）、HTTP history controller、build_manifest_smoke
- **桌面端 vitest**：useTerminalOutputStream(+Channel) 帧解析/跨帧裁剪/游标；terminal-flow 集成
- **移动端**（cargo test）：terminal_link 内联单测（TB v3/v2 解析、SessionCache push/snapshot 半块 slice/LRU、build_ack_frame 逐字节与桌面 parse_ack_frame 对齐）
- **移动端 vitest**：terminalBuffer store（事件同步/历史拼接/跨帧裁剪/缺口重拼接/截断/停止恢复/输入/ack/双速）、useTerminalBuffer、terminal-flow 集成（Rust 驱动事件链 + 历史拼接 FLUSH + terminal_send_input）

---

## 5. 相关文档

- `.scratch/mobile-ws-rust/spec.md`（本架构方案 + §0.1 两端语义对齐清单 + §7 契约）
- `.scratch/pty-byte-history/spec.md`（TB v3 基础方案：事件 index → 字节偏移）
- `.scratch/mobile-ws-rust/issues/03.md` / `04.md`（移动端落地记录、修复、偏差）
- `bedcode-mobile/docs/code-map.md`（移动端代码地图）
- AGENTS.md §9（协议两端同步部署；老端忽略未知字段）