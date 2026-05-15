//! WebSocket Manager
//!
//! 单例模式的 WebSocket 服务器管理器
//! 提供移动端远程控制功能的便捷操作 API

use crate::desktop::server::message::Message as BusinessMessage;
use crate::shared::websocket::{WsServer, WsServerConfig, WsServerEvent};
use crate::shared::system::error::AppError;
use crate::Result;
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// 客户端摘要（对外暴露的信息）
#[derive(Debug, Clone)]
pub struct ClientSummary {
    /// 客户端唯一标识符
    pub client_id: String,
    /// 设备名称（由客户端在认证时提供）
    pub device_name: Option<String>,
    /// 客户端的网络地址 (ip:port)
    pub addr: String,
    /// 是否已通过认证
    pub authenticated: bool,
    /// 连接建立时间（Unix 毫秒时间戳）
    pub connected_at: i64,
}

/// 业务消息处理器 trait
#[async_trait]
pub trait BusinessHandler: Send + Sync {
    /// 处理收到的消息
    async fn handle_message(
        &self,
        msg: BusinessMessage,
        client_id: &str,
    ) -> Result<Option<BusinessMessage>>;

    /// 客户端连接成功（可选实现）
    fn on_connected(&self, _client_id: &str, _device_name: Option<String>) {}

    /// 客户端认证成功（可选实现）
    fn on_authenticated(&self, _client_id: &str, _device_name: Option<String>) {}

    /// 客户端断开连接（可选实现）
    fn on_disconnected(&self, _client_id: &str) {}
}

/// 空业务处理器
// TODO: 后续任务将实现实际的 BusinessHandler，届时 NoopBusinessHandler 仅作为默认占位
#[derive(Debug, Clone, Default)]
pub struct NoopBusinessHandler;

#[async_trait]
impl BusinessHandler for NoopBusinessHandler {
    async fn handle_message(
        &self,
        _msg: BusinessMessage,
        _client_id: &str,
    ) -> Result<Option<BusinessMessage>> {
        Ok(None)
    }
}

/// WebSocket 管理器内部状态
struct WsManagerInner {
    /// 底层 WebSocket 服务器
    server: Option<Arc<WsServer>>,
    /// 业务消息处理器
    handler: Arc<dyn BusinessHandler>,
    /// client_id 到 SocketAddr 的映射
    client_id_to_addr: RwLock<HashMap<String, SocketAddr>>,
    /// SocketAddr 到 client_id 的反向映射
    addr_to_client_id: RwLock<HashMap<SocketAddr, String>>,
    /// SocketAddr 到设备名称的映射
    addr_to_device_name: RwLock<HashMap<SocketAddr, String>>,
    /// 连接时间戳（毫秒）
    addr_to_connected_at: RwLock<HashMap<SocketAddr, i64>>,
    /// 服务器端口
    port: RwLock<Option<u16>>,
    /// 是否已初始化
    initialized: RwLock<bool>,
}

impl WsManagerInner {
    fn new() -> Self {
        Self {
            server: None,
            handler: Arc::new(NoopBusinessHandler),
            client_id_to_addr: RwLock::new(HashMap::new()),
            addr_to_client_id: RwLock::new(HashMap::new()),
            addr_to_device_name: RwLock::new(HashMap::new()),
            addr_to_connected_at: RwLock::new(HashMap::new()),
            port: RwLock::new(None),
            initialized: RwLock::new(false),
        }
    }
}

/// WebSocket 管理器（单例）
pub struct WebSocketManager {
    inner: Arc<WsManagerInner>,
}

