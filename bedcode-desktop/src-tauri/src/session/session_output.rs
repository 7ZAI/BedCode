//! Session Output
//!
//! PTY 输出相关的组件：统一输出队列、会话输出管理、全局输出管理
//!
//! TB v3 字节连续语义（`.scratch/pty-byte-history/spec.md`）：
//! 连续性以会话内累计字节偏移（start_offset/end_offset）表达，取代事件 index；
//! 帧头"第几个事件" → "累计第几个字节"；游标/缺口/去重/ack/快照收敛到
//! `[start_offset, end_offset)` 区间运算。

use crate::system::config::AppConfig;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{watch, Notify, RwLock};

// ==================== Unified Output Queue ====================

/// 输出事件（= 历史保存/回放的最小字节块，每次 PTY read 一块）
///
/// `data` 存储原始字节数据，在发送到 WebSocket 时才编码为 TB v3 二进制帧
///（见 server/ws/terminal_ws/forward.rs）
///
/// `start_offset` 为会话内累计字节偏移（写入路径 SessionOutputManager::on_output
/// 在串行临界区内分配 = 队列 max_offset），事件字节区间为
/// `[start_offset, start_offset + data.len())`。连续性不变量：
/// 同流事件区间按序铺满字节空间，无重叠无空洞
#[derive(Debug, Clone)]
pub struct OutputEvent {
    pub session_id: String,
    pub data: Vec<u8>,
    pub start_offset: u64,
    pub timestamp: i64,
    pub is_waiting: bool,
}

impl OutputEvent {
    pub fn new(session_id: String, data: Vec<u8>, start_offset: u64, timestamp: i64, is_waiting: bool) -> Self {
        Self {
            session_id,
            data,
            start_offset,
            timestamp,
            is_waiting,
        }
    }

    /// 事件字节区间末端（start_offset + len）
    pub fn end_offset(&self) -> u64 {
        self.start_offset + self.data.len() as u64
    }
}

/// 历史保存字节块（队列元素）
///
/// `Bytes` Arc 共享：订阅者转发帧 / HTTP 历史截取零拷贝复用同一底层缓冲；
/// `slice()` 支持半块切（按 offset 截取快照起播 / 精调淘汰的基础）
#[derive(Debug, Clone)]
pub struct OutputChunk {
    /// 块在会话输出流中的起始位置（累计字节数）
    pub start_offset: u64,
    /// 块字节（Arc 共享，slice 半块切）
    pub bytes: Bytes,
    /// 保留尾部事件的 waiting 语义（等待提示）
    pub end_is_waiting: bool,
}

impl OutputChunk {
    pub fn end_offset(&self) -> u64 {
        self.start_offset + self.bytes.len() as u64
    }
}

/// 环上按游标切出的单块视图片段（拉取模型）
///
/// `bytes` 为 `Bytes::slice` 视图（Arc 共享零拷贝）：即使该块随后被环淘汰，
/// 已切出的视图仍安全可编码——这是历史回放不再整段 `to_vec()` 物化的基础。
/// `end_is_waiting` 继承源块尾部的 waiting 语义（帧 flags bit0 契约）。
#[derive(Debug, Clone)]
pub struct RingSlice {
    /// 切出区间起点（= 调用方传入的 `from`，落在块内时为半块裁头位置）
    pub start_offset: u64,
    /// 单块字节视图（不跨块合并）
    pub bytes: Bytes,
    /// 块尾 waiting 语义（编码进 TB v3 帧 flags）
    pub end_is_waiting: bool,
}

impl RingSlice {
    pub fn end_offset(&self) -> u64 {
        self.start_offset + self.bytes.len() as u64
    }
}

/// 一次游标拉取的返回（票 04 `output-ring-fetch` 原语数据面）
///
/// 语义与 [`crate::pty::pty_ring::PtyRingFetch`] 同形：会话输出环与插件私有
/// PTY 环共享同一字节偏移模型（`[实际起点, next_offset)` 区间，续拉不重复）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingFetchOutput {
    /// `[实际起点, next_offset)` 区间的原始字节（未解码，可能是非 UTF-8）
    pub data: Vec<u8>,
    /// 下一次拉取应传的游标
    pub next_offset: u64,
    /// 传入游标落后于环驻留起点（中间字节已被淘汰）→ 调用方按 resync 重建上下文
    pub truncated: bool,
}

/// 统一输出队列（字节块环形队列，TB v3）
///
/// 双重容量限制（均为字节/条目级软上限）：
/// - `max_total_bytes`: 最大总驻留字节（默认 50MB，配置化）
/// - `max_chunks`: 防御性条目上限（默认 65536，抗极小块风暴；覆盖旧 capacity 语义）
/// 任一限制超出时丢弃最旧块，min_offset 推进。
///
/// 偏移语义：`max_offset` = 产出端游标（会话内累计产字节），push 分配 start_offset
/// 并推进；块区间按序铺满字节空间：`chunks[i+1].start_offset == chunks[i].end_offset()`。
/// 队列层不依赖 offset 来源，仅保证「push 序 == 区间单调序，段内无重无缺」——
/// 历史回放可 from_offset 起播（chunk 跳过 + 半块 slice）
pub struct UnifiedOutputQueue {
    chunks: std::collections::VecDeque<OutputChunk>,
    /// 驻留最旧字节位置（front chunk 起点；空队列 = max_offset）
    min_offset: u64,
    /// 产出端游标（= 全量累计字节）
    max_offset: u64,
    /// 驻留字节（淘汰基准）
    total_bytes: u64,
    /// 字节上限（软上限；多订阅者 Arc 共享时实际驻留可能短暂超限——设计预期）
    max_total_bytes: u64,
    /// 防御性条目上限（抗极小块风暴）
    max_chunks: usize,
    /// 累计产出事件数（调试/审计字段，回放不依赖）
    total_produced: AtomicU64,
}

impl UnifiedOutputQueue {
    /// 创建指定字节上限的队列（条目上限取 config max_chunks）
    pub fn with_max_bytes(max_total_bytes: u64) -> Self {
        let config = AppConfig::global();
        Self::with_limits(max_total_bytes, config.channels.global_queue_max_chunks)
    }

    /// 创建指定字节/条目双上限的队列
    pub fn with_limits(max_total_bytes: u64, max_chunks: usize) -> Self {
        Self {
            chunks: std::collections::VecDeque::with_capacity(max_chunks.min(4096)),
            min_offset: 0,
            max_offset: 0,
            total_bytes: 0,
            max_total_bytes,
            max_chunks,
            total_produced: AtomicU64::new(0),
        }
    }

    /// 驻留最旧字节位置（front chunk 起点；空队列 = max_offset）
    pub fn min_offset(&self) -> u64 {
        self.min_offset
    }

