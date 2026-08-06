//! Session Output
//!
//! PTY 输出相关的组件：输出缓存、统一输出队列、会话输出管理、全局输出管理
//! OutputCache trait 已内联到此文件（只有一个实现）

use crate::pty::PtyOutputEvent;
use crate::system::config::AppConfig;
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

// ==================== Output Cache ====================

/// PTY 输出缓存 - 为移动端订阅提供历史输出
pub trait OutputCache: Send + Sync {
    async fn cache(&self, event: PtyOutputEvent);
    async fn get(&self, session_id: &str) -> Vec<PtyOutputEvent>;
    async fn clear(&self, session_id: &str);
    async fn clear_all(&self);
    async fn len(&self) -> usize;
}

pub struct DefaultOutputCache {
    cache: Arc<RwLock<HashMap<String, Vec<PtyOutputEvent>>>>,
    max_size: usize,
}

impl DefaultOutputCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_size,
        }
    }
}

impl OutputCache for DefaultOutputCache {
    async fn cache(&self, event: PtyOutputEvent) {
        let mut cache = self.cache.write().await;
        let entries = cache.entry(event.session_id.clone()).or_insert_with(Vec::new);

        if entries.len() >= self.max_size {
            entries.remove(0);
        }
        entries.push(event.clone());
    }

    async fn get(&self, session_id: &str) -> Vec<PtyOutputEvent> {
        let cache = self.cache.read().await;
        cache.get(session_id).cloned().unwrap_or_default()
    }

    async fn clear(&self, session_id: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(session_id);
    }

    async fn clear_all(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    async fn len(&self) -> usize {
        let cache = self.cache.read().await;
        cache.values().map(Vec::len).sum()
    }
}

// ==================== Output History Response ====================

/// 历史回放响应（供桌面端终端窗口恢复使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputHistoryResponse {
    /// 队列中最早事件的序号
    pub min_seq: u64,
    /// 队列中最新事件的序号
    pub max_seq: u64,
    /// 历史事件列表（data 为 Base64 编码）
    pub events: Vec<PtyOutputEvent>,
}

impl From<OutputEvent> for PtyOutputEvent {
    fn from(e: OutputEvent) -> Self {
        PtyOutputEvent::from_bytes(
            e.session_id,
            &e.data,
            Utc.timestamp_millis_opt(e.timestamp).single().unwrap_or_default(),
            e.is_waiting,
            e.index as usize,
        )
    }
}

// ==================== Unified Output Queue ====================

/// 输出事件
///
/// `data` 存储原始字节数据，在发送到 WebSocket 时才进行 Base64 编码
/// 避免在缓冲合并时多次编解码
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
            data: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &self.data,
            ),
            index: self.index,
            timestamp: self.timestamp,
            is_waiting: self.is_waiting,
        }
    }

    /// 获取 Base64 编码的数据
    pub fn data_base64(&self) -> String {
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &self.data,
        )
    }
}

/// 统一输出队列（环形缓冲区）
///
/// 双重容量限制：
/// - `capacity`: 最大事件条数（条目级限制）
/// - `max_total_bytes`: 最大总字节数（内存级限制）
/// 任一限制超出时丢弃最旧事件，与前端 buffer 逻辑一致
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

    /// 推入新事件
    ///
    /// 双重容量检查：条目数和总字节数任一超出时丢弃最旧事件
    pub fn push(&mut self, event: OutputEvent) {
        self.max_seq.store(event.index, Ordering::SeqCst);
        self.total_produced.fetch_add(1, Ordering::SeqCst);

        let event_bytes = event.data.len() as u64;
        self.total_bytes += event_bytes;

        // 条目数或总字节数超出时，丢弃最旧事件直到满足限制
        while (self.buffer.len() >= self.capacity || self.total_bytes > self.max_total_bytes)
            && !self.buffer.is_empty()
        {
            if let Some(old) = self.buffer.pop_front() {
                self.total_bytes -= old.data.len() as u64;
                self.min_seq.store(old.index + 1, Ordering::SeqCst);
            }
        }

        self.buffer.push_back(event);
    }

    /// 获取范围数据 [start_seq, max_seq]
    pub fn get_range(&self, start_seq: u64) -> Vec<OutputEvent> {
        let min_seq = self.min_seq.load(Ordering::SeqCst);
        let actual_start = start_seq.max(min_seq);

        self.buffer
            .iter()
            .filter(|e| e.index >= actual_start)
            .cloned()
            .collect()
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
    /// 独立发送通道（绑定该客户端的 WebSocket）
    pub send_queue: mpsc::Sender<OutputEvent>,
    /// inactive 期间的待发送缓冲，消除历史发送→激活之间的丢失窗口
    pub pending: RwLock<Vec<OutputEvent>>,
}

