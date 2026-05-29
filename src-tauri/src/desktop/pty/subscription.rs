//! PTY Output Subscription Module
//!
//! 提供 PTY 输出的订阅消费机制，满足"持久化订阅 + 实时广播"场景

use crate::desktop::model::PtyOutputEvent;
use crate::desktop::websocket_manager::WebSocketManager;
use crate::shared::enums::{TerminalAction, TerminalPayload};
use crate::shared::model::message::Message;
use crate::shared::system::error::AppError;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// 环形缓冲区 - 存储最近 N 条 PTY 输出消息
pub struct OutputRingBuffer {
    buffer: Vec<Option<PtyOutputEvent>>,
    capacity: usize,
    head: usize,
    count: usize,
    max_seq: AtomicU64,
    total_produced: AtomicU64,
}

impl OutputRingBuffer {
    /// 创建新的环形缓冲区
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![None; capacity],
            capacity,
            head: 0,
            count: 0,
            max_seq: AtomicU64::new(0),
            total_produced: AtomicU64::new(0),
        }
    }

    /// 推送新消息
    pub fn push(&mut self, event: PtyOutputEvent) {
        let index = event.index as u64;

        // 更新 max_seq
        self.max_seq.store(index, Ordering::SeqCst);
        self.total_produced.fetch_add(1, Ordering::SeqCst);

        // 写入环形缓冲区
        self.buffer[self.head] = Some(event);
        self.head = (self.head + 1) % self.capacity;

        if self.count < self.capacity {
            self.count += 1;
        }
    }

    /// 获取从 start_seq 之后的所有消息（包括 start_seq）
    pub fn get_since(&self, start_seq: u64) -> Vec<PtyOutputEvent> {
        if self.count == 0 {
            return vec![];
        }

        let mut result = Vec::new();

        // 遍历缓冲区，从最旧到最新
        for i in 0..self.count {
            let idx = (self.head + self.capacity - self.count + i) % self.capacity;
            if let Some(ref event) = self.buffer[idx] {
                if (event.index as u64) >= start_seq {
                    result.push(event.clone());
                }
            }
        }

        result
    }

    /// 获取当前最大序号
    pub fn max_seq(&self) -> u64 {
        self.max_seq.load(Ordering::SeqCst)
    }

    /// 获取历史总消息数
    pub fn total_produced(&self) -> u64 {
        self.total_produced.load(Ordering::SeqCst)
    }

    /// 获取缓冲区容量
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 获取当前消息数
    pub fn len(&self) -> usize {
        self.count
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// 清空缓冲区
    pub fn clear(&mut self) {
        for i in 0..self.capacity {
            self.buffer[i] = None;
        }
        self.head = 0;
        self.count = 0;
        self.max_seq.store(0, Ordering::SeqCst);
        self.total_produced.store(0, Ordering::SeqCst);
    }
}

impl Default for OutputRingBuffer {
    fn default() -> Self {
        Self::new(10000)
    }
}



// ==================== Subscription Manager ====================

/// 订阅状态
#[derive(Debug, Clone)]
pub struct Subscription {
    /// 客户端 ID
    pub client_id: String,
    /// 会话 ID
    pub session_id: String,
    /// 客户端指定起始序号
    pub start_seq: u64,
    /// 订阅时间戳（毫秒）
    pub subscribed_at: i64,
    /// 是否活跃
    pub active: bool,
}

/// 订阅响应
#[derive(Debug, Clone)]
pub struct SubscribeResponse {
    /// 当前最大序号
    pub current_max_seq: u64,
    /// 历史消息数量
    pub history_count: usize,
}

/// 单个会话的订阅状态
pub struct PtySessionSubscriptions {
    /// 会话 ID
    pub session_id: String,
    /// 输出环形缓冲区
    pub ring_buffer: Arc<RwLock<OutputRingBuffer>>,
    /// 实时广播发送器
    pub broadcast_tx: broadcast::Sender<PtyOutputEvent>,
    /// 订阅表
    pub subscriptions: RwLock<HashMap<String, Subscription>>,
}

/// 订阅管理器
///
/// 管理所有 PTY 会话的订阅状态，支持客户端指定起始序号进行历史回放
pub struct PtySubscriptionManager {
    /// 会话订阅状态表
    pub sessions: RwLock<HashMap<String, Arc<PtySessionSubscriptions>>>,
}

