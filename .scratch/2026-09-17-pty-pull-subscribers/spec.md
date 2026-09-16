# 桌面端 PTY 输出链路重构：单生产者环形缓存 + 每订阅者拉取游标（背压下移）

> 状态: **已实施**（2026-09-17；ticket 01–06 落地，07 待真机数据决定，08 门禁/文档完成、真机压测待用户执行——见 §10 状态列与 §13）
> 范围: **桌面端为主**（`pty/pty_reader.rs`、`session/session_output.rs`、`server/ws/terminal_ws.rs`、`server/ws/terminal_ws/forward.rs`）+ **移动端显示链路对齐优化**（§7，含已落地项与待办项）
> 关联: `docs/knowledge/pty-output-pipeline.md`、AGENTS.md §5（高内聚低耦合）/§8（日志红线）/§9（协议两端同步）、`.scratch/2026-09-12-pty-byte-history/spec.md`（TB v3 字节语义基础）、`bedcode-mobile/docs/terminal-output-pipeline-optimization.md` §13/§14（移动端两段订阅与两段背压）

---

## 1. 背景与动机

### 1.1 现状链路（四段式，推送模型）

```text
PTY master fd
  └─ pty_reader.rs：线程 read(4096) ─┬─ 每次 read 前轮询 should_pause()（源侧背压门）
                                     └─ blocking_send → mpsc(16384) 有序队列
  └─ 单消费者任务 → SessionOutputManager::on_output（output_serial 串行临界区）
        ├─ 分配 start_offset → UnifiedOutputQueue.push（50MB 字节块环，淘汰最旧）
        ├─ 会话级 unacked 记账（FIFO）
        └─ 逐订阅者广播：
             active   → sub.send_queue.try_send（满则**有界等待 2s**：阻塞 on_output）
             inactive → sub.pending（16384 事件上限）
  └─ 每订阅者 forward_loop：从 send_queue 收 → 合帧（realtime 时间窗/字节窗；batch 满 batch_bytes）→ WS actor
  └─ 客户端 ack（二进制）→ GlobalOutputManager::ack(session) → on_ack：弹 FIFO 降 unacked
```

### 1.2 症结（本次要解的四条）

1. **源侧背压与消费者耦合，且账目是"会话级共享"**：`unacked_bytes` 按会话整体记，一个订阅者的 ack 就能把水位降到低水位并恢复 PTY 读取（`on_ack` 弹的是 `end_offset ≤ acked_offset` 的共享 FIFO）。多订阅者并存时（桌面本地终端 + 移动端）实际水位语义退化为"最快订阅者的进度"，慢订阅者的落后无法被反馈到源头；反过来，任何一次"慢"都会以 2s 阻塞的形式打在 `on_output`（见第 2 条）。
2. **`on_output` 是阻塞点 = 全局队头阻塞**：订阅者 `send_queue` 满时 `on_output` 有界等待 2s（`SEND_BACKPRESSURE_TIMEOUT`）。等待期间整个会话的 offset 分配与广播停摆 → PTY 读停等 → **所有订阅者一起卡**（含桌面本地终端）。这是一条把"单个慢消费者"放大成"全会话冻结"的路径。
3. **源侧背压门把"读"和"流控"焊在一起**：`pty_reader` 线程里塞了 5ms 轮询 `should_pause()` 的循环。理论上"数据留内核管道、读线程恢复即续读"是零丢失的漂亮设计，但它把「会话级流控决策」放进了最不该关心消费者的地方（源），且决策依据是会话级共享水位（第 1 条）→ 决策不成立时，暂停就是纯粹的功能损失（2026-09-17 真机事故：移动端 ack 滞留 → 源永久暂停 → 滑不动 + 输入无回显，见移动端文档 §13）。
4. **每订阅者两级缓冲 + 历史全量拷贝**：`subscribe()` 用 `snapshot_from()` 把整段历史 `to_vec()` 物化成 `Vec<OutputEvent>`（4MB 历史 = 4MB 拷贝，且每订阅者各一份），再经 `send_queue`（容量 8192）逐个 `await send`；`pending`（16384 事件）与 `send_queue` 是**按订阅者复制的字节副本**——内存随订阅者数量线性放大。

---

## 2. 术语与不变量

| 术语 | 定义 |
| --- | --- |
| 输出环（ring） | 会话级字节块环形缓存（现 `UnifiedOutputQueue`），单生产者（PTY 读线程）写入，`[min_offset, max_offset)` 为驻留区间 |
| `max_offset` | 产出端游标（会话内累计产出字节），单调不减 |
| `min_offset` | 驻留最旧字节位置；环满淘汰最旧块时推进 |
| 订阅者游标 `next_offset` | 某订阅者「下一待发字节位置」，**该订阅者任务的私有状态**（位置指针） |
| ack 水位 `acked_offset` | 某客户端已渲染/已消化到的字节位置，**该订阅者私有** |
| 订阅者窗口 | `next_offset - acked_offset`（该订阅者"已发未确认"字节数） |
| 源侧 | PTY 读线程 + 入环路径（本设计**不含任何等待**） |
| 截断（truncation） | `next_offset < min_offset`：订阅者所需字节已被环淘汰，无法连续供给 |
| 重同步（resync） | 截断后的恢复动作：客户端清屏 + 从 `min_offset` 锚定重播（TB v3 既有语义） |

