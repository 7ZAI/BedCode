# PTY 历史保存颗粒度重构：事件 index → 字节偏移（TB v3）

> 状态: 方案定稿（桌面端实施待开工；移动端由另一 agent 同步实施）
> 范围: **本次只改桌面端（pty 服务端 + 桌面 WebView 前端）**；移动端按 §7 契约由另一 agent 改造。
> 关联: AGENTS.md §9（协议改动两端同步部署）、docs/knowledge/mobile-desktop-auth.md、.scratch 任务文档规范

---

## 1. 背景与动机

现状 PTY 输出连续性用**事件 index**（会话内单调序号）表达，历史保存颗粒度为**事件**（`OutputEvent`）。存在四类问题：

1. **跨帧无法精确裁剪（重复渲染缺陷）**：快照重播是全量从 `min_seq` 发出的（`subscribe()` 中 `queue.get_events()` 不裁剪不跳段）。重订阅后首个重播帧可能**跨过**已渲染游标（`frame.seq <= cursor < frame.lastSeq`），而帧头 flags 只编码事件**个数**、不带各事件**字节长**，客户端无法裁掉跨帧前半段 → 整帧重渲染 → 屏幕上已渲染内容重复输出（`\r\n`、文本打两遍）。
2. **双重度量混模型**：渲染游标/缺口检测用 seq；背压记账（`unacked_bytes`）、历史 LRU（`historyBytes`）、ack 节流阈值（`pendingAckBytes`）已经是字节。同一链路两种度量，转换点都是 bug 温床。
3. **存储颗粒度头部冗余**：`VecDeque<OutputEvent>` 每条携带 `session_id: String`（逐条重复）+ `timestamp` + `index`，回放只消费 `data`。64MB 预算下元数据占比高，且 `get_events()` 全量 clone 后逐订阅者各拷一份。
4. **快照截取能力缺失**：`history_start_mode=snapshot` 未实现，回放只能"全量重播 + 客户端游标跳过"，无法按字节位置截取。

**目标**：连续性记录改为**字节数目（会话内累计字节偏移）**；历史保存颗粒度降为**字节块（chunk）**；移动端重放历史直接依据字节 offset 截取快照。

---

## 2. 术语与不变量

| 术语 | 定义 |
| --- | --- |
| `start_offset` | 一段字节在会话输出流中的起始位置（累计字节数） |
| `end_offset` | `start_offset + len` |
| 全局 offset | 会话输出总量（`max_offset` = 已产出总字节），u64 无溢出风险 |
| 字节块 chunk | 历史保存/回放的最小单元：`{start_offset, bytes, end_is_waiting}` |
| 连续不变量 | `chunks[i+1].start_offset == chunks[i].start_offset + chunks[i].bytes.len()`，且对同一段流的所有切片（事件/帧/chunk）**字节区间铺满无重叠无空洞** |

**核心语义变化**：帧头"第几个事件"→"累计第几个字节"；游标推进、缺口检测、去重、ack、快照截取全部收敛到 `[start_offset, end_offset)` 区间运算。

---

## 3. 现状剖析（数据流 + 各组件）

> ⚠️ **【2026-09-12 已过时】本段为改造前（v2 时代）现状快照**：代码已全部迁移
> TB v3（字节块队列 / `encode_output_frame_v3` / `acked_offset` / 字节三件套），
> 下列 v2 函数名 / 行号 / 字段均已不存在。事实以 §4~§8 与代码为准
> （服务端 `session_output.rs` / `forward.rs`，前端 `useTerminalOutputStream*.ts`）。

```
PTY master fd（字节流）
  └─ pty_reader.rs: BufReader(read 4096) → 每次 read 一块 → OutputEvent{index,data,...}
       └─ mpsc(16384) 有序队列 → 单消费者
            └─ session_output.rs::on_output（output_serial 串行临界区）
                 ├─ UnifiedOutputQueue.push：分配 index = max_seq+1，双上限淘汰最旧
                 ├─ unacked 记账（unacked_fifo: (index, bytes)）
                 └─ 广播 → 订阅者 send_queue
                      └─ forward.rs::forward_loop：OutputBuffer 合并（≤128 事件 / 64KB / 时间窗）
                           └─ TB v2 帧：magic"TB"+version=2+flags(count-1<<1|waiting)+seq(8LE)+len(4LE)+data
                                └─ WS → 移动端 / 桌面 WebView
```

