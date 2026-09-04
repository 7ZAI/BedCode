//! Session Output
//!
//! PTY 输出相关的组件：统一输出队列、会话输出管理、全局输出管理

use crate::session::RendererSource;
use crate::system::config::AppConfig;
use serde::{Deserialize, Serialize};
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
#[derive(Debug, Clone)]
pub enum OutputFrame {
    Output(OutputEvent),
    HistoryEnd {
        /// 订阅时刻队列最新序号（可能因后续 push 继续增长）
        snapshot_seq: u64,
        /// 队列中最早存续事件序号
        min_seq: u64,
        /// 历史事件数量
        history_count: usize,
    },
}

// ==================== Unified Output Queue ====================

/// 输出事件
///
/// `data` 存储原始字节数据，在发送到 WebSocket 时才进行 Base64 编码
/// 避免在缓冲合并时多次编解码
///
/// `index` 为会话级序号：写入路径（SessionOutputManager::on_output）按会话内
/// 单调连续分配（队列 max_seq + 1），实时流不含跨会话空洞；入队前的全局计数
///（next_output_index）仅作路由/日志唯一标记
#[derive(Debug, Clone)]
pub struct OutputEvent {
    pub session_id: String,
    pub data: Vec<u8>,
    pub index: u64,
    pub timestamp: i64,
    pub is_waiting: bool,
}

/// 用于 JSON 序列化的临时结构（包含 Base64 编码的数据）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputEventSerialized {
    pub session_id: String,
    pub data: String,
    pub index: u64,
    pub timestamp: i64,
    pub is_waiting: bool,
}

impl OutputEvent {
    pub fn new(session_id: String, data: Vec<u8>, index: u64, timestamp: i64, is_waiting: bool) -> Self {
        Self {
            session_id,
            data,
            index,
            timestamp,
            is_waiting,
        }
    }

    /// 编码为可序列化的结构（用于 WebSocket 发送）
    pub fn to_serialized(&self) -> OutputEventSerialized {
        OutputEventSerialized {
            session_id: self.session_id.clone(),
            data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &self.data),
            index: self.index,
            timestamp: self.timestamp,
            is_waiting: self.is_waiting,
        }
    }

    /// 获取 Base64 编码的数据
    pub fn data_base64(&self) -> String {
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &self.data)
    }
}

/// 统一输出队列（环形缓冲区）
///
/// 双重容量限制：
/// - `capacity`: 最大事件条数（条目级限制）
/// - `max_total_bytes`: 最大总字节数（内存级限制）
/// 任一限制超出时丢弃最旧事件，与前端 buffer 逻辑一致
///
/// 序号语义：写入路径按会话连续分配 index（on_output 取队列 max_seq + 1），
/// 实时流无跨会话空洞；队列层保持 index 无关（防御性容忍任意单调 index），
/// 不变量仅保证「push 序 == index 单调序，段内无重无缺」——
/// 历史回放恒为整体重播，min_seq/max_seq 仅作元数据供响应携带（见 get_events 注释）
pub struct UnifiedOutputQueue {
    buffer: std::collections::VecDeque<OutputEvent>,
    capacity: usize,
    max_total_bytes: u64,
    total_bytes: u64,
    max_seq: AtomicU64,
    min_seq: AtomicU64,
    total_produced: AtomicU64,
}

impl UnifiedOutputQueue {
    pub fn new(capacity: usize) -> Self {
        let config = AppConfig::global();
        Self::with_max_bytes(capacity, config.channels.global_queue_max_bytes)
    }

    /// 创建指定字节上限的队列
    pub fn with_max_bytes(capacity: usize, max_total_bytes: u64) -> Self {
        Self {
            buffer: std::collections::VecDeque::with_capacity(capacity),
            capacity,
            max_total_bytes,
            total_bytes: 0,
            max_seq: AtomicU64::new(0),
            min_seq: AtomicU64::new(0),
            total_produced: AtomicU64::new(0),
        }
    }

    pub fn max_seq(&self) -> u64 {
        self.max_seq.load(Ordering::SeqCst)
    }