**不变量（I1–I7，均可测）：**

| 编号 | 不变量 |
| --- | --- |
| I1 | **单写者单调**：仅 PTY 读路径写入环；`max_offset` 单调不减；chunk 区间按序铺满 `[min_offset, max_offset)`（无重叠、无空洞） |
| I2 | **源零等待**：入环路径不等待任何订阅者；环满时淘汰最旧（`push` 恒 O(1) 均摊，不可阻塞） |
| I3 | **订阅者零丢失（窗口内）**：`next_offset ≥ min_offset` 时，该客户端收到的字节区间 `[from, next_offset)` 严格连续、无重复 |
| I4 | **越界即截断（局部化）**：`next_offset < min_offset` → 仅该订阅者进入 resync；其他订阅者与 `max_offset` 推进不受影响 |
| I5 | **背压局部化**：订阅者 park（窗口越界等待 ack）不改变源侧产出速率，不影响其他订阅者 |
| I6 | **ack 单调且私有**：ack offset 只前进（陈旧/乱序 ack 忽略），只作用于本订阅者窗口；不参与任何共享记账 |
| I7 | **快照边界**：首次订阅时 `snapshot_offset = ` 订阅时刻的 `max_offset`；帧序 `[历史 (start, snapshot_offset)) → HistoryEnd → [实时 (snapshot_offset, ∞))` |

---

## 3. 现状剖析（组件级，带文件锚点）

| 组件 | 现状 | 问题 |
| --- | --- | --- |
| `pty/pty_reader.rs:79-82` | 读前 `if pause() { sleep(5ms); continue; }`，`pause = GlobalOutputManager::global().should_pause(sid)` | 流控决策（会话级共享水位）在源侧；轮询睡眠 |
| `pty/pty_reader.rs:102` | 读到的块 `blocking_send` 到 16384 深队列；队列满 → **读线程阻塞** | 消费者慢 → 源停摆（另一条隐式背压路径） |
| `session/session_output.rs:454-582` | `on_output`：串行锁 → 分配 offset → `push` → 会话级 unacked 记账 → 逐订阅者广播（`try_send`，满则 2s 有界等待） | ②队头阻塞；①共享水位 |
| `session/session_output.rs:604-639` | `on_ack(acked_offset)`：弹共享 FIFO、降 `unacked_bytes` | 多订阅者语义退化为"最快者进度" |
| `session/session_output.rs:648-679` | `should_pause()`：会话级滞回（64KB/8KB） | 单一水位无法表达"每订阅者各自的水位" |
| `session/session_output.rs:702-801` | `subscribe()`：占位 → `snapshot_from()` 全量 `to_vec` 拷贝 → 逐条 `await send` → 排空 pending → 激活 | ④历史全量拷贝；占据位/pending 两份按订阅者复制的缓冲 |
| `server/ws/terminal_ws/forward.rs:forward_loop` | 二级转发：`output_rx`（订阅者队列）→ 合帧 → `out_tx`（WS actor） | 与订阅者通道重复缓冲；合帧与流控分离在两个组件 |
| `server/ws/terminal_ws.rs:778/1276` | 移动端新路由 / 桌面本地路由各自 `subscribe()` + spawn `forward_loop` | 同上 |

---

## 4. 目标设计

### 4.1 总览（三段式，拉取模型）

```text
                 ┌─────────────── 源（零等待） ────────────────┐
PTY fd → pty_reader 线程：read → 有序队列 → on_output
                                             ├─ 分配 start_offset
                                             ├─ ring.push（满则淘汰最旧）
                                             └─ watch::send(max_offset)  ← 唤醒信号（不做投递）
                 └────────────────────────────────────────────┘
                                     │  （订阅者各自拉取）
     ┌───────────────────────────────┼───────────────────────────────┐
     ▼                               ▼                               ▼
订阅者任务 A（移动端 WS）      订阅者任务 B（桌面本地终端）     订阅者任务 C（…）
 next_offset / acked_offset     next_offset / acked_offset      …
 循环：等唤醒 → 取区间 → 合帧 → 窗口门控 → out_tx
     └─ 窗口越界 → park（只停自己）
     └─ next_offset < min_offset → resync（只清自己这一路）
```

**一句话**：源只负责"读 PTY + 入环 + 通告水印"；**每个订阅链路一个独立执行体（tokio task，项目 ADR「Async Everywhere」），持自己的位置指针在环上循环读取**；ack 背压从源侧移到各自的订阅任务里，成为该任务自己的发送节流。

### 4.2 环（ring）—— 唯一缓冲

沿用 `UnifiedOutputQueue`（已具备字节块区间、`min_offset/max_offset`、`snapshot_from/range`、`Bytes` 零拷贝切片），**新增**：

