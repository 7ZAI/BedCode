//! Global Output Manager
//!
//! 全局输出管理器 - 管理所有 PTY 会话的输出管理器
//! 单例模式，统一接收 PTY 输出并分发给对应会话

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use std::sync::OnceLock;

use super::session_output_manager::{SessionOutputManager, SubscribeResponse};
use super::unified_output_queue::OutputEvent;

/// 全局输出管理器
pub struct GlobalOutputManager {
    /// 会话输出管理器表（session_id -> SessionOutputManager）
    sessions: RwLock<HashMap<String, Arc<SessionOutputManager>>>,
}

impl GlobalOutputManager {
    /// 创建新管理器
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// 获取单例
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

    /// 检查会话是否存在
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
    pub async fn subscribe(
        &self,
        session_id: &str,
        client_id: &str,
        ws_sender: mpsc::Sender<OutputEvent>,
    ) -> Option<SubscribeResponse> {
        let sessions = self.sessions.read().await;
        if let Some(manager) = sessions.get(session_id) {
            Some(manager.subscribe(client_id, ws_sender).await)
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

    /// 取消某客户端在所有会话中的订阅
    /// 用于客户端断开连接时清理所有订阅
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_event(session_id: &str, index: u64) -> OutputEvent {
        OutputEvent {
            session_id: session_id.to_string(),
            data: "dGVzdA==".to_string(),
            index,
            timestamp: Utc::now().timestamp_millis(),
            is_waiting: false,
        }
    }

    #[tokio::test]
    async fn test_register_and_on_output() {
        let manager = GlobalOutputManager::new();

        // 注册会话
        manager.register_session("session-1").await;

        // 创建订阅者
        let (tx, mut rx) = mpsc::channel(100);
        manager.subscribe("session-1", "client-1", tx).await;

        // 发送输出
        manager.on_output(make_event("session-1", 0)).await;

        // 接收
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

        manager.subscribe("session-1", "client-1", tx1).await;
        manager.subscribe("session-2", "client-2", tx2).await;

        manager.on_output(make_event("session-1", 0)).await;
        manager.on_output(make_event("session-2", 0)).await;

        // 各收到各自会话的输出
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

        // 发送输出应无效
        manager.on_output(make_event("session-1", 0)).await;
        // 不 crash 即通过
    }
}
