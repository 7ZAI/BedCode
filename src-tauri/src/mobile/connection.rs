//! Mobile Connection Manager
//!
//! 连接管理 - 使用 shared WsClient 实现连接/断开/重连

use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::shared::websocket::{
    ConnectionStatus as WsConnStatus, WsClient, WsClientConfig, WsClientEvent, WsMessage,
};
use crate::Result;

use super::handler::{MobileEvent, MobileHandler};

// Re-export ConnectionStatus for public API
pub use crate::shared::websocket::ConnectionStatus;

/// 目标设备信息
#[derive(Debug, Clone)]
pub struct TargetDevice {
    pub address: String,
    pub port: u16,
    pub name: Option<String>,
}

/// 连接管理器
pub struct ConnectionManager {
    /// 当前连接状态
    status: Arc<RwLock<WsConnStatus>>,
    /// 目标设备
    target: Arc<RwLock<Option<TargetDevice>>>,
    /// WebSocket 客户端
    client: Arc<RwLock<Option<Arc<WsClient>>>>,
    /// 消息处理器
    handler: Arc<MobileHandler>,
    /// 事件发送器
    event_tx: broadcast::Sender<MobileEvent>,
    /// 运行标记
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl ConnectionManager {
    /// 创建新的连接管理器
    pub fn new() -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(1024);
        let handler = MobileHandler::new();

        Arc::new(Self {
            status: Arc::new(RwLock::new(WsConnStatus::Disconnected)),
            target: Arc::new(RwLock::new(None)),
            client: Arc::new(RwLock::new(None)),
            handler,
            event_tx,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// 获取当前连接状态
    pub async fn get_status(&self) -> WsConnStatus {
        self.status.read().await.clone()
    }

    /// 获取目标设备
    pub async fn get_target(&self) -> Option<TargetDevice> {
        self.target.read().await.clone()
    }

    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<MobileEvent> {
        self.event_tx.subscribe()
    }

    /// 连接到目标设备
    pub async fn connect(&self, address: String, port: u16, name: Option<String>) -> Result<()> {
        // 检查当前状态
        {
            let status = self.status.read().await.clone();
            if status == WsConnStatus::Connecting || status == WsConnStatus::Connected || status == WsConnStatus::Paired {
                tracing::warn!("Already connected or connecting");
                return Ok(());
            }
        }

        // 设置状态为连接中
        *self.status.write().await = WsConnStatus::Connecting;

        // 保存目标设备
        *self.target.write().await = Some(TargetDevice {
            address: address.clone(),
            port,
            name,
        });

        // 创建配置
        let config = WsClientConfig::new(&address, port);

        // 创建客户端
        let client = WsClient::new(config);

        // 设置消息处理器
        client.set_handler(self.handler.clone());

        // 启动运行标记
        self.running.store(true, std::sync::atomic::Ordering::SeqCst);

        // 启动事件转发任务（在 async 上下文中）
        let handler = self.handler.clone();
        let event_tx = self.event_tx.clone();
        let running = self.running.clone();
        tokio::spawn(async move {
            let mut rx = handler.subscribe();
            while running.load(std::sync::atomic::Ordering::SeqCst) {
                if let Ok(event) = rx.recv().await {
                    let _ = event_tx.send(event);
                } else {
                    break;
                }
            }
        });

        let client_clone = client.clone();
        let status = self.status.clone();
        let status_clone = self.status.clone();
        let event_tx_clone = self.event_tx.clone();
        let running = self.running.clone();

        // 在后台任务中运行连接
        tokio::spawn(async move {
            // 订阅客户端事件
            let mut rx = client_clone.subscribe();

            // 启动连接
            if let Err(e) = client_clone.connect().await {
                tracing::error!("Failed to connect: {}", e);
                *status.write().await = WsConnStatus::Error(e.to_string());
                let _ = event_tx_clone.send(MobileEvent::Error {
                    message: format!("Connection failed: {}", e),
                });
                return;
            }

            // 连接成功，更新状态
            *status.write().await = WsConnStatus::Connected;
            let _ = event_tx_clone.send(MobileEvent::Connected);

            // 等待连接断开
            while running.load(std::sync::atomic::Ordering::SeqCst) {
                if let Ok(event) = rx.recv().await {
                    match event {
                        WsClientEvent::Disconnected => {
                            *status_clone.write().await = WsConnStatus::Disconnected;
                            let _ = event_tx_clone.send(MobileEvent::Disconnected);
                            break;
                        }
                        WsClientEvent::ServerClosed { reason } => {
                            *status_clone.write().await = WsConnStatus::Disconnected;
                            let _ = event_tx_clone.send(MobileEvent::ServerClosed { reason });
                            break;
                        }
                        WsClientEvent::Error { message } => {
                            *status_clone.write().await = WsConnStatus::Error(message.clone());
                            let _ = event_tx_clone.send(MobileEvent::Error { message });
                        }
                        _ => {}
                    }
                }
            }
        });

        // 保存客户端引用
        *self.client.write().await = Some(client);

        // 等待连接建立（带超时）
        let timeout_ms = 10000;
        let start = std::time::Instant::now();
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            let status = self.status.read().await.clone();
            if status == WsConnStatus::Connected || status == WsConnStatus::Paired {
                return Ok(());
            }
            if let WsConnStatus::Error(e) = &status {
                return Err(crate::AppError::WebSocket(e.clone()));
            }

            if start.elapsed().as_millis() > timeout_ms as u128 {
                return Err(crate::AppError::WebSocket("Connection timeout".to_string()));
            }
        }
    }

    /// 断开连接
    pub async fn disconnect(&self) {
        tracing::info!("Disconnecting...");

        // 停止运行
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);

        // 断开 WebSocket
        if let Some(client) = self.client.read().await.as_ref() {
            let _ = client.disconnect().await;
        }

        // 清除客户端
        *self.client.write().await = None;

        // 更新状态
        *self.status.write().await = WsConnStatus::Disconnected;

        // 清除目标
        *self.target.write().await = None;
    }

    /// 发送消息
    pub async fn send(&self, message: &WsMessage) -> Result<()> {
        if let Some(client) = self.client.read().await.as_ref() {
            client.send(message).await
        } else {
            Err(crate::AppError::WebSocket("Not connected".to_string()))
        }
    }

    /// 发送消息并等待响应
    pub async fn send_and_wait(&self, message: &WsMessage, timeout: std::time::Duration) -> Result<WsMessage> {
        if let Some(client) = self.client.read().await.as_ref() {
            client.send_and_wait(message, timeout).await
        } else {
            Err(crate::AppError::WebSocket("Not connected".to_string()))
        }
    }

    /// 检查是否已连接
    pub async fn is_connected(&self) -> bool {
        let status = self.status.read().await;
        matches!(*status, WsConnStatus::Connected | WsConnStatus::Paired)
    }

    /// 设置为已配对状态
    pub async fn set_paired(&self) {
        *self.status.write().await = WsConnStatus::Paired;
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        let handler = MobileHandler::new();

        Self {
            status: Arc::new(RwLock::new(WsConnStatus::Disconnected)),
            target: Arc::new(RwLock::new(None)),
            client: Arc::new(RwLock::new(None)),
            handler,
            event_tx,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

impl Clone for ConnectionManager {
    fn clone(&self) -> Self {
        Self {
            status: self.status.clone(),
            target: self.target.clone(),
            client: self.client.clone(),
            handler: self.handler.clone(),
            event_tx: self.event_tx.clone(),
            running: self.running.clone(),
        }
    }
}