```rust
impl UnifiedOutputQueue {
    /// 从 from 起切一段可发送区间（零拷贝）：返回 (chunk_start, Bytes)，None = from 已越过 max_offset
    /// - from < min_offset：返回 Err(min_offset) —— 调用方据此进入 resync（不返回残缺数据）
    /// - 半块切头：from 落在块中间时用 Bytes::slice 裁头
    /// - 只返回**单块**（不跨块合并）：合帧由订阅者任务按连续性决定（见 4.5）
    pub fn read_at(&self, from: u64) -> Result<Option<(u64, Bytes)>, u64>;

    /// 驻留水印（订阅者等待/唤醒用；不取锁即可读的原子快照）
    pub fn watermarks(&self) -> (u64 /* min */, u64 /* max */);
}
```

要点：
- **零拷贝**：`read_at` 返回 `Bytes`（Arc 共享）。订阅者已发出的 `Bytes` 即使在环侧被淘汰，也因 Arc 保活而安全 —— 历史回放不再整段拷贝（消除现状 `snapshot_from().to_vec()`）。
- **只返回单块**：跨块合并交给订阅者（它才知道自己的合帧策略与窗口余量）；环不做"替消费者拼帧"的事（高内聚）。

### 4.3 订阅者任务（每订阅链路一个执行体）

```rust
/// 订阅者句柄（会话管理器持有；per client_id）
pub struct SubscriberHandle {
    pub session_id: String,
    pub client_id: String,
    /// 订阅起点（首订阅的 from_offset；仅供日志/观测，任务内部用局部 `next` 推进）
    start_offset: u64,
    /// 本次订阅的历史边界（订阅时刻 max_offset；HistoryEnd 依据，I7）
    snapshot_offset: u64,
    /// 私有 ack 水位（客户端 ack 帧推进；I6）
    acked_offset: AtomicU64,
    /// ack 唤醒（park 期间等它）
    ack_notify: Notify,
    /// 订阅者游标（**仅任务自己写**，句柄侧只读做观测）
    next_offset: AtomicU64,
    /// 双速模式（realtime/batch）
    mode: Arc<AtomicU8>,
    pub stats: SubscriberStats, // lag/park/truncate/resync 计数（观测）
}

/// 订阅者任务（`spawn_with_error_boundary("terminal_subscriber", …)`）
async fn subscriber_loop(ring: Arc<RwLock<UnifiedOutputQueue>>,
                         max_watch: watch::Receiver<u64>,
                         sub: Arc<SubscriberHandle>,
                         out_tx: mpsc::Sender<ForwardOutput>, // → WS actor（有界，容量小）
                         cfg: SubscriberCfg) {
    let mut next = sub.start_offset;           // 位置指针（订阅请求的 from_offset / min_offset）
    let mut snapshot = sub.snapshot_offset;    // 历史边界（首订阅时冻结）
    let mut history_done = false;
    loop {
        // ① 等唤醒：max_watch.changed()（新数据）或被 park 时的 ack_notify（见 ③）
        tokio::select! {
            r = max_watch.changed() => { if r.is_err() { break } }
            _ = sub.ack_notify.notified() => {}
        }
        let (min, max) = ring.read().await.watermarks();

        // ② 截断（I4）：所需字节已淘汰 → resync，仅本订阅者
        if next < min {
            sub.stats.truncated.fetch_add(1);
            emit_resync(&out_tx, min, max, sub.session_id()).await;   // §4.7
            next = min;
            snapshot = max;            // 重新锚定：resync 后 min..max 由客户端历史拼接覆盖
            history_done = true;       // resync 帧自带历史边界，无需再发 HistoryEnd
        }

        // ③ 推进（I3）：按模式合帧 → 窗口门控 → 发送
        loop {
            let (min, max) = ring.read().await.watermarks();
            // 历史边界（I7）必须在"追平即退出"之前判定：订阅时恰好无历史
            // （next == snapshot == max）时若先 break，客户端永远等不到
            // history_end → 历史拼接卡到超时（移动端表现为"进页面不出内容"）
            if !history_done && next >= snapshot {
                send(&out_tx, ForwardOutput::HistoryEnd { snapshot_offset: snapshot, min_offset: min, .. }).await;
                history_done = true;
            }
            if next >= max { break }                         // 追平，回去等唤醒
            // 窗口门控（I5）：越位即 park，只停自己
            let window = next.saturating_sub(sub.acked_offset.load(SeqCst));
            if window >= cfg.high_water {
                sub.stats.park_count.fetch_add(1);
                if park_until_ack(&sub, cfg).await.is_err() {
                    break;   // 僵尸订阅者：超时计数达上限 → 任务退出（§4.8）
                }
                continue;
            }
            let batch = pick_batch(&ring, next, max, &cfg, sub.mode.load(SeqCst)).await; // 合帧
            if send(&out_tx, batch.frame).await.is_err() { break }  // WS actor 已退出
            next = batch.end;
            sub.next_offset.store(next, SeqCst);
        }
    }
}
```

**要点**