    /// 产出端游标（全量累计字节）
    pub fn max_offset(&self) -> u64 {
        self.max_offset
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// 驻留历史总字节（回放/响应元数据）
    pub fn history_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// 推入新事件（字节块），返回完整事件（供调用方广播给订阅者）
    ///
    /// start_offset 由 on_output 在串行临界区内分配（max_offset）；此处防御性
    /// 兜底重赋。双重容量检查：字节数与块条目数任一超限时淘汰最旧块。
    pub fn push(&mut self, mut event: OutputEvent) -> OutputEvent {
        // 防御性兜底：生产路径 on_output 已分配 start_offset = max_offset
        event.start_offset = self.max_offset;
        let len = event.data.len() as u64;
        self.total_bytes += len;

        // 字节/条目上限淘汰最旧块（均摊 O(1)）；min_offset 推进到新队首
        while (self.total_bytes > self.max_total_bytes || self.chunks.len() >= self.max_chunks)
            && !self.chunks.is_empty()
        {
            if let Some(front) = self.chunks.pop_front() {
                self.total_bytes -= front.bytes.len() as u64;
            }
        }
        // min_offset = 驻留最旧字节位置；队列被清空（超限单块）时回退为该块起点
        match self.chunks.front() {
            Some(front) => self.min_offset = front.start_offset,
            None => self.min_offset = event.start_offset,
        }

        self.chunks.push_back(OutputChunk {
            start_offset: event.start_offset,
            bytes: Bytes::copy_from_slice(&event.data),
            end_is_waiting: event.is_waiting,
        });
        self.max_offset += len;
        self.total_produced.fetch_add(1, Ordering::SeqCst);
        event
    }

    /// 截取 `[from, to)` 字节区间（HTTP 一次性历史）；越界端自动收敛到驻留范围，
    /// from >= to 或无可返回字节时返回空
    pub fn range(&self, from: u64, to: u64) -> Vec<u8> {
        if to <= from {
            return Vec::new();
        }
        let mut out = Vec::new();
        for chunk in &self.chunks {
            let chunk_end = chunk.end_offset();
            if chunk_end <= from {
                continue;
            }
            if chunk.start_offset >= to {
                break;
            }
            let lo = from.saturating_sub(chunk.start_offset) as usize;
            let hi = to.saturating_sub(chunk.start_offset) as usize;
            let hi = hi.min(chunk.bytes.len());
            if lo < hi {
                out.extend_from_slice(&chunk.bytes[lo..hi]);
            }
        }
        out
    }

    /// 按游标拉取至多 `max_bytes` 字节（跨块合并，对消费者隐藏块边界；票 04
    /// `output-ring-fetch` 原语的数据面，语义与 `pty_ring.rs::PtyRing::fetch` 一致）
    ///
    /// 游标钳位规则（全部路径都返回可用的 `next_offset`，调用方无需判错）：
    /// - 落后于 `min_offset`（数据已淘汰）→ 从 `min_offset` 起返回，`truncated = true`
    /// - 超前于 `max_offset`（非法/未来游标）→ 按「已追平」处理，回带 `max_offset` 自愈
    pub fn fetch(&self, from_offset: u64, max_bytes: usize) -> RingFetchOutput {
        let truncated = from_offset < self.min_offset;
        let start = from_offset.max(self.min_offset).min(self.max_offset);
        let end = self.max_offset.min(start.saturating_add(max_bytes as u64));
        if end <= start {
            return RingFetchOutput {
                data: Vec::new(),
                next_offset: start,
                truncated,
            };
        }
        RingFetchOutput {
            data: self.range(start, end),
            next_offset: end,
            truncated,
        }
    }

    // ==================== 拉取模型读取（订阅者游标推进） ====================

    /// 驻留水印 `(min_offset, max_offset)`——订阅者等待/唤醒用的原子快照
    /// （持读锁极短，无 await）
    pub fn watermarks(&self) -> (u64, u64) {
        (self.min_offset, self.max_offset)
    }

    /// 从 `from` 起切出**单块**可发送区间（零拷贝，不跨块合并）
    ///
    /// 返回值语义（背压/截断契约的单一事实源）：
    /// - `Err(min_offset)`：`from < min_offset`——所需字节已被环淘汰，**不得**
    ///   返回残缺数据；调用方据此进入重同步（清屏 + 从 min_offset 重锚）
    /// - `Ok(None)`：`from >= max_offset`——已追平产出端，无可发送字节
    /// - `Ok(Some(RingSlice))`：自 `from` 起的一块；`from` 落在块内时用
    ///   `Bytes::slice` 裁头（块视图，非拷贝）
    ///
    /// 跨块合帧由调用方（订阅者执行体）决定：只有它知道自己的合帧策略与
    /// 窗口余量；环不替消费者拼帧（高内聚）。
    pub fn read_at(&self, from: u64) -> Result<Option<RingSlice>, u64> {
        if from < self.min_offset {
            return Err(self.min_offset);
        }
        if from >= self.max_offset {
            return Ok(None);
        }
        // 块区间按序铺满 [min_offset, max_offset)：首个 end_offset > from 的块
        // 即包含 from 的块（二分定位，O(log n)，避免逐块线性扫描的 O(n²)）
        let idx = self.chunks.partition_point(|c| c.end_offset() <= from);
        match self.chunks.get(idx) {
            Some(chunk) if chunk.start_offset <= from => {
                let cut = (from - chunk.start_offset) as usize;
                let bytes = if cut == 0 {
                    chunk.bytes.clone()
                } else {
                    chunk.bytes.slice(cut..)
                };
                Ok(Some(RingSlice {
                    start_offset: from,
                    bytes,
                    end_is_waiting: chunk.end_is_waiting,
                }))
            }
            // 防御：区间永不空洞；异常时按「已淘汰」处理（调用方重同步自愈）
            _ => Err(self.min_offset),
        }
    }
}

impl Default for UnifiedOutputQueue {
    fn default() -> Self {
        let config = AppConfig::global();
        Self::with_limits(
            config.channels.global_queue_max_bytes,
            config.channels.global_queue_max_chunks,
        )
    }
}

// ==================== Session Output Manager ====================

// ==================== 链路调试节流（终端字节对账） ====================

/// 产出统计打点间隔（有新产出才打；不打逐帧日志，防输出风暴期日志淹没链路）
const PRODUCE_STATS_INTERVAL_MS: u64 = 5000;

/// 当前 Unix 毫秒（日志节流打点用）
fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 订阅响应（TB v3 字节三件套）
#[derive(Debug, Clone)]
pub struct SubscribeResponse {
    /// 队列中最早存续字节位置（环形淘汰后推进；客户端游标 < min_offset → 截断）
    pub min_offset: u64,
    /// 订阅时刻的累计字节数（= 当时队列 max_offset，历史边界元数据）
    pub snapshot_offset: u64,
    /// 驻留历史总字节数
    pub history_bytes: u64,
}

// ==================== 拉取模型订阅者（per-subscriber 游标 + 私有 ack） ====================

/// 订阅者传播模式（双速）：realtime = 读即传（时间窗 + 字节窗合并）；
/// batch = 累计满 batch_bytes 才转发一帧（无时间窗）
///
/// 常量定义在 session 层（订阅者状态的一部分），`server::ws::terminal_ws::forward`
/// 重导出以保持既有引用点；避免 session 反向依赖 server。
pub const MODE_REALTIME: u8 = 0;
pub const MODE_BATCH: u8 = 1;

/// 订阅者观测统计（结构化日志对账用，均为单调累加）
#[derive(Debug, Default)]
pub struct SubscriberStats {
    /// 已发出帧数
    pub frames_sent: AtomicU64,
    /// 已发出负载字节数
    pub bytes_sent: AtomicU64,
    /// 进入驻留（窗口越界等待 ack）次数
    pub park_count: AtomicU64,
    /// 驻留累计时长（毫秒）
    pub parked_ms: AtomicU64,
    /// 截断（游标早于驻留起点 → 重同步）次数
    pub truncated_count: AtomicU64,
    /// 首次订阅起始点早于驻留起点（订阅即截断）标记
    pub truncated_on_subscribe: AtomicBool,
}

/// 拉取模型订阅者句柄（会话管理器持有，per client_id）
///
/// 「位置指针」是订阅者任务的私有状态（局部 `next`），句柄侧的
/// `next_offset` 仅为观测镜像（供统计/日志对账，不参与判定）。
/// `acked_offset` 为**该订阅者私有**的 ack 水位（I6）：单调前移，
/// 只解除/施加本订阅者的窗口驻留，不做任何会话级共享记账。
pub struct SubscriberHandle {
    pub session_id: String,
    pub client_id: String,
    /// 订阅起点（首订阅的 from_offset；仅供日志/观测）
    pub start_offset: u64,
    /// 本次订阅的历史边界（订阅时刻 max_offset；HistoryEnd 依据，I7）
    pub snapshot_offset: u64,
    /// 私有 ack 水位（客户端 ack 帧推进；I6）
    acked_offset: AtomicU64,
    /// 订阅者游标观测镜像（仅任务自己写，句柄侧只读）
    next_offset: AtomicU64,
    /// ack 唤醒（park 期间等它；丢失唤醒由 park 轮询兜底）
    ack_notify: Notify,
    /// 已退订/已被替换（句柄已从管理器移除）：任务在下一个检查点退出
    ///
    /// 不能只靠 `ack_notify` 唤醒——任务可能正阻塞在 `watch::changed()` 上等待
    /// 新数据，`notify_waiters` 唤不醒它（退订后仍会继续转发），故用显式标志
    retired: AtomicBool,
    /// 双速模式（realtime/batch）
    pub mode: Arc<std::sync::atomic::AtomicU8>,
    /// 观测统计
    pub stats: SubscriberStats,
}

impl SubscriberHandle {
    pub fn new(
        session_id: String,
        client_id: String,
        start_offset: u64,
        snapshot_offset: u64,
        mode: Arc<std::sync::atomic::AtomicU8>,
    ) -> Self {
        Self {
            session_id,
            client_id,
            start_offset,
            snapshot_offset,
            acked_offset: AtomicU64::new(start_offset),
            next_offset: AtomicU64::new(start_offset),
            ack_notify: Notify::new(),
            retired: AtomicBool::new(false),
            mode,
            stats: SubscriberStats::default(),
        }
    }

