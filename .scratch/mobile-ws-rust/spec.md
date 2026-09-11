# 移动端终端 WS 迁入 Rust 后端 + 桌面端字节连续跟进（两速传播 + HTTP 历史）

> 状态: 实施中（桌面端 TB v3 收尾 → 双速/HTTP 历史 → 移动端 Rust 终端链路）
> 关联: `.scratch/pty-byte-history/spec.md`（TB v3 基础方案，桌面端已在途改动未完成）、
> AGENTS.md §9（协议两端同步）、docs/knowledge/mobile-desktop-auth.md
> 在途工作: bedcode-desktop/src-tauri/src/session/session_output.rs 已有半个 v3 迁移
> （OutputEvent.index→start_offset 已改、队列主体未改，当前编译不过）——按 §11 合并续做，禁止整文件回滚

---

## 0. 用户需求（本次直接指令，优先于一切文档）

1. **订阅时机**：移动端不再「进入已启动会话才订阅」，改为「会话启动（创建）时就订阅」。
   订阅由 Rust 后端管理、前端触发：创建/启动会话 → 开始订阅；停止会话 → 取消订阅；
   意外连接断开 → 自动重新订阅；手动断开 → 取消订阅。
2. **数据源头**：移动端终端数据真源 = 移动端 Rust 后端维护的缓存；终端背压（ack）交给
   移动端 Rust，输入由前端 → 移动端 Rust → 桌面端 PTY。
3. **桌面端跟进**：数据连续性以字节数目表达（TB v3，见 pty-byte-history spec）；订阅时
   桌面端维护两套缓存（实时数据缓存 + 历史数据缓存）；**历史不再走 WS，一次性 HTTP 按
   快照字节数截取**，拼接到实时数据缓存后，才通知前端终端组件开始消费；实时仍走 WS。
   桌面 PTY 两种传播速度：进入终端页实时显示 → 维持「读即传」；退出终端页但会话未停 →
   按批次传输（满 batch_bytes 才发，默认 64KB，可配置）。

## 0.1 订阅机制异构、数据语义同构（2026-09-12 用户裁决，优先于本节内一切实现细节）

- **订阅机制（生命周期/时机/持有方）两端不必一致**：桌面端保持现行「挂载订阅、卸载断开」
  （TerminalPreview → WS 环回 / Channel，非会话即订阅、无双速 UI 接线）；移动端按 §0 的
  会话即订阅（Rust 常驻）实施。两端各按自己的生命周期管理，不允许为对称而改动对方。
- **数据获取与连续判断语义必须两端严格对齐**，以桌面端实现为参照基准：
  1. 帧协议：TB v3（`magic"TB"+version=3+flags(bit0 is_waiting)+start_offset(8LE)+len(4LE)+data`）；
     `end_offset = start_offset + len` 直接可导，帧内无事件数编码
  2. 游标：`lastRenderedOffset` = 已渲染到的帧末 `endOffset`（跨重连/重启保留语义）
  3. 去重：`frame.endOffset <= cursor` → 整帧跳过
  4. 缺口：`frame.startOffset > cursor` → 重订阅（subscribe 带 `from_offset=cursor`），
     缺口帧不渲染不推进游标
  5. 跨帧裁剪：`overlap = cursor - frame.startOffset` → 渲染 `data.subarray(overlap)`，
     零重复（参照实现：桌面 `useTerminalOutputStream.ts` 的 deliverFrame）
  6. 截断：`min_offset > cursor`（subscribe_ok 或 history 帧元数据）→ 清屏 + truncated 提示
  7. ack：回发 `ackedOffset = cursor`（节流 64KB / 250ms 空闲兜底，与桌面端同参）
  8. 会话语义：`session_stopped` / `session_missing`（SESSION_NOT_FOUND）事件化，前端一致消费
- 两端各自实现以上语义时禁止引入第二套判定公式（如 seq 残留、`+1` 偏移、事件数推导）——
  唯一逃逸大道：服务端下发的字节区间元数据

---

## 1. 架构总览