关键现状（引用点）：
- `OutputEvent { session_id, data, index, timestamp, is_waiting }`（session_output.rs:41）
- `UnifiedOutputQueue`：`VecDeque<OutputEvent>` + `capacity=100_000` + `max_total_bytes=64MB`（config.rs:436-438），`min_seq/max_seq` 原子，push 时 while 淘汰最旧（session_output.rs:151）
- `subscribe()`：`SubscribeResponse { min_seq, snapshot_seq, history_count }` + 全量重播 + `HistoryEnd`（session_output.rs:520）
- 帧编码 `encode_output_frame_v2(seq, event_count, is_waiting, data)`（forward.rs:44）【⚠️ 已删 → `encode_output_frame_v3(start_offset, is_waiting, data)`】
- ack 帧：复用 v2 头，flags bit1=ACK，seq=last_rendered_seq，payload=session_id（control_frame.rs:83）【⚠️ 已改 → v3 头 `acked_offset` 语义；v2 头仅 ack 入站兼容接受（`parse_ack_frame`）】
- 桌面 WebView 前端解析 `useTerminalOutputStreamChannel.ts`（seq/eventCount/lastSeq 同 WS 语义）【⚠️ 已改 → v3：`startOffset/endOffset` + `lastRenderedOffset` 游标】

---

## 4. 目标设计

### 4.1 存储结构：字节块队列（替换 UnifiedOutputQueue）

```rust
pub struct OutputChunk {
    start_offset: u64,
    bytes: tokio::bytes::Bytes, // Arc 共享；slice() 支持半块切（快照截取/精调淘汰）
    end_is_waiting: bool,       // 保留尾部事件的 waiting 语义（等待提示）
}

pub struct UnifiedOutputQueue {
    chunks: VecDeque<OutputChunk>,
    min_offset: u64,          // = front().start_offset
    max_offset: u64,          // 产出端游标（= 全量累计字节）
    total_bytes: u64,         // 驻留字节（淘汰基准）
    max_total_bytes: u64,     // 字节上限（默认 50MB，配置化；沿用 channels.global_queue_max_bytes）
    max_chunks: usize,        // 防御性条目上限（默认 65536，抗极小块风暴）
    // 移除：capacity、max_seq、min_seq、total_produced（total_produced 如需日志可保留为调试字段）
}
```

- **chunk 颗粒度**：每次 read 一块（对齐现有事件粒度，零合并成本）；chunk 头部 16B，对比事件 48B+。
- **淘汰**：push 时 `while total_bytes > max_total_bytes || chunks.len() >= max_chunks { pop_front(); min_offset = front().start_offset }`，均摊 O(1)。整块淘汰为主；可选 `Bytes::slice` 半块裁头精调（默认不做，超限 ≤ 一个 chunk 可接受）。
- **回放**：迭代 chunks，`&chunk.bytes` 直接编帧，Arc 共享零拷贝广播；`Bytes::slice` 支持从任意 offset 起播（§4.4）。
- **上限语义**：50MB 是**队列自身**软上限；Arc 共享被订阅者转发帧/客户端缓冲持有时另计，多订阅者下实际驻留可能短暂超限——设计预期，不做硬顶。

### 4.2 事件模型：OutputEvent 精简

```rust
pub struct OutputEvent {
    pub session_id: String,   // 保留（路由/日志）
    pub data: Vec<u8>,
    pub start_offset: u64,    // index 语义替换（由 on_output 在串行临界区内分配 max_offset）
    pub timestamp: i64,       // 保留（调试/审计；回放不依赖）
    pub is_waiting: bool,
}
// start 之后可用 offset 由 start_offset + data.len() 直接推导，不落地存储
```

