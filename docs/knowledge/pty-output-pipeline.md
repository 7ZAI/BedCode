# PTY 输出全链路：桌面端 → 移动端/桌面端本地终端显示

本文档描述从桌面端 PTY 进程产生输出到终端（移动端 xterm / 桌面端 xterm）渲染的完整数据链路与订阅协议。

> **当前架构（TB v3 字节连续 + 移动端 WS 迁入 Rust + 移动端两段订阅 + 桌面端三段式拉取模型，2026-09-17）**：
> - 服务端真源 = **字节累计偏移**（`start_offset`/`end_offset`），订阅/游标/去重/截断/ack 全部收敛到 `[start, end)` 区间运算（TB v3，取代 v2 的 seq/事件数语义）
> - **桌面端三段式（源 / 环 / 每订阅者游标）**：源只做「读 PTY + 入环 + 通告水印」（零等待）；环（`UnifiedOutputQueue`，50MB）是唯一缓冲；**每个订阅链路一个独立执行体**持自己的位置指针在环上循环拉取（合帧 → 窗口门控 → 转发）
> - **背压归属（2026-09-17 下移）**：源**不参与**背压（`pty_reader` 无任何暂停判定）；`acked_offset` 是**每个订阅者私有**的窗口水位，只解除/施加本订阅者的驻留——单个慢/僵尸订阅者不再冻结链路（被截断时可自愈重同步，见 §1.5）
> - 桌面端双速传播（realtime 读即传 / batch 满 `batch_bytes` 才发）由订阅者执行体承载；历史经环上零拷贝拉取（不再整段物化），移动端另有 HTTP 快照回退
> - 移动端终端 WS **由 Rust 后端持有**（`bedcode-mobile/src-tauri/src/terminal_link.rs`）：每会话一连接、认证、缓存（真源）、ack 节流、退避重连；前端只触发订阅/模式/输入并消费 `terminal-*` 事件
> - **移动端两段订阅 + 两段背压**（2026-09-17 改造）：
>   - **段1（会话级）** Rust ↔ 桌面端 PTY 输出 — 会话 WS 连接成功后订阅、会话停止/设备断开取消，产物是 Rust 会话级字节缓存（真源）；背压水位 `acked` 锚定缓存游标（收帧即视为消化，桌面端不必为移动端渲染速度停摆）
>   - **段2（页面级）** 前端 ↔ Rust 缓存/实时流 — 进入终端页订阅、退出取消（`terminal_page_subscribe/unsubscribe`，订阅时携带页面级 Tauri Channel），决定输出帧是否推送；帧经 Channel 的 **TB v3 二进制 Raw 消息**投递（替代全局 `terminal-frame` 事件，省 base64 与 JSON 开销），状态/重锚仍走 `terminal-state` / `terminal-resync` 事件；背压水位 `frontend_rendered` 锚定前端渲染游标，未渲染窗口越高位水即停推（字节留缓存，ack 推进即按段补投）
> - 链路加密（ws-terminal 协商）为后续 ticket；当前 JWT 认证 + 明文帧（与 v2 时代明文终端 WS 同安全位）

---

## 全链路概览

> 📊 交互式流程图：[桌面端 PTY 输出数据流](../diagrams/pty-output-flow-desktop.html) · [移动端终端 PTY 输出链路](../diagrams/pty-output-flow-mobile.html)