impl WebSocketManager {
    /// 获取全局单例
    pub fn global() -> &'static Self {
        static INSTANCE: std::sync::LazyLock<WebSocketManager> =
            std::sync::LazyLock::new(|| WebSocketManager {
                inner: Arc::new(WsManagerInner::new()),
            });
        &INSTANCE
    }

    /// 初始化（可选择注入自定义 handler）
    pub async fn init(&self, handler: Option<Arc<dyn BusinessHandler>>) -> Result<()> {
        let mut initialized = self.inner.initialized.write().await;
        if *initialized {
            tracing::warn!("WebSocketManager already initialized");
            return Ok(());
        }

        if let Some(h) = handler {
            self.inner.handler = h;
        }

        *initialized = true;
        tracing::info!("WebSocketManager initialized");
        Ok(())
    }

    /// 启动 WebSocket 服务器
    pub async fn start(&self, port: u16) -> Result<()> {
        // 检查是否已初始化
        {
            let initialized = self.inner.initialized.read().await;
            if !*initialized {
                return Err(AppError::WebSocket(
                    "WebSocketManager not initialized, call init() first".to_string(),
                ));
            }
        }

        // 检查是否已启动
        if let Some(server) = &self.inner.server {
            if server.is_running().await {
                return Err(AppError::WebSocket(
                    "WebSocket server already running".to_string(),
                ));
            }
        }

        // 创建服务器配置
        let config = WsServerConfig {
            port,
            heartbeat_interval_secs: 30,
            heartbeat_timeout_secs: 90,
            message_queue_size: 256,
        };

        // 创建底层 WebSocket 服务器
        let server = Arc::new(WsServer::new(config));

        // 启动服务器
        let server_clone = server.clone();
        let manager = self.inner.clone();

        // 在后台启动事件处理任务
        tokio::spawn(async move {
            let mut rx = server_clone.subscribe();
            while let Ok(event) = rx.recv().await {
                Self::handle_server_event(manager.clone(), event).await;
            }
        });

        // 启动服务器
        let server_clone2 = server.clone();
        tokio::spawn(async move {
            if let Err(e) = server_clone2.start().await {
                tracing::error!("WebSocket server error: {}", e);
            }
        });

        // 等待服务器启动
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // 保存服务器引用和端口
        self.inner.server.replace(server);
        *self.inner.port.write().await = Some(port);

        tracing::info!("WebSocketManager started on port {}", port);
        Ok(())
    }

    /// 停止服务器
    pub async fn stop(&self) -> Result<()> {
        if let Some(server) = &self.inner.server {
            server.stop().await?;
            self.inner.server = None;
            *self.inner.port.write().await = None;
            tracing::info!("WebSocketManager stopped");
        }
        Ok(())
    }

    /// 服务器是否运行中
    pub async fn is_running(&self) -> bool {
        if let Some(server) = &self.inner.server {
            server.is_running().await
        } else {
            false
        }
    }

    /// 获取服务器端口
    pub fn port(&self) -> Option<u16> {
        self.inner.port.blocking_read().clone()
    }

    // ==================== Client Management APIs ====================

    /// 获取所有已连接客户端列表
    pub async fn list_clients(&self) -> Vec<ClientSummary> {
        if let Some(server) = &self.inner.server {
            let clients = server.clients().read().await;
            let addr_to_client_id = self.inner.addr_to_client_id.read().await;
            let addr_to_device_name = self.inner.addr_to_device_name.read().await;
            let addr_to_connected_at = self.inner.addr_to_connected_at.read().await;

            clients
                .iter()
                .map(|(addr, info)| {
                    let client_id = addr_to_client_id
                        .get(addr)
                        .cloned()
                        .unwrap_or_else(|| addr.to_string());
                    let device_name = addr_to_device_name.get(addr).cloned();
                    let connected_at = addr_to_connected_at
                        .get(addr)
                        .copied()
                        .unwrap_or_else(|| Utc::now().timestamp_millis());

                    ClientSummary {
                        client_id,
                        device_name,
                        addr: addr.to_string(),
                        authenticated: info.authenticated,
                        connected_at,
                    }
                })
                .collect()
        } else {
            vec![]
        }
    }

    /// 获取已认证客户端列表
    pub async fn list_authenticated_clients(&self) -> Vec<ClientSummary> {
        self.list_clients()
            .await
            .into_iter()
            .filter(|c| c.authenticated)
            .collect()
    }

    /// 获取指定客户端信息（通过 client_id）
    pub async fn get_client(&self, client_id: &str) -> Option<ClientSummary> {
        let addr = {
            let client_id_to_addr = self.inner.client_id_to_addr.read().await;
            client_id_to_addr.get(client_id).copied()
        };

        if let Some(addr) = addr {
            self.get_client_by_addr(&addr).await
        } else {
            None
        }
    }

    /// 获取指定客户端信息（通过 SocketAddr）
    pub async fn get_client_by_addr(&self, addr: &SocketAddr) -> Option<ClientSummary> {
        if let Some(server) = &self.inner.server {
            let client_info = server.get_client(addr).await?;
            let client_id = self.inner.addr_to_client_id.read().await
                .get(addr)
                .cloned()
                .unwrap_or_else(|| addr.to_string());
            let device_name = self.inner.addr_to_device_name.read().await
                .get(addr)
                .cloned();
            let connected_at = self.inner.addr_to_connected_at.read().await
                .get(addr)
                .copied()
                .unwrap_or_else(|| Utc::now().timestamp_millis());

            Some(ClientSummary {
                client_id,
                device_name,
                addr: addr.to_string(),
                authenticated: client_info.authenticated,
                connected_at,
            })
        } else {
            None
        }
    }

    /// 获取客户端数量
    pub async fn client_count(&self) -> usize {
        if let Some(server) = &self.inner.server {
            server.client_count().await
        } else {
            0
        }
    }

    /// 获取已认证客户端数量
    pub async fn authenticated_count(&self) -> usize {
        if let Some(server) = &self.inner.server {
            server.authenticated_count().await
        } else {
            0
        }
    }

    // ==================== Message Sending APIs ====================

    /// 向指定客户端发送消息（通过 client_id）
    pub async fn send_to_client(&self, client_id: &str, message: &BusinessMessage) -> Result<()> {
        let addr = {
            let client_id_to_addr = self.inner.client_id_to_addr.read().await;
            client_id_to_addr.get(client_id).copied()
        };

        if let Some(addr) = addr {
            Self::send_to_addr(&self.inner, &addr, message).await
        } else {
            Err(AppError::WebSocket(format!("Client {} not found", client_id)))
        }
    }

    /// 向指���客户端发送文本（通过 client_id）
    pub async fn send_text_to_client(&self, client_id: &str, text: &str) -> Result<()> {
        let message = BusinessMessage::from_json(text)
            .map_err(|e| AppError::WebSocket(format!("Invalid message: {}", e)))?;
        self.send_to_client(client_id, &message).await
    }

    /// 向指定客户端发送消息（通过 SocketAddr）
    pub async fn send_to_addr(&self, addr: &SocketAddr, message: &BusinessMessage) -> Result<()> {
        if let Some(server) = &self.inner.server {
            let json = message.to_json()?;
            server.send_text_to(addr, &json).await
        } else {
            Err(AppError::WebSocket("Server not started".to_string()))
        }
    }

    /// 向多个指定客户端发送消息
    pub async fn send_to_clients(&self, client_ids: &[&str], message: &BusinessMessage) -> Result<()> {
        let mut errors = vec![];

        for client_id in client_ids {
            if let Err(e) = self.send_to_client(client_id, message).await {
                errors.push(format!("{}: {}", client_id, e));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(AppError::WebSocket(format!(
                "Failed to send to some clients: {}",
                errors.join(", ")
            )))
        }
    }

    /// 向除指定客户端外的所有客户端广播
    pub async fn broadcast_to_others(
        &self,
        exclude_client_id: &str,
        message: &BusinessMessage,
    ) -> Result<()> {
        let exclude_addr = {
            let client_id_to_addr = self.inner.client_id_to_addr.read().await;
            client_id_to_addr.get(exclude_client_id).copied()
        };

        if let Some(server) = &self.inner.server {
            if let Some(addr) = exclude_addr {
                let json = message.to_json()?;
                let ws_msg = crate::shared::websocket::WsMessage::text(&json);
                server.broadcast_to_others(&addr, &ws_msg).await?;
            } else {
                self.broadcast(message).await?;
            }
            Ok(())
        } else {
            Err(AppError::WebSocket("Server not started".to_string()))
        }
    }

    /// 向所有已认证客户端广播
    pub async fn broadcast(&self, message: &BusinessMessage) -> Result<()> {
        if let Some(server) = &self.inner.server {
            let json = message.to_json()?;
            let ws_msg = crate::shared::websocket::WsMessage::text(&json);
            server.broadcast(&ws_msg).await
        } else {
            Err(AppError::WebSocket("Server not started".to_string()))
        }
    }

    /// 向所有客户端广播（包含未认证）
    pub async fn broadcast_all(&self, message: &BusinessMessage) -> Result<()> {
        self.broadcast(message).await
    }

    // ==================== Event Subscription ====================

    /// 订阅服务器事件
    pub fn subscribe(&self) -> broadcast::Receiver<WsServerEvent> {
        if let Some(server) = &self.inner.server {
            server.subscribe()
        } else {
            let (tx, _) = broadcast::channel(1);
            tx
        }
    }

    // ==================== Helper Methods ====================

    /// 设置设备名称（用于内部映射）
    pub async fn set_device_name(&self, addr: &SocketAddr, device_name: Option<String>) {
        if let Some(name) = device_name {
            let mut addr_to_device_name = self.inner.addr_to_device_name.write().await;
            addr_to_device_name.insert(*addr, name);
        }
    }

    /// 更新客户端认证状态
    pub async fn set_authenticated(&self, addr: &SocketAddr, client_id: Option<String>) {
        if let Some(server) = &self.inner.server {
            server.set_authenticated(addr, client_id.clone()).await;

            if let Some(cid) = client_id {
                let mut client_id_to_addr = self.inner.client_id_to_addr.write().await;
                client_id_to_addr.insert(cid, *addr);

                let mut addr_to_client_id = self.inner.addr_to_client_id.write().await;
                addr_to_client_id.insert(*addr, cid);
            }
        }
    }

    /// 客户端是否已认证
    pub async fn is_client_authenticated(&self, client_id: &str) -> bool {
        if let Some(client) = self.get_client(client_id).await {
            client.authenticated
        } else {
            false
        }
    }
}