移除 `index`；新增语义：`事件字节区间 = [start_offset, start_offset + data.len())`。

### 4.3 Wire：TB v3 帧（服务端→客户端输出帧）

```
16B 头：magic "TB"(2) + version=3(1) + flags(1) + start_offset(8 LE) + len(4 LE) + data
flags:  bit0 = is_waiting（保留现有语义）
        高 7 位 —— 不再编码事件数（删除 V2_FRAME_FLAG_COUNT_SHIFT 机制）
```

- `end_offset = start_offset + len` 直接可导，客户端无需 count 推导 lastSeq。
- **连续判定**：`frame.start_offset == 本地游标` 即连续；`>` 缺口（重订阅），`<` 重复（跨帧裁剪，§7 第 4 条）。
- 合并批次内事件个数不再需要 wire 表达（帧数据即字节数）；forward_loop 的 128 事件上限删除，保留字节窗（64KB，可配置）与时间窗。
- **版本分派**：解析器按 `version` 字段分派 v2（seq+count 语义）与 v3（offset 语义）。服务端握手/订阅响应中声明协议版本供客户端选择（过渡期兼容策略见 §8）。
  > ⚠️ 【2026-09-12 未按此实现】输出方向**无 v2/v3 分派**（v2 兼容仅剩 ack 入站：`parse_ack_frame` 接受 V2/V3 头）；`subscribe_ok` 无版本协商、`protocol` 恒 3。旧 v2 客户端收 v3 帧直接断（前端 `parseFrames` 只认 version=3）。

### 4.4 快照截取（移动端按字节 offset 索取历史）

`subscribe` 请求扩展，客户端可携带**字节游标**：

```
subscribe 控制帧（JSON）新增可选字段：{"type":"subscribe", "from_offset": <u64>?}
  - 省略：服务端从 min_offset 全量回放（现状行为，兼容）
  - 提供：服务端从 >= from_offset 的字节位置起播（chunk 级跳过 + 末块 Bytes::slice 半块裁头，
    end_is_waiting 仅保留末块）
```

`subscribe_ok` 响应元数据改字节三件套（替换 seq 版）：

```
subscribe_ok : { "snapshot_offset": <u64>, "min_offset": <u64>, "history_bytes": <u64> }
  - snapshot_offset = 订阅时刻 max_offset（历史边界；历史帧 end_offset <= 该值）
  - min_offset = 当前驻留最旧字节位置（客户端游标 < min_offset → 截断处理）
  - history_bytes = 驻留历史总字节（替代 history_count 事件条数）
```

`HistoryEnd` 标记保留（透传 snapshot_offset/min_offset/history_bytes），顺序语义不变（严格在历史帧之后）。

### 4.5 背压 ack：acked_seq → acked_offset

ack 帧复用 TB v3 头：`magic"TB" + version=3 + flags=ACK(bit1) + acked_offset(8 LE) + len(session_id 字节数) + session_id`。

桌面端 `unacked_fifo`：`VecDeque<(u64 index, u64 bytes)>` → `VecDeque<(u64 end_offset, u64 bytes)>`，弹出条件 `end_offset <= acked_offset`。背压水位（`unacked_bytes`/`should_pause`）已是字节，不动。

### 4.6 客户端（桌面 WebView + 移动端）游标语义统一

- `lastRenderedSeq` → `lastRenderedOffset`（已渲染到 end_offset）
- 去重：`frame.end_offset <= cursor` 整帧跳过
- 缺口：`frame.start_offset > cursor` → 重订阅（带 from_offset=cursor）
- 截断：`subscribe_ok.min_offset > cursor` → 清屏 + truncated 提示 + 锚定重播
- 跨帧裁剪：`cursor - frame.start_offset = overlap`，渲染 `data[overlap..]`（根治 §1.1 重复渲染；游标恒为事件边界，切片起点恒合法）

---

## 5. 桌面端改造清单（本次实施范围）