```text
桌面端                                       移动端
PTY 进程输出
  ↓ (os pipe)
PtyReader (std::thread) ─── 原始字节（不含任何背压/暂停判定）
  ↓ 单通道有序队列
UnifiedOutputQueue（字节块环，唯一缓冲，50MB）
  ├─ push：分配 start_offset、min_offset 推进、超限淘汰最旧（不阻塞）
  └─ watch::send(max_offset)  ← 只通告水印，不做投递
        │
        ├─ 订阅者执行体 A（移动端）   持 next/acked 游标 → 合帧 → 窗口门控
        ├─ 订阅者执行体 B（桌面本地） 同上（本地通道 4ms 合并窗口）
        └─ 订阅者执行体 …（旧 Message 路由客户端）
                ↓ encode_output_frame_v3（start_offset(8LE)+len(4LE)）
                ↓ WebSocket TB v3 帧（历史 → history_end → 实时）
   ├─→ 移动端: terminal_link.rs（Rust 持有）       ────────▶ HTTP GET /api/sessions/{id}/history?from=
   │      → 字节缓存（16MB LRU，真源）                       （一次性历史，快照字节截取）
   │      → ack 回发 → 桌面端**该订阅者私有**窗口解锁  ↑
   │      → 段2 页面级 Channel（TB v3 二进制帧）+ 事件 terminal-state / terminal-resync → 前端
   └─→ 桌面端: useTerminalOutputStreamChannel（Tauri Channel 原生 IPC；
         WS 环回链路 /ws/terminal/local 已下线删除）

桌面端每个订阅者执行体（一次订阅 = 一个执行体，互不影响）：

  next/acked 游标  等唤醒（watch 新数据 / ack）→ 环上 read_at(next) → 合帧
                  窗口门控：next − acked ≥ 高位水 → 驻留（只停自己）
                  游标早于环驻留起点 → 下发 resync（§1.5）后从 min_offset 重播
                  窗口持续不降超僵尸超时 → 回收该连接（只影响这一路）

移动端两段订阅（生命周期与背压各自独立）：

  段1（会话级）  会话 WS 连接成功 ──▶ terminal_subscribe ──▶ Rust 收帧入缓存（真源）
                 会话停止 / 设备断开 ──▶ terminal_unsubscribe
                 背压：桌面端**每订阅者私有**窗口（高 128KB / 低 64KB）→ 停发该订阅者
                       Rust 侧 acked 锚定「缓存游标」（收帧即消化，不等渲染，64KB/250ms 节流）

  段2（页面级）  进入终端页 ──▶ terminal_page_subscribe(channel) ──▶ 帧经 Channel 推送
                 退出终端页 ──▶ terminal_page_unsubscribe ──▶ 只入缓存不推帧
                 背压：Rust 侧「未渲染窗口」= cursor − 前端渲染游标（高 1MB / 低 256KB）
                       越高位水 → 停推（字节留缓存，零丢失）
                       前端 ack 推进游标 → 补投一批（单批 ≤ 256KB，由消费端节奏驱动）
```

两个消费出口共享**同一真源**，走**同一套 TB v3 二进制协议**；历史获取渠道不同（移动端 HTTP 一次性拉取、桌面本地终端经订阅快照）。

---

## 1. 服务端（桌面端）

### 1.1 输出读取与入队

- `pty/pty_reader.rs`：PTY 读取线程只构造 `OutputEvent`（不分配序号，**不含任何背压/暂停判定**）；字节区间起点由 `session/session_output.rs::on_output` 在串行临界区内分配（`start_offset = 队列 max_offset`）
- `UnifiedOutputQueue`（字节块环，唯一缓冲）：`VecDeque<OutputChunk{start_offset, bytes: Bytes, end_is_waiting}>`；`max_offset`（产出游标）/ `min_offset`（驻留最旧）/ `total_bytes` / `max_total_bytes`（50MB，可配置）/ `max_chunks`（65536 防极小块风暴）；push 时 while 淘汰最旧（min_offset 推进）
- `on_output(event)`：分配 offset → 入环（淘汰最旧，**不阻塞**）→ `watch::send_replace(max_offset)` 通告水印——**不向任何订阅者投递**
- 环读取助手：`watermarks() -> (min, max)`；`read_at(from) -> Result<Option<RingSlice>, u64>`（零拷贝单块视图，`Bytes::slice` 半块裁头；`from < min` → `Err(min)` 表示已淘汰，调用方进入重同步）；`range(from, to)` 供 HTTP 历史
- `register_subscriber(client_id, from_offset, mode)`：**不物化历史**，只读水印 + 建句柄（`SubscriberHandle`）+ 登记；返回 `PullSubscriber{handle, response{subscribe_ok 三件套}, max_watch}`；`ack_subscriber(client_id, offset)` 推进**该订阅者私有**水位；`unsubscribe_subscriber` 移除句柄