impl SubscriberState {
    pub fn new(client_id: String, send_queue: mpsc::Sender<OutputEvent>) -> Self {
        Self {
            client_id,
            active: AtomicBool::new(false),
            sent_seq: AtomicU64::new(0),
            send_queue,
            pending: RwLock::new(Vec::new()),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn activate(&self, sent_seq: u64) {
        self.sent_seq.store(sent_seq, Ordering::SeqCst);
        self.active.store(true, Ordering::SeqCst);
    }

    /// 排空 pending 缓冲，跳过已含在历史快照中的事件（index <= snapshot_max_seq）
    ///
    /// 订阅流程"插入占位 → 读取历史快照"之间存在一个微小的竞态窗口：
    /// 该窗口内 on_output() 可能把新事件同时写进输出队列（进入历史快照）
    /// 并缓存进 pending（因占位 subscriber 尚未 active）。若不跳过，这些事件
    /// 会被历史发送与 pending 排空各发一次，订阅者终端出现重复输出。
    async fn drain_pending(&self, snapshot_max_seq: u64) {
        let mut pending = self.pending.write().await;
        for event in pending.drain(..) {
            if event.index <= snapshot_max_seq {
                continue;
            }
            if let Err(e) = self.send_queue.send(event.clone()).await {
                tracing::warn!(
                    "[SessionOutputManager] Failed to send pending to {}: {}",
                    self.client_id, e
                );
            }
        }
    }
}

/// 订阅响应
#[derive(Debug, Clone)]
pub struct SubscribeResponse {
    pub min_seq: u64,
    pub max_seq: u64,
    pub history_count: usize,
}

/// 单个 PTY 会话的输出管理，包括输出队列和订阅者管理
pub struct SessionOutputManager {
    session_id: String,
    output_queue: Arc<RwLock<UnifiedOutputQueue>>,
    subscribers: RwLock<HashMap<String, SubscriberState>>,
}

impl SessionOutputManager {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            output_queue: Arc::new(RwLock::new(UnifiedOutputQueue::default())),
            subscribers: RwLock::new(HashMap::new()),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 处理新输出
    pub async fn on_output(&self, event: OutputEvent) {
        self.output_queue.write().await.push(event.clone());

        let subscribers = self.subscribers.read().await;
        for subscriber in subscribers.values() {
            if subscriber.is_active() {
                if let Err(e) = subscriber.send_queue.send(event.clone()).await {
                    tracing::warn!(
                        "[SessionOutputManager] Failed to send to subscriber {}: {}",
                        subscriber.client_id, e
                    );
                }
            } else {
                // inactive 期间缓存事件，激活时排空，消除历史发送→激活的丢失窗口
                if let Ok(mut pending) = subscriber.pending.try_write() {
                    pending.push(event.clone());
                }
            }
        }
    }

    /// 订阅会话输出
    ///
    /// 使用"先占位→发历史→排空 pending→原子激活"模式：
    /// 1. 先插入 active=false 的 subscriber（占位），释放写锁
    /// 2. 逐条发送历史（不持锁，不阻塞 on_output 的读锁）
    /// 3. 持写锁排空 pending + 原子激活（on_output 被阻塞，不会在排空和激活之间插入新事件）
    ///
    /// 占位期间 on_output() 会看到该 subscriber 但因 active=false 将事件缓存到 pending，
    /// 排空 pending 和 activate 在同一写锁内完成，保证零丢失且顺序正确
    pub async fn subscribe(
        &self,
        client_id: &str,
        ws_sender: mpsc::Sender<OutputEvent>,
        start_seq: Option<u64>,
    ) -> SubscribeResponse {
        let subscriber = SubscriberState::new(client_id.to_string(), ws_sender);

        // 第一步：插入占位 subscriber（active=false），释放写锁
        self.subscribers
            .write()
            .await
            .insert(client_id.to_string(), subscriber);

        // 第二步：读取历史并发送（不持锁，不阻塞 on_output）
        let queue = self.output_queue.read().await;
        let min_seq = queue.min_seq();
        let max_seq = queue.max_seq();

        // start_seq 为 None 或 0 时从头获取，否则从指定序号开始
        let actual_start = start_seq.unwrap_or(0);
        let history = queue.get_range(actual_start);
        drop(queue);

        // 通过该订阅者的独立通道发送历史（保证顺序）
        {
            let subscribers = self.subscribers.read().await;
            if let Some(sub) = subscribers.get(client_id) {
                for event in &history {
                    if let Err(e) = sub.send_queue.send(event.clone()).await {
                        tracing::warn!(
                            "[SessionOutputManager] Failed to send history to {}: {}",
                            client_id, e
                        );
                    }
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
                    // 跳过历史快照（max_seq 之前）已发送的事件，避免重复输出
                    sub.drain_pending(max_seq).await;

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
            "[SessionOutputManager] Client {} subscribed to session {}, start_seq={:?}, history_count={}",
            client_id,
            self.session_id,
            start_seq,
            history.len()
        );

        SubscribeResponse {
            min_seq,
            max_seq,
            history_count: history.len(),
        }
    }

    /// 取消订阅
    pub async fn unsubscribe(&self, client_id: &str) {
        if self.subscribers.write().await.remove(client_id).is_some() {
            tracing::info!(
                "[SessionOutputManager] Client {} unsubscribed from session {}",
                client_id, self.session_id
            );
        }
    }

    pub async fn is_subscribed(&self, client_id: &str) -> bool {
        self.subscribers.read().await.contains_key(client_id)
    }

    pub async fn active_subscriber_count(&self) -> usize {
        self.subscribers
            .read()
            .await
            .values()
            .filter(|s| s.is_active())
            .count()
    }

    /// 获取历史输出（供桌面端回放使用）
    ///
    /// 从 UnifiedOutputQueue 读取指定序号之后的全部事件，
    /// 转换为 PtyOutputEvent 格式返回
    pub async fn get_history(&self, start_seq: Option<u64>) -> OutputHistoryResponse {
        let queue = self.output_queue.read().await;
        let min_seq = queue.min_seq();
        let max_seq = queue.max_seq();
        let actual_start = start_seq.unwrap_or(0);
        let events = queue.get_range(actual_start);

        OutputHistoryResponse {
            min_seq,
            max_seq,
            events: events.into_iter().map(|e| e.into()).collect(),
        }
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

    /// 订阅会话输出
    ///
    /// - `start_seq = None` 或 `0`：从头补完所有历史
    /// - `start_seq = N (N > 0)`：从指定序号开始获取
    pub async fn subscribe(
        &self,
        session_id: &str,
        client_id: &str,
        ws_sender: mpsc::Sender<OutputEvent>,
        start_seq: Option<u64>,
    ) -> Option<SubscribeResponse> {
        let sessions = self.sessions.read().await;
        if let Some(manager) = sessions.get(session_id) {
            Some(manager.subscribe(client_id, ws_sender, start_seq).await)
        } else {
            tracing::warn!(
                "[GlobalOutputManager] Session {} not found for subscribe",
                session_id
            );
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
                client_id, session_id
            );
        }
        tracing::info!(
            "[GlobalOutputManager] Cleaned up subscriptions for client {} across {} sessions",
            client_id, sessions.len()
        );
    }

    /// 获取会话历史输出（供桌面端终端窗口回放使用）
    pub async fn get_history(
        &self,
        session_id: &str,
        start_seq: Option<u64>,
    ) -> Option<OutputHistoryResponse> {
        let sessions = self.sessions.read().await;
        if let Some(manager) = sessions.get(session_id) {
            Some(manager.get_history(start_seq).await)
        } else {
            None
        }
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

    fn make_event(index: u64) -> OutputEvent {
        OutputEvent::new(
            "test".to_string(),
            b"test".to_vec(),
            index,
            Utc::now().timestamp_millis(),
            false,
        )
    }

    #[test]
    fn test_push_and_get_range() {
        let mut queue = UnifiedOutputQueue::new(10);

        for i in 0..5 {
            queue.push(make_event(i));
        }

        let events = queue.get_range(0);
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].index, 0);
        assert_eq!(events[4].index, 4);
    }

    #[test]
    fn test_overflow_updates_min_seq() {
        let mut queue = UnifiedOutputQueue::new(3);

        for i in 0..5 {
            queue.push(make_event(i));
        }

        assert_eq!(queue.min_seq(), 2);
        assert_eq!(queue.max_seq(), 4);
        assert_eq!(queue.len(), 3);

        let events = queue.get_range(0);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].index, 2);
    }

    #[test]
    fn test_get_range_from_middle() {
        let mut queue = UnifiedOutputQueue::new(10);

        for i in 0..10 {
            queue.push(make_event(i));
        }

        let events = queue.get_range(5);
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].index, 5);
    }

    #[test]
    fn test_max_bytes_limit_evicts_oldest() {
        // 容量 100 条，但字节上限 10 字节
        // make_event 的 data 是 b"test" = 4 字节
        let mut queue = UnifiedOutputQueue::with_max_bytes(100, 10);

        // push 3 个事件：4+4+4 = 12 字节 > 10，第一个应被淘汰
        queue.push(make_event(0));
        queue.push(make_event(1));
        queue.push(make_event(2));

        // 第一个事件被淘汰，剩余 2 个：4+4 = 8 字节
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.min_seq(), 1);
        assert_eq!(queue.max_seq(), 2);
    }

    #[test]
    fn test_max_bytes_single_event_exceeds_limit() {
        // 单条事件就超过字节上限时，仍保留该事件（不能丢弃刚 push 的事件）
        let mut queue = UnifiedOutputQueue::with_max_bytes(100, 2);

        // b"test" = 4 字节 > 2 字节上限，但事件已 push
        queue.push(make_event(0));
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.total_bytes, 4);
    }

    #[tokio::test]
    async fn test_subscribe_and_on_output() {
        let manager = SessionOutputManager::new("test-session");

        let (tx, mut rx) = mpsc::channel(100);

        manager.output_queue.write().await.push(make_event(0));
        manager.output_queue.write().await.push(make_event(1));

        let response = manager.subscribe("client-1", tx, None).await;
        assert_eq!(response.min_seq, 0);
        assert_eq!(response.max_seq, 1);
        assert_eq!(response.history_count, 2);

        let event1 = rx.recv().await.unwrap();
        assert_eq!(event1.index, 0);
        let event2 = rx.recv().await.unwrap();
        assert_eq!(event2.index, 1);

        manager.on_output(make_event(2)).await;
        let event3 = rx.recv().await.unwrap();
        assert_eq!(event3.index, 2);
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let manager = SessionOutputManager::new("test-session");

        let (tx1, mut rx1) = mpsc::channel(100);
        let (tx2, mut rx2) = mpsc::channel(100);

        manager.subscribe("client-1", tx1, None).await;
        manager.subscribe("client-2", tx2, None).await;

        manager.on_output(make_event(0)).await;

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert_eq!(e1.index, 0);
        assert_eq!(e2.index, 0);
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

        manager.on_output(make_session_event("session-1", 0)).await;

        let event = rx.recv().await.unwrap();
        assert_eq!(event.session_id, "session-1");
        assert_eq!(event.index, 0);
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

        let e1 = rx1.recv().await.unwrap();
        assert_eq!(e1.session_id, "session-1");

        let e2 = rx2.recv().await.unwrap();
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

    #[tokio::test]
    async fn test_subscribe_with_start_seq() {
        let manager = SessionOutputManager::new("test-session");

        // 预填充 5 个事件
        for i in 0..5 {
            manager.output_queue.write().await.push(make_event(i));
        }

        let (tx, mut rx) = mpsc::channel(100);

        // start_seq=3：只应收到 index >= 3 的事件
        let response = manager.subscribe("client-1", tx, Some(3)).await;
        assert_eq!(response.min_seq, 0);
        assert_eq!(response.max_seq, 4);
        assert_eq!(response.history_count, 2); // index 3, 4

        let e1 = rx.recv().await.unwrap();
        assert_eq!(e1.index, 3);
        let e2 = rx.recv().await.unwrap();
        assert_eq!(e2.index, 4);
    }

    #[tokio::test]
    async fn test_subscribe_with_start_seq_zero() {
        let manager = SessionOutputManager::new("test-session");

        for i in 0..3 {
            manager.output_queue.write().await.push(make_event(i));
        }

        let (tx, mut rx) = mpsc::channel(100);

        // start_seq=0 等同于 None，从头获取所有历史
        let response = manager.subscribe("client-1", tx, Some(0)).await;
        assert_eq!(response.history_count, 3);

        for i in 0..3 {
            let e = rx.recv().await.unwrap();
            assert_eq!(e.index, i);
        }
    }

    /// 验证 pending 缓冲消除订阅丢失窗口：
    /// subscribe 期间 on_output 产生的事件应通过 pending 缓冲补齐，零丢失
    #[tokio::test]
    async fn test_pending_covers_subscribe_gap() {
        let manager = SessionOutputManager::new("test-session");

        // 预填充历史
        for i in 0..5 {
            manager.output_queue.write().await.push(make_event(i));
        }

        let (tx, mut rx) = mpsc::channel(100);

        // subscribe 会：占位 → 发历史(0-4) → 排空 pending → activate
        let response = manager.subscribe("client-1", tx, None).await;
        assert_eq!(response.history_count, 5);

        // 收到历史 0-4
        for i in 0..5 {
            let e = rx.recv().await.unwrap();
            assert_eq!(e.index, i);
        }

        // subscribe 完成后，on_output 应正常接收
        manager.on_output(make_event(5)).await;
        let e = rx.recv().await.unwrap();
        assert_eq!(e.index, 5);
    }

    /// 验证 pending 排空跳过历史快照中已发送的事件：
    /// "占位→历史快照"窗口内的事件同时在历史与 pending 中，不能重复发送
    #[tokio::test]
    async fn test_drain_pending_skips_history_overlap() {
        let (tx, mut rx) = mpsc::channel(100);
        let sub = SubscriberState::new("client-1".to_string(), tx);

        // 模拟占位→快照窗口：事件 3、4 在占位后被 push，同时进入 pending；
        // 历史快照已包含 index <= 3 的事件（snapshot_max_seq = 3）
        sub.pending.write().await.push(make_event(3));
        sub.pending.write().await.push(make_event(4));

        sub.drain_pending(3).await;

        // 只应发送 index 4；index 3 已在历史快照中发送过，不能重复
        let e = rx.recv().await.unwrap();
        assert_eq!(e.index, 4);
        assert!(rx.try_recv().is_err());
    }

    /// 验证 inactive 期间 on_output 缓存到 pending
    #[tokio::test]
    async fn test_on_output_caches_to_pending_when_inactive() {
        let manager = SessionOutputManager::new("test-session");

        // 预填充历史
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
                sub.send_queue.send(make_event(i)).await.unwrap();
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
                sub.send_queue.send(event).await.unwrap();
            }
            drop(pending);
            let current_max = manager.output_queue.read().await.max_seq();
            sub.activate(current_max);
        }

        // 收到历史 + pending 事件
        for i in 0..5 {
            let e = rx.recv().await.unwrap();
            assert_eq!(e.index, i);
        }

        // 激活后 on_output 正常发送
        manager.on_output(make_event(5)).await;
        let e = rx.recv().await.unwrap();
        assert_eq!(e.index, 5);
    }

    /// 验证 activate 使用最新 max_seq，而非历史读取时的旧值
    #[tokio::test]
    async fn test_activate_uses_current_max_seq() {
        let manager = SessionOutputManager::new("test-session");

        // 预填充历史
        for i in 0..3 {
            manager.output_queue.write().await.push(make_event(i));
        }

        let (tx, mut rx) = mpsc::channel(100);

        // subscribe 完成后，sent_seq 应为当前 max_seq
        let response = manager.subscribe("client-1", tx, None).await;
        assert_eq!(response.max_seq, 2);

        // 验证 subscriber 的 sent_seq 是最新的
        {
            let subs = manager.subscribers.read().await;
            let sub = subs.get("client-1").unwrap();
            assert!(sub.is_active());
            assert_eq!(sub.sent_seq.load(Ordering::SeqCst), 2);
        }

        // 排空 subscribe 阶段已发送的历史事件，聚焦验证后续 on_output 无重复无丢失
        for i in 0..3 {
            let e = rx.recv().await.unwrap();
            assert_eq!(e.index, i);
        }

        // 后续 on_output 正常接收，无重复无丢失
        manager.on_output(make_event(3)).await;
        let e = rx.recv().await.unwrap();
        assert_eq!(e.index, 3);
    }
}
