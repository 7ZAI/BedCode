//! Lifecycle Module - Connection State Machine
//!
//! 职责：管理连接生命周期状态，提供状态转换和事件钩子
//! 状态：未连接、连接中、已连接、已配对、断开、重连中

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// 连接状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// 未连接
    Disconnected,
    /// 正在连接
    Connecting,
    /// 已连接（WebSocket 连接已建立，等待认证）
    Connected,
    /// 已认证（配对成功）
    Paired,
    /// 连接错误
    Error(String),
}

/// 连接事件
#[derive(Debug, Clone)]
pub enum LifecycleEvent {
    /// 状态变为已连接
    Connected,
    /// 状态变为已配对
    Paired,
    /// 状态变为断开
    Disconnected,
    /// 进入重连中
    Reconnecting {
        attempt: u32,
        delay_secs: u64,
    },
    /// 连接错误
    Error {
        message: String,
    },
    /// 重连成功
    Reconnected,
    /// 重连失败（达到最大重试次数）
    ReconnectFailed {
        attempts: u32,
        last_error: String,
    },
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
        let (event_tx, _) = broadcast::channel(1024);
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
                let _ = self.event_tx.send(LifecycleEvent::Error {
                    message: msg.clone(),
                });
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

    /// 检查是否已连接（Connected 或 Paired）
    pub async fn is_connected(&self) -> bool {
        let status = self.status.read().await;
        *status == ConnectionStatus::Connected || *status == ConnectionStatus::Paired
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
            ConnectionStatus::Disconnected | ConnectionStatus::Error(_) | ConnectionStatus::Paired
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
            event_tx: broadcast::channel(1024).0,
            client_id: RwLock::new(None),
        }
    }
}