// ==================== Private Helper Methods ====================

impl WebSocketManager {
    /// 处理服务器事件
    async fn handle_server_event(inner: Arc<WsManagerInner>, event: WsServerEvent) {
        match event {
            WsServerEvent::ClientConnected { addr, client_id } => {
                // 记录连接时间
                {
                    let mut connected_at = inner.addr_to_connected_at.write().await;
                    connected_at.insert(addr, Utc::now().timestamp_millis());
                }

                // 调用业务处理器
                let device_name = inner.addr_to_device_name.read().await.get(&addr).cloned();
                let client_id = client_id.unwrap_or_else(|| addr.to_string());
                inner.handler.on_connected(&client_id, device_name);

                tracing::info!("Client connected: {}", addr);
            }
            WsServerEvent::ClientDisconnected { addr, client_id } => {
                // 清理映射
                Self::cleanup_client(&inner, &addr).await;

                // 调用业务处理器
                if let Some(cid) = client_id {
                    inner.handler.on_disconnected(&cid);
                }

                tracing::info!("Client disconnected: {}", addr);
            }
            WsServerEvent::TextMessage { addr, client_id, message_id: _, content } => {
                // 解析业务消息
                match BusinessMessage::from_json(&content) {
                    Ok(msg) => {
                        let cid = client_id.unwrap_or_else(|| addr.to_string());
                        // 调用业务处理器
                        if let Some(response) = inner.handler.handle_message(msg, &cid).await {
                            // 发送响应
                            if let Err(e) = Self::send_to_addr_internal(&inner, &addr, &response).await {
                                tracing::error!("Failed to send response: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse message: {}", e);
                    }
                }
            }
            WsServerEvent::AuthSuccess { addr, client_id } => {
                // 更新 client_id 映射
                {
                    let mut client_id_to_addr = inner.client_id_to_addr.write().await;
                    client_id_to_addr.insert(client_id.clone(), addr);
                }
                {
                    let mut addr_to_client_id = inner.addr_to_client_id.write().await;
                    addr_to_client_id.insert(addr, client_id.clone());
                }

                // 调用业务处理器
                let device_name = inner.addr_to_device_name.read().await.get(&addr).cloned();
                inner.handler.on_authenticated(&client_id, device_name);

                tracing::info!("Client authenticated: {} ({})", client_id, addr);
            }
            _ => {}
        }
    }

    /// 清理客户端连接数据
    async fn cleanup_client(inner: &Arc<WsManagerInner>, addr: &SocketAddr) {
        // 获取 client_id
        let client_id = {
            let addr_to_client_id = inner.addr_to_client_id.read().await;
            addr_to_client_id.get(addr).cloned()
        };

        // 清理映射
        if let Some(cid) = client_id {
            let mut client_id_to_addr = inner.client_id_to_addr.write().await;
            client_id_to_addr.remove(&cid);
        }

        {
            let mut addr_to_client_id = inner.addr_to_client_id.write().await;
            addr_to_client_id.remove(addr);
        }

        {
            let mut addr_to_device_name = inner.addr_to_device_name.write().await;
            addr_to_device_name.remove(addr);
        }

        {
            let mut addr_to_connected_at = inner.addr_to_connected_at.write().await;
            addr_to_connected_at.remove(addr);
        }
    }

    /// 向指定地址发送消息
    async fn send_to_addr_internal(
        inner: &Arc<WsManagerInner>,
        addr: &SocketAddr,
        message: &BusinessMessage,
    ) -> Result<()> {
        if let Some(server) = &inner.server {
            let json = message.to_json()?;
            server.send_text_to(addr, &json).await
        } else {
            Err(AppError::WebSocket("Server not started".to_string()))
        }
    }
}