    pub fn min_seq(&self) -> u64 {
        self.min_seq.load(Ordering::SeqCst)
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// 推入新事件，返回完整事件（供调用方转发给订阅者）
    ///
    /// 双重容量检查：条目数和总字节数任一超出时丢弃最旧事件。
    /// index 由 SessionOutputManager::on_output 按会话连续分配（max_seq + 1），
    /// push 只登记 max_seq/min_seq
    pub fn push(&mut self, event: OutputEvent) -> OutputEvent {
        self.max_seq.store(event.index, Ordering::SeqCst);
        self.total_produced.fetch_add(1, Ordering::SeqCst);

        let event_bytes = event.data.len() as u64;
        self.total_bytes += event_bytes;

        // 条目数或总字节数超出时，丢弃最旧事件直到满足限制
        while (self.buffer.len() >= self.capacity || self.total_bytes > self.max_total_bytes) && !self.buffer.is_empty()
        {
            if let Some(old) = self.buffer.pop_front() {
                self.total_bytes -= old.data.len() as u64;
                self.min_seq.store(old.index + 1, Ordering::SeqCst);
            }
        }

        self.buffer.push_back(event);
        self.buffer.back().unwrap().clone()
    }

    /// 获取缓冲内全部事件（FIFO 序，整段克隆，无裁剪/范围语义）
    ///
    /// 写入路径按会话连续分配 index（见 on_output），缓冲内默认无跨会话空洞；
    /// 队列层不依赖 index 来源、保持整段 FIFO 返回——历史回放恒为整体重播，
    /// min_seq/snapshot_seq 仅作元数据供响应携带
    pub fn get_events(&self) -> Vec<OutputEvent> {
        self.buffer.iter().cloned().collect()
    }
}

impl Default for UnifiedOutputQueue {
    fn default() -> Self {
        let config = AppConfig::global();
        Self::new(config.channels.global_queue_capacity)
    }
}

// ==================== Session Output Manager ====================

/// 订阅者状态
pub struct SubscriberState {
    pub client_id: String,
    /// 订阅是否活跃（历史发送完成后才标记为 true）
    pub active: AtomicBool,
    pub sent_seq: AtomicU64,
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

/// 背压水位：未 ack 字节超过该值 → 暂停该会话 PTY 读取（spec 04-06 渲染反馈环）
/// 取值须 > 前端 ack 阈值(64KB) + 渲染/写管线 in-flight 余量；1MB 保守兜底
const BACKPRESSURE_WATERMARK_BYTES: u64 = 1024 * 1024;
/// 未 ack 记账 FIFO 容量（事件数）：防 ack 停滞时无限增长；满则冻结记账，
/// unacked 保持近满态触发暂停（保守），ack 弹出后自动恢复精确记账
const UNACKED_FIFO_CAP: usize = 8192;

impl SubscriberState {
    pub fn new(client_id: String, send_queue: mpsc::Sender<OutputFrame>) -> Self {
        Self {
            client_id,
            active: AtomicBool::new(false),
            sent_seq: AtomicU64::new(0),
            send_queue,
            pending: RwLock::new(Vec::new()),
            dropped: AtomicU64::new(0),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn activate(&self, sent_seq: u64) {
        self.sent_seq.store(sent_seq, Ordering::SeqCst);
        self.active.store(true, Ordering::SeqCst);
    }

    /// 排空 pending 缓冲并发送为 Output 帧
    ///
    /// 占位期内 on_output() 缓存的事件均在快照（subscribe 持队列读锁收集历史）
    /// 之后 push，index 必然 > 快照时 snapshot_seq——历史与 pending 无重叠，
    /// 无需跳过（旧版按 index 去重是因快照与 pending 存在竞态窗口）
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

/// 订阅响应
#[derive(Debug, Clone)]
pub struct SubscribeResponse {
    /// 队列中最早存续事件序号（元数据，环形淘汰后推进）
    pub min_seq: u64,
    /// 订阅时刻的快照序号（= 当时队列 max_seq，历史边界元数据）
    pub snapshot_seq: u64,
    /// 历史事件数量
    pub history_count: usize,
}

/// 单个 PTY 会话的输出管理，包括输出队列和订阅者管理
pub struct SessionOutputManager {
    session_id: String,
    output_queue: Arc<RwLock<UnifiedOutputQueue>>,
    subscribers: RwLock<HashMap<String, SubscriberState>>,
    /// 背压记账：已产出未 ack 的字节数（渲染反馈环，前端 onWriteParsed 后
    /// 回发 ack；超水位 → 暂停该会话 PTY 读取）
    unacked_bytes: AtomicU64,
    /// 未 ack 事件 FIFO（index → bytes）：ack 按序弹出精减 unacked_bytes；
    /// std Mutex 仅作短临界区（无 await 保持），热路径成本低
    unacked_fifo: std::sync::Mutex<std::collections::VecDeque<(u64, u64)>>,
}

impl SessionOutputManager {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            output_queue: Arc::new(RwLock::new(UnifiedOutputQueue::default())),
            subscribers: RwLock::new(HashMap::new()),
            unacked_bytes: AtomicU64::new(0),
            unacked_fifo: std::sync::Mutex::new(std::collections::VecDeque::new()),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 处理新输出
    ///
    /// 先入队再用事件广播给订阅者；事件不携带偏移，
    /// 订阅者侧按订阅内连续流自洽（见 forward 层合成游标）
    ///
    /// 背压保护：同步 try_send 而非 await send——慢订阅者（移动端弱网，
    /// 8192 事件通道 + 有界合并 + 转发通道逐级排满）不能阻塞 on_output，
    /// 否则同会话所有订阅者（含桌面端本地 WS）输出同步冻结、PTY 读取
    /// 停摆。满时丢弃该事件：客户端重订阅全量重播整体回补，事件仍保留
    /// 在输出队列中
    pub async fn on_output(&self, event: OutputEvent) {
        // 序号按会话连续分配（队列 max_seq + 1），替代跨会话全局计数器（next_output_index）：
        // 消除「多会话并发 → 会话内实时帧 seq 带跨会话空洞 → 客户端 `seq > last_rendered + 1 →
        // 重订阅` 缺口检测被误触发 → 反复重订阅风暴」（桌面端 opencode 会话输入时后台日志
        // 反复「连/订阅/断」刷屏即此根因）。按会话连续后缺口检测只在真实丢帧
        // （背压 / 占位 pending 溢出）时命中，由重订阅 → 全量重播 → 按游标跳过去重自愈补回
        let mut event = event;
        {
            let mut queue = self.output_queue.write().await;
            event.index = queue.max_seq() + 1;
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
                fifo.push_back((event.index, event_bytes));
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
                                        "[SessionOutputManager] Subscriber {} send stalled >{}s, dropped event #{} (index={})",
                                        subscriber.client_id,
                                        SEND_BACKPRESSURE_TIMEOUT.as_secs(),
                                        n,
                                        event.index
                                    );
                                }
                                self.revert_unacked(event.index, event.data.len() as u64);
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
                // 会导致 unacked_bytes 虚高），缺口由客户端重订阅全量重播自愈
                //
                // 握手期间 drain_pending 持 pending 写锁是短暂窗口（毫秒级历史回放），
                // 退化为阻塞 write().await 等待持锁方释放——避免序号已分配但帧永久丢失
                // （try_write 失败时静默丢弃会让客户端 seq 缺口、触发不必要的重订阅）。
                // 该等待不持有 self.output_queue / subscribers 锁，不会与其他锁路径死锁
                let mut pending = subscriber.pending.write().await;
                if pending.len() >= PENDING_EVENT_CAP {
                    let n = subscriber.dropped.fetch_add(1, Ordering::SeqCst) + 1;
                    if n <= 3 || n % 100 == 0 {
                        tracing::warn!(
                            "[SessionOutputManager] Subscriber {} pending overflow, dropped event #{} (index={})",
                            subscriber.client_id,
                            n,
                            event.index
                        );
                    }
                    self.revert_unacked(event.index, event.data.len() as u64);
                } else {
                    pending.push(event.clone());
                }
            }
        }
    }

