//! Session Output
//!
//! PTY 输出相关的组件：统一输出队列、会话输出管理、全局输出管理
//!
//! TB v3 字节连续语义（`.scratch/pty-byte-history/spec.md`）：
//! 连续性以会话内累计字节偏移（start_offset/end_offset）表达，取代事件 index；
//! 帧头"第几个事件" → "累计第几个字节"；游标/缺口/去重/ack/快照收敛到
//! `[start_offset, end_offset)` 区间运算。

use bytes::Bytes;
use crate::session::RendererSource;
use crate::system::config::AppConfig;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};

// ==================== Output Frame ====================

/// 订阅者通道帧：输出事件或历史结束标记
///
/// HistoryEnd 使消费端（forward 层）明确知道历史边界——旧路由（05 过渡）
/// 无可编码帧、直接吞掉；06 新路由编码为 `{"type":"history_end"}` JSON 控制帧
///
/// 字节化（TB v3）：连续性以累计字节偏移表达，快照元数据为字节三件套
///（snapshot_offset / min_offset / history_bytes）
#[derive(Debug, Clone)]
pub enum OutputFrame {
    Output(OutputEvent),
    HistoryEnd {
        /// 订阅时刻队列累计字节数（历史边界，可能因后续 push 继续增长）
        snapshot_offset: u64,
        /// 队列中最早存续字节位置（环形淘汰后推进）
        min_offset: u64,
        /// 驻留历史总字节数
        history_bytes: u64,
    },
}

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

    /// 按字节锚点截取历史快照（TB v3）
    ///
    /// - `from_offset = None`：全量回放（自 min_offset 起，兼容旧客户端）
    /// - `Some(from)`：自 >= from 的字节位置起播——chunk 级跳过 + 末块
    ///   `Bytes::slice` 半块裁头；`end_is_waiting` 由各块自带
    ///
    /// 返回 `(快照事件列表, min_offset, snapshot_offset, history_bytes)`
    pub fn snapshot_from(&self, from_offset: Option<u64>) -> (Vec<OutputEvent>, u64, u64, u64) {
        let mut events = Vec::with_capacity(self.chunks.len().min(8192));
        let from = from_offset.unwrap_or(self.min_offset);
        for chunk in &self.chunks {
            let chunk_end = chunk.end_offset();
            if chunk_end <= from {
                continue; // chunk 级跳过
            }
            if chunk.start_offset < from {
                // 半块切片：起点落在块内
                let cut = (from - chunk.start_offset) as usize;
                let data = chunk.bytes.slice(cut..).to_vec();
                events.push(OutputEvent {
                    session_id: String::new(),
                    data,
                    start_offset: from,
                    timestamp: 0,
                    is_waiting: chunk.end_is_waiting,
                });
            } else {
                let data = chunk.bytes.to_vec();
                events.push(OutputEvent {
                    session_id: String::new(),
                    data,
                    start_offset: chunk.start_offset,
                    timestamp: 0,
                    is_waiting: chunk.end_is_waiting,
                });
            }
        }
        (
            events,
            self.min_offset,
            self.max_offset,
            self.total_bytes,
        )
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

/// 订阅者状态
pub struct SubscriberState {
    pub client_id: String,
    /// 订阅是否活跃（历史发送完成后才标记为 true）
    pub active: AtomicBool,
    pub sent_offset: AtomicU64,
    /// 独立发送通道（绑定该客户端的 WebSocket，承载 OutputFrame 流）
    pub send_queue: mpsc::Sender<OutputFrame>,
    /// inactive 期间的待发送缓冲，消除历史发送→激活之间的丢失窗口
    pub pending: RwLock<Vec<OutputEvent>>,
    /// 背压丢弃计数（try_send 满时递增，限频日志用）
    pub dropped: AtomicU64,
}

/// inactive 占位期间 pending 缓存上限：超出丢弃新事件（客户端重订阅
/// 时全量重播整体回补，事件仍留在输出队列）
/// 16384：大历史重播（数万事件）期间实时输出缓存余量，降低重订阅风暴频率
const PENDING_EVENT_CAP: usize = 16384;

/// 发送背压超时：订阅者 send_queue 满时 on_output 有界等待发送的时限。
/// 等待期间 on_output 阻塞 → PTY 读循环停等 → 数据留在 PTY 内核缓冲，零丢失
/// （实时流不允许中断的契约）。超时仅用于客户端死亡/连接悬挂的极端场景：
/// 丢弃该事件并冲正背压记账，防止 PERMANENT 阻塞拖死整个会话
const SEND_BACKPRESSURE_TIMEOUT: Duration = Duration::from_secs(2);

/// 背压高位水（暂停）：未 ack 字节超过该值 → 暂停该会话 PTY 读取
/// （spec 04-06 渲染反馈环）。
///
/// 关键约束：必须低于 WebKitGTK WS 接收缓冲容量（约 64KB–256KB）。旧值
/// 1MB 远高于 WS 缓冲——前端每 64KB 就 ack 一次，unacked 正常只在 64~128KB
/// 震荡、永远到不了 1MB，PTY 从不暂停；而 WS 缓冲在 ~128KB 就溢出丢整消息
/// （opencode 滚动残渣 + Parsing error 的直接来源）。降到 64KB 后源头在
/// 「WS 缓冲装满前」即停住，从根上杜绝丢消息。参考 VS Code 终端流控
/// HighWatermarkChars=100KB（Electron IPC 可靠通道）的本地位取值。
const BACKPRESSURE_HIGH_BYTES: u64 = 64 * 1024;
/// 背压低水位（恢复）：已暂停时未 ack 降到该值以下才恢复读（滞回防抖，
/// 避免单阈值在临界点反复暂停/恢复抖振）。参考 VS Code LowWatermarkChars=5KB；
/// 取 8KB 与前端 ack 粒度（64KB 全量释放）配合——一次 ack 即回落穿破低位水。
const BACKPRESSURE_RESUME_BYTES: u64 = 8 * 1024;
/// 未 ack 记账 FIFO 容量（事件数）：防 ack 停滞时无限增长；满则冻结记账，
/// unacked 保持近满态触发暂停（保守），ack 弹出后自动恢复精确记账
const UNACKED_FIFO_CAP: usize = 8192;

impl SubscriberState {
    pub fn new(client_id: String, send_queue: mpsc::Sender<OutputFrame>) -> Self {
        Self {
            client_id,
            active: AtomicBool::new(false),
            sent_offset: AtomicU64::new(0),
            send_queue,
            pending: RwLock::new(Vec::new()),
            dropped: AtomicU64::new(0),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn activate(&self, sent_offset: u64) {
        self.sent_offset.store(sent_offset, Ordering::SeqCst);
        self.active.store(true, Ordering::SeqCst);
    }

    /// 排空 pending 缓冲并发送为 Output 帧
    ///
    /// 占位期内 on_output() 缓存的事件均在快照（subscribe 持队列读锁收集历史）
    /// 之后 push，start_offset 必然 >= 快照时 snapshot_offset——历史与 pending
    /// 无重叠，无需跳过（旧版按 seq 去重是因快照与 pending 存在竞态窗口）
    async fn drain_pending(&self) {
        let mut pending = self.pending.write().await;
        for event in pending.drain(..) {
            if let Err(e) = self.send_queue.send(OutputFrame::Output(event.clone())).await {
                tracing::warn!(
                    "[SessionOutputManager] Failed to send pending to {}: {}",
                    self.client_id,
                    e
                );
            }
        }
    }
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

/// 单个 PTY 会话的输出管理，包括输出队列和订阅者管理
pub struct SessionOutputManager {
    session_id: String,
    output_queue: Arc<RwLock<UnifiedOutputQueue>>,
    subscribers: RwLock<HashMap<String, SubscriberState>>,
    /// 背压记账：已产出未 ack 的字节数（渲染反馈环，前端 onWriteParsed 后
    /// 回发 ack；超水位 → 暂停该会话 PTY 读取）
    unacked_bytes: AtomicU64,
    /// 未 ack 事件 FIFO（end_offset → bytes）：ack 按序弹出精减 unacked_bytes；
    /// std Mutex 仅作短临界区（无 await 保持），热路径成本低
    unacked_fifo: std::sync::Mutex<std::collections::VecDeque<(u64, u64)>>,
    /// 背压暂停滞回状态：true = 已暂停（等 unacked 降到低水位才恢复）。
    /// 由 PTY 读线程经 should_pause() 同步读写，纯原子无锁；会话新建为 false
    paused: AtomicBool,
    /// 输出入队串行锁：串行化 on_output 的「offset 分配 + 入队 + 广播」整段
    /// 临界区。PtyReader 用 spawn 并发调 on_output，多个任务并发时 offset
    /// 分配（写锁）与广播（读锁）之间可被其他任务插入，导致 send_queue 顺序
    /// 与 offset 顺序错乱（60 先于 59 到达 forward_loop）→ 帧 offset 错乱/空洞 →
    /// 前端误判 gap。串行后 offset 顺序 = 广播顺序，根治该竞态。
    output_serial: tokio::sync::Mutex<()>,
}

impl SessionOutputManager {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            output_queue: Arc::new(RwLock::new(UnifiedOutputQueue::default())),
            subscribers: RwLock::new(HashMap::new()),
            unacked_bytes: AtomicU64::new(0),
            unacked_fifo: std::sync::Mutex::new(std::collections::VecDeque::new()),
            paused: AtomicBool::new(false),
            output_serial: tokio::sync::Mutex::new(()),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 处理新输出
    ///
    /// 先入队再用事件广播给订阅者；事件的 start_offset 在此串行临界区内分配
    /// （= 队列 max_offset），订阅者按订阅内连续字节流自洽（见 forward 层合成游标）
    ///
    /// 背压保护：同步 try_send 而非 await send——慢订阅者（移动端弱网，
    /// 8192 事件通道 + 有界合并 + 转发通道逐级排满）不能阻塞 on_output，
    /// 否则同会话所有订阅者（含桌面端本地 WS）输出同步冻结、PTY 读取
    /// 停摆。满时丢弃该事件：客户端重订阅全量重播整体回补，事件仍保留
    /// 在输出队列中
    pub async fn on_output(&self, event: OutputEvent) {
        // 串行化整个「offset 分配 + 入队 + 广播」临界区：PtyReader 用 spawn
        // 并发调 on_output，若不串行，多个任务在 offset 分配（写锁）与广播
        // （读锁）之间互相插入，send_queue 顺序与 offset 顺序错乱（60 先于 59）
        // → forward_loop 帧 offset 错乱/空洞 → 前端误判 gap。串行后顺序一致。
        let _guard = self.output_serial.lock().await;

        // 字节偏移按会话连续分配（队列 max_offset），替代跨会话全局计数器
        // （next_output_index）：消除「多会话并发 → 会话内实时帧带跨会话空洞 → 客户端
        // `start_offset > cursor → 重订阅` 缺口检测被误触发 → 反复重订阅风暴」。
        // 按会话连续后缺口检测只在真实丢帧（背压 / 占位 pending 溢出）时命中，
        // 由重订阅 → 按游标锚点重播 → 按游标裁过去重自愈补回
        let mut event = event;
        {
            let mut queue = self.output_queue.write().await;
            event.start_offset = queue.max_offset();
            event = queue.push(event);
        }

        // 背压记账：产出字节累加 + FIFO 登记（ack 按序弹出精减）。FIFO 满表示
        // ack 严重停滞 → 冻结记账：unacked 保持近满态触发暂停（保守方向），
        // ack 弹出腾出空间后自动恢复精确记账
        {
            let event_bytes = event.data.len() as u64;
            let mut fifo = match self.unacked_fifo.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            if fifo.len() < UNACKED_FIFO_CAP {
                self.unacked_bytes.fetch_add(event_bytes, Ordering::SeqCst);
                fifo.push_back((event.end_offset(), event_bytes));
            }
        }

        let subscribers = self.subscribers.read().await;
        for subscriber in subscribers.values() {
            if subscriber.is_active() {
                match subscriber.send_queue.try_send(OutputFrame::Output(event.clone())) {
                    Ok(()) => {}
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        // 背压：队列满 = 该订阅者消费停滞。有界等待发送——等待期间
                        // on_output 阻塞 → 上游 PTY 读循环停等 → 数据留在 PTY 内核
                        // 缓冲，零丢失（满足「实时流不允许中断」契约）；超时仅兜底
                        // 客户端死亡/连接悬挂异常，超时丢弃并冲正记账（见 revert_unacked）
                        match tokio::time::timeout(
                            SEND_BACKPRESSURE_TIMEOUT,
                            subscriber.send_queue.send(OutputFrame::Output(event.clone())),
                        )
                        .await
                        {
                            Ok(Ok(())) => {}
                            Ok(Err(_)) => {
                                // Closed：订阅者已移除/连接断开，静默忽略
                            }
                            Err(_) => {
                                // 超时：satellite 死亡等极端场景，丢弃该事件并冲正
                                // 未 ack 记账（丢的事件永远不会有 ack，不冲正则
                                // unacked_bytes 永久虚高 → 背压水位永久暂停 → 饿死）
                                let n = subscriber.dropped.fetch_add(1, Ordering::SeqCst) + 1;
                                if n <= 3 || n % 100 == 0 {
                                    tracing::warn!(
                                        "[SessionOutputManager] Subscriber {} send stalled >{}s, dropped event #{} (start_offset={})",
                                        subscriber.client_id,
                                        SEND_BACKPRESSURE_TIMEOUT.as_secs(),
                                        n,
                                        event.start_offset
                                    );
                                }
                                self.revert_unacked(event.end_offset(), event.data.len() as u64);
                            }
                        }
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                        // 订阅者已移除：静默忽略
                    }
                }
            } else {
                // inactive 期间缓存事件，激活时排空，消除历史发送→激活的丢失窗口。
                // 有界保护：占位窗口（历史排空）可能因慢链路持续很久，pending 无上限
                // 会持续吃内存；超出容量丢弃并冲正记账（丢的事件不产生 ack，不冲正
                // 会导致 unacked_bytes 虚高），缺口由客户端重订阅按游标重播自愈
                //
                // 握手期间 drain_pending 持 pending 写锁是短暂窗口（毫秒级历史回放），
                // 退化为阻塞 write().await 等待持锁方释放——避免 offset 已分配但帧永久丢失
                // （try_write 失败时静默丢弃会让客户端字节缺口、触发不必要的重订阅）。
                // 该等待不持有 self.output_queue / subscribers 锁，不会与其他锁路径死锁
                let mut pending = subscriber.pending.write().await;
                if pending.len() >= PENDING_EVENT_CAP {
                    let n = subscriber.dropped.fetch_add(1, Ordering::SeqCst) + 1;
                    if n <= 3 || n % 100 == 0 {
                        tracing::warn!(
                            "[SessionOutputManager] Subscriber {} pending overflow, dropped event #{} (start_offset={})",
                            subscriber.client_id,
                            n,
                            event.start_offset
                        );
                    }
                    self.revert_unacked(event.end_offset(), event.data.len() as u64);
                } else {
                    pending.push(event.clone());
                }
            }
        }
    }

    /// 冲正未 ack 记账：事件被丢弃（发送超时/pending 溢出）后永远不会收到 ack，
    /// 将其从 FIFO 移除并回减 unacked_bytes，防止永久虚高导致背压水位永久暂停
    /// （饥饿）。FIFO 上限 8K，retain 线性扫描可接受（std Mutex 仅短临界区无 await）
    fn revert_unacked(&self, end_offset: u64, bytes: u64) {
        let mut fifo = match self.unacked_fifo.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let before = fifo.len();
        fifo.retain(|(e, _)| *e != end_offset);
        if fifo.len() != before {
            self.unacked_bytes.fetch_sub(bytes, Ordering::SeqCst);
        }
    }

    /// 客户端 ack：释放 end_offset <= acked_offset 的未 ack 字节（渲染反馈环回调）
    ///
    /// offset 单调前进（前端只对已渲染区间回发）；比当前水位陈旧或超出已产出
    ///（重订阅竞态/陈旧客户端）的 ack 被 FIFO 弹出条件天然忽略——只弹
    /// end_offset <= offset 的条目，无匹配即不动，unacked 不会越界减为负
    pub fn on_ack(&self, acked_offset: u64) {
        let mut fifo = match self.unacked_fifo.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        while let Some(&(end_offset, bytes)) = fifo.front() {
            if end_offset <= acked_offset {
                fifo.pop_front();
                self.unacked_bytes.fetch_sub(bytes, Ordering::SeqCst);
            } else {
                break;
            }
        }
    }

    /// 背压判定（PTY 读线程同步调用，零锁零阻塞可高频轮询）。
    ///
    /// 滞回语义（对比 VS Code 终端流控 High/LowWatermark）：
    /// - 未暂停时：unacked > 高位水（64KB）→ 置暂停态并暂停读；
    /// - 已暂停时：unacked ≤ 低水位（8KB）→ 清暂停态并恢复读；
    /// 区间内保持当前态，避免单阈值在临界点反复抖振（反复暂停/恢复会
    /// 打断 PTY 读节奏、加剧延迟）。纯原子读 + 原子写，无锁无阻塞。
    pub fn should_pause(&self) -> bool {
        let unacked = self.unacked_bytes.load(Ordering::SeqCst);
        if self.paused.load(Ordering::SeqCst) {
            // 已暂停：降到低水位才恢复（滞回下沿）
            if unacked <= BACKPRESSURE_RESUME_BYTES {
                self.paused.store(false, Ordering::SeqCst);
                return false;
            }
            return true;
        }
        // 未暂停：超高位水才暂停（滞回上沿）
        if unacked > BACKPRESSURE_HIGH_BYTES {
            self.paused.store(true, Ordering::SeqCst);
            return true;
        }
        false
    }

    /// 订阅会话输出（05 快照协议 + TB v3 字节锚点）
    ///
    /// 帧流顺序：`[历史 Output × N] → HistoryEnd → [实时 Output]`，
    /// 历史完成后才标记 active（实时帧绝不先于 HistoryEnd 到达）
    ///
    /// 锁序（queue → subscribers 固定，勿调换）：
    /// 1. 写锁插入 active=false 的 subscriber（占位）
    /// 2. 读锁读取快照元数据 + 历史段——持锁期间 on_output 写锁被阻塞，
    ///    保证快照与历史严格一致；随后 drop 队列读锁
    ///    （from_offset 起播：chunk 级跳过 + 半块 slice，见 snapshot_from）
    /// 3. 读 `history_start_mode` 配置：snapshot 模式尚未实现，恒回退 min
    /// 4. response 经 oneshot 前置返回（不被历史背压阻塞，避免客户端 10s 订阅
    ///    超时误判失败——订阅实际已建立，重复订阅产生孤儿任务 → 重复流）
    /// 5. subscribers 读锁内逐条发送历史（持读锁发送防止订阅者在历史发送中
    ///    被替换导致旧任务历史注入新通道）
    /// 6. 同一读锁作用域内发送 HistoryEnd 帧（字节三件套）
    /// 7. 写锁排空 pending + 原子激活（on_output 被阻塞，排空与激活之间无新事件）
    ///
    /// 占位期间 on_output() 看到该 subscriber 但 active=false → 缓存到 pending；
    /// pending 中事件 start_offset 必 >= 快照 snapshot_offset（第 2 步持读锁时的
    /// max_offset），与历史衔接无缝；排空与激活同锁完成，零丢失、顺序正确
    pub async fn subscribe(
        &self,
        client_id: &str,
        ws_sender: mpsc::Sender<OutputFrame>,
        response_tx: Option<tokio::sync::oneshot::Sender<SubscribeResponse>>,
        from_offset: Option<u64>,
    ) -> SubscribeResponse {
        let subscriber = SubscriberState::new(client_id.to_string(), ws_sender);

        // 第一步：插入占位 subscriber（active=false），释放写锁
        self.subscribers.write().await.insert(client_id.to_string(), subscriber);

        // 第二步：读取历史并发送（不持锁，不阻塞 on_output）
        // 持读锁期间 on_output 的写锁被阻塞 → 快照与历史严格一致
        let queue = self.output_queue.read().await;
        let (history, min_offset, snapshot_offset, history_bytes) = queue.snapshot_from(from_offset);
        drop(queue);

        // 历史回放起点模式：snapshot（2J 清屏快照点 offset 化记录）尚未实现，
        // 恒回退 min 严格回放；快照机制随后续 ticket 引入
        if AppConfig::global().channels.history_start_mode == crate::system::config::HistoryStartMode::Snapshot {
            tracing::warn!(
                "[SessionOutputManager] history_start_mode=snapshot 未实现（快照回放延后），回退 min_offset 严格回放"
            );
        }

        let response = SubscribeResponse {
            min_offset,
            snapshot_offset,
            history_bytes,
        };

        // 订阅响应前置：历史入队可能被通道背压阻塞（容量 8192 + 大历史 +
        // 慢链路时排空极慢），若等历史发完再回响应，客户端订阅超时（10s）
        // 会误判失败——订阅实际已建立，后续重新订阅会替换订阅者，旧任务
        // 残留缓冲帧形成重复流（连续性违反风暴）。先回响应消息，历史帧
        // 随后按序到达，客户端语义不变（帧仍晚于响应）
        if let Some(tx) = response_tx {
            let _ = tx.send(response.clone());
        }

        // 通过该订阅者的独立通道发送历史与 HistoryEnd 标记（保证顺序）
        // 持读锁发送：防止订阅者在历史发送中被替换（替换会 abort 本任务，
        // 但锁内发送可避免新订阅者误收旧历史）
        {
            let subscribers = self.subscribers.read().await;
            if let Some(sub) = subscribers.get(client_id) {
                for event in &history {
                    if let Err(e) = sub.send_queue.send(OutputFrame::Output(event.clone())).await {
                        tracing::warn!(client_id = %client_id, error = %e, "[SessionOutputManager] Failed to send history");
                    }
                }
                // 历史边界标记：消费端据此明确"此后为实时流"；旧路由吞掉
                if sub
                    .send_queue
                    .send(OutputFrame::HistoryEnd {
                        snapshot_offset,
                        min_offset,
                        history_bytes,
                    })
                    .await
                    .is_err()
                {
                    tracing::warn!(
                        "[SessionOutputManager] Failed to send history_end to {}: channel closed",
                        client_id
                    );
                }
            }
        }

        // 第三步：排空 pending + 原子激活
        // 先读锁检查 pending 是否为空，空则无需写锁，避免不必要地阻塞 on_output()
        // 非空时升级为写锁，保证排空和激活之间不会有新事件进入 pending
        {
            let need_drain = {
                let subscribers = self.subscribers.read().await;
                match subscribers.get(client_id) {
                    Some(sub) => !sub.pending.read().await.is_empty(),
                    None => false,
                }
            };

            if need_drain {
                let subscribers = self.subscribers.write().await;
                if let Some(sub) = subscribers.get(client_id) {
                    // pending 全部为快照后事件（见 drain_pending 注释），无重叠无需跳过
                    sub.drain_pending().await;

                    // 读取最新 max_offset，此时 on_output 被写锁阻塞，max_offset 不会继续增长
                    let current_max = self.output_queue.read().await.max_offset();
                    sub.activate(current_max);
                }
            } else {
                // pending 为空，只需读锁激活
                let subscribers = self.subscribers.read().await;
                if let Some(sub) = subscribers.get(client_id) {
                    let current_max = self.output_queue.read().await.max_offset();
                    sub.activate(current_max);
                }
            }
        }

        tracing::info!(
            client_id = %client_id,
            session_id = %self.session_id,
            history_bytes = history_bytes,
            from_offset = ?from_offset,
            "[SessionOutputManager] Client subscribed"
        );

        response
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

    /// 取消订阅
    pub async fn unsubscribe(&self, client_id: &str) {
        if self.subscribers.write().await.remove(client_id).is_some() {
            tracing::info!(
                "[SessionOutputManager] Client {} unsubscribed from session {}",
                client_id,
                self.session_id
            );
        }
    }

    pub async fn is_subscribed(&self, client_id: &str) -> bool {
        self.subscribers.read().await.contains_key(client_id)
    }

    pub async fn active_subscriber_count(&self) -> usize {
        self.subscribers.read().await.values().filter(|s| s.is_active()).count()
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
        if self.sessions.write().await.remove(session_id).is_some() {
            tracing::info!(session_id = %session_id, "[GlobalOutputManager] Session unregistered");
        }
    }

    pub async fn has_session(&self, session_id: &str) -> bool {
        self.sessions.read().await.contains_key(session_id)
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

    /// 背压判定（PtyReader 阻塞读线程同步调用，非阻塞零锁）：会话存在且
    /// 未 ack 超额 → 返回 true（暂停读）。try_read 拿不到读锁（写入者持锁
    /// 的瞬态窗口）或会话不存在时保守返回 false（不暂停）
    pub fn should_pause(&self, session_id: &str) -> bool {
        if let Ok(sessions) = self.sessions.try_read() {
            if let Some(manager) = sessions.get(session_id) {
                return manager.should_pause();
            }
        }
        false
    }

    /// 客户端 ack（背压反馈环 Rust 侧入口）：推进会话未 ack 记账，释放
    /// `acked_offset`（= 已渲染区间末端）及之前的输出字节；会话不存在时忽略
    ///
    /// 背压水位按会话整体记账（每产出事件 +bytes，不区分订阅者），因此任何
    /// 已认证订阅端的渲染确认都应推进记账。此前实现有正统门控（仅 current
    /// canonical 的 ack 有效），造成「桌面启动会话、移动端观看」等正统端不在
    /// 消费路径的场景死锁：观看端 ack 被丢弃 → unacked 超高位水（64KB）→
    /// 会话 PTY 读整体暂停 → 输出停滞且无法恢复（仅 ack/revert 能降水位）——
    /// 即移动端「显示一点就卡住」根因。unacked 是共享流量水位而非尺寸裁决
    /// 依据，门控「非正统渲染格式不匹配、吞吐不代表权威消费速度」的顾虑不
    /// 成立（订阅端收到的都是同一 PTY 字节流，消费确认即释放）；多端并发时
    /// 慢端 ack 只落后不加速，快端 ack 照常推进，无拖垮风险。
    /// 自 2.1.x 起放开：非正统端 ack 同样推进记账（`_source` 保留签名，供日志/审计）
    pub async fn ack(&self, session_id: &str, acked_offset: u64, _source: RendererSource) {
        let sessions = self.sessions.read().await;
        if let Some(manager) = sessions.get(session_id) {
            manager.on_ack(acked_offset);
            tracing::trace!(
                session_id,
                acked_offset,
                unacked_bytes = manager.unacked_bytes.load(Ordering::SeqCst),
                "output ack applied"
            );
        } else {
            tracing::debug!(session_id, acked_offset, "ack for unknown session ignored");
        }
    }

    /// 订阅会话输出（TB v3 快照协议：按 from_offset 字节锚点截取回放 + HistoryEnd
    /// 标记；from_offset = None 时全量回放，兼容旧客户端）
    pub async fn subscribe(
        &self,
        session_id: &str,
        client_id: &str,
        ws_sender: mpsc::Sender<OutputFrame>,
        response_tx: Option<tokio::sync::oneshot::Sender<SubscribeResponse>>,
        from_offset: Option<u64>,
    ) -> Option<SubscribeResponse> {
        let sessions = self.sessions.read().await;
        if let Some(manager) = sessions.get(session_id) {
            Some(manager.subscribe(client_id, ws_sender, response_tx, from_offset).await)
        } else {
            tracing::warn!(session_id = %session_id, "[GlobalOutputManager] Session not found for subscribe");
            None
        }
    }

    /// 截取驻留历史字节区间（HTTP 一次性历史；会话不存在 → None）
    pub async fn snapshot_bytes(
        &self,
        session_id: &str,
        from: u64,
    ) -> Option<(Vec<u8>, u64, u64, u64)> {
        let sessions = self.sessions.read().await;
        if let Some(manager) = sessions.get(session_id) {
            Some(manager.snapshot_bytes(from).await)
        } else {
            None
        }
    }

    /// 取消订阅
    pub async fn unsubscribe(&self, session_id: &str, client_id: &str) -> bool {
        let sessions = self.sessions.read().await;
        if let Some(manager) = sessions.get(session_id) {
            manager.unsubscribe(client_id).await;
            true
        } else {
            false
        }
    }

    /// 取消某客户端在所有会话中的订阅（客户端断开时调用）
    pub async fn unsubscribe_all_for_client(&self, client_id: &str) {
        let sessions = self.sessions.read().await;
        for (session_id, manager) in sessions.iter() {
            manager.unsubscribe(client_id).await;
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

    /// 数据为固定 4 字节 "test"，start_offset 直接传入（on_output 路径会重新分配）
    fn make_event(start_offset: u64) -> OutputEvent {
        OutputEvent::new(
            "test".to_string(),
            b"test".to_vec(),
            start_offset,
            Utc::now().timestamp_millis(),
            false,
        )
    }

    /// 从订阅通道收集全部 OutputFrame，直到通道关闭（返回 None）
    async fn collect_frames(mut rx: mpsc::Receiver<OutputFrame>) -> Vec<OutputFrame> {
        let mut frames = Vec::new();
        while let Some(frame) = rx.recv().await {
            frames.push(frame);
        }
        frames
    }

    /// 解包 Output 帧的 start_offset（HistoryEnd 视为断言失败）
    fn output_start(frame: &OutputFrame) -> u64 {
        match frame {
            OutputFrame::Output(e) => e.start_offset,
            _ => panic!("expected Output frame, got HistoryEnd"),
        }
    }

    // ==================== 字节块队列 ====================

    /// push 后快照整段返回全部字节区间（FIFO 序，start_offset 连续铺满）
    #[test]
    fn test_push_and_snapshot_full() {
        let mut queue = UnifiedOutputQueue::with_limits(u64::MAX, 100);

        for i in 0..5 {
            queue.push(make_event(i));
        }

        let (events, _, snapshot_offset, history_bytes) = queue.snapshot_from(None);
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].start_offset, 0);
        assert_eq!(events[4].start_offset, 16);
        assert_eq!(events[4].end_offset(), 20);
        assert_eq!(snapshot_offset, 20);
        assert_eq!(history_bytes, 20);
        // 连续不变量：区间按序铺满，无重无缺
        for w in events.windows(2) {
            assert_eq!(w[1].start_offset, w[0].end_offset());
        }
    }

    /// 条目上限（max_chunks）淘汰推进 min_offset
    #[test]
    fn test_chunk_cap_updates_min_offset() {
        let mut queue = UnifiedOutputQueue::with_limits(u64::MAX, 3);

        for i in 0..5 {
            queue.push(make_event(i));
        }

        assert_eq!(queue.min_offset(), 8);
        assert_eq!(queue.max_offset(), 20);
        assert_eq!(queue.len(), 3);

        let (events, _, _, _) = queue.snapshot_from(None);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].start_offset, 8);
    }

    /// 字节上限超限时淘汰最旧块（min_offset 推进到新队首）
    #[test]
    fn test_max_bytes_limit_evicts_oldest() {
        let mut queue = UnifiedOutputQueue::with_limits(10, 100);
        queue.push(make_event(0));
        queue.push(make_event(1));
        queue.push(make_event(2));

        // 3×4B = 12B > 10B → 淘汰最旧 1 块 → 剩 8B
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.min_offset(), 4);
        assert_eq!(queue.max_offset(), 12);
        assert_eq!(queue.history_bytes(), 8);
    }

    /// push 推进 max_offset（产出端游标 = 全量累计字节）
    #[test]
    fn test_push_advances_max_offset() {
        let mut queue = UnifiedOutputQueue::with_limits(u64::MAX, 100);

        queue.push(make_event(0));
        queue.push(make_event(1));
        queue.push(make_event(2));

        assert_eq!(queue.max_offset(), 12);
        assert_eq!(queue.len(), 3);
    }

    /// 单块超过字节上限时仍保留（不能丢弃刚 push 的块；min_offset 为该块起点）
    #[test]
    fn test_max_bytes_single_chunk_exceeds_limit() {
        let mut queue = UnifiedOutputQueue::with_limits(2, 100);
        queue.push(make_event(0));
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.total_bytes, 4);
        assert_eq!(queue.min_offset(), 0);
        assert_eq!(queue.max_offset(), 4);
    }

    /// from_offset 快照：chunk 级跳过 + 半块 slice 起播
    #[test]
    fn test_snapshot_from_offset_slices_half_chunk() {
        let mut queue = UnifiedOutputQueue::with_limits(u64::MAX, 100);
        // 三个 4 字节块：区间 [0,4) [4,8) [8,12)
        queue.push(make_event(0));
        queue.push(make_event(1));
        queue.push(make_event(2));

        // 锚点落在块边界：自 [4,12) 起播
        let (events, min, snapshot, bytes) = queue.snapshot_from(Some(4));
        assert_eq!(min, 0);
        assert_eq!(snapshot, 12);
        assert_eq!(bytes, 12);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].start_offset, 4);
        assert_eq!(events[0].data, b"test");
        assert_eq!(events[1].start_offset, 8);

        // 锚点落在块内：半块切片（[6,12) = "st" + "test"）
        let (events, _, _, _) = queue.snapshot_from(Some(6));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].start_offset, 6);
        assert_eq!(events[0].data, b"st");
        assert_eq!(events[1].start_offset, 8);
        assert_eq!(events[1].data, b"test");
        // 拼接后区间连续
        assert_eq!(events[0].end_offset(), events[1].start_offset);

        // 锚点超出驻留范围：空快照
        let (events, _, _, _) = queue.snapshot_from(Some(100));
        assert!(events.is_empty());
    }

    /// range(from, to)：HTTP 一次性历史的字节区间截取（含半块裁头/截尾）
    #[test]
    fn test_range_bytes() {
        let mut queue = UnifiedOutputQueue::with_limits(u64::MAX, 100);
        queue.push(make_event(0)); // [0,4) "test"
        queue.push(make_event(1)); // [4,8)
        queue.push(make_event(2)); // [8,12)

        assert_eq!(queue.range(0, 12), b"testtesttest");
        assert_eq!(queue.range(5, 9), b"estt".to_vec()); // [5,9)：块1尾 3 字节 + 块2头 1 字节
        assert_eq!(queue.range(2, 6), b"stte".to_vec());
        assert_eq!(queue.range(6, 4), b"".to_vec()); // from >= to → 空
        assert_eq!(queue.range(100, 200), b"".to_vec());
    }

    // ==================== 快照协议订阅（TB v3 字节语义） ====================

    /// 订阅帧流严格顺序：[历史 Output × N] → [HistoryEnd] → [实时 Output]
    #[tokio::test]
    async fn test_subscribe_history_then_marker_then_live() {
        let manager = SessionOutputManager::new("test-session");

        for i in 0..3 {
            manager.output_queue.write().await.push(make_event(i));
        }

        let (tx, mut rx) = mpsc::channel(100);
        let response = manager.subscribe("client-1", tx, None, None).await;
        assert_eq!(response.min_offset, 0);
        assert_eq!(response.snapshot_offset, 12);
        assert_eq!(response.history_bytes, 12);

        // 历史 3 帧（start_offset 0/4/8）
        for expect in [0u64, 4, 8] {
            let frame = rx.recv().await.unwrap();
            assert_eq!(output_start(&frame), expect);
        }
        // HistoryEnd 标记（字节三件套）
        let marker = rx.recv().await.unwrap();
        match marker {
            OutputFrame::HistoryEnd {
                snapshot_offset,
                min_offset,
                history_bytes,
            } => {
                assert_eq!(snapshot_offset, 12);
                assert_eq!(min_offset, 0);
                assert_eq!(history_bytes, 12);
            }
            _ => panic!("expected HistoryEnd after history"),
        }
        // 实时帧在标记之后（on_output 分配 start_offset = 12）
        manager.on_output(make_event(3)).await;
        let live = rx.recv().await.unwrap();
        assert_eq!(output_start(&live), 12);
    }

    /// 空队列订阅：无历史帧，直接 [HistoryEnd]
    #[tokio::test]
    async fn test_empty_history_subscribe() {
        let manager = SessionOutputManager::new("test-session");

        let (tx, mut rx) = mpsc::channel(100);
        let response = manager.subscribe("client-1", tx, None, None).await;
        assert_eq!(response.history_bytes, 0);
        assert_eq!(response.snapshot_offset, 0);

        let marker = rx.recv().await.unwrap();
        match marker {
            OutputFrame::HistoryEnd {
                snapshot_offset,
                history_bytes,
                ..
            } => {
                assert_eq!(snapshot_offset, 0);
                assert_eq!(history_bytes, 0);
            }
            _ => panic!("expected HistoryEnd only"),
        }
        // 无更多帧
        assert!(rx.try_recv().is_err());
    }

    /// from_offset 订阅：客户端带字节游标重订阅（重连自愈路径），
    /// 历史从游标处起播，已渲染区间不再重发
    #[tokio::test]
    async fn test_subscribe_from_offset_skips_rendered() {
        let manager = SessionOutputManager::new("test-session");

        for i in 0..3 {
            manager.output_queue.write().await.push(make_event(i));
        }

        let (tx, mut rx) = mpsc::channel(100);
        // 客户端已渲染到 6（半块内）→ 服务端从 6 起播
        let response = manager.subscribe("client-1", tx, None, Some(6)).await;
        assert_eq!(response.min_offset, 0);
        assert_eq!(response.snapshot_offset, 12);

        // 历史从 6 起：半块 "st"（[6,8)）+ 完整块 "test"（[8,12)）
        let f1 = rx.recv().await.unwrap();
        assert_eq!(output_start(&f1), 6);
        let f2 = rx.recv().await.unwrap();
        assert_eq!(output_start(&f2), 8);
        let marker = rx.recv().await.unwrap();
        assert!(matches!(marker, OutputFrame::HistoryEnd { .. }));

        // 实时续流：从 snapshot_offset=12 起，无缝衔接
        manager.on_output(make_event(9)).await;
        let live = rx.recv().await.unwrap();
        assert_eq!(output_start(&live), 12);
    }

    /// 淘汰后（min_offset > 0）订阅：覆盖全部现存字节（长期会话重订阅）
    #[tokio::test]
    async fn test_subscribe_full_snapshot_after_eviction() {
        let manager = SessionOutputManager::new("test-session");

        // 换小上限队列：push 3 条 → [0,4) 淘汰，min_offset=4
        *manager.output_queue.write().await = UnifiedOutputQueue::with_limits(10, 100);
        manager.output_queue.write().await.push(make_event(0));
        manager.output_queue.write().await.push(make_event(1));
        manager.output_queue.write().await.push(make_event(2));

        let (tx, mut rx) = mpsc::channel(100);
        let response = manager.subscribe("client-1", tx, None, None).await;
        assert_eq!(response.min_offset, 4);
        assert_eq!(response.snapshot_offset, 12);

        // 全量现存（[4,12) 按 FIFO，start_offset 4/8）
        let f1 = rx.recv().await.unwrap();
        assert_eq!(output_start(&f1), 4);
        let f2 = rx.recv().await.unwrap();
        assert_eq!(output_start(&f2), 8);
        let marker = rx.recv().await.unwrap();
        assert!(matches!(marker, OutputFrame::HistoryEnd { .. }));
    }

    /// 占位期竞态：subscribe 历史发送被背压挂起期间 on_output 产生的事件
    /// 全部进入 pending，排空后严格跟在 HistoryEnd 之后——无重无漏
    #[tokio::test]
    async fn test_placeholder_race_no_dup_no_gap() {
        let manager = Arc::new(SessionOutputManager::new("test-session"));

        for i in 0..3 {
            manager.output_queue.write().await.push(make_event(i));
        }

        // 容量 1 + 不消费：subscribe 发送第 1 帧后即被背压挂起，历史发送窗口被拉长
        let (tx, rx) = mpsc::channel(1);

        // subscribe 后台执行：占位 → 快照(3) → 发历史（第 1 帧占满缓冲后挂起）
        let sub_handle = tokio::spawn({
            let manager = manager.clone();
            async move { manager.subscribe("client-race", tx, None, None).await }
        });

        // 让出调度，等待 subscribe 已插入占位 subscriber（历史第一帧占满通道挂起）
        for _ in 0..100 {
            tokio::task::yield_now().await;
            if manager.is_subscribed("client-race").await {
                break;
            }
        }
        assert!(
            manager.is_subscribed("client-race").await,
            "subscribe must have inserted placeholder"
        );
        // 此时通道已被第 1 帧占满，subscribe 必然挂起在背压上（未激活）

        // 占位期（未激活）on_output 产生新事件 → 进入 pending，不入历史
        manager.on_output(make_event(3)).await;
        manager.on_output(make_event(4)).await;

        // 现在才启动消费，释放背压：subscribe 完成历史发送 + pending 排空 + 激活
        let collect_handle = tokio::spawn(collect_frames(rx));

        let response = sub_handle.await.unwrap();
        assert_eq!(response.snapshot_offset, 12);
        assert_eq!(response.history_bytes, 12);

        // 关闭通道（unsubscribe 释放订阅者 send_queue），让收集任务退出
        manager.unsubscribe("client-race").await;
        let frames = tokio::time::timeout(std::time::Duration::from_secs(2), collect_handle)
            .await
            .expect("collector finished within timeout")
            .unwrap();

        // 严格顺序：[历史 0,4,8] → HistoryEnd → [pending 12,16]，无重无漏
        let offsets: Vec<u64> = frames
            .iter()
            .map(|f| match f {
                OutputFrame::Output(e) => e.start_offset,
                OutputFrame::HistoryEnd { .. } => u64::MAX, // 标记占位
            })
            .collect();
        assert_eq!(offsets, vec![0, 4, 8, u64::MAX, 12, 16]);
        // HistoryEnd 位于历史之后、pending 之前
        assert!(matches!(frames[3], OutputFrame::HistoryEnd { .. }));
    }

    /// 基础订阅 + 实时输出：响应携带字节元数据，帧流顺序被正确维持
    #[tokio::test]
    async fn test_subscribe_and_on_output() {
        let manager = SessionOutputManager::new("test-session");

        let (tx, mut rx) = mpsc::channel(100);

        manager.output_queue.write().await.push(make_event(0));
        manager.output_queue.write().await.push(make_event(1));

        let response = manager.subscribe("client-1", tx, None, None).await;
        assert_eq!(response.min_offset, 0);
        assert_eq!(response.snapshot_offset, 8);
        assert_eq!(response.history_bytes, 8);

        let f1 = rx.recv().await.unwrap();
        assert_eq!(output_start(&f1), 0);
        let f2 = rx.recv().await.unwrap();
        assert_eq!(output_start(&f2), 4);
        let marker = rx.recv().await.unwrap();
        assert!(matches!(marker, OutputFrame::HistoryEnd { .. }));

        manager.on_output(make_event(2)).await;
        let f3 = rx.recv().await.unwrap();
        assert_eq!(output_start(&f3), 8);
    }

    /// 多订阅者：同一事件广播给所有订阅者
    ///
    /// 空历史订阅 → 每个订阅者先收 HistoryEnd，再收实时帧
    #[tokio::test]
    async fn test_multiple_subscribers() {
        let manager = SessionOutputManager::new("test-session");

        let (tx1, mut rx1) = mpsc::channel(100);
        let (tx2, mut rx2) = mpsc::channel(100);

        manager.subscribe("client-1", tx1, None, None).await;
        manager.subscribe("client-2", tx2, None, None).await;

        // 两个订阅者各自先排空空历史（HistoryEnd）
        assert!(matches!(rx1.recv().await.unwrap(), OutputFrame::HistoryEnd { .. }));
        assert!(matches!(rx2.recv().await.unwrap(), OutputFrame::HistoryEnd { .. }));

        manager.on_output(make_event(0)).await;

        let f1 = rx1.recv().await.unwrap();
        let f2 = rx2.recv().await.unwrap();
        // on_output 按会话连续分配：空队列首块 start_offset = 0
        assert_eq!(output_start(&f1), 0);
        assert_eq!(output_start(&f2), 0);
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let manager = SessionOutputManager::new("test-session");

        let (tx, _rx) = mpsc::channel(100);
        manager.subscribe("client-1", tx, None, None).await;

        manager.unsubscribe("client-1").await;

        assert!(!manager.is_subscribed("client-1").await);
    }

    fn make_session_event(session_id: &str, start_offset: u64) -> OutputEvent {
        OutputEvent {
            session_id: session_id.to_string(),
            data: b"test".to_vec(),
            start_offset,
            timestamp: Utc::now().timestamp_millis(),
            is_waiting: false,
        }
    }

    #[tokio::test]
    async fn test_register_and_on_output() {
        let manager = GlobalOutputManager::new();

        manager.register_session("session-1").await;

        let (tx, mut rx) = mpsc::channel(100);
        manager.subscribe("session-1", "client-1", tx, None, None).await;

        // 空历史：先收 HistoryEnd
        assert!(matches!(rx.recv().await.unwrap(), OutputFrame::HistoryEnd { .. }));

        manager.on_output(make_session_event("session-1", 0)).await;

        let frame = rx.recv().await.unwrap();
        let OutputFrame::Output(e) = frame else {
            panic!("expected output frame");
        };
        assert_eq!(e.session_id, "session-1");
        // on_output 按会话连续分配：本会话首个事件 start_offset = 0，end = 4
        assert_eq!(e.start_offset, 0);
        assert_eq!(e.end_offset(), 4);
    }

    #[tokio::test]
    async fn test_backpressure_accounting_pause_and_resume() {
        let manager = GlobalOutputManager::new();
        manager.register_session("session-bp").await;

        let (tx, mut rx) = mpsc::channel(1000);
        manager.subscribe("session-bp", "client-ack", tx, None, None).await;
        let _ = rx.recv().await.unwrap(); // 空历史 HistoryEnd

        // 每个事件 8KB；16 事件 = 128KB > 高位水 64KB → 暂停读
        let big = vec![b'x'; 8 * 1024];
        for _ in 0..16 {
            manager
                .on_output(OutputEvent {
                    session_id: "session-bp".to_string(),
                    data: big.clone(),
                    start_offset: 0,
                    timestamp: Utc::now().timestamp_millis(),
                    is_waiting: false,
                })
                .await;
        }
        // 16×8KB = 128KB > 64KB 高位水 → 暂停读
        assert!(manager.should_pause("session-bp"), "burst should pause");

        // 收集事件 start_offset（订阅通道 16 帧；on_output 按会话连续分配 0..122880）
        let mut offsets = Vec::new();
        while let Ok(OutputFrame::Output(e)) = rx.try_recv() {
            offsets.push(e.start_offset);
        }
        assert_eq!(offsets.len(), 16);
        assert_eq!(offsets[0], 0);
        assert_eq!(offsets[15], 15 * 8192);

        // ack 到事件 14 末端（14×8KB = 114688）：剩余 2×8KB = 16KB，仍在
        // (8KB, 64KB] 滞回区间 → 保持暂停
        manager.ack("session-bp", 14 * 8192, RendererSource::Desktop).await;
        assert!(
            manager.should_pause("session-bp"),
            "hysteresis should hold pause while unacked in (low, high]"
        );

        // ack 到事件 15 末端（15×8KB = 122880）：剩余 1×8KB = 8KB ≤ 低水位 → 恢复
        manager.ack("session-bp", 15 * 8192, RendererSource::Desktop).await;
        assert!(
            !manager.should_pause("session-bp"),
            "ack below low watermark should resume"
        );

        // 一次性 ack 超限 offset：全部释放，unacked 不为负
        manager.ack("session-bp", 999_999_999, RendererSource::Desktop).await;
        assert!(!manager.should_pause("session-bp"));

        // 会话不存在：ack 静默忽略，不 panic
        manager.ack("no-such-session", 5, RendererSource::Desktop).await;
    }

    /// 非正统端（移动端旁观）ack 同样推进记账：背压水位按会话整体消费，任何
    /// 订阅端的渲染确认都释放未 ack 字节（2.1.x 修复「桌面启动会话/移动端观看」
    /// 死锁的回归护栏——此前仅正统端 ack 有效，观看端被丢弃导致 unacked 永久
    /// 高位 → PTY 读整体暂停 → 输出卡死）
    #[tokio::test]
    async fn test_ack_from_mobile_observer_releases_backpressure() {
        let manager = GlobalOutputManager::new();
        manager.register_session("session-nc").await;

        let (tx, mut rx) = mpsc::channel(1000);
        manager.subscribe("session-nc", "client-mobile", tx, None, None).await;
        let _ = rx.recv().await.unwrap(); // 空历史 HistoryEnd

        // 每事件 8KB；16 事件 = 128KB > 64KB 高位水 → 暂停读
        let big = vec![b'x'; 8 * 1024];
        for _ in 0..16 {
            manager
                .on_output(OutputEvent {
                    session_id: "session-nc".to_string(),
                    data: big.clone(),
                    start_offset: 0,
                    timestamp: Utc::now().timestamp_millis(),
                    is_waiting: false,
                })
                .await;
        }
        assert!(manager.should_pause("session-nc"), "burst should pause");

        // 移动端身份 ack：与正统端等效推进（无需 AppContext 门控）→ 降回低水位恢复
        manager
            .ack(
                "session-nc",
                999_999_999,
                RendererSource::Mobile {
                    device_name: "observer-phone".to_string(),
                },
            )
            .await;
        assert!(
            !manager.should_pause("session-nc"),
            "non-canonical (mobile) ack must release watermark"
        );
    }

    #[tokio::test]
    async fn test_backpressure_fifo_cap_freeze_and_drain() {
        let manager = GlobalOutputManager::new();
        manager.register_session("session-bp-cap").await;

        let tiny = vec![b'c'; 1];
        // 9000 小事件 > FIFO 容量(8192)：冻结记账，unacked 不再增长
        for _ in 0..9000 {
            manager
                .on_output(OutputEvent {
                    session_id: "session-bp-cap".to_string(),
                    data: tiny.clone(),
                    start_offset: 0,
                    timestamp: Utc::now().timestamp_millis(),
                    is_waiting: false,
                })
                .await;
        }
        // FIFO 满冻结：unacked = 8192B，远小于水位 → 不暂停
        assert!(!manager.should_pause("session-bp-cap"));

        // 超限 ack：FIFO 内全部弹出，unacked 归零，不因冻结期欠记而变负
        manager.ack("session-bp-cap", 999_999_999, RendererSource::Desktop).await;
        assert!(!manager.should_pause("session-bp-cap"));
        // 重复 ack：空 FIFO 无匹配，no-op，不越界
        manager.ack("session-bp-cap", 999_999_999, RendererSource::Desktop).await;
    }

    #[tokio::test]
    async fn test_multiple_sessions() {
        let manager = GlobalOutputManager::new();

        manager.register_session("session-1").await;
        manager.register_session("session-2").await;

        let (tx1, mut rx1) = mpsc::channel(100);
        let (tx2, mut rx2) = mpsc::channel(100);

        manager.subscribe("session-1", "client-1", tx1, None, None).await;
        manager.subscribe("session-2", "client-2", tx2, None, None).await;

        manager.on_output(make_session_event("session-1", 0)).await;
        manager.on_output(make_session_event("session-2", 0)).await;

        // 各自订阅先排空历史帧（空历史 → HistoryEnd），再接收实时输出
        let _ = rx1.recv().await.unwrap();
        let _ = rx2.recv().await.unwrap();

        let f1 = rx1.recv().await.unwrap();
        let OutputFrame::Output(e1) = f1 else {
            panic!("expected output frame");
        };
        assert_eq!(e1.session_id, "session-1");

        let f2 = rx2.recv().await.unwrap();
        let OutputFrame::Output(e2) = f2 else {
            panic!("expected output frame");
        };
        assert_eq!(e2.session_id, "session-2");
    }

    #[tokio::test]
    async fn test_unregister_session() {
        let manager = GlobalOutputManager::new();

        manager.register_session("session-1").await;
        manager.unregister_session("session-1").await;

        assert!(!manager.has_session("session-1").await);

        manager.on_output(make_session_event("session-1", 0)).await;
    }

    /// 实时流连续性契约：send_queue 满时 on_output 有界等待发送（背压），
    /// 事件零丢失——满足「环形可丢最旧、实时不允许中断」
    #[tokio::test]
    async fn test_send_queue_full_waits_no_drop() {
        let manager = GlobalOutputManager::new();
        manager.register_session("session-wait").await;

        // 容量 1：第 2 个事件起 try_send 必 Full → 内部 await send 等待
        let (tx, mut rx) = mpsc::channel(1);
        manager.subscribe("session-wait", "client-wait", tx, None, None).await;
        let _ = rx.recv().await.unwrap(); // 空历史 HistoryEnd

        // 并发消费者：腾出通道空间让 on_output 的等待发送完成
        let consumer = tokio::spawn(async move {
            let mut got = 0;
            while got < 5 {
                let frame = tokio::time::timeout(Duration::from_millis(500), rx.recv()).await;
                if let Ok(Some(OutputFrame::Output(_))) = frame {
                    got += 1;
                }
            }
            got
        });

        for _ in 0..5 {
            manager.on_output(make_session_event("session-wait", 0)).await;
        }

        let got = consumer.await.unwrap();
        assert_eq!(got, 5, "队列满时等待发送：事件必须零丢失到达");

        // dropped 计数必须为 0（无任何丢弃路径命中）
        {
            let sessions = manager.sessions.read().await;
            let m = sessions.get("session-wait").unwrap();
            let subs = m.subscribers.read().await;
            let sub = subs.get("client-wait").unwrap();
            assert_eq!(
                sub.dropped.load(Ordering::SeqCst),
                0,
                "有界等待场景不得丢弃任何事件"
            );
        }
    }

    /// 发送停滞超时兜底：客户端死亡/悬挂时丢弃事件并冲正未 ack 记账
    /// （丢的事件永远不会有 ack，不冲正 → unacked 虚高 → 背压永久暂停饿死）
    #[tokio::test(start_paused = true)]
    async fn test_send_stall_timeout_drops_with_revert() {
        // Arc 包装以支持并发任务（GlobalOutputManager 不可 Clone）
        let manager = Arc::new(GlobalOutputManager::new());
        manager.register_session("session-stall").await;

        let (tx, mut rx) = mpsc::channel(1);
        manager.subscribe("session-stall", "client-stall", tx, None, None).await;
        let _ = rx.recv().await.unwrap(); // 空历史 HistoryEnd

        // 事件 1 入队成功（占满容量 1），之后不再消费 → 事件 2 等待超时
        manager.on_output(make_session_event("session-stall", 0)).await;

        let stall = {
            let manager = manager.clone();
            tokio::spawn(async move { manager.on_output(make_session_event("session-stall", 0)).await })
        };
        // 快进虚拟时钟越过 SEND_BACKPRESSURE_TIMEOUT(2s) → 超时丢弃
        tokio::time::sleep(Duration::from_millis(10)).await;
        tokio::time::advance(Duration::from_secs(3)).await;
        stall.await.unwrap();

        let sessions = manager.sessions.read().await;
        let m = sessions.get("session-stall").unwrap().clone();
        drop(sessions);
        {
            let subscribers = m.subscribers.read().await;
            let sub = subscribers.get("client-stall").unwrap();
            assert_eq!(sub.dropped.load(Ordering::SeqCst), 1, "超时应丢弃一次");
        }
        // 冲正：被丢弃事件的字节从记账中移除（事件 1 的 4B 仍在）
        // FIFO 与 unacked 同步：事件 2 已冲正，此时未 ack = 事件 1 的 4B
        {
            let fifo = m.unacked_fifo.lock().unwrap();
            assert_eq!(fifo.len(), 1, "FIFO 仅保留事件 1");
            assert_eq!(fifo[0], (4, 4), "事件 1 end_offset=4, 4 字节");
        }
        assert_eq!(m.unacked_bytes.load(Ordering::SeqCst), 4, "冲正后 unacked 回落");
        // 未 ack 已低于水位 → 不暂停（冲正防止背压永久暂停）
        assert!(!m.should_pause());
    }

    /// inactive 占位期 pending 溢出：丢弃并冲正记账（缺口由客户端重订阅
    /// 按游标重播自愈，但不允许 unacked 虚高饿死背压）
    #[tokio::test]
    async fn test_pending_overflow_drops_with_revert() {
        let manager = GlobalOutputManager::new();
        manager.register_session("session-pending-overflow").await;
        let sessions = manager.sessions.read().await;
        let m = sessions.get("session-pending-overflow").unwrap().clone();
        drop(sessions);

        // 直接构造 inactive 订阅者并把 pending 填到容量上限（模拟占位期漫链路）
        let (tx, _rx) = mpsc::channel(1);
        let sub = SubscriberState::new("client-pending".to_string(), tx);
        {
            let mut pending = sub.pending.try_write().unwrap();
            for _ in 0..PENDING_EVENT_CAP {
                pending.push(make_event(0));
            }
        }
        m.subscribers.write().await.insert("client-pending".to_string(), sub);

        // 溢出事件（start_offset 由 on_output 分配）：丢弃 + 冲正
        m.on_output(make_event(0)).await;

        let subs = m.subscribers.read().await;
        let sub2 = subs.get("client-pending").unwrap();
        assert_eq!(sub2.dropped.load(Ordering::SeqCst), 1, "pending 溢出丢弃一次");
        // 全部未 ack 记账已被冲正 → unacked 为 0（不虚高）
        assert_eq!(m.unacked_bytes.load(Ordering::SeqCst), 0, "冲正后无虚高");
        let fifo = m.unacked_fifo.lock().unwrap();
        assert_eq!(fifo.len(), 0);
        assert!(!m.should_pause());
    }

    /// pending 缓冲消除订阅丢失窗口：subscribe 前后的事件零丢失
    #[tokio::test]
    async fn test_pending_covers_subscribe_gap() {
        let manager = SessionOutputManager::new("test-session");

        for i in 0..5 {
            manager.output_queue.write().await.push(make_event(i));
        }

        let (tx, mut rx) = mpsc::channel(100);

        let response = manager.subscribe("client-1", tx, None, None).await;
        assert_eq!(response.history_bytes, 20);

        // 历史 0/4/8/12/16 + HistoryEnd
        for expect in [0u64, 4, 8, 12, 16] {
            let f = rx.recv().await.unwrap();
            assert_eq!(output_start(&f), expect);
        }
        assert!(matches!(rx.recv().await.unwrap(), OutputFrame::HistoryEnd { .. }));

        // subscribe 完成后，on_output 正常接收
        manager.on_output(make_event(5)).await;
        let f = rx.recv().await.unwrap();
        assert_eq!(output_start(&f), 20);
    }

    /// 排空 pending 发送全部事件（无 skip：pending 全部为快照后事件）
    #[tokio::test]
    async fn test_drain_pending_sends_all_pending() {
        let (tx, mut rx) = mpsc::channel(100);
        let sub = SubscriberState::new("client-1".to_string(), tx);

        sub.pending.write().await.push(make_event(3));
        sub.pending.write().await.push(make_event(4));

        sub.drain_pending().await;

        let f1 = rx.recv().await.unwrap();
        assert_eq!(output_start(&f1), 3);
        let f2 = rx.recv().await.unwrap();
        assert_eq!(output_start(&f2), 4);
        assert!(rx.try_recv().is_err());
    }

    /// inactive 期间 on_output 缓存到 pending，激活后按顺序送达
    #[tokio::test]
    async fn test_on_output_caches_to_pending_when_inactive() {
        let manager = SessionOutputManager::new("test-session");

        for i in 0..3 {
            manager.output_queue.write().await.push(make_event(i));
        }

        let (tx, mut rx) = mpsc::channel(100);

        // 手动模拟：先占位（inactive），然后 on_output，再激活
        let subscriber = SubscriberState::new("client-1".to_string(), tx);
        manager
            .subscribers
            .write()
            .await
            .insert("client-1".to_string(), subscriber);

        // 模拟 subscribe 第二步：占位后先发历史快照（与 subscribe 实现一致），
        // 否则接收端只收到 pending [12,16]，收不到历史 [0,4,8]
        {
            let subs = manager.subscribers.read().await;
            let sub = subs.get("client-1").unwrap();
            for i in 0..3 {
                sub.send_queue
                    .send(OutputFrame::Output(make_event(i * 4)))
                    .await
                    .unwrap();
            }
        }

        // inactive 期间 on_output 应缓存到 pending（连续分配 12/16）
        manager.on_output(make_event(3)).await;
        manager.on_output(make_event(4)).await;

        // 验证 pending 中有 2 个事件
        {
            let subs = manager.subscribers.read().await;
            let sub = subs.get("client-1").unwrap();
            let pending = sub.pending.read().await;
            assert_eq!(pending.len(), 2);
            assert_eq!(pending[0].start_offset, 12);
            assert_eq!(pending[1].start_offset, 16);
        }

        // 排空 pending 并激活（持写锁，与 subscribe 实际逻辑一致）
        {
            let mut subs = manager.subscribers.write().await;
            let sub = subs.get("client-1").unwrap();
            let mut pending = sub.pending.write().await;
            for event in pending.drain(..) {
                sub.send_queue.send(OutputFrame::Output(event)).await.unwrap();
            }
            drop(pending);
            let current_max = manager.output_queue.read().await.max_offset();
            sub.activate(current_max);
        }

        // 收到历史 + pending 事件（此测试手动模拟 subscribe 两步，
        // 不发送 HistoryEnd 标记；直接验证后续 on_output 正常送达）
        let mut seen = Vec::new();
        while let Ok(OutputFrame::Output(e)) = rx.try_recv() {
            seen.push(e.start_offset);
        }
        assert_eq!(seen, vec![0, 4, 8, 12, 16]);
    }

    /// activate 使用最新 max_offset：订阅结束前 on_output 推入的真实值
    #[tokio::test]
    async fn test_activate_uses_current_max_offset() {
        let manager = SessionOutputManager::new("test-session");

        for i in 0..3 {
            manager.output_queue.write().await.push(make_event(i));
        }

        let (tx, mut rx) = mpsc::channel(100);

        let response = manager.subscribe("client-1", tx, None, None).await;
        assert_eq!(response.snapshot_offset, 12);

        // 验证 subscriber 的 sent_offset 是最新的
        {
            let subs = manager.subscribers.read().await;
            let sub = subs.get("client-1").unwrap();
            assert!(sub.is_active());
            assert_eq!(sub.sent_offset.load(Ordering::SeqCst), 12);
        }

        // 排空 subscribe 阶段的历史与标记，聚焦验证后续 on_output 无重复无丢失
        for expect in [0u64, 4, 8] {
            let f = rx.recv().await.unwrap();
            assert_eq!(output_start(&f), expect);
        }
        assert!(matches!(rx.recv().await.unwrap(), OutputFrame::HistoryEnd { .. }));

        manager.on_output(make_event(3)).await;
        let f = rx.recv().await.unwrap();
        assert_eq!(output_start(&f), 12);
    }

    /// snapshot_bytes：HTTP 一次性历史（按 from 字节锚点截取 + 元数据三件套）
    #[tokio::test]
    async fn test_snapshot_bytes_http() {
        let manager = GlobalOutputManager::new();
        manager.register_session("session-http").await;

        // 推送 3 个 4 字节事件 → [0,12)
        for i in 0..3 {
            manager
                .on_output(make_session_event("session-http", 0))
                .await;
        }

        let (data, min, snapshot, history_bytes) = manager
            .snapshot_bytes("session-http", 4)
            .await
            .expect("session exists");
        assert_eq!(min, 0);
        assert_eq!(snapshot, 12);
        assert_eq!(history_bytes, 12);
        assert_eq!(data, b"testtest");

        // from 旧于 min_offset：以 min_offset 为起点
        let (data, _, _, _) = manager
            .snapshot_bytes("session-http", 0)
            .await
            .unwrap();
        assert_eq!(data, b"testtesttest");

        // 会话不存在 → None
        assert!(manager.snapshot_bytes("no-such", 0).await.is_none());
    }

    /// 全局快照截取 + 淘汰联动：from 越过驻留头部后返回空
    #[tokio::test]
    async fn test_snapshot_bytes_after_eviction() {
        let manager = GlobalOutputManager::new();
        manager.register_session("session-http-evict").await;
        {
            let sessions = manager.sessions.read().await;
            let m = sessions.get("session-http-evict").unwrap();
            *m.output_queue.write().await = UnifiedOutputQueue::with_limits(6, 100);
        }
        for _ in 0..3 {
            manager.on_output(make_session_event("session-http-evict", 0)).await;
        }
        // 12B > 6B：while 淘汰至 6B 内（4B 块每次淘汰一块）→ 仅驻留 [8,12)
        let (data, min, snapshot, history_bytes) = manager
            .snapshot_bytes("session-http-evict", 0)
            .await
            .unwrap();
        assert_eq!(min, 8);
        assert_eq!(snapshot, 12);
        assert_eq!(history_bytes, 4);
        assert_eq!(data, b"test");
    }
}