    /// 标记退订/被替换（管理器移除句柄时调用）：唤醒 + 任务在检查点退出
    pub fn retire(&self) {
        self.retired.store(true, Ordering::SeqCst);
        self.ack_notify.notify_waiters();
    }

    pub fn is_retired(&self) -> bool {
        self.retired.load(Ordering::SeqCst)
    }

    pub fn acked_offset(&self) -> u64 {
        self.acked_offset.load(Ordering::SeqCst)
    }

    pub fn next_offset(&self) -> u64 {
        self.next_offset.load(Ordering::SeqCst)
    }

    /// 任务内推进游标（同时刷新观测镜像）
    pub fn set_next_offset(&self, next: u64) {
        self.next_offset.store(next, Ordering::SeqCst);
    }

    /// 客户端 ack：水位只前进（I6，陈旧/乱序 ack 天然忽略）并唤醒驻留
    pub fn on_ack(&self, acked_offset: u64) {
        let prev = self.acked_offset.fetch_max(acked_offset, Ordering::SeqCst);
        if acked_offset > prev {
            self.ack_notify.notify_waiters();
        }
    }

    /// 取消订阅/连接结束时的唤醒：让驻留中的任务立即重查退出条件
    pub fn wake(&self) {
        self.ack_notify.notify_waiters();
    }

    /// 等一次 ack 唤醒（配 `tokio::time::timeout` 做驻留兜底轮询）
    pub async fn wait_ack(&self) {
        self.ack_notify.notified().await;
    }