    /// 冲正未 ack 记账：事件被丢弃（发送超时/pending 溢出）后永远不会收到 ack，
    /// 将其从 FIFO 移除并回减 unacked_bytes，防止永久虚高导致背压水位永久暂停
    /// （饥饿）。FIFO 上限 8K，retain 线性扫描可接受（std Mutex 仅短临界区无 await）
    fn revert_unacked(&self, index: u64, bytes: u64) {
        let mut fifo = match self.unacked_fifo.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let before = fifo.len();
        fifo.retain(|(i, _)| *i != index);
        if fifo.len() != before {
            self.unacked_bytes.fetch_sub(bytes, Ordering::SeqCst);
        }
    }

    /// 客户端 ack：释放 ≤ last_rendered_seq 的未 ack 字节（渲染反馈环回调）
    ///
    /// seq 单调前进（前端只对已渲染帧回发）；比当前水位陈旧或超出已产出
    ///（重订阅竞态/陈旧客户端）的 ack 被 FIFO 弹出条件天然忽略——只弹
    /// index ≤ seq 的条目，无匹配即不动，unacked 不会越界减为负
    pub fn on_ack(&self, last_rendered_seq: u64) {
        let mut fifo = match self.unacked_fifo.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        while let Some(&(index, bytes)) = fifo.front() {
            if index <= last_rendered_seq {
                fifo.pop_front();
                self.unacked_bytes.fetch_sub(bytes, Ordering::SeqCst);
            } else {
                break;
            }
        }
    }

    /// 背压判定（PTY 读线程同步调用）：未 ack 字节超过水位 → 暂停读取。
    /// 纯原子读，零锁零阻塞，可安全地从阻塞读线程高频轮询
    pub fn should_pause(&self) -> bool {
        self.unacked_bytes.load(Ordering::SeqCst) > BACKPRESSURE_WATERMARK_BYTES
    }

