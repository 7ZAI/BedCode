//! WebSocket Manager
//!
//! 单例模式的服务器管理器
//! 使用 Actix Web 提供 HTTP REST API + WebSocket 终端
//! 客户端跟踪和消息广播通过 WsSessionRegistry 实现
//!
//! 服务依赖通过 AppContext::global() 获取，不再重复存储

use crate::server::message::Message as BusinessMessage;
use crate::server::ws::registry::WsSessionRegistry;
use crate::session::GlobalOutputManager;
use crate::error::AppError;
use crate::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 客户端摘要（对外暴露的信息）
#[derive(Debug, Clone)]
pub struct ClientSummary {
    pub client_id: String,
    pub device_name: Option<String>,
    pub fingerprint: Option<String>,
    pub addr: String,
    pub authenticated: bool,
    pub connected_at: i64,
}

/// WebSocket 管理器内部状态
struct WsManagerInner {
    /// 服务器端口
    port: RwLock<Option<u16>>,
    /// 是否已初始化
    initialized: RwLock<bool>,
    /// Actix Web 服务器句柄，用于优雅停机
    server_handle: RwLock<Option<actix_web::dev::ServerHandle>>,
}

impl WsManagerInner {
    fn new() -> Self {
        Self {
            port: RwLock::new(None),
            initialized: RwLock::new(false),
            server_handle: RwLock::new(None),
        }
    }
}

/// WebSocket 管理器（单例）
pub struct WebSocketManager {
    inner: Arc<WsManagerInner>,
}