- **合帧（`pick_batch`）从 `forward_loop` 迁入订阅者任务**：realtime = 时间窗（`flush_interval`）+ 字节窗（`max_buffer_size`）合并；batch = 满 `batch_bytes` 才发。合帧只合并**环上连续**的块（跨洞必须切帧——帧头区间与负载必须一致，既有契约不变）。
- **`out_tx` 是有界小容量通道**（如 8），但它**不再是背压依据**：真正的节流是窗口门控（③），`out_tx` 只承担"交给 WS actor"的短途交接。这样即使 WS actor 短暂慢，也不会把等待传导到源（源根本不参与）。
  - 取值约束：`out_tx 容量 × 单帧字节上限 ≪ 订阅者高水位`——否则"窗口已到位但帧还堵在通道里"，水位失去对实际在途字节的约束力。
- **历史段同样受窗口门控**：大历史（数 MB）不再一次性灌入客户端 WS 接收缓冲，而是按 ack 节奏分批（今天 `subscribe()` 是无界 `await send`，正是 WebKit WS 缓冲溢出丢消息的历史来源之一）。
- **订阅者空转成本 ≈ 0**：没有 per-subscriber 字节副本（今日有 `pending` 16384 事件 + `send_queue` 8192）；慢/僵订阅者的唯一代价是它自己的 WS 连接。

### 4.4 唤醒与等待

| 事件 | 机制 | 说明 |
| --- | --- | --- |
| 新数据入环 | 会话级 `tokio::sync::watch::Sender<u64>`（承载 `max_offset`） | 天然合并多次 push；`changed()` 无丢唤醒；`watch` 保存最新值，不需要 `Notify` 的"先查后等"配对 |
| 客户端 ack | 每订阅者 `Notify`（`ack_notify`） | park 期间等它；ack 是稀疏事件，`Notify` 足够（`notified()` 注册在前，无丢唤醒窗口） |
| park 兜底 | `timeout(park_poll, ack_notify.notified())` | 防僵尸订阅者永久 park；超时后重查窗口与 `min_offset` |

### 4.5 合帧策略（双速语义保持）

| 模式 | 触发条件 | 语义 |
| --- | --- | --- |
| realtime（默认，页面在前台消费） | `bytes ≥ terminal.max_buffer_size` **或** `距上次 flush ≥ terminal.flush_interval_ms` | 读即传，延迟 ≤ flush_interval |
| batch（客户端声明不看，仅入缓存） | `bytes ≥ terminal.batch_bytes`（无时间窗） | 减少空转帧数；数据留环，客户端重进时由历史拼接回补 |
| 零缓冲直通（桌面本地通道） | `terminal.merge_output=false` 或该通道 `flush_interval = ZERO` | 每块立即转发，模式无关（本地通道恒直通，保持现状） |

模式仍由客户端控制帧 `{"type":"mode",…}` 驱动（`Arc<AtomicU8>`）；模式翻转即时 flush 残留批次（沿用今日 forward 的语义）。

### 4.6 ack 语义（背压下移的核心）

| | 现状 | 目标 |
| --- | --- | --- |
| 归属 | 会话级共享 `unacked_bytes` + 共享 FIFO | **每订阅者私有** `acked_offset` |
| 作用 | 暂停/恢复 PTY 读取（影响所有订阅者） | 只解除/施加**本订阅者**的 park |
| 陈旧 ack | 弹共享 FIFO（可能误放别人的账） | 只做 `max()` 前移（I6），无副作用 |
| 丢失 ack | 源永久暂停（会话级无自愈路径） | 本订阅者 park 超时 → 僵尸回收（§4.8），其他订阅者无感 |

**零丢失不再是源头契约**：源不再暂停，环是唯一缓冲。代价是"极慢订阅者在环窗口耗尽后被截断（可自愈的重同步）"，收益是"任何单个消费者都不能冻结链路"。这是本次方案最重要的取舍（§11）。

### 4.7 截断与重同步（resync）

- 触发：`next_offset < min_offset`（订阅者长期 park / 客户端断网重连后游标过旧）。
- 动作：发送 `Resync { min_offset, snapshot_offset }` 控制帧（JSON，新类型）；随后从 `min_offset` 连续重播（当前驻留区间）。
- 客户端契约（与 TB v3 既有截断语义一致，**老端可忽略未知帧**，退化为既有 gap→重拼接自愈路径）：
  1. 清屏（xterm `clear()`，保留/重置游标策略与本端既有 `onTruncated` 路径一致）
  2. 以 `min_offset` 为起播锚点重建 `lastRenderedOffset`
  3. 提示用户"历史不完整"（移动端已有 `mobile.terminal.historyTruncated` 文案；桌面本地终端补一条 toast）
- `truncated_notified` 节流：同一订阅者的一次截断只提示一次（既有语义）。

### 4.8 僵尸订阅者回收

park 期间按 `terminal.subscriber_park_poll_ms`（默认 200ms）轮询窗口；**持续 `terminal.subscriber_zombie_timeout_ms`（默认 30s）窗口不降** → 判定客户端已死：