    /// 订阅会话输出（05 快照协议）
    ///
    /// 帧流顺序：`[历史 Output × N] → HistoryEnd → [实时 Output]`，
    /// 历史完成后才标记 active（实时帧绝不先于 HistoryEnd 到达）
    ///
    /// 锁序（queue → subscribers 固定，勿调换）：
    /// 1. 写锁插入 active=false 的 subscriber（占位）
    /// 2. 读锁读取快照元数据 + 全段历史——持锁期间 on_output 写锁被阻塞，
    ///    保证快照与历史严格一致；随后 drop 队列读锁
    /// 3. 读 `history_start_mode` 配置：snapshot 模式尚未实现，恒回退 min
    /// 4. response 经 oneshot 前置返回（不被历史背压阻塞，避免客户端 10s 订阅
    ///    超时误判失败——订阅实际已建立，重复订阅产生孤儿任务 → 重复流）
    /// 5. subscribers 读锁内逐条发送历史（持读锁发送防止订阅者在历史发送中
    ///    被替换导致旧任务历史注入新通道）
    /// 6. 同一读锁作用域内发送 HistoryEnd 帧
    /// 7. 写锁排空 pending + 原子激活（on_output 被阻塞，排空与激活之间无新事件）
    ///
    /// 占位期间 on_output() 看到该 subscriber 但 active=false → 缓存到 pending；
    /// pending 中事件 index 必 > 快照 seq（第 2 步持读锁时的 max_seq），
    /// 排空与激活同锁完成，保证零丢失、顺序正确、无重无漏
    pub async fn subscribe(
        &self,
        client_id: &str,
        ws_sender: mpsc::Sender<OutputFrame>,
        response_tx: Option<tokio::sync::oneshot::Sender<SubscribeResponse>>,
    ) -> SubscribeResponse {
        let subscriber = SubscriberState::new(client_id.to_string(), ws_sender);

        // 第一步：插入占位 subscriber（active=false），释放写锁
        self.subscribers.write().await.insert(client_id.to_string(), subscriber);

        // 第二步：读取历史并发送（不持锁，不阻塞 on_output）
        // 持读锁期间 on_output 的写锁被阻塞 → 快照与历史严格一致
        let queue = self.output_queue.read().await;
        let min_seq = queue.min_seq();
        let snapshot_seq = queue.max_seq();
        // 全段重播（不裁剪不跳段）；seq 为全局计数器可能有跨会话空洞，禁止范围算术
        let history = queue.get_events();
        drop(queue);

        // 历史回放起点模式：snapshot（2J 清屏快照点 seq 化记录）尚未实现，
        // 恒回退 min 严格回放；快照机制随后续 ticket 引入
        if AppConfig::global().channels.history_start_mode == crate::system::config::HistoryStartMode::Snapshot {
            tracing::warn!(
                "[SessionOutputManager] history_start_mode=snapshot 未实现（快照回放延后），回退 min_seq 严格回放"
            );
        }

        let response = SubscribeResponse {
            min_seq,
            snapshot_seq,
            history_count: history.len(),
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
                        tracing::warn!("[SessionOutputManager] Failed to send history to {}: {}", client_id, e);
                    }
                }
                // 历史边界标记：消费端据此明确"此后为实时流"；旧路由吞掉
                if sub
                    .send_queue
                    .send(OutputFrame::HistoryEnd {
                        snapshot_seq,
                        min_seq,
                        history_count: history.len(),
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
                let mut subscribers = self.subscribers.write().await;
                if let Some(sub) = subscribers.get(client_id) {
                    // pending 全部为快照后事件（见 drain_pending 注释），无重叠无需跳过
                    sub.drain_pending().await;

                    // 读取最新 max_seq，此时 on_output 被写锁阻塞，max_seq 不会继续增长
                    let current_max = self.output_queue.read().await.max_seq();
                    sub.activate(current_max);
                }
            } else {
                // pending 为空，只需读锁激活
                let subscribers = self.subscribers.read().await;
                if let Some(sub) = subscribers.get(client_id) {
                    let current_max = self.output_queue.read().await.max_seq();
                    sub.activate(current_max);
                }
            }
        }

        tracing::info!(
            "[SessionOutputManager] Client {} subscribed to session {}, history_count={}",
            client_id,
            self.session_id,
            history.len()
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

        tracing::info!("[GlobalOutputManager] Session {} registered", session_id);
        manager
    }