    /// 当前订阅者窗口 `next - acked`（已发未确认字节数）
    pub fn window(&self) -> u64 {
        self.next_offset().saturating_sub(self.acked_offset())
    }
}

/// 订阅建立结果（拉取模型）：句柄 + 响应元数据 + 产出端唤醒接收器
pub struct PullSubscriber {
    pub handle: Arc<SubscriberHandle>,
    pub response: SubscribeResponse,
    /// 产出端唤醒（承载会话 max_offset；`changed()` 无丢唤醒）
    pub max_watch: watch::Receiver<u64>,
}

/// 单个 PTY 会话的输出管理：输出环 + 每订阅者游标句柄
///
/// **源零等待（I2）**：`on_output` 只做「分配 offset + 入环 + 通告水印」，
/// 不向任何订阅者投递、不等待任何订阅者；环满淘汰最旧。投递、合帧与背压
/// 节流全部归属各订阅者执行体（`server/ws/terminal_ws/subscriber.rs`）。
pub struct SessionOutputManager {
    session_id: String,
    output_queue: Arc<RwLock<UnifiedOutputQueue>>,
    /// 拉取模型订阅者表（每订阅链路一个位置指针 + 私有 ack 水位）
    pull_subscribers: RwLock<HashMap<String, Arc<SubscriberHandle>>>,
    /// 产出端唤醒发送端（承载 `max_offset`）：源只入环 + 通告水印，
    /// 不做任何投递；订阅者各自 `changed()` 后从环上拉取
    max_watch_tx: watch::Sender<u64>,
    /// 输出入队串行锁（防御性）：串行化 `on_output` 的「offset 分配 + 入环」
    /// 临界区。入环仍走单消费者任务（见 `pty_reader`），理论上无并发；保留该锁
    /// 是为了在任何未来并发入环路径下仍保证「offset 分配序 = 入环序」
    output_serial: tokio::sync::Mutex<()>,
    // ==================== 链路调试统计（终端字节对账） ====================
    /// 上次产出统计打点时刻（PRODUCE_STATS_INTERVAL_MS 节流）
    last_produce_stats_ms: AtomicU64,
    /// 累计产出事件数（对账用）
    produced_events: AtomicU64,
}

impl SessionOutputManager {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            output_queue: Arc::new(RwLock::new(UnifiedOutputQueue::default())),
            pull_subscribers: RwLock::new(HashMap::new()),
            max_watch_tx: watch::channel(0).0,
            output_serial: tokio::sync::Mutex::new(()),
            last_produce_stats_ms: AtomicU64::new(0),
            produced_events: AtomicU64::new(0),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 处理新输出（源路径：零等待、恒定 O(1) 均摊）
    ///
    /// 字节偏移按会话连续分配（= 环 `max_offset`），入环（满则淘汰最旧），
    /// 然后 `watch::send_replace` 通告水印——不向订阅者投递任何数据。
    pub async fn on_output(&self, mut event: OutputEvent) {
        let _guard = self.output_serial.lock().await;

        let (queue_min, queue_max, queue_bytes) = {
            let mut queue = self.output_queue.write().await;
            event.start_offset = queue.max_offset();
            queue.push(event);
            (queue.min_offset(), queue.max_offset(), queue.history_bytes())
        };

        // 产出端唤醒：只通告水印（watch 保存最新值，天然合并多次 push），
        // 不向任何订阅者投递——投递与节流是各订阅者自己的事（背压下移）
        self.max_watch_tx.send_replace(queue_max);

        // 链路调试（终端字节对账）：产出统计周期打点（5s 且有新产出才打）。
        // produced_bytes 与移动端 terminal_link 收帧统计对齐——比对两端累计字节
        // 可定位丢字节环节；不打逐帧日志，防输出风暴期日志风暴
        self.produced_events.fetch_add(1, Ordering::SeqCst);
        let now_ms = system_now_ms();
        let last_stats = self.last_produce_stats_ms.load(Ordering::SeqCst);
        if now_ms.saturating_sub(last_stats) >= PRODUCE_STATS_INTERVAL_MS
            && self
                .last_produce_stats_ms
                .compare_exchange(last_stats, now_ms, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        {
            let subscriber_count = self.pull_subscribers.read().await.len();
            tracing::debug!(
                session_id = %self.session_id,
                produced_events = self.produced_events.load(Ordering::SeqCst),
                produced_bytes = queue_max,
                queue_min_offset = queue_min,
                queue_resident_bytes = queue_bytes,
                subscriber_count,
                "pty output produce stats (periodic)"
            );
        }
    }

    /// 通过插件 TerminalHandler 管道处理输出
    ///
    /// 将输出数据解码，依次调用所有 Rust terminal handler 的 `on_output`，
    /// 如果任一 handler 修改了数据，使用修改后的数据重建事件。
    /// 无 terminal handler 或非 UTF-8 输出（二进制数据）时直接透传。
    ///
    /// 位于真源（入队前）：所有消费出口（本地 WS / 移动端 WS / 历史）语义一致
    #[allow(dead_code)] // 预留：入队前 plugin 处理路径尚未接入调用方
    async fn process_through_plugins(&self, mut event: OutputEvent) -> OutputEvent {
        let ctx = crate::system::app_context::AppContext::global();
        let plugin_host = ctx.plugin_host();

        // 无 terminal handler 时直接透传：避免每次输出都做
        // UTF-8 校验 + 字符串拷贝（绝大多数运行场景无插件）
        if !plugin_host.has_terminal_handlers().await {
            return event;
        }

        let text = match String::from_utf8(event.data.clone()) {
            Ok(t) => t,
            Err(_) => return event, // 非 UTF-8 输出（二进制数据），跳过插件处理
        };

        // 通过插件管道处理
        let processed = plugin_host.process_terminal_output(&event.session_id, &text).await;

        // 如果数据未被修改，直接返回原始事件
        if processed == text {
            return event;
        }

        event.data = processed.into_bytes();
        event
    }

    // ==================== 拉取模型订阅者管理 ====================

    /// 会话输出环句柄（订阅者执行体与管理器共享同一 `Arc`，只读拉取）
    pub fn ring(&self) -> Arc<RwLock<UnifiedOutputQueue>> {
        self.output_queue.clone()
    }

    /// 产出端唤醒接收器：订阅时刻已见当前值，此后 `changed()` 表示有新字节入环
    ///
    /// watch 天然合并多次 push（只存最新 `max_offset`）且无丢唤醒窗口，
    /// 不需 `Notify` 的「先查后等」配对
    pub fn watch_receiver(&self) -> watch::Receiver<u64> {
        self.max_watch_tx.subscribe()
    }

    /// 注册拉取模型订阅者（同 client_id 原子替换旧句柄）
    ///
    /// 不物化任何历史：只做「读水印 → 建句柄 → 插入」。历史回放由订阅者
    /// 任务按自己的游标从环上零拷贝拉取（消除旧 `snapshot_from().to_vec()` 的
    /// 整段拷贝），且与实时输出共用同一条窗口门控路径。
    ///
    /// 边界（spec §5.2）：
    /// - `from_offset = None` → 自 `min_offset` 全量回放（老客户端兼容）
    /// - `from_offset > max_offset` → 收敛到 `max_offset`（防御）
    /// - `from_offset < min_offset` → 以 `min_offset` 起播并标记订阅即截断
    ///   （任务首轮发 Resync 信号，客户端清屏重锚）
    pub async fn register_subscriber(
        &self,
        client_id: &str,
        from_offset: Option<u64>,
        mode: Arc<std::sync::atomic::AtomicU8>,
    ) -> PullSubscriber {
        let (min_offset, max_offset, history_bytes) = {
            let queue = self.output_queue.read().await;
            let (min, max) = queue.watermarks();
            (min, max, queue.history_bytes())
        };

        let requested = from_offset.unwrap_or(min_offset);
        let truncated = requested < min_offset;
        let start_offset = requested.clamp(min_offset, max_offset);
        let snapshot_offset = max_offset;

        let handle = Arc::new(SubscriberHandle::new(
            self.session_id.clone(),
            client_id.to_string(),
            start_offset,
            snapshot_offset,
            mode,
        ));
        if truncated {
            handle.stats.truncated_on_subscribe.store(true, Ordering::SeqCst);
        }

        // 原子替换：旧句柄被移出后**主动标记退订**并唤醒（任务在检查点退出；
        // 其残留帧另由流代数门控兜底）
        let previous = self
            .pull_subscribers
            .write()
            .await
            .insert(client_id.to_string(), handle.clone());
        if let Some(prev) = previous {
            prev.retire();
        }

        tracing::info!(
            session_id = %self.session_id,
            client_id = %client_id,
            start_offset,
            snapshot_offset,
            min_offset,
            history_bytes,
            truncated,
            "[SessionOutputManager] Client subscribed (pull model)"
        );

        PullSubscriber {
            handle,
            response: SubscribeResponse {
                min_offset,
                snapshot_offset,
                history_bytes,
            },
            max_watch: self.watch_receiver(),
        }
    }

    /// 移除拉取订阅者句柄（任务在下一个检查点退出；环不动）
    pub async fn unsubscribe_subscriber(&self, client_id: &str) -> bool {
        let removed = self.pull_subscribers.write().await.remove(client_id);
        match removed {
            Some(handle) => {
                // 标记退订 + 唤醒：驻留中/阻塞在等待上的任务都会退出
                handle.retire();
                tracing::info!(
                    session_id = %self.session_id,
                    client_id = %client_id,
                    next_offset = handle.next_offset(),
                    acked_offset = handle.acked_offset(),
                    "[SessionOutputManager] Client unsubscribed (pull model)"
                );
                true
            }
            None => false,
        }
    }

    /// 查询拉取订阅者句柄（ack 路由/测试观测）
    pub async fn subscriber_handle(&self, client_id: &str) -> Option<Arc<SubscriberHandle>> {
        self.pull_subscribers.read().await.get(client_id).cloned()
    }

    /// 拉取订阅者数量（测试/观测）
    pub async fn pull_subscriber_count(&self) -> usize {
        self.pull_subscribers.read().await.len()
    }

    /// 客户端 ack 路由：推进**该订阅者私有**的 ack 水位（I6）
    ///
    /// 不再是会话级共享记账——一个订阅者的 ack 不释放别人的窗口，
    /// 也不会因别的订阅者落后而虚高。会话/订阅者不存在时静默忽略。
    pub async fn ack_subscriber(&self, client_id: &str, acked_offset: u64) -> bool {
        let handle = self.pull_subscribers.read().await.get(client_id).cloned();
        match handle {
            Some(handle) => {
                handle.on_ack(acked_offset);
                true
            }
            None => false,
        }
    }

    /// 唤醒会话内全部拉取订阅者（会话产出端关闭/会话结束路径）
    pub async fn wake_all_subscribers(&self) {
        for handle in self.pull_subscribers.read().await.values() {
            handle.wake();
        }
    }

    /// 截取驻留历史字节区间（HTTP 一次性历史，TB v3 字节锚点）
    ///
    /// 返回 `(data_bytes, min_offset, snapshot_offset, history_bytes)`；
    /// from 旧于 min_offset 时自 min_offset 起（客户端游标落后于驻留头部时
    /// 以 min_offset 为准，由消费端按 response 做截断处理）
    pub async fn snapshot_bytes(&self, from: u64) -> (Vec<u8>, u64, u64, u64) {
        let queue = self.output_queue.read().await;
        let min_offset = queue.min_offset();
        let snapshot_offset = queue.max_offset();
        let history_bytes = queue.history_bytes();
        let from = from.max(min_offset);
        let data = queue.range(from, snapshot_offset);
        (data, min_offset, snapshot_offset, history_bytes)
    }
}

// ==================== Global Output Manager ====================

use std::sync::OnceLock;

/// 全局输出管理器 - 管理所有 PTY 会话的输出管理器（单例）
pub struct GlobalOutputManager {
    sessions: RwLock<HashMap<String, Arc<SessionOutputManager>>>,
}

impl GlobalOutputManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub fn global() -> Arc<Self> {
        static INSTANCE: OnceLock<Arc<GlobalOutputManager>> = OnceLock::new();
        INSTANCE.get_or_init(|| Arc::new(GlobalOutputManager::new())).clone()
    }

