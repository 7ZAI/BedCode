//! Pty Subscription Handler
//!
//! 实现 PtyOutputHandler trait，用于移动端订阅场景
//! - 将 PTY 输出写入环形缓冲区（持久化）
//! - 触发实时广播给所有活跃订阅者
//! - 触发历史消息补推（当新订阅者加入时）

use crate::desktop::model::PtyOutputEvent;
use crate::desktop::pty::subscription::{PtySubscriptionManager, Subscription};
use crate::desktop::traits::PtyOutputHandler;
use crate::desktop::websocket_manager::WebSocketManager;
use crate::shared::enums::message::{Message, OutputPayload};
use async_trait::async_trait;
use std::sync::Arc;

/// PTY 订阅处理器
///
/// 实现 PtyOutputHandler trait，作为 PTY 输出事件的处理器：
/// 1. 将输出事件写入环形缓冲区（持久化）
/// 2. 通过 broadcast 通道广播给所有实时订阅者
/// 3. 自动触发新订阅者的历史消息补推
pub struct PtySubscriptionHandler {
    name: String,
    session_id: String,
    subscription_manager: Arc<PtySubscriptionManager>,
}

impl PtySubscriptionHandler {
    /// 创建新的订阅处理器
    pub fn new(
        session_id: impl Into<String>,
        subscription_manager: Arc<PtySubscriptionManager>,
    ) -> Self {
        let session_id_str = session_id.into();
        Self {
            name: format!("PtySubscriptionHandler-{}", session_id_str),
            session_id: session_id_str,
            subscription_manager,
        }
    }

    /// 发送实时输出给所有活跃订阅者
    async fn broadcast_to_subscribers(&self, event: &PtyOutputEvent) {
        let ws_manager = WebSocketManager::global();

        // 获取所有活跃订阅者
        let subscribers = self
            .subscription_manager
            .get_active_subscribers(&self.session_id)
            .await;

        if subscribers.is_empty() {
            return;
        }

        // 构建消息
        let message = Message::Output {
            message_id: format!("realtime-{}-{}", self.session_id, event.index),
            session_id: self.session_id.clone(),
            timestamp: event.timestamp.timestamp_millis(),
            payload: OutputPayload {
                data: event.data.clone(),
                is_waiting: event.is_waiting,
                index: event.index,
            },
        };

        // 广播给所有订阅者
        for client_id in subscribers {
            if let Err(e) = ws_manager.send_to_client(&client_id, &message).await {
                tracing::warn!(
                    "Failed to send realtime output to client {}: {}",
                    client_id,
                    e
                );
            }
        }
    }

    /// 处理新订阅者：发送历史消息补推
    async fn send_pending_history(&self, subscription: &Subscription) {
        // 如果没有历史消息，跳过
        let ring_buffer = {
            let sessions = self.subscription_manager.sessions.read().await;
            match sessions.get(&self.session_id) {
                Some(session) => session.ring_buffer.clone(),
                None => return,
            }
        };

        let buffer = ring_buffer.read().await;
        let max_seq = buffer.max_seq();
        drop(buffer);

        // 如果订阅的起始序号大于等于当前最大序号，说明没有历史消息需要补推
        if subscription.start_seq >= max_seq {
            tracing::debug!(
                "No pending history for client {} (start_seq={}, max_seq={})",
                subscription.client_id,
                subscription.start_seq,
                max_seq
            );
            return;
        }

        // 发送历史消息
        let ws_manager = WebSocketManager::global();
        let buffer = ring_buffer.read().await;
        let messages = buffer.get_since(subscription.start_seq);
        let count = messages.len();
        drop(buffer);

        if count == 0 {
            return;
        }

        tracing::info!(
            "Sending {} pending history messages to client {}, start_seq={}",
            count,
            subscription.client_id,
            subscription.start_seq
        );

        for event in messages {
            let message = Message::Output {
                message_id: format!("history-{}-{}", self.session_id, event.index),
                session_id: self.session_id.clone(),
                timestamp: event.timestamp.timestamp_millis(),
                payload: OutputPayload {
                    data: event.data,
                    is_waiting: event.is_waiting,
                    index: event.index,
                },
            };

            if let Err(e) = ws_manager
                .send_to_client(&subscription.client_id, &message)
                .await
            {
                tracing::warn!(
                    "Failed to send pending history message {} to client {}: {}",
                    event.index,
                    subscription.client_id,
                    e
                );
            }
        }

        tracing::debug!(
            "Sent {} pending history messages to client {}",
            count,
            subscription.client_id
        );
    }
}

#[async_trait]
impl PtyOutputHandler for PtySubscriptionHandler {
    async fn handle(&self, event: PtyOutputEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 1. 将事件写入环形缓冲区（持久化）
        {
            let sessions = self.subscription_manager.sessions.read().await;
            if let Some(session) = sessions.get(&self.session_id) {
                let mut buffer = session.ring_buffer.write().await;
                buffer.push(event.clone());
            } else {
                tracing::warn!("Session {} not found in subscription manager", self.session_id);
            }
        }

        // 2. 触发待处理的订阅者发送历史消息
        let pending_subscriptions: Vec<Subscription> = {
            let sessions = self.subscription_manager.sessions.read().await;
            if let Some(session) = sessions.get(&self.session_id) {
                let subs = session.subscriptions.read().await;
                subs.values()
                    .filter(|s| s.active)
                    .cloned()
                    .collect()
            } else {
                vec![]
            }
        };

        // 检查每个订阅者是否有待补推的历史消息
        for subscription in pending_subscriptions {
            let ring_buffer = {
                let sessions = self.subscription_manager.sessions.read().await;
                match sessions.get(&self.session_id) {
                    Some(session) => session.ring_buffer.clone(),
                    None => continue,
                }
            };

            let max_seq = {
                let buffer = ring_buffer.read().await;
                buffer.max_seq()
            };

            // 如果订阅的起始序号 < 当前最大序号，说明有新消息需要补推
            if subscription.start_seq < max_seq {
                self.send_pending_history(&subscription).await;
            }
        }

        // 3. 广播实时输出给所有活跃订阅者
        self.broadcast_to_subscribers(&event).await;

        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
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

    #[tokio::test]
    async fn test_handler_name() {
        let manager = Arc::new(PtySubscriptionManager::new());
        let handler = PtySubscriptionHandler::new("session-1", manager);

        assert!(handler.name().contains("PtySubscriptionHandler"));
        assert!(handler.name().contains("session-1"));
    }
}