    /// 注销会话（PTY 会话销毁时调用）
    pub async fn unregister_session(&self, session_id: &str) {
        if self.sessions.write().await.remove(session_id).is_some() {
            tracing::info!("[GlobalOutputManager] Session {} unregistered", session_id);
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
    /// `last_rendered_seq` 及之前的输出字节；会话不存在时忽略
    ///
    /// 背压门控：仅正统渲染端（current canonical）的 ack 推进记账；非正统端
    /// 的 ack 直接丢弃（其渲染格式可能与 PTY 尺寸不匹配，吞吐不代表权威消费
    /// 速度，混入会污染水位）。会话无归属时保守接受，避免水位锁死。
    pub async fn ack(&self, session_id: &str, last_rendered_seq: u64, source: RendererSource) {
        let sessions = self.sessions.read().await;
        if let Some(manager) = sessions.get(session_id) {
            let is_canonical = match crate::system::app_context::AppContext::try_global() {
                Some(ctx) => match ctx.session_manager().canonical_renderer_of(session_id).await {
                    Some(c) => c == source,
                    // 会话无正统归属（尚未 resize）：保守接受，避免背压水位永久暂停
                    None => true,
                },
                // 无 AppContext（无头/测试上下文）：跳过门控，保守接受
                None => true,
            };
            if !is_canonical {
                tracing::debug!(
                    session_id,
                    last_rendered_seq,
                    source = ?source,
                    "ack from non-canonical renderer ignored"
                );
                return;
            }
            manager.on_ack(last_rendered_seq);
            tracing::trace!(
                session_id,
                last_rendered_seq,
                unacked_bytes = manager.unacked_bytes.load(Ordering::SeqCst),
                "output ack applied"
            );
        } else {
            tracing::debug!(session_id, last_rendered_seq, "ack for unknown session ignored");
        }
    }

    /// 订阅会话输出（05 快照协议：全量历史 + HistoryEnd 标记，无 start_seq）
    pub async fn subscribe(
        &self,
        session_id: &str,
        client_id: &str,
        ws_sender: mpsc::Sender<OutputFrame>,
        response_tx: Option<tokio::sync::oneshot::Sender<SubscribeResponse>>,
    ) -> Option<SubscribeResponse> {
        let sessions = self.sessions.read().await;
        if let Some(manager) = sessions.get(session_id) {
            Some(manager.subscribe(client_id, ws_sender, response_tx).await)
        } else {
            tracing::warn!("[GlobalOutputManager] Session {} not found for subscribe", session_id);
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

    fn make_event(index: u64) -> OutputEvent {
        OutputEvent::new(
            "test".to_string(),
            b"test".to_vec(),
            index,
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

    /// 解包 Output 帧的 index（HistoryEnd 视为断言失败）
    fn output_index(frame: &OutputFrame) -> u64 {
        match frame {
            OutputFrame::Output(e) => e.index,
            _ => panic!("expected Output frame, got HistoryEnd"),
        }
    }

    /// push 后 get_events 整段返回全部事件（FIFO 序）
    #[test]
    fn test_push_and_get_events() {
        let mut queue = UnifiedOutputQueue::new(10);

        for i in 0..5 {
            queue.push(make_event(i));
        }

        let events = queue.get_events();
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].index, 0);
        assert_eq!(events[4].index, 4);
    }

    /// 环形淘汰推进 min_seq，max_seq 为最新 push 的 index
    #[test]
    fn test_overflow_updates_min_seq() {
        let mut queue = UnifiedOutputQueue::new(3);

        for i in 0..5 {
            queue.push(make_event(i));
        }

        assert_eq!(queue.min_seq(), 2);
        assert_eq!(queue.max_seq(), 4);
        assert_eq!(queue.len(), 3);

        let events = queue.get_events();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].index, 2);
    }

    /// get_events 整段克隆无裁剪：index 含跨会话空洞也原样返回（禁止范围算术）
    #[test]
    fn test_get_events_full_no_trim() {
        let mut queue = UnifiedOutputQueue::new(10);

        // 模拟跨会话全局计数器空洞（如 A 会话 = [0,2,4]）
        for i in [0u64, 2, 4, 100] {
            queue.push(make_event(i));
        }

        let events = queue.get_events();
        assert_eq!(events.len(), 4);
        assert_eq!(
            events.iter().map(|e| e.index).collect::<Vec<_>>(),
            vec![0, 2, 4, 100],
            "整段 FIFO 返回，不得做范围算术/裁剪"
        );
    }

    /// 淘汰后 min_seq 推进到队首事件 index + 1；min_seq/max_seq 为纯元数据
    #[test]
    fn test_eviction_advances_min_seq() {
        let mut queue = UnifiedOutputQueue::new(2);

        queue.push(make_event(0));
        queue.push(make_event(1));
        queue.push(make_event(2));

        assert_eq!(queue.min_seq(), 1);
        assert_eq!(queue.max_seq(), 2);
        assert_eq!(queue.len(), 2);
    }

    /// 字节上限超限时淘汰最旧事件
    #[test]
    fn test_max_bytes_limit_evicts_oldest() {
        let mut queue = UnifiedOutputQueue::with_max_bytes(100, 10);
        queue.push(make_event(0));
        queue.push(make_event(1));
        queue.push(make_event(2));

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.min_seq(), 1);
        assert_eq!(queue.max_seq(), 2);
    }

    /// push 推进 max_seq（不再分配字节偏移）
    #[test]
    fn test_push_advances_max_seq() {
        let mut queue = UnifiedOutputQueue::new(10);

        queue.push(make_event(0));
        queue.push(make_event(1));
        queue.push(make_event(2));

        assert_eq!(queue.max_seq(), 2);
        assert_eq!(queue.len(), 3);
    }

    /// 单条事件超过字节上限时仍保留（不能丢弃刚 push 的事件）
    #[test]
    fn test_max_bytes_single_event_exceeds_limit() {
        let mut queue = UnifiedOutputQueue::with_max_bytes(100, 2);
        queue.push(make_event(0));
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.total_bytes, 4);
    }

    // ==================== 快照协议订阅 ====================

    /// 订阅帧流严格顺序：[历史 Output × N] → [HistoryEnd] → [实时 Output]
    #[tokio::test]
    async fn test_subscribe_history_then_marker_then_live() {
        let manager = SessionOutputManager::new("test-session");

        for i in 0..3 {
            manager.output_queue.write().await.push(make_event(i));
        }

        let (tx, mut rx) = mpsc::channel(100);
        let response = manager.subscribe("client-1", tx, None).await;
        assert_eq!(response.min_seq, 0);
        assert_eq!(response.snapshot_seq, 2);
        assert_eq!(response.history_count, 3);

        // 历史 3 帧
        for i in 0..3 {
            let frame = rx.recv().await.unwrap();
            assert_eq!(output_index(&frame), i);
        }
        // HistoryEnd 标记（携带 snapshot_seq/min_seq/history_count）
        let marker = rx.recv().await.unwrap();
        match marker {
            OutputFrame::HistoryEnd {
                snapshot_seq,
                min_seq,
                history_count,
            } => {
                assert_eq!(snapshot_seq, 2);
                assert_eq!(min_seq, 0);
                assert_eq!(history_count, 3);
            }
            _ => panic!("expected HistoryEnd after history"),
        }
        // 实时帧在标记之后
        manager.on_output(make_event(3)).await;
        let live = rx.recv().await.unwrap();
        assert_eq!(output_index(&live), 3);
    }