impl WebSocketManager {
    pub fn global() -> &'static Self {
        static INSTANCE: std::sync::LazyLock<WebSocketManager> =
            std::sync::LazyLock::new(|| WebSocketManager {
                inner: Arc::new(WsManagerInner::new()),
            });
        &INSTANCE
    }

    /// 初始化
    pub async fn init(&self) -> Result<()> {
        let mut initialized = self.inner.initialized.write().await;
        if *initialized {
            tracing::warn!("WebSocketManager already initialized");
            return Ok(());
        }
        *initialized = true;

        tracing::info!("WebSocketManager initialized");
        Ok(())
    }

    /// 启动 Actix Web 服务器（HTTP + WS 统一端口）
    ///
    /// 在独立线程中启动 Actix runtime，返回 `ServerHandle` 供调用方保存用于优雅停机
    pub async fn start(&self, port: u16) -> Result<actix_web::dev::ServerHandle> {
        {
            let initialized = self.inner.initialized.read().await;
            if !*initialized {
                return Err(AppError::WebSocket(
                    "WebSocketManager not initialized, call init() first".to_string(),
                ));
            }
        }

        {
            let port_lock = self.inner.port.read().await;
            if port_lock.is_some() {
                return Err(AppError::WebSocket("Server already running".to_string()));
            }
        }

        // 使用 oneshot 通道从 Actix 线程传回 ServerHandle
        let (handle_tx, handle_rx) = tokio::sync::oneshot::channel::<std::io::Result<actix_web::dev::ServerHandle>>();

        std::thread::spawn(move || {
            let rt = actix_rt::Runtime::new().expect("Failed to create Actix runtime");
            rt.block_on(async move {
                let result = crate::server::app::start_http_server(port).await;
                let _ = handle_tx.send(result);
            });
        });

        // 等待 Actix 服务器启动并获取 handle
        let handle = handle_rx.await
            .map_err(|_| AppError::WebSocket("Actix server thread panicked before returning handle".to_string()))?
            .map_err(|e| AppError::WebSocket(format!("Failed to start Actix server: {}", e)))?;

        {
            let mut port_lock = self.inner.port.write().await;
            *port_lock = Some(port);
        }

        {
            let mut handle_lock = self.inner.server_handle.write().await;
            *handle_lock = Some(handle.clone());
        }

        tracing::info!("Actix Web server (HTTP + WS) started on port {}", port);
        Ok(handle)
    }

    /// 停止服务器
    ///
    /// 调用 `ServerHandle::stop(true)` 优雅停机，并清理所有 WS 客户端连接
    pub async fn stop(&self) -> Result<()> {
        // 优雅停机
        {
            let mut handle_lock = self.inner.server_handle.write().await;
            if let Some(handle) = handle_lock.take() {
                handle.stop(true).await;
                tracing::info!("Actix Web server stopped via ServerHandle");
            }
        }

        // 清理所有 WS 客户端连接
        let registry = WsSessionRegistry::global();
        let clients = registry.list_clients().await;
        for client in &clients {
            let global_manager = GlobalOutputManager::global();
            global_manager.unsubscribe_all_for_client(&client.client_id).await;
        }
        registry.clear_all().await;

        {
            let mut port_lock = self.inner.port.write().await;
            *port_lock = None;
        }

        tracing::info!("WebSocketManager stopped");
        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        self.inner.port.read().await.is_some()
    }

    pub fn port(&self) -> Option<u16> {
        self.inner.port.blocking_read().clone()
    }

    // ==================== Client Management APIs ====================

    /// 获取所有已连接客户端列表
    pub async fn list_clients(&self) -> Vec<ClientSummary> {
        let registry = WsSessionRegistry::global();
        let summaries = registry.list_clients().await;
        summaries.into_iter().map(|s| ClientSummary {
            client_id: s.client_id,
            device_name: s.device_name,
            fingerprint: s.fingerprint,
            addr: s.addr,
            authenticated: s.authenticated,
            connected_at: s.connected_at,
        }).collect()
    }

    /// 获取已认证客户端列表
    pub async fn list_authenticated_clients(&self) -> Vec<ClientSummary> {
        self.list_clients().await.into_iter().filter(|c| c.authenticated).collect()
    }

    /// 获取指定客户端信息（通过 client_id）
    pub async fn get_client(&self, client_id: &str) -> Option<ClientSummary> {
        let registry = WsSessionRegistry::global();
        registry.get_client(client_id).await.map(|s| ClientSummary {
            client_id: s.client_id,
            device_name: s.device_name,
            fingerprint: s.fingerprint,
            addr: s.addr,
            authenticated: s.authenticated,
            connected_at: s.connected_at,
        })
    }

    /// 获取指定客户端信息（通过 SocketAddr）
    pub async fn get_client_by_addr(&self, addr: &SocketAddr) -> Option<ClientSummary> {
        let registry = WsSessionRegistry::global();
        registry.get_client_by_addr(addr).await.map(|s| ClientSummary {
            client_id: s.client_id,
            device_name: s.device_name,
            fingerprint: s.fingerprint,
            addr: s.addr,
            authenticated: s.authenticated,
            connected_at: s.connected_at,
        })
    }

    /// 获取客户端数量
    pub async fn client_count(&self) -> usize {
        WsSessionRegistry::global().client_count().await
    }

    /// 获取已认证客户端数量
    pub async fn authenticated_count(&self) -> usize {
        WsSessionRegistry::global().authenticated_count().await
    }

    // ==================== Message Sending APIs ====================

    /// 向指定客户端发送消息（通过 client_id）
    pub async fn send_to_client(&self, client_id: &str, message: &BusinessMessage) -> Result<()> {
        let json = message.to_json()?;
        WsSessionRegistry::global()
            .send_to_client(client_id, json)
            .await
            .map_err(AppError::WebSocket)
    }

    /// 向指定客户端发送文本（通过 client_id）
    pub async fn send_text_to_client(&self, client_id: &str, text: &str) -> Result<()> {
        WsSessionRegistry::global()
            .send_to_client(client_id, text.to_string())
            .await
            .map_err(AppError::WebSocket)
    }

    /// 向指定客户端发送消息（通过 SocketAddr）
    pub async fn send_to_addr(&self, addr: &SocketAddr, message: &BusinessMessage) -> Result<()> {
        let json = message.to_json()?;
        WsSessionRegistry::global()
            .send_to_addr(addr, json)
            .await
            .map_err(AppError::WebSocket)
    }

    /// 向多个指定客户端发送消息
    pub async fn send_to_clients(&self, client_ids: &[&str], message: &BusinessMessage) -> Result<()> {
        for client_id in client_ids {
            let _ = self.send_to_client(client_id, message).await;
        }
        Ok(())
    }

    /// 向除指定客户端外的所有客户端广播
    pub async fn broadcast_to_others(
        &self,
        exclude_client_id: &str,
        message: &BusinessMessage,
    ) -> Result<()> {
        let json = message.to_json()?;
        let registry = WsSessionRegistry::global();
        let device_name = registry.get_device_name(exclude_client_id).await;
        registry.broadcast(json, device_name.as_deref()).await;
        Ok(())
    }

    /// 向所有已认证客户端广播
    pub async fn broadcast(&self, message: &BusinessMessage) -> Result<()> {
        let json = message.to_json()?;
        WsSessionRegistry::global().broadcast(json, None).await;
        Ok(())
    }

    /// 向所有客户端广播（包含未认证）
    pub async fn broadcast_all(&self, message: &BusinessMessage) -> Result<()> {
        self.broadcast(message).await
    }

    /// 向除指定设备外的所有已认证客户端广播（基于设备名称）
    pub async fn broadcast_sync_to_others(
        &self,
        exclude_device_name: &str,
        message: &BusinessMessage,
    ) -> Result<()> {
        let json = message.to_json()?;
        WsSessionRegistry::global().broadcast(json, Some(exclude_device_name)).await;
        Ok(())
    }

    // ==================== Event Subscription ====================

    /// 订阅服务器事件
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<ServerEvent> {
        let (_, rx) = tokio::sync::broadcast::channel(1);
        rx
    }

    // ==================== Helper Methods ====================

    /// 设置设备名称
    pub async fn set_device_name(&self, addr: &SocketAddr, device_name: Option<String>) {
        if let Some(name) = device_name {
            let registry = WsSessionRegistry::global();
            if let Some(summary) = registry.get_client_by_addr(addr).await {
                registry.set_device_name(&summary.client_id, Some(name)).await;
            }
        }
    }

    /// 获取设备名称（通过地址）
    pub async fn get_device_name_by_addr(&self, addr: &SocketAddr) -> Option<String> {
        WsSessionRegistry::global().get_device_name_by_addr(addr).await
    }

    /// 更新客户端认证状态
    pub async fn set_authenticated(&self, _addr: &SocketAddr, client_id: Option<String>, fingerprint: Option<String>) {
        // TerminalWs actor 认证时已通过 WsSessionRegistry 更新
        // 此方法保留用于 auth_service 等外部调用者的兼容性
        if let Some(cid) = client_id {
            let registry = WsSessionRegistry::global();
            let current_name = registry.get_device_name(&cid).await;
            registry.set_authenticated(&cid, current_name, fingerprint).await;
        }
    }

    /// 客户端是否已认证
    pub async fn is_client_authenticated(&self, client_id: &str) -> bool {
        WsSessionRegistry::global().is_authenticated(client_id).await
    }

    /// 清理客户端连接数据
    pub async fn cleanup_client_by_addr(&self, addr: SocketAddr) {
        let registry = WsSessionRegistry::global();

        if let Some(client_id) = registry.unregister_by_addr(&addr).await {
            let global_manager = GlobalOutputManager::global();
            global_manager.unsubscribe_all_for_client(&client_id).await;
            tracing::info!("[WebSocketManager] Cleaned up all subscriptions for client {}", client_id);
        }
    }
}

/// 服务器事件
#[derive(Debug, Clone)]
pub enum ServerEvent {
    Started,
    Stopped,
}