```
桌面端                                 移动端
PTY 字节流
 → chunk 队列（历史，50MB，字节偏移）     terminal rust:
 → 订阅者 forward_loop（双速模式）        每会话一个 WS（Rust 持有）
     realtime: 时间窗+字节窗合并           ├─ 收帧 → 字节缓存（真源）
     batch:    满 batch_bytes 才发         ├─ ack（按渲染游标）→ 桌面
 → WS v3 帧 (start_offset + len)          ├─ 输入：命令 → WS input → 桌面 PTY
   └─ subscribe 支持 from_offset          ├─ 历史：HTTP 一次性拉取 → 缓存
     └─ subscribe_ok 字节三件套            └─ 事件 → 前端（terminal-frame 等）
 → HTTP GET /api/sessions/{id}/history    前端 xterm 只消费渲染；触发订阅/模式/输入
    （JWT，?from=<offset> 按字节截取）
```

---

## 2. 桌面端改动

### D1 TB v3 收尾（续在途改动）
- `session/session_output.rs`：
  - `OutputFrame::HistoryEnd { snapshot_offset, min_offset, history_bytes }`（已在途改好字段）
  - `OutputEvent { …, start_offset, … }` + `end_offset()`（已在途改好）
  - `UnifiedOutputQueue` 重写为字节块队列：`VecDeque<OutputChunk{start_offset, bytes: Bytes, end_is_waiting}>`，
    `max_offset`（产出游标）/`min_offset`（驻留最旧）/`total_bytes`/`max_total_bytes`（默认 50MB）/
    `max_chunks`（65536，防极小块风暴）；push 时淘汰最旧；移除 `capacity`/`max_seq`/`min_seq`
  - `on_output`：`event.start_offset = queue.max_offset()`（串行临界区内分配）；unacked_fifo 改
    `(end_offset, bytes)`；`on_ack(acked_offset)` 按 `end_offset <= acked_offset` 弹出
  - `subscribe(client_id, send_queue, response_tx, from_offset)`：from_offset 起播（chunk 级跳过 +
    半块 `Bytes::slice`）；`SubscribeResponse { snapshot_offset, min_offset, history_bytes }`
  - 新增 `snapshot_from(from_offset)` / `range(from, to)`（HTTP 历史用）；内联测试全量适配
- `pty/pty_reader.rs`：`next_output_index()` 占位不再需要（on_output 分配 offset）；读线程只构造
  `OutputEvent::new(session_id, bytes, 0, ts, false)`，start_offset 由 on_output 覆盖
- `server/ws/terminal_ws/forward.rs`：
  - `encode_output_frame_v3(start_offset, is_waiting, data)`：`magic"TB"+version=3+flags(bit0 waiting)+start_offset(8LE)+len(4LE)+data`，
    删除 count 编码与 128 事件上限
  - `OutputBuffer` 记录 `start_offset`（= 首块），flush 编 v3
  - `forward_loop` 新增双速模式：`mode: Arc<AtomicU8>`（0=realtime 1=batch）+ `batch_bytes: usize`；
    realtime 走现有时间窗/字节窗；batch 模式仅当 `buffer.len() >= batch_bytes` 才发（无时间窗，
    退出页面的客户端不消费、数据留在移动端缓存）；模式切换时 realtime 收尾 flush 残留
  - HistoryEnd 透传字节三件套；测试适配
- `server/ws/terminal_ws/control_frame.rs`：
  - `ClientFrame::Subscribe { from_offset: Option<u64> }`（老客户端 `{"type":"subscribe"}` 兼容）
  - 新增 `ClientFrame::SetMode { mode: WatchMode }`（`{"type":"mode","mode":"realtime"|"batch"}`）
  - `ServerFrame::SubscribeOk { snapshot_offset, min_offset, history_bytes }`；`HistoryEnd { snapshot_offset }`
  - ack 解析：v3 取 acked_offset；**v2 兼容**（version=2 帧取 acked_seq，过渡期解码不作精度要求）
- `server/ws/terminal_ws.rs`：
  - subscribe 解析 from_offset → `global_manager.subscribe(…, from_offset)`
  - SubscribeOk/HistoryEnd 编码切换字节三件套；ack 处理 acked_offset
  - SetMode 帧 → 更新该订阅者的 mode AtomicU8（每订阅者一个，存 actor map）