    /// 空队列订阅：无历史帧，直接 [HistoryEnd]
    #[tokio::test]
    async fn test_empty_history_subscribe() {
        let manager = SessionOutputManager::new("test-session");

        let (tx, mut rx) = mpsc::channel(100);
        let response = manager.subscribe("client-1", tx, None).await;
        assert_eq!(response.history_count, 0);
        assert_eq!(response.snapshot_seq, 0);

        let marker = rx.recv().await.unwrap();
        match marker {
            OutputFrame::HistoryEnd {
                snapshot_seq,
                history_count,
                ..
            } => {
                assert_eq!(snapshot_seq, 0);
                assert_eq!(history_count, 0);
            }
            _ => panic!("expected HistoryEnd only"),
        }
        // 无更多帧
        assert!(rx.try_recv().is_err());
    }

    /// 淘汰后（min_seq > 0）订阅：覆盖全部现存事件（长期会话重订阅）
    #[tokio::test]
    async fn test_subscribe_full_history_snapshot() {
        let manager = SessionOutputManager::new("test-session");

        // 换小容量队列（2）：push 3 条 → index 0 淘汰，min_seq=1
        *manager.output_queue.write().await = UnifiedOutputQueue::new(2);
        manager.output_queue.write().await.push(make_event(0));
        manager.output_queue.write().await.push(make_event(1));
        manager.output_queue.write().await.push(make_event(2));

        let (tx, mut rx) = mpsc::channel(100);
        let response = manager.subscribe("client-1", tx, None).await;
        assert_eq!(response.min_seq, 1);
        assert_eq!(response.snapshot_seq, 2);

        // 全量现存（[1,2] 按 FIFO）
        let f1 = rx.recv().await.unwrap();
        assert_eq!(output_index(&f1), 1);
        let f2 = rx.recv().await.unwrap();
        assert_eq!(output_index(&f2), 2);
        let marker = rx.recv().await.unwrap();
        assert!(matches!(marker, OutputFrame::HistoryEnd { .. }));
    }

    /// 队列层 index 无关性（防御性）：直接 push 任意单调 index（模拟极端/历史数据），
    /// get_events 仍整段返回；生产路径 on_output 已按会话分配连续 seq，不会产生此空洞
    #[tokio::test]
    async fn test_subscribe_with_global_seq_gaps() {
        let manager = SessionOutputManager::new("test-session");

        // 模拟多会话并发：本会话事件 index = [0, 2, 4, 100]（含跨会话空洞）
        for i in [0u64, 2, 4, 100] {
            manager.output_queue.write().await.push(make_event(i));
        }

        let (tx, mut rx) = mpsc::channel(100);
        let response = manager.subscribe("client-1", tx, None).await;
        assert_eq!(response.history_count, 4);

        for expect in [0u64, 2, 4, 100] {
            let f = rx.recv().await.unwrap();
            assert_eq!(output_index(&f), expect);
        }
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
            async move { manager.subscribe("client-race", tx, None).await }
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
        assert_eq!(response.snapshot_seq, 2);
        assert_eq!(response.history_count, 3);

        // 关闭通道（unsubscribe 释放订阅者 send_queue），让收集任务退出
        manager.unsubscribe("client-race").await;
        let frames = tokio::time::timeout(std::time::Duration::from_secs(2), collect_handle)
            .await
            .expect("collector finished within timeout")
            .unwrap();

        // 严格顺序：[历史 0,1,2] → HistoryEnd → [pending 3,4]，无重无漏
        let indices: Vec<u64> = frames
            .iter()
            .map(|f| match f {
                OutputFrame::Output(e) => e.index,
                OutputFrame::HistoryEnd { .. } => u64::MAX, // 标记占位
            })
            .collect();
        assert_eq!(indices, vec![0, 1, 2, u64::MAX, 3, 4]);
        // HistoryEnd 位于历史之后、pending 之前
        assert!(matches!(frames[3], OutputFrame::HistoryEnd { .. }));
    }

    /// 基础订阅 + 实时输出：响应携带快照元数据，帧流顺序被正确维持
    #[tokio::test]
    async fn test_subscribe_and_on_output() {
        let manager = SessionOutputManager::new("test-session");

        let (tx, mut rx) = mpsc::channel(100);

        manager.output_queue.write().await.push(make_event(0));
        manager.output_queue.write().await.push(make_event(1));

        let response = manager.subscribe("client-1", tx, None).await;
        assert_eq!(response.min_seq, 0);
        assert_eq!(response.snapshot_seq, 1);
        assert_eq!(response.history_count, 2);

        let f1 = rx.recv().await.unwrap();
        assert_eq!(output_index(&f1), 0);
        let f2 = rx.recv().await.unwrap();
        assert_eq!(output_index(&f2), 1);
        let marker = rx.recv().await.unwrap();
        assert!(matches!(marker, OutputFrame::HistoryEnd { .. }));

        manager.on_output(make_event(2)).await;
        let f3 = rx.recv().await.unwrap();
        assert_eq!(output_index(&f3), 2);
    }

