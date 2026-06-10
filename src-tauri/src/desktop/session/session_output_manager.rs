//! Session Output Manager
//!
//! 单个 PTY 会话的输出管理，包括输出队列和订阅者管理
//! 支持多移动端同时订阅，每个订阅者独立发送通道

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use super::unified_output_queue::{OutputEvent, UnifiedOutputQueue};

/// 订阅者状态
pub struct SubscriberState {
    /// 客户端 ID
    pub client_id: String,
    /// 订阅是否活跃（历史发送完成后才标记为 true）
    pub active: AtomicBool,
    /// 已发送到的序号（用于调试）
    pub sent_seq: AtomicU64,
    /// 独立发送通道（绑定该客户端的 WebSocket）
    pub send_queue: mpsc::Sender<OutputEvent>,
}

impl SubscriberState {
    /// 创建新订阅者状态
    pub fn new(client_id: String, send_queue: mpsc::Sender<OutputEvent>) -> Self {
        Self {
            client_id,
            active: AtomicBool::new(false),
            sent_seq: AtomicU64::new(0),
            send_queue,
        }
    }

    /// 检查是否活跃
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// 激活订阅
    pub fn activate(&self, sent_seq: u64) {
        self.sent_seq.store(sent_seq, Ordering::SeqCst);
        self.active.store(true, Ordering::SeqCst);
    }
}

/// 订阅响应
#[derive(Debug, Clone)]
pub struct SubscribeResponse {
    /// 最小可用序号
    pub min_seq: u64,
    /// 最大序号
    pub max_seq: u64,
    /// 历史消息数量
    pub history_count: usize,
}

/// 会话输出管理器
pub struct SessionOutputManager {
    /// 会话 ID
    session_id: String,
    /// 统一输出队列
    output_queue: Arc<RwLock<UnifiedOutputQueue>>,
    /// 订阅者表（client_id -> SubscriberState）
    subscribers: RwLock<HashMap<String, SubscriberState>>,
}

impl SessionOutputManager {
    /// 创建新管理器
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            output_queue: Arc::new(RwLock::new(UnifiedOutputQueue::default())),
            subscribers: RwLock::new(HashMap::new()),
        }
    }

    /// 获取会话 ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 处理新输出
    pub async fn on_output(&self, event: OutputEvent) {
        // 1. 写入共享队列
        self.output_queue.write().await.push(event.clone());

        // 2. 分别发送给每个活跃订阅者（独立通道）
        let subscribers = self.subscribers.read().await;
        for subscriber in subscribers.values() {
            if subscriber.is_active() {
                // 每个订阅者独立发送，保证各自顺序
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
    /// ws_sender: 该客户端的 WebSocket 发送通道
    pub async fn subscribe(
        &self,
        client_id: &str,
        ws_sender: mpsc::Sender<OutputEvent>,
    ) -> SubscribeResponse {
        // 创建订阅者状态（先不激活）
        let subscriber = SubscriberState::new(client_id.to_string(), ws_sender);

        // 获取队列状态和历史数据
        let queue = self.output_queue.read().await;
        let min_seq = queue.min_seq();
        let max_seq = queue.max_seq();
        let history = queue.get_range(min_seq);
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

        // 历史发送完成后，激活订阅（开始接收实时数据）
        subscriber.activate(max_seq);

        // 注册订阅者
        self.subscribers
            .write()
            .await
            .insert(client_id.to_string(), subscriber);

        tracing::info!(
            "[SessionOutputManager] Client {} subscribed to session {}, history_count={}",
            client_id,
            self.session_id,
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

    /// 检查客户端是否订阅
    pub async fn is_subscribed(&self, client_id: &str) -> bool {
        self.subscribers.read().await.contains_key(client_id)
    }

    /// 获取活跃订阅者数量
    pub async fn active_subscriber_count(&self) -> usize {
        self.subscribers
            .read()
            .await
            .values()
            .filter(|s| s.is_active())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(index: u64) -> OutputEvent {
        OutputEvent::new(
            "test-session".to_string(),
            b"test".to_vec(),
            index,
            chrono::Utc::now().timestamp_millis(),
            false,
        )
    }

    #[tokio::test]
    async fn test_subscribe_and_on_output() {
        let manager = SessionOutputManager::new("test-session");

        // 创建模拟发送通道
        let (tx, mut rx) = mpsc::channel(100);

        // 先写入一些历史数据
        manager.output_queue.write().await.push(make_event(0));
        manager.output_queue.write().await.push(make_event(1));

        // 订阅
        let response = manager.subscribe("client-1", tx).await;
        assert_eq!(response.min_seq, 0);
        assert_eq!(response.max_seq, 1);
        assert_eq!(response.history_count, 2);

        // 接收历史数据
        let event1 = rx.recv().await.unwrap();
        assert_eq!(event1.index, 0);
        let event2 = rx.recv().await.unwrap();
        assert_eq!(event2.index, 1);

        // 发送实时数据
        manager.on_output(make_event(2)).await;
        let event3 = rx.recv().await.unwrap();
        assert_eq!(event3.index, 2);
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let manager = SessionOutputManager::new("test-session");

        let (tx1, mut rx1) = mpsc::channel(100);
        let (tx2, mut rx2) = mpsc::channel(100);

        // 两个客户端订阅
        manager.subscribe("client-1", tx1).await;
        manager.subscribe("client-2", tx2).await;

        // 发送实时数据
        manager.on_output(make_event(0)).await;

        // 两个客户端都收到
        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert_eq!(e1.index, 0);
        assert_eq!(e2.index, 0);
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let manager = SessionOutputManager::new("test-session");

        let (tx, _rx) = mpsc::channel(100);
        manager.subscribe("client-1", tx).await;

        manager.unsubscribe("client-1").await;

        assert!(!manager.is_subscribed("client-1").await);
    }
}