### 1.2 订阅者执行体（合帧 + 窗口门控）与编码（TB v3）

- `server/websocket/terminal_ws/subscriber.rs`（每订阅链路一个执行体）：
  - 循环：等唤醒（`watch` 新数据 / ack）→ 读水印 → 截断检测 → 合帧推进 → 窗口门控 → `out_tx` 交接
  - **合帧**：realtime = 时间窗（`flush_interval`）+ 字节窗（`max_buffer_size`）；batch = 满 `batch_bytes` 才发（无时间窗）；`flush_interval = ZERO` = 零缓冲直通；模式翻转即时 flush 残留批次
  - **窗口门控**：`next − acked ≥ 高位水` → 驻留（`park_until_ack`，ack 唤醒 + `park_poll` 兜底轮询）；窗口降到低位水解除
  - **僵尸回收**：窗口持续不降超 `subscriber_zombie_timeout_ms` → 下发 `error{code:"lag_truncated"}` + 关闭该连接（只影响这一路）
  - 历史边界（I7）：`[历史帧] → HistoryEnd → [实时帧]`；**空历史也必发 HistoryEnd**（否则客户端等不到边界）；重同步后不再发（resync 帧自带边界）
- `server/websocket/terminal_ws/forward.rs`：
  - `encode_output_frame_v3(start_offset, is_waiting, data)`：`magic "TB"(2) + version=3(1) + flags(1) + start_offset(8 LE) + len(4 LE) + data`（16 字节头；flags bit0 = is_waiting；无事件数编码、无 128 上限）
  - `OutputBuffer`（纯决策/编码单元）：`is_contiguous_with(start)` 连续性切批判定 + `should_flush(mode, batch_bytes, max_buffer_size, since_last_flush, flush_interval)` 合帧触发判定——**新旧路径共用同一实现**（帧头区间 = 负载；带洞/重叠必切批）
- `server/websocket/terminal_ws/control_frame.rs` 控制帧（JSON）：
  - 客户端 → 服务端：`auth {token}`、`subscribe {from_offset?}`（缺省 = 服务端从 min_offset 全量回放，老客户端兼容）、`input {data(base64), special_key?}`、`mode {realtime|batch}`
  - 服务端 → 客户端：`auth_ok`、`subscribe_ok {protocol:3, snapshot_offset, min_offset, history_bytes}`、`history_end {snapshot_offset}`、`resync {min_offset, snapshot_offset}`（§1.5）、`session_stopped {session_id}`、`error {code, message}`
- 背压 ack（客户端 → 服务端二进制）：TB 帧头 + flags ACK(0x02) + `acked_offset(8 LE)` + `len(4 LE)` + session_id 负载；服务端按 `client_id`（= 连接地址）路由到**该订阅者私有**的 ack 水位（单调前移，I6；陈旧/乱序 ack 天然忽略）

### 1.3 HTTP 一次性历史（移动端首选路径）

- `GET /api/sessions/{id}/history?from=<u64>`（JWT 认证，见 app.rs 路由 + session_controller::get_session_history）
- 返回统一信封 `ApiResponse{code, message, data:{min_offset, snapshot_offset, history_bytes, data_base64}}`
  - `[from, snapshot_offset)` 字节（chunk 级跳过 + 半块 slice）；session 不存在 → code=1002
- 移动端 `terminal_get_history`：**缓存优先**（真源 = Rust 缓存）；缓存空/头被淘汰时回退此接口增量拉取

### 1.4 认证