    /// 多订阅者：同一事件广播给所有订阅者
    ///
    /// 空历史订阅 → 每个订阅者先收 HistoryEnd，再收实时帧
    #[tokio::test]
    async fn test_multiple_subscribers() {
        let manager = SessionOutputManager::new("test-session");

        let (tx1, mut rx1) = mpsc::channel(100);
        let (tx2, mut rx2) = mpsc::channel(100);

        manager.subscribe("client-1", tx1, None).await;
        manager.subscribe("client-2", tx2, None).await;

        // 两个订阅者各自先排空空历史（HistoryEnd）
        assert!(matches!(rx1.recv().await.unwrap(), OutputFrame::HistoryEnd { .. }));
        assert!(matches!(rx2.recv().await.unwrap(), OutputFrame::HistoryEnd { .. }));

        manager.on_output(make_event(0)).await;

        let f1 = rx1.recv().await.unwrap();
        let f2 = rx2.recv().await.unwrap();
        // on_output 按会话连续分配：空队列首事件 seq = max_seq(0) + 1 = 1
        assert_eq!(output_index(&f1), 1);
        assert_eq!(output_index(&f2), 1);
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let manager = SessionOutputManager::new("test-session");

        let (tx, _rx) = mpsc::channel(100);
        manager.subscribe("client-1", tx, None).await;

        manager.unsubscribe("client-1").await;

        assert!(!manager.is_subscribed("client-1").await);
    }

    fn make_session_event(session_id: &str, index: u64) -> OutputEvent {
        OutputEvent {
            session_id: session_id.to_string(),
            data: b"test".to_vec(),
            index,
            timestamp: Utc::now().timestamp_millis(),
            is_waiting: false,
        }
    }

    #[tokio::test]
    async fn test_register_and_on_output() {
        let manager = GlobalOutputManager::new();

        manager.register_session("session-1").await;

        let (tx, mut rx) = mpsc::channel(100);
        manager.subscribe("session-1", "client-1", tx, None).await;

        // 空历史：先收 HistoryEnd
        assert!(matches!(rx.recv().await.unwrap(), OutputFrame::HistoryEnd { .. }));

        manager.on_output(make_session_event("session-1", 0)).await;

        let frame = rx.recv().await.unwrap();
        let OutputFrame::Output(e) = frame else {
            panic!("expected output frame");
        };
        assert_eq!(e.session_id, "session-1");
        // on_output 按会话连续分配：本会话首个事件 seq = 队列 max_seq(0) + 1 = 1
        assert_eq!(e.index, 1);
    }

    #[tokio::test]
    async fn test_backpressure_accounting_pause_and_resume() {
        let manager = GlobalOutputManager::new();
        manager.register_session("session-bp").await;

        let (tx, mut rx) = mpsc::channel(1000);
        manager.subscribe("session-bp", "client-ack", tx, None).await;
        let _ = rx.recv().await.unwrap(); // 空历史 HistoryEnd

        let big = vec![b'x'; 32 * 1024];
        for _ in 0..40 {
            manager
                .on_output(OutputEvent {
                    session_id: "session-bp".to_string(),
                    data: big.clone(),
                    index: 0,
                    timestamp: Utc::now().timestamp_millis(),
                    is_waiting: false,
                })
                .await;
        }
        // 40×32KB = 1.25MB > 1MB 水位 → 暂停读
        assert!(manager.should_pause("session-bp"), "burst should pause");

        // 收集事件 seq（订阅通道 40 帧；on_output 按会话连续分配 1..40）
        let mut seqs = Vec::new();
        while let Ok(OutputFrame::Output(e)) = rx.try_recv() {
            seqs.push(e.index);
        }
        assert_eq!(seqs.len(), 40);
        assert_eq!(seqs[0], 1);

        // ack 到 seq 20：剩余 20×32KB = 640KB < 水位 → 恢复读
        manager.ack("session-bp", 20, RendererSource::Desktop).await;
        assert!(!manager.should_pause("session-bp"), "ack advance should resume");

        // 一次性 ack 超限 seq：全部释放，unacked 不为负
        manager.ack("session-bp", 9999, RendererSource::Desktop).await;
        assert!(!manager.should_pause("session-bp"));

        // 会话不存在：ack 静默忽略，不 panic
        manager.ack("no-such-session", 5, RendererSource::Desktop).await;
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
                    index: 0,
                    timestamp: Utc::now().timestamp_millis(),
                    is_waiting: false,
                })
                .await;
        }
        // FIFO 满冻结：unacked = 8192B，远小于水位 → 不暂停
        assert!(!manager.should_pause("session-bp-cap"));

