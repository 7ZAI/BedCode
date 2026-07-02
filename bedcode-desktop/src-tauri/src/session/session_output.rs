//! Session Output
//!
//! PTY 输出相关的组件：输出缓存、统一输出队列、会话输出管理、全局输出管理
//! OutputCache trait 已内联到此文件（只有一个实现）

use crate::pty::PtyOutputEvent;
use crate::system::config::AppConfig;
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
pub struct UnifiedOutputQueue {
    buffer: std::collections::VecDeque<OutputEvent>,
    capacity: usize,
    max_seq: AtomicU64,
    min_seq: AtomicU64,
    total_produced: AtomicU64,
}

impl UnifiedOutputQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: std::collections::VecDeque::with_capacity(capacity),
            capacity,
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
    pub fn push(&mut self, event: OutputEvent) {
        self.max_seq.store(event.index, Ordering::SeqCst);
        self.total_produced.fetch_add(1, Ordering::SeqCst);

        if self.buffer.len() >= self.capacity {
            if let Some(old) = self.buffer.pop_front() {
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
}

impl SubscriberState {
    pub fn new(client_id: String, send_queue: mpsc::Sender<OutputEvent>) -> Self {
        Self {
            client_id,
            active: AtomicBool::new(false),
            sent_seq: AtomicU64::new(0),
            send_queue,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn activate(&self, sent_seq: u64) {
        self.sent_seq.store(sent_seq, Ordering::SeqCst);
        self.active.store(true, Ordering::SeqCst);
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
            }
        }
    }

    /// 订阅会话输出
    ///
    /// - `start_seq = None` 或 `0`：从头补完所有历史
    /// - `start_seq = N (N > 0)`：从指定序号开始获取，用于断线重连从断点继续
    pub async fn subscribe(
        &self,
        client_id: &str,
        ws_sender: mpsc::Sender<OutputEvent>,
        start_seq: Option<u64>,
    ) -> SubscribeResponse {
        let subscriber = SubscriberState::new(client_id.to_string(), ws_sender);

        let queue = self.output_queue.read().await;
        let min_seq = queue.min_seq();
        let max_seq = queue.max_seq();

        // start_seq 为 None 或 0 时从头获取，否则从指定序号开始
        let actual_start = start_seq.unwrap_or(0);
        let history = queue.get_range(actual_start);
        drop(queue);

        // 通过该订阅者的独立通道发送历史（保证顺序）
        for event in &history {
            if let Err(e) = subscriber.send_queue.send(event.clone()).await {
                tracing::warn!(
                    "[SessionOutputManager] Failed to send history to {}: {}",
                    client_id, e
                );
            }
        }

        subscriber.activate(max_seq);

        self.subscribers
            .write()
            .await
            .insert(client_id.to_string(), subscriber);

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
}