| # | 文件 | 改动 |
| --- | --- | --- |
| 1 | `src/pty/pty_reader.rs` | 每次 read 构造 OutputEvent 时 index 位先占（start_offset 最终由 on_output 分配）；其余不动 |
| 2 | `src/session/session_output.rs` | `OutputEvent` 增 start_offset 删 index；`UnifiedOutputQueue` 重写为字节块队列（§4.1）；on_output 分配 `start_offset = max_offset`；subscribe 支持 from_offset + 字节三件套响应 + 半块切片回放；unacked_fifo 改 end_offset；相关内联测试全量适配 |
| 3 | `src/server/ws/terminal_ws/forward.rs` | `encode_output_frame_v3(start_offset, is_waiting, data)`；删 count 编码与 128 事件上限；OutputBuffer 记录 start_offset（= 首块 offset）；测试适配 |
| 4 | `src/server/ws/terminal_ws/control_frame.rs` | `parse_ack_frame` → acked_offset；版本分派 v2/v3；新增 subscribe 请求 from_offset 解析 【实际：版本分派仅 ack 入站 v2/v3 接受，输出无分派】 |
| 5 | `src/server/ws/terminal_ws.rs` | subscribe 响应结构、ack 处理、协议版本声明/协商 【实际：协商未实现——`subscribe_ok.protocol` 恒 3（terminal_ws.rs）】 |
| 6 | `src/system/config.rs` | `global_queue_max_bytes` 默认 64MB → 50MB（或保留 64MB，按产品决策）；新增 `max_chunks`（65536）；`history_start_mode` 相关注释更新（snapshot 现在可按 offset 实现，本次不落地） |
| 7 | `bedcode-desktop/src/composables/useTerminalOutputStreamChannel.ts` | TB v3 解析、lastRenderedOffset、缺口/去重/截断/跨帧裁剪逻辑同步（与 WS 路径同语义） |
| 8 | `bedcode-desktop/src/__tests__/` | 相关单测/集成适配 |
| 9 | 文档 | `docs/knowledge/` 协议描述（spec §5.3 引用处）同步更新 |

---

## 6. 不做的事（防止范围蔓延）

- 移动端（`bedcode-mobile/`）前端与 store 改造 —— 另一 agent 按 §7 契约实施
- `history_start_mode=snapshot`（最近清屏快照点）机制落地 —— 依赖 §4.4 基础设施，另立 ticket
- 全链路历史内存硬核算（订阅者缓冲纳入 50MB）—— 单独课题
- 旧 v2 帧编码长期保留 —— 仅过渡期兼容（§8）
  > ⚠️ 【与代码矛盾（2026-09-12）】v2 输出帧编码**已删除**（forward.rs v3-only，无 `encode_output_frame_v2`）；本条"保留"与 §8"过渡期旧客户端仍可用 v2"均未成立。

---

## 7. 移动端配合契约（另一 agent 交接）

> **订阅机制异构（2026-09-12 用户裁决）**：桌面端保持现行「挂载订阅、卸载断开」（TerminalPreview →
> WS 环回 / Channel，非会话语义）；移动端按 mobile-ws-rust spec §0 的会话即订阅（Rust 常驻）实施——
> 两端生命周期互不对称、各自管理，禁止为对称改动对方。**但数据获取与连续判断语义两端严格对齐**
>（对齐清单见 mobile-ws-rust/spec.md §0.1，以桌面端 `useTerminalOutputStream.ts` deliverFrame 为参照实现）。

移动端按此契约实施（`src/composables/useTerminalSocket.ts` / `src/stores/terminalBuffer.ts` / `src/services/linkCrypto.ts` 相关）：

1. **帧解析**（useTerminalSocket.ts `parseFrame`）：TB v3 头（version=3，start_offset 语义）；兼容 v2 头（version=2，seq+count 语义）解析并存续 `TerminalSocketFrame { startOffset, endOffset, isWaiting }`（v2 换算 `startOffset=seq, endOffset=seq+eventCount-1` 仅作过渡，正式版删除）。
    ⚠️ 桌面端 v3-only：移动端 `terminal_link.rs::parse_tb_frames` 保留的 v2 分支（TB_VERSION_V2）为**不可达死代码**——桌面永不发 v2 帧；"正式版删除"应尽快落地。