- `system/config.rs`：`global_queue_max_bytes` 64MB→50MB；新增 `global_queue_max_chunks`(65536)；
  新增 `terminal.batch_bytes`(默认 64KB)；descriptions 同步
- WebView 前端 `composables/useTerminalOutputStreamChannel.ts`：TB v3 解析（start_offset/end_offset/
  跨帧裁剪 overlap）、lastRenderedOffset、缺口重订阅带 from_offset、截断分支、ack 带 acked_offset；
  单测适配
- `server/ws/message.rs` + `server/services/session_control.rs`（旧路由）：`event.index` 引用改
  `start_offset`（旧路由 JSON 仍带 seq 字段的改成 start_offset 兼容值——旧路由已带 start_offset/end_offset
  可选字段，把 start_index 语义替换为 start_offset 即可）

### D2 双速传播 + HTTP 历史（新）
- `forward_loop` 双速模式（见 D1）
- `server/app.rs` + 新 `server/controllers/session_history_controller.rs`：
  `GET /api/sessions/{id}/history?from=<u64>`（JWT）→
  `{ min_offset, snapshot_offset, history_bytes, data_base64 }`（`[from, snapshot_offset)` 字节，
  chunk 级跳过 + 半块 slice）——一次性历史，移动端首选路径
- 协议版本：subscribe_ok 携带 `protocol: 3` 字段（老客户端忽略）

## 3. 移动端改动

### M1 Rust 终端链路（新模块 `src-tauri/src/terminal_link.rs`，注册 lib.rs）
- `TerminalSessionManager`（单例，state.rs 注册）：session_id → `TerminalSessionLink`
- `TerminalSessionLink`：每会话一个 tokio-tungstenite WS（路径 `/ws/terminal/session/{id}`），
  JWT 首消息认证（get_global_token + target 地址，与 get_terminal_ws_info 同源）；
  链路加密按 linkCrypto 开关（复用 bedcode_link_crypto，机制对齐 ws_event 通道，见 ws_client.rs 模式）
- 帧解析：TB v3（v2 兼容过渡）；游标 `lastRenderedOffset`；去重/缺口（重订阅带 from_offset）/
  截断（min_offset 越过游标 → 清屏事件）；跨帧裁剪由前端做
- 缓存：会话级字节缓存（Vec<bytes> 片段 + 字节区间），上限（如 16MB）LRU；即收即缓存
- ack：`terminal_ack_rendered(session, offset)` 命令 → 节流（64KB/250ms 与现前端一致）→ v3 ACK 帧
- 重连：意外断开（onClose）→ 退避重连 + 重订阅（保留游标）；手动取消（terminal_unsubscribe）→ 关连接
- 事件（router/event.rs 或新 emit 路径）：
  - `terminal-frame`（session_id, start_offset, end_offset, data_b64, is_waiting）实时帧
  - `terminal-history-ready`（session_id, start_offset, snapshot_offset, data_b64）历史一次性交付
    （HTTP 拉取 + 缓存拼接后发出，之后才开始转发实时事件——「拼接完历史才通知前端消费」）
  - `terminal-state`（session_id, phase / reconnect / truncated / stopped / session_missing）
- 命令：`terminal_subscribe(session_id)` / `terminal_unsubscribe(session_id)` /
  `terminal_set_mode(session_id, mode)` / `terminal_send_input(session_id, data, special_key)` /
  `terminal_ack_rendered(session_id, offset)` / `terminal_get_state(session_id)`
- 订阅联动：start_session 成功 / SyncSessionStatusChanged→Running → 自动 subscribe；
  SyncSessionStatusChanged→Stopped / remove → unsubscribe；设备断开 → 取消全部 + 连接恢复后重新 subscribe

### M2 前端
- `useTerminalSocket.ts` 退役（终端 WS 全部进 Rust）；`terminalBuffer` store 改为驱动 Rust 命令 +
  消费事件 + 维护 lastRenderedOffset；历史拼接：进入终端页 → history-ready 写完 → 实时帧消费；
  去重/缺口/截断状态机保留语义