- `ws/terminal/session/{id}` 连接后首消息 JWT 认证（`auth {token}`），成功回 `auth_ok` 后客户端才能 subscribe；随 WS 关闭会话资源释放。链路加密协商后续 ticket

### 1.5 重同步（resync）协议与客户端契约

**触发**：订阅者游标早于环驻留起点（`next < min_offset`）。两类来源——
① 订阅时 `from_offset < min_offset`（订阅即截断，响应 `subscribe_ok.min_offset` 已携带）；
② 驻留/断网期间环淘汰越过游标（长期不 ack，或客户端断网重连后游标过旧）。

**服务端动作**（`subscriber.rs`）：先落盘残留批次（帧区间必须与负载一致）→ 下发
`resync {min_offset, snapshot_offset}` → 游标/快照重锚到 `min_offset`/当前 `max_offset`
→ 从 `min_offset` 连续重播；此后的历史边界即 resync 帧（**不再补发 history_end**）。

**客户端契约**（只增不改，老端忽略未知控制帧后退化为既有间接自愈路径）：

1. 清屏（`xterm.clear()`）
2. 游标重锚到 `min_offset`（此后重播帧恰从该点起 → 无缺口、无重复）
3. 提示「历史不完整」（移动端 `mobile.terminal.historyTruncated`；桌面本地终端 toast），
   同一订阅者一次截断只提示一次
4. 移动端额外：重锚缓存（`SessionCache::reset_to`）+ acked/cursor/frontend_rendered 全部
   置到 `min_offset` + 立即回发一次 ack（让桌面端窗口归零、立刻解除驻留）

| 端 | 消费点 | 说明 |
| --- | --- | --- |
| 移动端 | `terminal_link.rs::apply_resync` + `terminal-resync` 事件 | Rust 重锚缓存与水位；前端清屏重锚（`stores/terminalBuffer.ts::onResyncEvent`）。既有「帧首越过游标 → forceReplay → getHistory → minOffset 越界 → 清屏」保留为兜底 |
| 桌面本地终端 | `useTerminalOutputStreamChannel.ts`（Channel JSON 控制帧） | 清屏 + 重锚 + 提示；显式信号省去一次「缺口 → 重订阅」往返 |
| 旧 Message 路由客户端 | 吞掉（不识别未知控制帧） | 依赖其既有 gap 自愈路径，行为不变 |

### 1.6 三段容量 / 水位预算（禁单独调整）

| 段 | 容量 | 停推/驻留水位 | 解除水位 |
| --- | --- | --- | --- |
| 桌面环（唯一缓冲，源侧） | 50MB（`channels.global_queue_max_bytes`） | 满则淘汰最旧（不暂停源） | — |
| 桌面订阅者窗口（每订阅者私有） | — | 高位水 128KB（`terminal.subscriber_high_water_bytes`） | 低位水 64KB（`terminal.subscriber_low_water_bytes`） |
| 移动端 Rust 缓存（段1 产物，真源） | 16MB LRU | 满则淘汰头部（越界由 resync 自愈） | — |
| 移动端段2（Rust → 前端） | — | 未渲染窗口 1MB（`SEG2_HIGH_WATER_BYTES`） | 不按水位解除：**ack 推进即补投**，单批 ≤ 256KB（`SEG2_ACK_PUSH_CHUNK_BYTES`，见移动端优化文档 §17） |

**必须成立的解锁关系**：`客户端 ack 阈值 ≤ 订阅者低位水 < 订阅者高位水`，且
`高位水 − ack 阈值 ≤ 低位水`（一次 ack 即能把窗口压到低位水以下解锁）。
当前客户端 ack 阈值为 64KB（桌面 `useTerminalOutputStreamChannel`、移动端
`terminal_link::ACK_BYTES_THRESHOLD`，两端一致）→ 128 − 64 = 64 ≤ 64 ✓。
另有 `高位水 < 环驻留上限`（上游缓存必须大于下游窗口，否则下游还没驻留就已被淘汰 →
无谓截断）。桌面端 `TerminalConfig::subscriber_budget_violation` 在装配订阅者时校验
该关系并告警，默认配置有单测锁定；**单独抬高任一阈值前必须先核对这条关系**。