impl PtySubscriptionManager {
    /// 创建新的订阅管理器
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// 注册会话（由 PtySession 创建时调用）
    ///
    /// 返回环形缓冲区的引用和广播发送器
    pub fn register_session(
        &self,
        session_id: &str,
    ) -> (Arc<RwLock<OutputRingBuffer>>, broadcast::Sender<PtyOutputEvent>) {
        let ring_buffer = Arc::new(RwLock::new(OutputRingBuffer::new(10000)));
        let (broadcast_tx, _) = broadcast::channel(1024);

        let session = Arc::new(PtySessionSubscriptions {
            session_id: session_id.to_string(),
            ring_buffer: ring_buffer.clone(),
            broadcast_tx: broadcast_tx.clone(),
            subscriptions: RwLock::new(HashMap::new()),
        });

        let mut sessions = self.sessions.blocking_write();
        sessions.insert(session_id.to_string(), session);

        (ring_buffer, broadcast_tx)
    }

    /// 取消注册会话
    pub fn unregister_session(&self, session_id: &str) {
        let mut sessions = self.sessions.blocking_write();
        sessions.remove(session_id);
    }

    /// 客户端订阅
    ///
    /// - `client_id`: 客户端标识
    /// - `session_id`: 要订阅的会话 ID
    /// - `start_seq`: 起始序号，None 表示从头补完
    pub async fn subscribe(
        &self,
        client_id: String,
        session_id: String,
        start_seq: Option<u64>,
    ) -> Result<SubscribeResponse, String> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| format!("Session {} not found", session_id))?;

        let ring_buffer = session.ring_buffer.read().await;
        let max_seq = ring_buffer.max_seq();
        let total_count = ring_buffer.total_produced() as usize;
        drop(ring_buffer);

        // 确定起始序号：None 则从 0 开始
        let actual_start = start_seq.unwrap_or(0);

        // 注册订阅
        let subscription = Subscription {
            client_id: client_id.clone(),
            session_id: session_id.clone(),
            start_seq: actual_start,
            subscribed_at: Utc::now().timestamp_millis(),
            active: true,
        };

        session
            .subscriptions
            .write()
            .await
            .insert(client_id.clone(), subscription);

        // 异步发送历史消息（不阻塞订阅响应）
        let client_id_clone = client_id.clone();
        let session_id_clone = session_id.clone();
        let start_seq_clone = actual_start;

        tokio::spawn(async move {
            Self::send_history(&session_id_clone, &client_id_clone, start_seq_clone).await;
        });

        Ok(SubscribeResponse {
            current_max_seq: max_seq,
            history_count: total_count,
        })
    }

    /// 取消订阅
    pub async fn unsubscribe(&self, client_id: &str, session_id: &str) -> Result<(), String> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| format!("Session {} not found", session_id))?;

        let mut subs = session.subscriptions.write().await;
        if let Some(sub) = subs.get_mut(client_id) {
            sub.active = false;
        }
        subs.remove(client_id);

        Ok(())
    }

    /// 发送历史消息（内部方法）
    /// 将指定起始序号之后的所有消息发送给指定客户端
    pub async fn send_history_to_client(
        &self,
        client_id: &str,
        session_id: &str,
        start_seq: u64,
    ) -> Result<usize, AppError> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| AppError::Internal(format!("Session {} not found", session_id)))?;

        // 从环形缓冲区获取历史消息
        let ring_buffer = session.ring_buffer.read().await;
        let messages = ring_buffer.get_since(start_seq);
        let count = messages.len();
        drop(ring_buffer);

        if count == 0 {
            tracing::debug!(
                "No history messages to send to client {} for session {} (start_seq={})",
                client_id,
                session_id,
                start_seq
            );
            return Ok(0);
        }

        // 通过 WebSocketManager 发送历史消息
        let ws_manager = WebSocketManager::global();
        let mut last_index: Option<usize> = None;

        for event in messages {
            last_index = Some(event.index);

            let message = Message::Terminal {
                message_id: format!("history-{}-{}", session_id, event.index),
                expect_response: false,
                timestamp: event.timestamp.timestamp_millis(),
                session_id: session_id.to_string(),
                token: String::new(),
                payload: TerminalPayload {
                    action: TerminalAction::Output {
                        data: event.data,
                        is_waiting: event.is_waiting,
                        index: event.index,
                    },
                },
            };

            if let Err(e) = ws_manager.send_to_client(client_id, &message).await {
                tracing::warn!(
                    "Failed to send history message {} to client {}: {}",
                    event.index,
                    client_id,
                    e
                );
            }
        }

        tracing::debug!(
            "Sent {} history messages to client {} for session {}, last_index={:?}",
            count,
            client_id,
            session_id,
            last_index
        );

        Ok(count)
    }

    /// 内部：发送历史消息（由 subscribe 调用）
    async fn send_history(session_id: &str, client_id: &str, start_seq: u64) {
        let ws_manager = WebSocketManager::global();
        let subscription_manager = ws_manager.subscription_manager();

        // 获取会话
        let sessions = subscription_manager.sessions.read().await;
        let session = match sessions.get(session_id) {
            Some(s) => s,
            None => {
                tracing::warn!("Session {} not found for history send", session_id);
                return;
            }
        };

        // 从环形缓冲区获取历史消息
        let ring_buffer = session.ring_buffer.read().await;
        let messages = ring_buffer.get_since(start_seq);
        let count = messages.len();
        drop(ring_buffer);

        if count == 0 {
            tracing::debug!(
                "No history messages to send to client {} for session {} (start_seq={})",
                client_id,
                session_id,
                start_seq
            );
            return;
        }

        // 释放 sessions 锁
        drop(sessions);

        // 通过 WebSocketManager 发送历史消息
        for event in messages {
            let message = Message::Terminal {
                message_id: format!("history-{}-{}", session_id, event.index),
                expect_response: false,
                timestamp: event.timestamp.timestamp_millis(),
                session_id: session_id.to_string(),
                token: String::new(),
                payload: TerminalPayload {
                    action: TerminalAction::Output {
                        data: event.data,
                        is_waiting: event.is_waiting,
                        index: event.index,
                    },
                },
            };

            if let Err(e) = ws_manager.send_to_client(client_id, &message).await {
                tracing::warn!(
                    "Failed to send history message {} to client {}: {}",
                    event.index,
                    client_id,
                    e
                );
            }
        }

        tracing::debug!(
            "Sent {} history messages to client {} for session {}, start_seq={}",
            count,
            client_id,
            session_id,
            start_seq
        );
    }

    /// 获取会话的广播发送器
    pub fn get_broadcast_sender(&self, session_id: &str) -> Option<broadcast::Sender<PtyOutputEvent>> {
        let sessions = self.sessions.blocking_read();
        sessions.get(session_id).map(|s| s.broadcast_tx.clone())
    }

    /// 检查客户端是否订阅了指定会话
    pub async fn is_subscribed(&self, client_id: &str, session_id: &str) -> bool {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            let subs = session.subscriptions.read().await;
            subs.get(client_id).map(|s| s.active).unwrap_or(false)
        } else {
            false
        }
    }

    /// 获取会话的所有活跃订阅者
    pub async fn get_active_subscribers(&self, session_id: &str) -> Vec<String> {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            let subs = session.subscriptions.read().await;
            subs.values()
                .filter(|s| s.active)
                .map(|s| s.client_id.clone())
                .collect()
        } else {
            vec![]
        }
    }

    /// 获取订阅信息
    pub async fn get_subscription(
        &self,
        client_id: &str,
        session_id: &str,
    ) -> Option<Subscription> {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            let subs = session.subscriptions.read().await;
            subs.get(client_id).cloned()
        } else {
            None
        }
    }
}

