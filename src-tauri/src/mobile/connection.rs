//! Mobile Connection Manager
//!
//! 连接管理 - 使用 shared WsClient 实现连接/断开/重连

use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tauri::{AppHandle, Emitter};
use log;

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
///
/// 职责：作为业务层代理，直接使用 WsClient 的状态
/// 不再维护独立的 ConnectionStatus 和 running 状态，避免重复
pub struct ConnectionManager {
    /// 目标设备
    target: Arc<RwLock<Option<TargetDevice>>>,
    /// WebSocket 客户端
    client: Arc<RwLock<Option<Arc<WsClient>>>>,
    /// 消息处理器
    handler: Arc<MobileHandler>,
    /// 事件发送器（用于内部业务逻辑监听）
    event_tx: broadcast::Sender<MobileEvent>,
}

impl ConnectionManager {
    /// 创建新的连接管理器
    pub fn new() -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(1024);
        let handler = MobileHandler::new();

        Arc::new(Self {
            target: Arc::new(RwLock::new(None)),
            client: Arc::new(RwLock::new(None)),
            handler,
            event_tx,
        })
    }

    /// 获取当前连接状态（从 WsClient 获取）
    pub async fn get_status(&self) -> WsConnStatus {
        if let Some(client) = self.client.read().await.as_ref() {
            client.get_status().await
        } else {
            WsConnStatus::Disconnected
        }
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
    ///
    /// 简化逻辑：
    /// 1. 检查当前状态
    /// 2. 创建 WsClient 并直接 await 连接
    /// 3. WsClient 内部已经 spawn 了 sender_task 和 receiver_task
    pub async fn connect(&self, app_handle: AppHandle, address: String, port: u16, name: Option<String>) -> Result<()> {
        // 检查当前状态（从 WsClient 获取）
        {
            let status = self.get_status().await;
            if status == WsConnStatus::Connecting {
                let _ = app_handle.emit("ws_connecting", serde_json::json!({
                    "address": address,
                    "port": port,
                    "status": "already_connecting"
                }));
                log::info!("Already connecting");
                return Ok(());
            }
            if status == WsConnStatus::Connected || status == WsConnStatus::Paired {
                let _ = app_handle.emit("ws_connected", ());
                log::info!("Already connected");
                return Ok(());
            }
        }

        // 发射连接开始事件
        let _ = app_handle.emit("ws_connecting", serde_json::json!({
            "address": address,
            "port": port,
        }));
        log::info!("WebSocket connecting to {}:{}", address, port);

        // 保存目标设备
        log::debug!("Saving target device...");
        *self.target.write().await = Some(TargetDevice {
            address: address.clone(),
            port,
            name,
        });
        log::debug!("Target device saved");

        // 创建配置和客户端
        log::debug!("Creating WsClientConfig with address: {}, port: {}", address, port);
        let config = WsClientConfig::new(&address, port);
        log::debug!("WsClientConfig created, url: {}", config.url());

        log::debug!("Creating WsClient...");
        let client = WsClient::new(config);
        log::debug!("WsClient created");

        log::debug!("Setting handler (async)...");
        client.set_handler(self.handler.clone()).await;
        log::debug!("Handler set, now calling client.connect()...");
        log::info!("About to call client.connect(), this should show Connection log...");
        match client.connect().await {
            Ok(_) => {
                log::info!("client.connect() succeeded");
            }
            Err(e) => {
                log::error!("client.connect() failed: {}", e);
                let _ = app_handle.emit("ws_error", serde_json::json!({
                    "message": format!("Connection failed: {}", e)
                }));
                return Err(e);
            }
        }

        // 保存客户端引用
        *self.client.write().await = Some(client);

        // 短暂等待连接稳定
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // 发射连接成功事件
        let _ = app_handle.emit("ws_connected", ());
        log::info!("Connection established");

        Ok(())
    }

    /// 连接（不带 AppHandle，用于测试）
    pub async fn connect_without_emit(&self, address: String, port: u16, name: Option<String>) -> Result<()> {
        // 检查当前状态
        let status = self.get_status().await;
        if status == WsConnStatus::Connected || status == WsConnStatus::Paired {
            return Ok(());
        }

        // 保存目标设备
        *self.target.write().await = Some(TargetDevice {
            address: address.clone(),
            port,
            name,
        });

        // 创建配置和客户端
        let config = WsClientConfig::new(&address, port);
        let client = WsClient::new(config);
        client.set_handler(self.handler.clone());

        // 直接 await 连接
        client.connect().await?;

        // 保存客户端引用
        *self.client.write().await = Some(client);

        // 短暂等待连接稳定
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        Ok(())
    }

    /// 断开连接
    pub async fn disconnect(&self) {
        log::info!("Disconnecting...");

        // 断开 WebSocket
        if let Some(client) = self.client.read().await.as_ref() {
            let _ = client.disconnect().await;
        }

        // 清除客户端
        *self.client.write().await = None;

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
        let status = self.get_status().await;
        matches!(status, WsConnStatus::Connected | WsConnStatus::Paired)
    }

    /// 设置为已配对状态
    pub async fn set_paired(&self) {
        if let Some(client) = self.client.read().await.as_ref() {
            client.set_status(WsConnStatus::Paired).await;
        }
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        let handler = MobileHandler::new();

        Self {
            target: Arc::new(RwLock::new(None)),
            client: Arc::new(RwLock::new(None)),
            handler,
            event_tx,
        }
    }
}

impl Clone for ConnectionManager {
    fn clone(&self) -> Self {
        Self {
            target: self.target.clone(),
            client: self.client.clone(),
            handler: self.handler.clone(),
            event_tx: self.event_tx.clone(),
        }
    }
}