---

## 2. 前端

### 2.1 桌面端本地终端（`composables/useTerminalOutputStream.ts` + Channel 变体）

- 挂载订阅 / 卸载断开（非会话即订阅）；TB v3 解析 + lastRenderedOffset 游标 + **跨帧裁剪**（`overlap = cursor - start_offset` → `data.subarray(overlap)` 零重复）；ack 带 acked_offset
- 桌面端不接线双速 UI（保持 realtime；双速能力给移动端页面进出用）

### 2.2 移动端终端（Rust 后端持有 + 事件驱动）

- **Rust 层** `terminal_link.rs`（`terminal_link_manager` 单例，lib.rs 已注册）：
  - **段1**：每会话一个 tokio-tungstenite WS；JWT 首消息认证；`subscribe from_offset = 已接收游标`（重连续传不重发已缓存区）
  - 会话级字节缓存 `SessionCache`（VecDeque 片段 + 字节区间，16MB LRU 淘汰头部）：WS 收帧即收即缓存（真源、与页面无关）；`contiguous_runs(from)` 供段2 补推按「连续段」切分（跨洞不合并）
  - **段1 ack**：`acked` 水位 = 缓存游标（收帧即视为已消化，锚点是 Rust 缓存而非前端渲染）；节流回发（64KB 阈值 + 250ms 空闲兜底 + 空闲轮询定时器——见移动端优化文档 §13）
  - **段2 订阅态与背压**：`frontend_subscribed`（`Arc<AtomicBool>`，由**管理器**持有而非链路对象——链路会话停止/恢复重建后沿用同一份，页面存活期间不断流）；`frontend_rendered`（前端 `terminal_ack_rendered` 推进，上界钳到缓存游标）；未渲染窗口 `cursor − frontend_rendered` 越高位水（1MB）停推；**补投由 ack 驱动**（`ack_backlog_push_range` 判定「ack 推进且有滞留」→ `seg2_drain` 按缓存连续段切片补投，单批 ≤ 256KB），不再要求窗口先回落低位水
  - **段2 帧出口**：页面通道 `Channel<InvokeResponseBody>`（`Arc<Mutex<Option<..>>>`，与订阅态同由管理器持有、链路重建沿用），帧以 `InvokeResponseBody::Raw(TB v3 数据帧)` 发送；发送失败 = 消费端已离去 → 记 warn + 就地清槽（不依赖额外握手）
  - **重同步消费**（§1.5）：`resync` 控制帧 → `apply_resync`（缓存 `reset_to(min_offset)`、cursor/acked/frontend_rendered 重锚、置 `resynced`）+ 立即回发 ack；重播帧在 `resynced` 下**直达前端**（历史边界不再适用，否则恢复内容要退回一次 getHistory 往返）；同时发 `terminal-resync` 事件让前端清屏重锚
  - 推送门控 `should_emit_live_frame`：`(resynced || end > snapshot)` + 握手完成（subscribe_ok）+ 段2 已订阅 + 段2 未暂停——全真才推（经 Channel 发送），否则只入缓存
  - 重连：意外断开放弃 + 指数退避（500ms→8s 封顶）+ 重订阅（保留游标）；会话缺失（SESSION_NOT_FOUND）有限重试后停止；手动取消 → 关连接不再重连
  - `terminal-state` 事件携带 phase（idle/connecting/auth/history/live）/ cursor / snapshot_offset / min_offset / mode / detail
  - 命令：`terminal_subscribe` / `terminal_unsubscribe`（**段1**，会话级）· `terminal_page_subscribe(session_id, channel)` / `terminal_page_unsubscribe`（**段2**，页面级）· `terminal_unsubscribe_all` / `terminal_remove` / `terminal_send_input`（base64 + special_key）/ `terminal_set_mode`（realtime/batch）/ `terminal_ack_rendered` / `terminal_get_history` / `terminal_get_state`（含段2 诊断字段 frontendSubscribed/frontendRendered/seg2UnrenderedBytes/seg2Paused）
  - **invoke 返回值使用 camelCase 键**（Tauri invoke 只转换请求参数、返回值原样传递——必须与前端 TS 接口逐字对齐）