impl Default for PtySubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_event(index: usize) -> PtyOutputEvent {
        PtyOutputEvent {
            session_id: "test".to_string(),
            data: format!("data{}", index),
            timestamp: Utc::now(),
            is_waiting: false,
            index,
        }
    }

    #[test]
    fn test_ring_buffer_push_and_get() {
        let mut buffer = OutputRingBuffer::new(10);

        for i in 0..5 {
            buffer.push(make_event(i));
        }

        let messages = buffer.get_since(0);
        assert_eq!(messages.len(), 5);
    }

    #[test]
    fn test_ring_buffer_wrap_around() {
        let mut buffer = OutputRingBuffer::new(3);

        for i in 0..5 {
            buffer.push(make_event(i));
        }

        // 应该只保留最新的 3 条 (index 2, 3, 4)
        let messages = buffer.get_since(0);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].index, 2);
    }

    #[test]
    fn test_get_since_start() {
        let mut buffer = OutputRingBuffer::new(10);

        for i in 0..10 {
            buffer.push(make_event(i));
        }

        // 从 index 5 开始获取
        let messages = buffer.get_since(5);
        assert_eq!(messages.len(), 5); // 5, 6, 7, 8, 9
        assert_eq!(messages[0].index, 5);
    }

    #[test]
    fn test_empty_buffer() {
        let buffer = OutputRingBuffer::new(10);
        assert!(buffer.is_empty());
        assert_eq!(buffer.get_since(0).len(), 0);
    }

    #[test]
    fn test_max_seq() {
        let mut buffer = OutputRingBuffer::new(10);

        buffer.push(make_event(5));
        assert_eq!(buffer.max_seq(), 5);

        buffer.push(make_event(10));
        assert_eq!(buffer.max_seq(), 10);
    }

    #[test]
    fn test_total_produced() {
        let mut buffer = OutputRingBuffer::new(3);

        buffer.push(make_event(0));
        buffer.push(make_event(1));
        buffer.push(make_event(2));
        buffer.push(make_event(3)); // 触发环覆盖

        // total_produced 仍然累加
        assert_eq!(buffer.total_produced(), 4);
    }
}