        // 超限 ack：FIFO 内全部弹出，unacked 归零，不因冻结期欠记而变负
        manager.ack("session-bp-cap", 99999, RendererSource::Desktop).await;
        assert!(!manager.should_pause("session-bp-cap"));
        // 重复 ack：空 FIFO 无匹配，no-op，不越界
        manager.ack("session-bp-cap", 99999, RendererSource::Desktop).await;
    }

    #[tokio::test]
    async fn test_multiple_sessions() {
        let manager = GlobalOutputManager::new();

        manager.register_session("session-1").await;
        manager.register_session("session-2").await;

        let (tx1, mut rx1) = mpsc::channel(100);
        let (tx2, mut rx2) = mpsc::channel(100);

        manager.subscribe("session-1", "client-1", tx1, None).await;
        manager.subscribe("session-2", "client-2", tx2, None).await;

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
        manager.subscribe("session-wait", "client-wait", tx, None).await;
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
        manager.subscribe("session-stall", "client-stall", tx, None).await;
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
        drop(sessions);        {
            let subscribers = m.subscribers.read().await;
            let sub = subscribers.get("client-stall").unwrap();
            assert_eq!(sub.dropped.load(Ordering::SeqCst), 1, "超时应丢弃一次");
        }
        // 冲正：被丢弃事件的字节从记账中移除（事件 1 的 4B 仍在）
        // FIFO 与 unacked 同步：事件 2 已冲正，此时未 ack = 事件 1 的 4B
        {
            let fifo = m.unacked_fifo.lock().unwrap();
            assert_eq!(fifo.len(), 1, "FIFO 仅保留事件 1");
            assert_eq!(fifo[0], (1, 4), "事件 1 index=1, 4 字节");
        }
        assert_eq!(m.unacked_bytes.load(Ordering::SeqCst), 4, "冲正后 unacked 回落");
        // 未 ack 已低于水位 → 不暂停（冲正防止背压永久暂停）
        assert!(!m.should_pause());
    }

    /// inactive 占位期 pending 溢出：丢弃并冲正记账（缺口由客户端重订阅
    /// 全量重播自愈，但不允许 unacked 虚高饿死背压）
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

        // 溢出事件（index 由 on_output 分配）：丢弃 + 冲正
        m.on_output(make_event(0)).await;

        let subs = m.subscribers.read().await;
        let sub2 = subs.get("client-pending").unwrap();
        assert_eq!(sub2.dropped.load(Ordering::SeqCst), 1, "pending 溢出丢弃一次");
        // 正常时间事件的所有未 ack 记账已被冲正 → unacked 为 0（不虚高）
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

        let response = manager.subscribe("client-1", tx, None).await;
        assert_eq!(response.history_count, 5);

        // 历史 0-4 + HistoryEnd
        for i in 0..5 {
            let f = rx.recv().await.unwrap();
            assert_eq!(output_index(&f), i);
        }
        assert!(matches!(rx.recv().await.unwrap(), OutputFrame::HistoryEnd { .. }));

        // subscribe 完成后，on_output 正常接收
        manager.on_output(make_event(5)).await;
        let f = rx.recv().await.unwrap();
        assert_eq!(output_index(&f), 5);
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
        assert_eq!(output_index(&f1), 3);
        let f2 = rx.recv().await.unwrap();
        assert_eq!(output_index(&f2), 4);
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
        // 否则接收端只收到 pending [3,4]，收不到历史 [0,1,2]，断言顺序不成立
        {
            let subs = manager.subscribers.read().await;
            let sub = subs.get("client-1").unwrap();
            for i in 0..3 {
                sub.send_queue.send(OutputFrame::Output(make_event(i))).await.unwrap();
            }
        }

        // inactive 期间 on_output 应缓存到 pending
        manager.on_output(make_event(3)).await;
        manager.on_output(make_event(4)).await;

        // 验证 pending 中有 2 个事件
        {
            let subs = manager.subscribers.read().await;
            let sub = subs.get("client-1").unwrap();
            let pending = sub.pending.read().await;
            assert_eq!(pending.len(), 2);
            assert_eq!(pending[0].index, 3);
            assert_eq!(pending[1].index, 4);
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
            let current_max = manager.output_queue.read().await.max_seq();
            sub.activate(current_max);
        }

        // 收到历史 + pending 事件（此测试手动模拟 subscribe 两步，
        // 不发送 HistoryEnd 标记；直接验证后续 on_output 正常送达）
        for i in 0..5 {
            let f = rx.recv().await.unwrap();
            assert_eq!(output_index(&f), i);
        }

        // 激活后 on_output 正常发送
        manager.on_output(make_event(5)).await;
        let f = rx.recv().await.unwrap();
        assert_eq!(output_index(&f), 5);
    }

    /// activate 使用最新 max_seq：订阅结束前 on_output 推入的真实值
    #[tokio::test]
    async fn test_activate_uses_current_max_seq() {
        let manager = SessionOutputManager::new("test-session");

        for i in 0..3 {
            manager.output_queue.write().await.push(make_event(i));
        }

        let (tx, mut rx) = mpsc::channel(100);

        let response = manager.subscribe("client-1", tx, None).await;
        assert_eq!(response.snapshot_seq, 2);

        // 验证 subscriber 的 sent_seq 是最新的
        {
            let subs = manager.subscribers.read().await;
            let sub = subs.get("client-1").unwrap();
            assert!(sub.is_active());
            assert_eq!(sub.sent_seq.load(Ordering::SeqCst), 2);
        }

        // 排空 subscribe 阶段的历史与标记，聚焦验证后续 on_output 无重复无丢失
        for i in 0..3 {
            let f = rx.recv().await.unwrap();
            assert_eq!(output_index(&f), i);
        }
        assert!(matches!(rx.recv().await.unwrap(), OutputFrame::HistoryEnd { .. }));

        manager.on_output(make_event(3)).await;
        let f = rx.recv().await.unwrap();
        assert_eq!(output_index(&f), 3);
    }
}