1. `warn!` 留痕（`session_id` / `client_id` / `lag_bytes` / `parked_ms`，结构化字段）
2. 结束订阅者任务、关闭该 WS 连接、`unsubscribe(client_id)`
3. 若 WS 仍可写，先发一条 `error {code: "lag_truncated"}`（尽力而为）

判据只看"窗口不降"（ack 不动），不看 `next_offset`（它可能被 park 停住）——避免把"客户端在渲染但很慢"误判为僵尸（阈值放大到 30s 量级即可区分）。

### 4.9 订阅/退订/替换时序

| 场景 | 目标行为 |
| --- | --- |
| 首次订阅 `from_offset` | `snapshot = max_offset`（订阅时刻）；`next = clamp(from_offset, min_offset..=max_offset)`；`from < min` → 立即 resync 路径（提示截断） |
| 重订阅（同 client_id） | 原子替换：先 abort 旧任务（流代数门控，防旧任务残留帧注入），再插入新句柄。**不再需要 `pending` 占位缓冲**——旧任务的"未发字节"在环里，新任务从客户端游标继续读；占位期竞态（历史段/pending 重叠）自然消失 |
| 退订 | 移除句柄 → 任务在下一次唤醒退出；环不动 |
| 会话结束 | watch 发送端 drop → 所有 `changed()` 返回 Err → 任务退出（无需广播 stop） |

> 替换路径去掉 `pending`/`active` 占位后，`drain_pending`/`PENDING_EVENT_CAP`/`SEND_BACKPRESSURE_TIMEOUT`/`revert_unacked` 四处机制整体删除（现状里它们是复杂度与 bug 的主要来源）。

---

## 5. 契约

### 5.1 帧与顺序

- 帧序：`[历史 Output × N] → HistoryEnd → [实时 Output]`（I7 不变）；重订阅/重连后新一轮订阅同样以 HistoryEnd 划界。
- 帧头区间 = 负载（既有 TB v3 契约，不变）；合帧只合连续块。
- 新增 `Resync` 控制帧（§4.7）；老客户端忽略未知控制帧即可（AGENTS §9「老端忽略未知字段」）。

### 5.2 边界与错误

| 情形 | 行为 |
| --- | --- |
| `from_offset > max_offset` | 收敛到 `max_offset`（防御；客户端游标不会超前） |
| `from_offset < min_offset` | 以 `min_offset` 起播 + 截断提示（等价于一次 resync） |
| 环淘汰最旧（正常） | 仅推进 `min_offset`，不动任何订阅者状态 |
| `out_tx` 关闭（WS actor 退出） | 订阅者任务退出（连接生命周期结束） |
| ring 读锁竞争 | `read_at`/`watermarks` 持读锁极短（无 await）；写锁只在 `push` |

### 5.3 观测字段（结构化日志，AGENTS §8）

- `subscriber stats (periodic)`：`session_id` / `client_id` / `next_offset` / `acked_offset` / `lag_bytes` / `parked_ms` / `park_count` / `truncated_count` / `frames_sent` / `bytes_sent`
- 状态迁移点：`park entered/exited`（debug，节流）、`subscriber truncation → resync`（warn）、`subscribe/unsubscribe`（info）、`zombie subscriber reclaimed`（warn）

---

## 6. 桌面端改造清单

| # | 文件 | 改动 |
| --- | --- | --- |
| 1 | `pty/pty_reader.rs` | 删除 `pause_check` / `should_pause` 轮询 / `BACKPRESSURE_POLL_INTERVAL`（**源不再背压**）；保留有序队列 + 生命周期事件；相关测试迁移到订阅者侧（§9） |
| 2 | `session/session_output.rs` | `on_output` 去阻塞：删逐订阅者 broadcast / `pending` / `unacked_*`；改为 `push` + `watch::send(max_offset)`；`should_pause` / `on_ack` / `SEND_BACKPRESSURE_TIMEOUT` / `PENDING_EVENT_CAP` / `revert_unacked` 整体下线；新增 `SubscriberHandle` / `subscriber_loop` / `read_at` / `watermarks` |
| 3 | `session/session_output.rs::subscribe` | 不再物化历史快照；只做「插入句柄 + 起任务」，`SubscribeResponse`（字节三件套）语义不变（值取订阅时刻水印） |
| 4 | `server/ws/terminal_ws/forward.rs` | `forward_loop` 的合帧逻辑抽为纯函数（`pick_batch`/`encode_output_frame_v3` 保留），由订阅者任务调用；`ForwardOutput` 语义不变 |
| 5 | `server/ws/terminal_ws.rs` | 订阅路径改为「起订阅者任务」；ack 处理改为 `handle.on_ack(offset)`（去掉 `GlobalOutputManager::ack` 的会话级记账）；新增 `Resync` 帧下发 |
| 6 | `system/config.rs` | 新增/下线配置（§8） |
| 7 | 两端文档 | `docs/knowledge/pty-output-pipeline.md`（链路图 + §1/§3 拆解）；AGENTS §0 无关，无需改 |

