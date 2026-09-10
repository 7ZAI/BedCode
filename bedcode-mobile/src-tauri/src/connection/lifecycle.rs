//! Lifecycle Module - Connection State Machine
//!
//! 职责：管理连接生命周期状态，提供状态转换和事件钩子
//! 状态：未连接、连接中、已连接、已认证（HTTP）、已配对、断开、重连中
//!
//! 注意：认证已 HTTP 化后（spec §4.5），设备级的「已认证」语义由
//! ConnectionManager 自有 status 承载（Authed），本 LifecycleManager 仅服务
//! WsClient 的 WS 层状态（04 事件 WS 重新引入后才恢复完整语义）。

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::system::constants::connection::BROADCAST_CHANNEL_CAPACITY;

/// 连接状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// 未连接
    Disconnected,
    /// 正在连接
    Connecting,
    /// 已连接（WebSocket 连接已建立，等待认证）
    Connected,
    /// 已认证（设备级 HTTP 认证成功，04 的事件 WS 复用此状态）
    Authed,
    /// 已配对（WS 时代遗留语义，保留兼容）
    Paired,
    /// 连接错误
    Error(String),
}

/// 连接事件
#[derive(Debug, Clone)]
pub enum LifecycleEvent {
    /// 状态变为已连接
    Connected,
    /// 状态变为已认证（设备级 HTTP 认证成功）
    Authed,
    /// 状态变为已配对
    Paired,
    /// 状态变为断开
    Disconnected,
    /// 进入重连中
    Reconnecting { attempt: u32, delay_secs: u64 },
    /// 连接错误
    Error { message: String },
    /// 重连成功
    Reconnected,
    /// 重连失败（达到最大重试次数）
    ReconnectFailed { attempts: u32, last_error: String },
}

/// 生命周期管理器
pub struct LifecycleManager {
    /// 当前连接状态
    status: RwLock<ConnectionStatus>,
    /// 事件广播器
    event_tx: broadcast::Sender<LifecycleEvent>,
    /// 客户端ID（配对后设置）
    client_id: RwLock<Option<String>>,
}

impl LifecycleManager {
    /// 创建新的生命周期管理器
    pub fn new() -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        Arc::new(Self {
            status: RwLock::new(ConnectionStatus::Disconnected),
            event_tx,
            client_id: RwLock::new(None),
        })
    }

    /// 获取当前状态
    pub async fn get_status(&self) -> ConnectionStatus {
        self.status.read().await.clone()
    }

    /// 设置状态
    pub async fn set_status(&self, status: ConnectionStatus) {
        let old_status = self.status.read().await.clone();
        *self.status.write().await = status.clone();

        // 根据状态变化发送事件
        match &status {
            ConnectionStatus::Connected => {
                let _ = self.event_tx.send(LifecycleEvent::Connected);
            }
            ConnectionStatus::Authed => {
                let _ = self.event_tx.send(LifecycleEvent::Authed);
            }
            ConnectionStatus::Paired => {
                let _ = self.event_tx.send(LifecycleEvent::Paired);
            }
            ConnectionStatus::Disconnected => {
                let _ = self.event_tx.send(LifecycleEvent::Disconnected);
            }
            ConnectionStatus::Connecting => {
                // 连接中不需要特殊事件
            }
            ConnectionStatus::Error(msg) => {
                let _ = self.event_tx.send(LifecycleEvent::Error { message: msg.clone() });
            }
        }

        tracing::debug!("Status changed: {:?} -> {:?}", old_status, status);
    }

    /// 设置客户端ID（配对成功后调用）
    pub async fn set_client_id(&self, client_id: impl Into<String>) {
        let mut guard = self.client_id.write().await;
        *guard = Some(client_id.into());
    }

    /// 获取客户端ID
    pub async fn get_client_id(&self) -> Option<String> {
        self.client_id.read().await.clone()
    }

    /// 检查是否已连接（Connected、Authed 或 Paired）
    pub async fn is_connected(&self) -> bool {
        let status = self.status.read().await;
        matches!(
            *status,
            ConnectionStatus::Connected | ConnectionStatus::Authed | ConnectionStatus::Paired
        )
    }

    /// 检查是否正在连接
    pub async fn is_connecting(&self) -> bool {
        let status = self.status.read().await;
        *status == ConnectionStatus::Connecting
    }

    /// 检查是否可以重连
    pub async fn can_reconnect(&self) -> bool {
        let status = self.status.read().await;
        matches!(
            *status,
            ConnectionStatus::Disconnected
                | ConnectionStatus::Error(_)
                | ConnectionStatus::Authed
                | ConnectionStatus::Paired
        )
    }

    /// 订阅生命周期事件
    pub fn subscribe(&self) -> broadcast::Receiver<LifecycleEvent> {
        self.event_tx.subscribe()
    }

    /// 发送事件
    pub fn emit(&self, event: LifecycleEvent) {
        let _ = self.event_tx.send(event);
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self {
            status: RwLock::new(ConnectionStatus::Disconnected),
            event_tx: broadcast::channel(BROADCAST_CHANNEL_CAPACITY).0,
            client_id: RwLock::new(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 状态转换 + 判定 + Authed 事件发射
    #[tokio::test]
    async fn status_transitions_and_judgements() {
        let mgr = LifecycleManager::new();
        assert_eq!(mgr.get_status().await, ConnectionStatus::Disconnected);
        assert!(!mgr.is_connected().await);
        assert!(mgr.can_reconnect().await);

        // Connected：判定为已连接（未认证的 WS 语义）
        mgr.set_status(ConnectionStatus::Connected).await;
        assert!(mgr.is_connected().await);

        // Authed：纳入已连接与可重连判定，并发射 Authed 事件
        let mut rx = mgr.subscribe();
        mgr.set_status(ConnectionStatus::Authed).await;
        assert_eq!(mgr.get_status().await, ConnectionStatus::Authed);
        assert!(mgr.is_connected().await, "Authed 应视为已连接");
        assert!(mgr.can_reconnect().await, "Authed 应允许重连");
        let ev = rx.try_recv().expect("set_status(Authed) 应发 Authed 事件");
        assert!(matches!(ev, LifecycleEvent::Authed));

        // 断开：判定复位
        mgr.set_status(ConnectionStatus::Disconnected).await;
        assert!(!mgr.is_connected().await);
        let ev = rx.try_recv().expect("set_status(Disconnected) 应发 Disconnected 事件");
        assert!(matches!(ev, LifecycleEvent::Disconnected));
    }
}