    /// 注册会话（PTY 会话创建时调用）
    pub async fn register_session(&self, session_id: &str) -> Arc<SessionOutputManager> {
        let manager = Arc::new(SessionOutputManager::new(session_id));
        self.sessions
            .write()
            .await
            .insert(session_id.to_string(), manager.clone());

        tracing::info!(session_id = %session_id, "[GlobalOutputManager] Session registered");
        manager
    }

    /// 注销会话（PTY 会话销毁时调用）
    pub async fn unregister_session(&self, session_id: &str) {
        let removed = self.sessions.write().await.remove(session_id);
        if let Some(manager) = removed {
            // 唤醒全部订阅者：产出端已结束，任务在下一次唤醒退出
            manager.wake_all_subscribers().await;
            tracing::info!(session_id = %session_id, "[GlobalOutputManager] Session unregistered");
        }
    }

    pub async fn has_session(&self, session_id: &str) -> bool {
        self.sessions.read().await.contains_key(session_id)
    }

    /// 取会话输出管理器句柄（订阅链路装配：环 + 句柄登记）
    pub async fn session(&self, session_id: &str) -> Option<Arc<SessionOutputManager>> {
        self.sessions.read().await.get(session_id).cloned()
    }

    /// 处理 PTY 输出（由 PtyReader 调用）
    pub async fn on_output(&self, event: OutputEvent) {
        let sessions = self.sessions.read().await;
        if let Some(manager) = sessions.get(&event.session_id) {
            manager.on_output(event).await;
        } else {
            tracing::warn!(
                "[GlobalOutputManager] Session {} not found for output event",
                event.session_id
            );
        }
    }

    /// 客户端 ack 路由：推进**该订阅者私有**的 ack 水位（spec §4.6 背压下移）。
    ///
    /// 不再是会话级共享记账：`acked_offset` 只作用于 `client_id` 这一路订阅者的
    /// 窗口（陈旧/乱序 ack 天然忽略，I6），其他订阅者与源产出无感。
    /// 订阅者不存在（连接已断/会话已销毁）时静默返回 false。
    pub async fn ack_subscriber(&self, session_id: &str, client_id: &str, acked_offset: u64) -> bool {
        let sessions = self.sessions.read().await;
        match sessions.get(session_id) {
            Some(manager) => manager.ack_subscriber(client_id, acked_offset).await,
            None => {
                tracing::debug!(session_id, client_id, acked_offset, "ack for unknown session ignored");
                false
            }
        }
    }