**删除清单（净减复杂度）**：`SubscriberState.pending/active/drain_pending`、`PENDING_EVENT_CAP`、`SEND_BACKPRESSURE_TIMEOUT`、`BACKPRESSURE_HIGH_BYTES/RESUME_BYTES`（源侧）、`UNACKED_FIFO_CAP`+`unacked_*`、`should_pause`、`spawn 并发 on_output 乱序`相关的 `output_serial`（若仍保持单消费者入环则保留，见风险 §11）。

---

## 7. 移动端显示链路优化（对齐 + 增量）

移动端已完成「两段订阅 + 两段背压」（`bedcode-mobile/docs/terminal-output-pipeline-optimization.md` §14）：段1（会话级，Rust ↔ 桌面端）、段2（页面级，前端 ↔ Rust 缓存），段2 水位 1MB/256KB。

**本次对齐与增量：**

| # | 项 | 状态 | 说明 |
| --- | --- | --- | --- |
| M1 | 段1 ack 锚点 = Rust 缓存游标 | ✅ 已落地 | 与桌面端"订阅者自有 ack"语义一致：移动端 Rust 就是桌面端的一个订阅者，它的 ack 表达"我能吸收多少" |
| M2 | 段2 订阅门控 + 水位 + 补推 | ✅ 已落地 | §14：未订阅只入缓存、越位停推、恢复按连续段补推 |
| M3 | 段1 截断 → resync 显式化 | 🔜 待办 | 桌面端 §4.7 新增 `Resync` 帧后，移动端链路消费它（比今天"gap→forceReplay→getHistory→minOffset>x→清屏"的间接判定更直接、少一次往返）；保留既有间接路径作为兜底 |
| M4 | 段1 ack 阈值重定 | 🔜 待办 | 桌面端改为 per-subscriber 窗口后，移动端 64KB/250ms 的 ack 节流可放宽（如 256KB），减少 ack 帧数；上界由**移动端 WS 接收缓冲**决定（沿用"水位必须低于客户端 WS 缓冲"的既有约束） |
| M5 | 段2 合帧（减少 IPC 次数） | 🔜 待办 | 未渲染窗口内按字节窗（如 64KB）合并连续帧为单个 `terminal-frame` 事件，降低 WebView 事件分发 + base64 解码次数；与桌面 `pick_batch` 同构 |
| M6 | lag 预算表统一 | 🔜 待办 | 定义「桌面环 50MB ⊃ 移动 Rust 缓存 16MB ⊃ 前端缓冲 8MB」的容量关系与告警阈值，避免上游缓存小于下游导致的无谓截断 |
| M7 | 段2 水位自适应 | 🅿️ 延后 | 按 ack 往返 + 渲染滞后估算动态调（替代固定 1MB/256KB）；先收集真机数据再决定是否值得 |

**M3/M4 的硬约束**：桌面端协议新增帧必须两端同步部署（AGENTS §9）；只增不改，老移动端忽略 `Resync` 帧后退化为既有自愈路径（不破坏）。

---

## 8. 配置与观测

| 配置键 | 默认 | 说明 |
| --- | --- | --- |
| `channels.global_queue_max_bytes` | 50MB（不变） | 环驻留上限（唯一缓冲） |
| `channels.global_queue_max_chunks` | 65536（不变） | 条目上限（抗碎片） |
| `terminal.subscriber_high_water_bytes` | **128KB**（新；定稿） | 订阅者窗口高位水：`next - acked ≥ 该值` → park |
| `terminal.subscriber_low_water_bytes` | **64KB**（新；定稿） | 低位水（滞回下沿）：降到该值以下解除 park |
| `terminal.subscriber_park_poll_ms` | 200（新） | park 兜底轮询（配合 `ack_notify`） |
| `terminal.subscriber_zombie_timeout_ms` | 30000（新） | 窗口不降持续该时长 → 僵尸回收 |
| `terminal.batch_bytes` / `terminal.flush_interval_ms` / `terminal.max_buffer_size` / `terminal.merge_output` | 不变 | 双速模式与合帧参数（归属迁到订阅者任务，语义不变） |
| `terminal.read_buffer_size` | 不变 | PTY 读缓冲 |

**水位定稿依据（ticket 06/08）**：客户端 ack 阈值 64KB（两端一致），解锁关系
`ack(64) ≤ low(64) < high(128)` 且 `high − ack = 64 ≤ low = 64`（一次 ack 即解锁）；
另 `high(128KB) ≪ 环(50MB)`。故 §8 原拟的 64KB/8KB **调整为 128KB/64KB**：64/8 组合下
ack 阈值等于高位水、且一次 ack 释放量与低位水不匹配（12KB 滞回区间过窄，实机易出现
「驻留—解锁」抖动）。校验逻辑落在 `TerminalConfig::subscriber_budget_violation`
（装配订阅者时告警 + 默认配置单测锁定），预算表见
`docs/knowledge/pty-output-pipeline.md` §1.6。

下线：`BACKPRESSURE_HIGH_BYTES` / `BACKPRESSURE_RESUME_BYTES` / `SEND_BACKPRESSURE_TIMEOUT` / `PENDING_EVENT_CAP` / `UNACKED_FIFO_CAP`（源码常量，未暴露配置的不占键）。