- **前端**（`stores/terminalBuffer.ts` + `composables/useTerminalBuffer.ts` + `views/TerminalView.vue`）：
  - **段1 生命周期（会话级）**：会话启动（startSession）→ `terminal_subscribe`；Stopped → `markSessionStopped`（取消订阅 + 游离标）；设备断开/恢复配对 → 全量取消 / 重建订阅；意外断开重连由 Rust 自动处理。页面进出**不**触发段1
  - **段2 生命周期（页面级）**：进入终端页 → `markPageEntered`（新建页面 Channel 并**先挂 `onmessage` 再** `terminal_page_subscribe(sessionId, channel)` + `set_mode(realtime)`）+ 历史拼接；退出（会话未停）→ `markPageLeft`（先把 `onmessage` 换成空操作——在途帧不得写进已卸载的 xterm——再 `terminal_page_unsubscribe` + `set_mode(batch)`）。未订阅期间 Rust 只入缓存不推帧，前端不再渲染
  - **段2 帧消费**：`Channel.onmessage`（`ArrayBuffer`，Rust 侧 `InvokeResponseBody::Raw`）→ `onChannelMessage` 按 16B 帧头逐帧解析（**一条消息可承载多帧**：补投切片只落在字节边界）→ 与事件路径共用 `handleFrame`（拼接期缓冲 / 之后投递渲染）；帧头异常（magic / 版本 / 长度越界）停止解析并告警，交由缺口自愈路径重拼接
  - 历史拼接：进入页面 → `terminalGetHistory(from=游标)`（缓存优先/HTTP 回退）→ 历史段经 writeParsed 写入 xterm（写解析完成才推进游标）→ 期间到达的实时帧缓冲（historyPreparing）→ 拼接完成 FLUSH——「拼完历史才通知前端消费」；拼接完成后立即 `ackRendered`（不能只依赖 onWriteParsed 的触发时序）
  - 字节连续判定（与桌面端同构，spec §0.1 8 条对齐清单）：`lastRenderedOffset` 游标；去重 `endOffset ≤ cursor` 整帧跳过；缺口 `startOffset > cursor` → `forceReplay`（重拼接带 from_offset，3s 冷却）；跨帧裁剪 `overlap = cursor - startOffset`；截断 `min_offset > cursor` → 清屏 + truncated 提示
  - `sendInput` → `terminal_send_input`（Rust → WS → 桌面 PTY）；resize 保持既有路径；**段2 渲染背压** `onWriteParsed → terminal_ack_rendered`（推进 `frontend_rendered`，段2 补推的触发点）

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
| 重同步 | `resync {min_offset, snapshot_offset}` 控制帧（§1.5）：清屏 + 重锚 + 一次性提示，随后从 `min_offset` 连续重播（历史边界由该帧自带） |
| ack（**段1**） | 二进制帧（ACK flag + acked_offset(8LE) + session_id 负载）；节流 64KB / 250ms 兜底 + 空闲轮询；水位锚定 Rust 缓存游标；**桌面端按 client_id 路由到该订阅者私有窗口**（不是会话级共享记账） |
| 源侧背压 | **无**：`pty_reader` 不含任何暂停/水位判定；慢订阅者的节流只发生在它自己的订阅者执行体（`park_until_ack`） |
| 段2 订阅 | `terminal_page_subscribe(session_id, channel)` / `terminal_page_unsubscribe`（页面进出配对）；Rust 未订阅时只入缓存不推帧 |
| 段2 帧出口 | Tauri Channel + `InvokeResponseBody::Raw`（**复用同一 TB v3 数据帧布局**，一条消息可承载多帧）；状态/重锚仍是 `terminal-state` / `terminal-resync` 全局事件 |
| 段2 背压 | 未渲染窗口 `cursor − frontend_rendered`；高位水 1MB 停推；**ack 推进即补投**（单批 ≤ `SEG2_ACK_PUSH_CHUNK_BYTES` = 256KB，按缓存连续段切片），推完按窗口重估暂停态 |
| 双速 | realtime 时间窗+字节窗合并；batch 仅满 batch_bytes 才发（默认 64KB，config `terminal.batch_bytes`） |
| HTTP 历史 | `GET /api/sessions/{id}/history?from=` → ApiResponse 信封 + data_base64（snake_case 内层，移动端映射 camelCase） |