    /// 移除拉取订阅者（会话不存在 → false）
    pub async fn unsubscribe_subscriber(&self, session_id: &str, client_id: &str) -> bool {
        let sessions = self.sessions.read().await;
        match sessions.get(session_id) {
            Some(manager) => manager.unsubscribe_subscriber(client_id).await,
            None => false,
        }
    }

    /// 查询拉取订阅者句柄（会话不存在 → None）
    pub async fn subscriber_handle(&self, session_id: &str, client_id: &str) -> Option<Arc<SubscriberHandle>> {
        let sessions = self.sessions.read().await;
        match sessions.get(session_id) {
            Some(manager) => manager.subscriber_handle(client_id).await,
            None => None,
        }
    }

    /// 截取驻留历史字节区间（HTTP 一次性历史；会话不存在 → None）
    pub async fn snapshot_bytes(&self, session_id: &str, from: u64) -> Option<(Vec<u8>, u64, u64, u64)> {
        let sessions = self.sessions.read().await;
        if let Some(manager) = sessions.get(session_id) {
            Some(manager.snapshot_bytes(from).await)
        } else {
            None
        }
    }

    /// 取消订阅（移除该客户端的订阅者句柄；执行体在下一次唤醒退出）
    pub async fn unsubscribe(&self, session_id: &str, client_id: &str) -> bool {
        let sessions = self.sessions.read().await;
        if let Some(manager) = sessions.get(session_id) {
            manager.unsubscribe_subscriber(client_id).await
        } else {
            false
        }
    }

    /// 取消某客户端在所有会话中的订阅（客户端断开时调用）
    pub async fn unsubscribe_all_for_client(&self, client_id: &str) {
        let sessions = self.sessions.read().await;
        for (session_id, manager) in sessions.iter() {
            manager.unsubscribe_subscriber(client_id).await;
            tracing::debug!(
                "[GlobalOutputManager] Unsubscribed client {} from session {}",
                client_id,
                session_id
            );
        }
        tracing::info!(
            "[GlobalOutputManager] Cleaned up subscriptions for client {} across {} sessions",
            client_id,
            sessions.len()
        );
    }
}

impl Default for GlobalOutputManager {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::sync::Arc;

    /// 数据为固定 4 字节 "test" 的事件（start_offset 由 on_output 分配）
    fn make_event() -> OutputEvent {
        OutputEvent::new(
            "test".to_string(),
            b"test".to_vec(),
            0,
            Utc::now().timestamp_millis(),
            false,
        )
    }

    /// 指定负载/会话的事件（on_output 路径）
    fn session_event(session_id: &str, data: &[u8]) -> OutputEvent {
        OutputEvent {
            session_id: session_id.to_string(),
            data: data.to_vec(),
            start_offset: 0,
            timestamp: 0,
            is_waiting: false,
        }
    }

    // ==================== 字节块环（UnifiedOutputQueue） ====================

    /// push 推进 max_offset；`range` 全量取回，区间按序铺满（无重无缺）
    #[test]
    fn push_advances_max_offset_and_range_covers_all() {
        let mut queue = UnifiedOutputQueue::with_limits(u64::MAX, 100);

        for _ in 0..5 {
            queue.push(make_event());
        }

        assert_eq!(queue.len(), 5);
        assert_eq!(queue.watermarks(), (0, 20));
        assert_eq!(queue.history_bytes(), 20);
        assert_eq!(queue.range(0, 20), b"testtesttesttesttest");
    }

    /// 条目上限（max_chunks）淘汰推进 min_offset
    #[test]
    fn chunk_cap_updates_min_offset() {
        let mut queue = UnifiedOutputQueue::with_limits(u64::MAX, 3);

        for _ in 0..5 {
            queue.push(make_event());
        }

        assert_eq!(queue.min_offset(), 8);
        assert_eq!(queue.max_offset(), 20);
        assert_eq!(queue.len(), 3);
        assert_eq!(queue.range(queue.min_offset(), queue.max_offset()), b"testtesttest");
    }

    /// 字节上限超限时淘汰最旧块（min_offset 推进到新队首）
    #[test]
    fn max_bytes_limit_evicts_oldest() {
        let mut queue = UnifiedOutputQueue::with_limits(10, 100);
        queue.push(make_event());
        queue.push(make_event());
        queue.push(make_event());

        // 3×4B = 12B > 10B → 淘汰最旧 1 块 → 剩 8B
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.min_offset(), 4);
        assert_eq!(queue.max_offset(), 12);
        assert_eq!(queue.history_bytes(), 8);
    }