---

## 9. 测试计划

**环（`UnifiedOutputQueue`）**：既有（push/淘汰/slice/快照/区间）全保留；新增 `read_at`（半块裁头 / `from < min` 返回 Err / `from ≥ max` 返回 None）、`watermarks` 一致性。

**订阅者任务（核心，须能脱离 WS 单测：注入 mock 环 + 内存 `out_tx`）**

| 用例 | 断言 |
| --- | --- |
| 顺序零丢失 | N 次 push → 收到的字节序列与产出完全一致（I1/I3） |
| 窗口门控 | 窗口达高水位 → 不再发送（不推进 `next_offset`）；ack 后从低位水恢复并补齐 |
| **park 不影响源** | A park 期间持续 push → `max_offset` 单调增长、B 收齐（I2/I5，本次回归护栏） |
| 多订阅者隔离 | A 永不 ack（最终被僵尸回收）→ B 全程正常；A 的 resync 不产生 B 的帧 |
| 截断局部化 | 令 `min_offset` 越过 A 的 `next_offset` → A 收到 `Resync` 且从 `min_offset` 重播；B 无感（I4） |
| 历史边界 | 首订阅 → 历史帧严格早于 `HistoryEnd`，实时帧严格晚于（I7） |
| 空历史边界 | 订阅时 `next == snapshot == max`（无历史可发）→ **仍必须发 `HistoryEnd`**（否则客户端等不到边界，历史拼接卡到超时） |
| 零拷贝保活 | `read_at` 返回的 `Bytes` 在环淘汰该块后仍可安全编码发送 |
| 僵尸回收 | 窗口不降超时 → 任务终止 + 连接关闭 + warn 留痕 |
| 模式切换 | real→batch / batch→real 即时 flush 残留批次（语义不退化） |

**回归护栏（必须新增）**

- `pty_reader` 不再持有任何 `should_pause` 引用（编译期即可保证：API 删除）。
- **单订阅者慢不冻结链路**：构造 A 慢（不 ack）+ B 正常，断言 PTY 读线程累计读字节持续增长且无 2s 级停顿（对照现状行为）。
- 历史回放不再有整段 `to_vec`（以 `bytes_sent` 与环驻留对账；可选：分配计数断言）。

**端到端（双端）**：桌面 `cargo test` + 移动端 `pnpm run test:run` 全绿；真机烟测：移动端页面关闭期间输出风暴 → 桌面环持续增长且桌面本地终端不受影响 → 重进页面历史拼接完整。

---

## 10. 分期实施（tickets 已发布）

**已拆分为 8 张 ticket，落盘于 `issues/`（每票一文件，`Status: ready-for-agent`）。**
形态是**深改的 expand → migrate → contract**：单笔改动无法同时保证全绿，因此先「新旧并存」（扩张），再按路由分批迁移，最后删旧机制。

| 序 | Ticket | 形态 | 依赖 | 状态 |
| --- | --- | --- | --- | --- |
| 01 | [拉取模型订阅者（expand）](issues/01-pull-subscriber-task.md) | expand（含前置改造：环读助手 + 合帧纯函数抽取） | — | ✅ 已落地 |
| 02 | [移动端 WS 订阅切到拉取模型](issues/02-migrate-mobile-ws-route.md) | migrate 批 1（tracer bullet） | 01 | ✅ 已落地（新路由 + 旧 Message 路由同批迁移） |
| 03 | [桌面本地终端订阅切到拉取模型](issues/03-migrate-desktop-local-route.md) | migrate 批 2 | 01 | ✅ 已落地（Tauri Channel 通道；`terminal_channel_ack` 增 clientId） |
| 04 | [源侧去背压 + 旧机制下线](issues/04-source-backpressure-removal.md) | contract | 02, 03 | ✅ 已落地（净删 ~500 行） |
| 05 | [Resync 协议（两端）](issues/05-resync-protocol.md) | 新增能力 | 01, 02 | ✅ 已落地（两端消费 + 兜底路径保留） |
| 06 | [三段容量/水位预算对齐](issues/06-capacity-watermark-budget.md) | 调参/约束 | 02 | ✅ 已落地（128KB/64KB 定稿 + 校验函数 + 单测） |
| 07 | [移动端段2 合帧](issues/07-mobile-seg2-coalescing.md) | 移动端本地 | — | ⏳ 未实施（见下方说明） |
| 08 | [全链路压测与收口](issues/08-load-test-and-dod.md) | integrate & verify | 01–07 | ◐ 门禁与文档完成；**真机压测待用户执行**（§13） |

**07 未实施说明**：桌面端已在环上按 `flush_interval(30ms)/max_buffer_size(64KB)` 合帧
（订阅者执行体），移动端收到的已是 ≤64KB 的批次；段2 再做一次合帧的边际收益仅限
「桌面在 30ms 窗口内发出多帧」的低频小块场景，而引入「持帧等待」需要新增时间兜底
（否则又是「等不到触发条件」的滞留，与移动端文档 §13 的教训同型）。本次先不做，
待真机压测（§13 的第 5 项）给出段2 事件数与解码耗时占比后再决定是否值得。