---

## 4. 测试覆盖

- **桌面端**（cargo test）：session_output（环 push/淘汰/range/read_at/watermarks、订阅者注册与私有 ack、HTTP snapshot_bytes）、subscriber（**I1–I7 全覆盖**：顺序零丢失、历史边界、空历史边界、窗口门控与 ack 补发、驻留不影响源与其他订阅者、截断重同步与重播、零拷贝保活、僵尸回收、模式切换、订阅即截断、水位预算）、forward（v3 编码/合帧纯决策/连续性切分）、control_frame（subscribe from_offset/mode/resync/ack 解析 v3+v2）、pty_reader（源零背压：零订阅者仍全量入环）、ws_session_route（v3 断言）、HTTP history controller、build_manifest_smoke
- **桌面端 vitest**：useTerminalOutputStream(+Channel) 帧解析/跨帧裁剪/游标/**resync 控制帧（清屏+重锚+提示一次）**；terminal-flow 集成
- **移动端**（cargo test）：terminal_link 内联单测（TB v3/v2 解析、`encode_data_frame` 编码、SessionCache push/snapshot 半块 slice/LRU/`contiguous_runs` 按洞切分/**`reset_to` 重锚无缺口**、段2 门控 `should_emit_live_frame` 四条件 + **resynced 旁路**、段2 水位滞回 `seg2_paused_after`、**ack 驱动补投 `ack_backlog_push_range`**、管理器段2 订阅态会话隔离与持久性、build_ack_frame 逐字节与桌面 parse_ack_frame 对齐、ack 节流 `should_send_ack` 含空闲兜底回归）
- **移动端 vitest**：terminalBuffer store（事件同步/历史拼接/跨帧裁剪/缺口重拼接/截断/**terminal-resync 清屏重锚**/停止恢复/输入/ack/双速/段2 订阅与页面进出配对）、useTerminalBuffer、terminal-flow 集成（段2 Channel 帧链 + 历史拼接 FLUSH + terminal_send_input）

---

## 5. 相关文档

- `.scratch/2026-09-12-mobile-ws-rust/spec.md`（移动端 WS 迁 Rust 架构方案 + 两端语义对齐清单 + 契约）
- `.scratch/2026-09-12-pty-byte-history/spec.md`（TB v3 基础方案：事件 index → 字节偏移）
- **`.scratch/2026-09-17-pty-pull-subscribers/spec.md`**（**目标态**：桌面端「单生产者环形缓存 + 每订阅者拉取游标」，背压从源侧下移到各订阅者任务；含移动端显示链路对齐优化）
- `bedcode-mobile/docs/terminal-output-pipeline-optimization.md`（移动端链路优化记录：§13 背压 ack 空闲滞留、§14 两段订阅与两段背压、§17 段2 背压死锁修复、§18 PTY 输出改 Channel）
- `bedcode-mobile/docs/code-map.md`（移动端代码地图）
- AGENTS.md §9（协议两端同步部署；老端忽略未知字段）