- `useTerminalBuffer.ts`：subscribe/unsubscribe/setMode 接线（进入页面 → realtime 模式；
  退出页面 → batch 模式）；sendInput 走 `terminal_send_input`（Rust→WS→桌面 PTY）
- `useMobileConnection.ts`：startSession 成功后 terminal_subscribe；session stopped → unsubscribe；
  重连后 re-subscribe；手动断开 → 全量 unsubscribe
- `TerminalView.vue`：卸载时不再「取消订阅」而是切换 batch 模式（会话未停）；重进时 history-ready
- 测试：mockInvoke + 事件模拟适配；跨帧裁剪用例

## 4. 验证
- 桌面：`cargo test`（session_output/forward/control_frame/pty_reader）；`pnpm run test:run`；`pnpm exec eslint .`
- 移动：`cargo test`；`pnpm run test:run`；`pnpm exec eslint .`
- 收尾 `lens_diagnostics mode=all` 无 blocker

## 5. 待确认/风险
- 移动端链路加密：Rust 侧复用 bedcode_link_crypto 客户端（ws_event 通道已有先例）；若风险大先明文 +
  开关降级（保持现有 strict 语义）
- 旧路由 message.rs JSON（ws 文本帧老协议）仅做最小适配（event.index → start_offset），不动其 wire
- 桌面 WebView 环回通道同步切 v3（同一编码迁移点）
---

## 6. 落地核对（2026-09-12 晚，ticket 03/04 完成后记录）

实施与本文档的偏离/细化（均已落地并验证，写在这里取代「文档先行」的中间态）：

- **D-A1 `terminal-history-ready` 事件 → `terminal_get_history` 命令返回值**：spec M2 原定历史拼接
  经 `terminal-history-ready` 事件通知；实际实现为前端进入页面时 invoke `terminal_get_history`
  （Rust 缓存优先，头淘汰时回退桌面 HTTP）直接拿历史 + 快照三件套。语义等价——「拼接完历史才
  开始消费实时帧」由 store 的 historyPreparing 缓冲 + spliceHistory FLUSH 保证。事件面只剩
  `terminal-frame` / `terminal-state`（ticket 03 Answer 同述）。
- **D-A2 历史主路径 = WS 重播入缓存而非 HTTP 先行**：spec §0 写「历史不再走 WS，一次性 HTTP」；
  实际实现：首次 subscribe from_offset=cursor(=0) → 服务端全量重播 [min, snapshot) → 移动端
  Rust 静默入缓存（end ≤ snapshot 不推事件）→ getHistory 缓存优先。这同时化解了 review R1/R2/R3
  的三快照竞态（单一真源 = Rust 缓存；HTTP 仅在缓存空/头淘汰时增量回退）。代价：长会话全量重播
  受 16MB LRU 淘汰 → 截断语义（min_offset > cursor → 清屏）兜底，与 R3 结论一致。
- **D-A3 ack 水位 = max(缓存游标, 渲染游标)**：ingest 即把 acked 推到缓存游标（Rust 缓存即真源，
  收到即视为已消费），`terminal_ack_rendered` 只升不降——比 spec 的「渲染游标」更早释放背压，
  但语义安全（桌面端按 end_offset ≤ acked_offset 弹出）。
- **D-A4 移动端 invoke 返回值用 camelCase 键**（`terminal_get_history`/`terminal_get_state`）：
  Tauri invoke 只对**请求参数**做 camelCase 转换、返回值原样传递——必须在 Rust 侧与前端 TS 接口
  逐字对齐（曾因 snake_case 键导致前端读 undefined，已修，见 ticket 03 Answer）。事件 payload 仍
  snake_case（Rust emit 原样传递，两端约定一致）。
- **D-A5 链路加密本轮明文 + JWT**（spec §5 风险项采纳降级方案），ws-terminal 协商留后续 ticket。
- **D-A6 store 增补**：markSessionStopped 推进 replayGeneration（中止在途 splice 重写游标）；
  historyPreparing 期间实时帧 8MB 防御缓冲上限；缺口重拼接 3s 冷却——均为 spec 未细化的实现细节。