    /// 单块超过字节上限时仍保留（不能丢弃刚 push 的块；min_offset 为该块起点）
    #[test]
    fn max_bytes_single_chunk_exceeds_limit() {
        let mut queue = UnifiedOutputQueue::with_limits(2, 100);
        queue.push(make_event());
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.total_bytes, 4);
        assert_eq!(queue.min_offset(), 0);
        assert_eq!(queue.max_offset(), 4);
    }

    /// range(from, to)：HTTP 一次性历史的字节区间截取（含半块裁头/截尾）
    #[test]
    fn range_slices_bytes() {
        let mut queue = UnifiedOutputQueue::with_limits(u64::MAX, 100);
        queue.push(make_event());
        queue.push(make_event());
        queue.push(make_event());

        assert_eq!(queue.range(0, 12), b"testtesttest");
        assert_eq!(queue.range(5, 9), b"estt".to_vec()); // [5,9)：块尾 3 字节 + 块头 1 字节
        assert_eq!(queue.range(2, 6), b"stte".to_vec());
        assert_eq!(queue.range(6, 4), b"".to_vec()); // from >= to → 空
        assert_eq!(queue.range(100, 200), b"".to_vec());
    }

    /// 环淘汰后 range 从新驻留起点起（越界端收敛）
    #[test]
    fn range_after_eviction_converges_to_resident() {
        let mut queue = UnifiedOutputQueue::with_limits(6, 100);
        for _ in 0..3 {
            queue.push(make_event());
        }
        // 12B > 6B：while 淘汰至 6B 内（4B 块每次淘汰一块）→ 仅驻留 [8,12)
        assert_eq!(queue.min_offset(), 8);
        assert_eq!(queue.range(0, 12), b"test"); // 起点收敛到 min_offset
    }

    // ==================== 票 04：游标拉取（output-ring-fetch 数据面） ====================

    /// 基本拉取：跨块合并 + next_offset 续拉不重复 + 追平返回空段
    #[test]
    fn fetch_basic_merge_and_catchup() {
        let mut queue = UnifiedOutputQueue::with_limits(u64::MAX, 100);
        for _ in 0..3 {
            queue.push(make_event()); // 3×4B = [0,12)
        }

        let first = queue.fetch(0, 16);
        assert_eq!(first.data, b"testtesttest");
        assert_eq!(first.next_offset, 12);
        assert!(!first.truncated);

        // 追平：游标 = 产出端 → 空段（truncated = false，不是缺口）
        let catchup = queue.fetch(12, 16);
        assert!(catchup.data.is_empty());
        assert_eq!(catchup.next_offset, 12);
        assert!(!catchup.truncated);
    }

    /// max_bytes 截断：单次拉取不超过预算；半块裁头跨块合并（隐藏块边界）
    #[test]
    fn fetch_respects_budget_and_slices_chunks() {
        let mut queue = UnifiedOutputQueue::with_limits(u64::MAX, 100);
        for _ in 0..3 {
            queue.push(make_event()); // [0,12)
        }

        let capped = queue.fetch(0, 7);
        assert_eq!(capped.data, b"testtes");
        assert_eq!(capped.next_offset, 7);
        assert!(!capped.truncated);

        // [5,11)：块尾 3B + 块头 3B 合并，对消费者无块边界
        let mid = queue.fetch(5, 6);
        assert_eq!(mid.data, b"esttes");
        assert_eq!(mid.next_offset, 11);
        assert!(!mid.truncated);
    }

    /// 环淘汰后游标落后 → truncated = true，返回现存最早段（从新 min_offset 起）
    #[test]
    fn fetch_truncated_after_eviction() {
        let mut queue = UnifiedOutputQueue::with_limits(6, 100);
        for _ in 0..3 {
            queue.push(make_event()); // 12B > 6B → 淘汰至驻留 [8,12)
        }
        assert_eq!(queue.min_offset(), 8);

        let fetched = queue.fetch(0, 16);
        assert!(fetched.truncated, "游标落后于驻留起点必须报截断");
        assert_eq!(fetched.data, b"test"); // 现存最早段 [8,12)
        assert_eq!(fetched.next_offset, 12);

        // 后续按 next_offset 续拉不再报截断
        let follow = queue.fetch(fetched.next_offset, 16);
        assert!(!follow.truncated);
        assert!(follow.data.is_empty());
    }

    /// 未来游标自愈：from > max_offset 按追平处理（不报错、不回带非法游标）
    #[test]
    fn fetch_future_cursor_self_heals() {
        let mut queue = UnifiedOutputQueue::with_limits(u64::MAX, 100);
        queue.push(make_event()); // [0,4)

        let fetched = queue.fetch(999, 16);
        assert!(fetched.data.is_empty());
        assert_eq!(fetched.next_offset, 4); // 收敛回产出端
        assert!(!fetched.truncated);
    }

    // ==================== 会话级订阅者管理（拉取模型） ====================

    /// 注册订阅者：返回订阅时刻水印，句柄起点 = clamp(from_offset, min..=max)
    #[tokio::test]
    async fn register_subscriber_returns_watermarks_and_clamped_start() {
        let manager = SessionOutputManager::new("s");
        for _ in 0..3 {
            manager.on_output(session_event("s", b"abcd")).await;
        }

        // 无 from_offset：自 min_offset 全量回放
        let pull = manager
            .register_subscriber("c1", None, Arc::new(std::sync::atomic::AtomicU8::new(MODE_REALTIME)))
            .await;
        assert_eq!(pull.response.min_offset, 0);
        assert_eq!(pull.response.snapshot_offset, 12);
        assert_eq!(pull.response.history_bytes, 12);
        assert_eq!(pull.handle.start_offset, 0);
        assert_eq!(pull.handle.snapshot_offset, 12);
        assert!(!pull.handle.stats.truncated_on_subscribe.load(Ordering::SeqCst));

        // from_offset > max_offset：收敛到 max_offset（防御）
        let pull = manager
            .register_subscriber(
                "c2",
                Some(999),
                Arc::new(std::sync::atomic::AtomicU8::new(MODE_REALTIME)),
            )
            .await;
        assert_eq!(pull.handle.start_offset, 12);

        // from_offset 落在块内：保留精确游标（半块起播）
        let pull = manager
            .register_subscriber("c3", Some(2), Arc::new(std::sync::atomic::AtomicU8::new(MODE_REALTIME)))
            .await;
        assert_eq!(pull.handle.start_offset, 2);

        assert_eq!(manager.pull_subscriber_count().await, 3);
    }

    /// 订阅起点早于驻留起点（游标过旧）→ 起播锚点收敛到 min_offset 并标记截断
    #[tokio::test]
    async fn register_subscriber_with_stale_cursor_marks_truncation() {
        let manager = SessionOutputManager::new("s");
        {
            let ring_arc = manager.ring();
            let mut ring = ring_arc.write().await;
            *ring = UnifiedOutputQueue::with_limits(8, 100);
        }
        for _ in 0..3 {
            manager.on_output(session_event("s", b"abcd")).await;
        }
        // 12B > 8B：淘汰至 ≤8B → 驻留 [4,12)，min_offset = 4
        let pull = manager
            .register_subscriber("c1", Some(0), Arc::new(std::sync::atomic::AtomicU8::new(MODE_REALTIME)))
            .await;
        assert_eq!(pull.response.min_offset, 4);
        assert_eq!(pull.handle.start_offset, 4, "起播锚点必须收敛到驻留起点");
        assert!(
            pull.handle.stats.truncated_on_subscribe.load(Ordering::SeqCst),
            "订阅即截断必须留痕（执行体据此先发重同步信号）"
        );
    }

    /// 生产端唤醒：`on_output` 通告 watch（承载 max_offset），订阅者据此拉取
    #[tokio::test]
    async fn on_output_publishes_watermark_to_watch() {
        let manager = SessionOutputManager::new("s");
        let mut watch = manager.watch_receiver();
        assert_eq!(*watch.borrow_and_update(), 0);

        manager.on_output(session_event("s", b"abcd")).await;
        watch.changed().await.expect("有新产出必须唤醒");
        assert_eq!(*watch.borrow(), 4);

        // 多次 push 合并为最新值（watch 语义：只存最新水印）
        manager.on_output(session_event("s", b"ef")).await;
        manager.on_output(session_event("s", b"gh")).await;
        watch.changed().await.unwrap();
        assert_eq!(*watch.borrow(), 8);
    }

    /// ack 路由：只推进目标订阅者的私有水位；不存在的订阅者返回 false
    #[tokio::test]
    async fn ack_subscriber_is_private_and_monotonic() {
        let manager = SessionOutputManager::new("s");
        let a = manager
            .register_subscriber("a", None, Arc::new(std::sync::atomic::AtomicU8::new(MODE_REALTIME)))
            .await;
        let b = manager
            .register_subscriber("b", None, Arc::new(std::sync::atomic::AtomicU8::new(MODE_REALTIME)))
            .await;

        assert!(manager.ack_subscriber("a", 100).await);
        assert_eq!(a.handle.acked_offset(), 100);
        assert_eq!(b.handle.acked_offset(), 0, "ack 必须只作用于本订阅者");

        // 陈旧 ack 不后退（I6）
        assert!(manager.ack_subscriber("a", 50).await);
        assert_eq!(a.handle.acked_offset(), 100);

        assert!(!manager.ack_subscriber("nobody", 10).await, "无此订阅者 → false");
    }

    /// 注册同 client_id = 原子替换（旧句柄被唤醒退出，新句柄接管）
    #[tokio::test]
    async fn register_subscriber_replaces_same_client_id() {
        let manager = SessionOutputManager::new("s");
        let first = manager
            .register_subscriber("c1", None, Arc::new(std::sync::atomic::AtomicU8::new(MODE_REALTIME)))
            .await;
        let second = manager
            .register_subscriber("c1", None, Arc::new(std::sync::atomic::AtomicU8::new(MODE_REALTIME)))
            .await;
        assert!(!Arc::ptr_eq(&first.handle, &second.handle));
        assert_eq!(manager.pull_subscriber_count().await, 1);

        // 旧句柄已被唤醒（park 中的任务会立即重查）——此处仅验证句柄仍可用
        //（唤醒是幂等 no-op，无 waiter 时 notify_waiters 不产生副作用）
        first.handle.wake();
    }

    /// 退订：移除句柄 + 唤醒（任务在下一次唤醒退出）；重复退订返回 false
    #[tokio::test]
    async fn unsubscribe_subscriber_removes_handle_and_wakes() {
        let manager = SessionOutputManager::new("s");
        let pull = manager
            .register_subscriber("c1", None, Arc::new(std::sync::atomic::AtomicU8::new(MODE_REALTIME)))
            .await;
        assert!(manager.unsubscribe_subscriber("c1").await);
        assert_eq!(manager.pull_subscriber_count().await, 0);
        assert!(manager.subscriber_handle("c1").await.is_none());
        assert!(!manager.unsubscribe_subscriber("c1").await, "重复退订 → false");
        // 句柄仍可读（观测），但已脱离管理器
        assert_eq!(pull.handle.client_id, "c1");
    }

    /// 会话注销：唤醒全部订阅者（产出端结束 → 任务自然退出）
    #[tokio::test]
    async fn unregister_session_wakes_subscribers() {
        let manager = GlobalOutputManager::new();
        manager.register_session("s1").await;
        let session = manager.session("s1").await.expect("会话管理器");
        let pull = session
            .register_subscriber("c1", None, Arc::new(std::sync::atomic::AtomicU8::new(MODE_REALTIME)))
            .await;
        assert_eq!(session.pull_subscriber_count().await, 1);

        manager.unregister_session("s1").await;
        assert!(!manager.has_session("s1").await);
        // 唤醒信号已发出（notify_waiters 无 waiter 时无副作用，此处验证不 panic）
        pull.handle.wake();
    }

    /// 多会话隔离：A 的产出不影响 B 的环与水印
    #[tokio::test]
    async fn multiple_sessions_are_isolated() {
        let manager = GlobalOutputManager::new();
        manager.register_session("s1").await;
        manager.register_session("s2").await;

        manager.on_output(session_event("s1", b"aaaa")).await;
        manager.on_output(session_event("s2", b"bb")).await;

        let s1 = manager.session("s1").await.unwrap();
        let s2 = manager.session("s2").await.unwrap();
        assert_eq!(s1.ring().read().await.watermarks(), (0, 4));
        assert_eq!(s2.ring().read().await.watermarks(), (0, 2));
    }

    /// 未知会话：产出/退订/acl 全部静默安全（不 panic）
    #[tokio::test]
    async fn unknown_session_operations_are_safe() {
        let manager = GlobalOutputManager::new();
        manager.on_output(session_event("no-such", b"x")).await;
        assert!(!manager.unsubscribe("no-such", "c1").await);
        assert!(!manager.ack_subscriber("no-such", "c1", 5).await);
        assert!(manager.session("no-such").await.is_none());
        assert!(manager.subscriber_handle("no-such", "c1").await.is_none());
        manager.unregister_session("no-such").await;
    }

    /// session_id 与环句柄：同一 Arc（订阅者任务与管理器共享）
    #[test]
    fn session_id_and_ring_share_state() {
        let manager = SessionOutputManager::new("sess-1");
        assert_eq!(manager.session_id(), "sess-1");
        assert!(Arc::ptr_eq(&manager.ring(), &manager.ring()));
    }

    // ==================== HTTP 一次性历史（snapshot_bytes） ====================

    /// snapshot_bytes：按 from 字节锚点截取 + 元数据三件套
    #[tokio::test]
    async fn snapshot_bytes_http() {
        let manager = GlobalOutputManager::new();
        manager.register_session("session-http").await;

        for _ in 0..3 {
            manager.on_output(session_event("session-http", b"test")).await;
        }

        let (data, min, snapshot, history_bytes) =
            manager.snapshot_bytes("session-http", 4).await.expect("session exists");
        assert_eq!(min, 0);
        assert_eq!(snapshot, 12);
        assert_eq!(history_bytes, 12);
        assert_eq!(data, b"testtest");

        // from 旧于 min_offset：以 min_offset 为起点
        let (data, _, _, _) = manager.snapshot_bytes("session-http", 0).await.unwrap();
        assert_eq!(data, b"testtesttest");

        // 会话不存在 → None
        assert!(manager.snapshot_bytes("no-such", 0).await.is_none());
    }

    /// 全局快照截取 + 淘汰联动：from 越过驻留头部后返回空
    #[tokio::test]
    async fn snapshot_bytes_after_eviction() {
        let manager = GlobalOutputManager::new();
        manager.register_session("session-http-evict").await;
        {
            let session = manager.session("session-http-evict").await.unwrap();
            let ring_arc = session.ring();
            let mut ring = ring_arc.write().await;
            *ring = UnifiedOutputQueue::with_limits(6, 100);
        }
        for _ in 0..3 {
            manager.on_output(session_event("session-http-evict", b"test")).await;
        }
        let (data, min, snapshot, history_bytes) = manager.snapshot_bytes("session-http-evict", 0).await.unwrap();
        assert_eq!(min, 8);
        assert_eq!(snapshot, 12);
        assert_eq!(history_bytes, 4);
        assert_eq!(data, b"test");
    }
}