**frontier（当前可立即开工）**：07（如真机数据显示段2 事件分发仍是瓶颈）。

---

## 11. 风险与取舍

| 风险 | 影响 | 处置 |
| --- | --- | --- |
| **有损化**（源不再暂停 → 极慢订阅者被截断） | 该订阅者需清屏重播，可能丢历史可见性 | 环 50MB 足以吸收正常抖动；截断有明确信号 + 自动重锚 + 用户提示；单订阅者损害不扩散（I4/I5）。这是本方案的核心取舍：**以"单订阅者可自愈的截断"换"链路整体不可冻结"** |
| 单消费者入环任务仍可能成为瓶颈 | `output_tx` 16384 满 → 读线程 `blocking_send` 阻塞 | 目标态下 `on_output` 无 await 等待（只 push + watch 通告），队列不会堆积；若真出现，把入环直接放到读线程（单写者）作为后备方案 |
| `watch` 唤醒放大 | 每次 push 一次 `send`（合并后仍可能高频） | `watch` 只存最新值，接收端 `changed()` 合并；风暴期订阅者按自己的批次节奏消费，不逐条唤醒 |
| `output_serial` 串行锁是否还需要 | 若入环仍走单消费者任务则不需要；若多任务并发入环必须保留 | 保守做法：保留单消费者（现状），串行锁保留为防御性（可观测是否命中） |
| 客户端兼容 | 新 `Resync` 帧 / ack 语义变化 | 只增不改 + 老端忽略未知帧；ack 语义变化是**桌面内部**行为（客户端仍只发 offset），无需客户端改造 |
| 水位取值 | 64KB/8KB 沿用自"WS 缓冲必须大于水位"的约束 | 移动端 ack 阈值与订阅者水位必须成对调整（M4 + §8），禁单独抬高订阅者水位 |

---

## 12. 备选方案与否决理由

| 方案 | 否决理由 |
| --- | --- |
| 保留源侧背压，但改按"最慢订阅者"记账 | 语义正确但仍是耦合：最慢端一卡就冻结全链路（含桌面本地终端），与本次目标相反 |
| 每订阅者独立队列（复制字节） | 内存 × 订阅者数（现状 `send_queue` + `pending` 即此类）；环 + 游标已能表达同样的隔离性 |
| 引入 `tokio::sync::broadcast` 做分发 | 二次缓冲（环已有一份），且 `Lagged` 语义与字节游标语义重叠；跨订阅者复制的字节仍然存在 |
| 订阅者直接读 PTY fd（多读者） | PTY 只允许单读者；且历史回放/游标语义会散落到各订阅者 |
| 源侧保留"快满即暂停"作为兜底 | 与"源零等待"矛盾；兜底一旦触发就是全链路冻结（正是要消除的失效模式），如确需限流应在**环容量**上做（增大/告警）而不是暂停源 |

---

## 13. 落地验收（Definition of Done）

- [x] I1–I7 每条都有对应单测（§9 表全覆盖）——见
      `server/ws/terminal_ws/subscriber.rs` 的 14 个用例（顺序零丢失 / 历史边界 /
      空历史边界 / 窗口门控与 ack 补发 / 驻留不影响源与他人 / 截断重同步 / 零拷贝保活 /
      僵尸回收 / 模式切换 / 订阅即截断 / 水位预算）
- [x] `pty_reader` 内不存在任何背压/暂停逻辑；`should_pause`/`unacked_*`/`pending`/
      `SEND_BACKPRESSURE_TIMEOUT` 全部删除（编译期即保证：相关 API 已不存在）
- [x] 多订阅者隔离回归：一个慢/僵尸订阅者不改变其他订阅者与源的行为
      （`park_does_not_stall_source_or_other_subscriber` / `zombie_subscriber_is_reclaimed_after_window_stalls`
      / `ack_subscriber_is_private_and_monotonic`）
- [x] 桌面端 `cargo test` 全绿（790 lib + 集成）；移动端 `cargo test` 全绿（323 lib + 集成）；
      两端 `pnpm run test:run` 全绿（桌面分块执行：单次全量在本机内存压力下属既有环境问题，
      逐块/逐文件均绿）；根目录 `pnpm exec eslint .` 0 error（124 warning，均非本次新增）
- [x] `docs/knowledge/pty-output-pipeline.md` 链路图与组件表同步（含「三段式 + 每订阅者游标
      + 背压归属」+ §1.5 resync 协议 + §1.6 三段容量/水位预算表）
- [ ] **真机烟测（待用户执行）**：输出风暴 + 移动端切后台/断网/回前台，桌面本地终端全程不卡顿，
      移动端重进页面历史完整；并观察日志中 `subscriber park/truncation` 与 `terminal resync`
      次数（预期：正常网络下 resync = 0；慢订阅者只影响自己）
- [ ] 移动端段2 合帧（ticket 07）待真机数据决定是否实施