2. **游标**：`lastRenderedSeq` → `lastRenderedOffset`（= frame.endOffset）。
3. **去重/缺口/截断**（deliverFrame）：`endOffset <= cursor` 跳过；`startOffset > cursor` 重订阅（subscribe 带 `from_offset=cursor`）；`subscribe_ok.minOffset > cursor` 清屏 + `onTruncated` + 锚定重播。
4. **跨帧裁剪**：`overlap = cursor - frame.startOffset`，渲染 `frame.data.subarray(overlap)` —— **根治重复渲染，实现后去掉"重播帧整帧跳过"的近似**。
5. **ack**（ackRendered/buildAckFrame）：`acked_offset = lastRenderedOffset`；节流阈值/空闲兜底保持字节语义，不动。
6. **subscribe_ok**：读字节三件套（snapshot_offset/min_offset/history_bytes）；history/live phase 分流条件改为 `frame.endOffset > snapshotOffset`。
7. **linkCrypto**：加密信封载荷语义不随帧格式变化（外层加密字节不变，仅内层头字段语义变）。
8. **测试**：mockInvoke/帧构造 fixture 全部换 v3；补跨帧裁剪用例（v2 无法表达的：合并帧跨游标）。

接口对齐点：`TerminalSocketFrame` 形状、deliverFrame 状态机四个分支、ackedThrough 推进、liveBuffer 缓冲条件。

---

## 8. 兼容与部署

- AGENTS.md §9 硬约束：**协议改动必须两端同步部署**。桌面端与移动端同一发布窗口上线 v3；桌面端服务端在 subscribe_ok 中带协议版本，客户端声明版本，服务端按版本选 v2/v3 帧编码——过渡期旧客户端仍可用 v2（seq 语义字段不再演进的冻结编码）。
  > ⚠️ 【与代码矛盾（2026-09-12）】"按版本选 v2/v3 帧编码"未实现：`subscribe_ok.protocol` 硬编码 3、客户端无版本声明字段；旧 v2 客户端**输出必断**——勿宣传 v2 可用（与 `mobile-ws-rust/review.md` R6 一致）。
- 增量演进：老客户端忽略 subscribe 扩展字段（from_offset 可选）；新字段不破坏旧解析。
- 桌面端内部（WebView 环回）与 WS 远程通道共用同一帧编码迁移点，同步切换，避免双实现漂移。

---

## 9. 验证计划

- `cd bedcode-desktop/src-tauri && cargo test`：session_output（队列淘汰/回放/from_offset 切片/截断）、forward（v3 编码/合并/时间窗）、control_frame（ack v3/版本分派）、pty_reader 全绿
  > ⚠️ 已执行（2026-09-12，lib 608 全绿）；"版本分派"实际为 ack 入站 v2/v3 头接受（无输出版本分派）。
- `cd bedcode-desktop && pnpm run test:run`：useTerminalOutputStreamChannel 解析/去重/截断/跨帧裁剪用例
- 根目录 `pnpm exec eslint .` 0 error
- 手工冒烟：桌面本地会话 + 移动端订阅 —— 背压暂停恢复、丢帧重订阅回放、高水位淘汰后重连（截断路径）三条链路

---

## 10. 风险与待确认

- `max_total_bytes` 默认值：50MB（用户提议）vs 现状 64MB —— 实施前确认
- `timestamp`/`total_produced` 去留：建议保留（调试价值），成本低
- 半块 slice 回放的字节精确性依赖 `Bytes` 引入 —— Cargo.toml 新增 `tokio-util`/直接 `bytes` 依赖（bytes 已是 tokio 传递依赖，显式声明即可）
- from_offset 的 wire 位置：JSON subscribe 控制帧扩展字段（推荐，向后兼容）vs 二进制帧 —— 推荐前者