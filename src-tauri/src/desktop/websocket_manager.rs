//! WebSocket Manager
//!
//! 单例模式的 WebSocket 服务器管理器
//! 提供移动端远程控制功能的便捷操作 API

use crate::desktop::pty::PtySubscriptionManager;
use crate::desktop::server::handlers::{
    AuthHandler, TerminalHandler,
    SessionControlHandler, SessionConfigHandler,
};
use crate::desktop::server::message::Message as BusinessMessage;
use crate::desktop::session::SessionManager;
use crate::desktop::plugin::PluginManager;
use crate::shared::model::message::Message;
use crate::shared::system::config::AppConfig;
use crate::shared::websocket::{
    WsServer, WsServerConfig, WsServerEvent,
};
use crate::desktop::server::router::BusinessRouter;
use crate::shared::websocket::server::DefaultMessageHandler;
use crate::shared::system::error::AppError;
use crate::shared::auth::QrTokenManager;
use crate::Result;
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::AppHandle;
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

/// WebSocket 管理器内部状态
struct WsManagerInner {
    /// 底层 WebSocket 服务器
    server: RwLock<Option<Arc<WsServer>>>,
    /// PTY 输出订阅管理器
    subscription_manager: Arc<PtySubscriptionManager>,
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
    /// 数据库实例（从 lib.rs 传入，避免重复创建）
    db: RwLock<Option<Arc<tokio::sync::Mutex<crate::shared::db::Database>>>>,
    /// QR Token 管理器（从 lib.rs 传入，确保与 Tauri State 共享同一实例）
    qr_manager: RwLock<Option<Arc<QrTokenManager>>>,
    /// 配对服务（从 lib.rs 传入，确保与 Tauri State 共享同一实例）
    pairing_service: RwLock<Option<Arc<crate::desktop::server::services::PairingService>>>,
    /// Tauri AppHandle（用于向前端发送事件）
    app_handle: RwLock<Option<Arc<AppHandle>>>,
    /// 会话管理器（用于会话控制请求）
    session_manager: RwLock<Option<Arc<SessionManager>>>,
    /// 插件管理器（用于会话控制请求）
    plugin_manager: RwLock<Option<Arc<PluginManager>>>,
}

impl WsManagerInner {
    fn new() -> Self {
        Self {
            server: RwLock::new(None),
            subscription_manager: Arc::new(PtySubscriptionManager::new()),
            client_id_to_addr: RwLock::new(HashMap::new()),
            addr_to_client_id: RwLock::new(HashMap::new()),
            addr_to_device_name: RwLock::new(HashMap::new()),
            addr_to_connected_at: RwLock::new(HashMap::new()),
            port: RwLock::new(None),
            initialized: RwLock::new(false),
            db: RwLock::new(None),
            qr_manager: RwLock::new(None),
            pairing_service: RwLock::new(None),
            app_handle: RwLock::new(None),
            session_manager: RwLock::new(None),
            plugin_manager: RwLock::new(None),
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

    /// 初始化（接受外部传入的实例，确保与 Tauri State 共享）
    pub async fn init(
        &self,
        db: Arc<tokio::sync::Mutex<crate::shared::db::Database>>,
        qr_manager: Arc<QrTokenManager>,
        pairing_service: Arc<crate::desktop::server::services::PairingService>,
        app_handle: Arc<AppHandle>,
        session_manager: Arc<SessionManager>,
        plugin_manager: Arc<PluginManager>,
    ) -> Result<()> {
        {
            let mut initialized = self.inner.initialized.write().await;
            if *initialized {
                tracing::warn!("WebSocketManager already initialized");
                return Ok(());
            }
            *initialized = true;
        }

        // 存储 db 实例
        {
            let mut db_lock = self.inner.db.write().await;
            *db_lock = Some(db);
        }

        // 存储 qr_manager 实例（关键：确保与前端 generate_qr_code 命令使用同一实例）
        {
            let mut qr_lock = self.inner.qr_manager.write().await;
            *qr_lock = Some(qr_manager);
        }

        // 存储 pairing_service 实例（关键：确保与前端 generate_pairing_code 命令使用同一实例）
        {
            let mut ps_lock = self.inner.pairing_service.write().await;
            *ps_lock = Some(pairing_service);
        }

        // 存储 app_handle 实例（用于向前端发送设备连接事件）
        {
            let mut handle_lock = self.inner.app_handle.write().await;
            *handle_lock = Some(app_handle);
        }

        // 存储 session_manager 实例（用于会话控制请求）
        {
            let mut sm_lock = self.inner.session_manager.write().await;
            *sm_lock = Some(session_manager);
        }

        // 存储 plugin_manager 实例（用于会话控制请求）
        {
            let mut pm_lock = self.inner.plugin_manager.write().await;
            *pm_lock = Some(plugin_manager);
        }

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
        {
            let server = self.inner.server.read().await;
            if let Some(s) = &*server {
                if s.is_running().await {
                    return Err(AppError::WebSocket(
                        "WebSocket server already running".to_string(),
                    ));
                }
            }
        }

        // 创建服务器配置（使用默认的心跳配置）
        // 创建 HTTP 路由器（暂无 handler，后续添加 Plugin API 等）
        let http_router = Arc::new(
            crate::shared::websocket::server::http_router::HttpRouter::new()
        );

        let config = WsServerConfig {
            port,
            http_router: Some(http_router),
            ..WsServerConfig::default()
        };

        // 创建 WsServer
        let server = Arc::new(WsServer::new(config));

        // 获取数据库实例
        let db = {
            let db_lock = self.inner.db.read().await;
            db_lock.clone().ok_or_else(|| AppError::WebSocket(
                "Database not initialized, call init() first".to_string(),
            ))?
        };

        // 使用 init 时传入的 qr_manager 实例（与 Tauri State 共享）
        let qr_manager = {
            let qr_lock = self.inner.qr_manager.read().await;
            qr_lock.clone().ok_or_else(|| AppError::WebSocket(
                "QrTokenManager not initialized, call init() first".to_string(),
            ))?
        };

        // 使用 init 时传入的 pairing_service 实例（与 Tauri State 共享）
        // 关键：确保与前端 generate_pairing_code 命令使用同一实例
        let pairing_service = {
            let ps_lock = self.inner.pairing_service.read().await;
            ps_lock.clone().ok_or_else(|| AppError::WebSocket(
                "PairingService not initialized, call init() first".to_string(),
            ))?
        };

        // 获取 session_manager 和 plugin_manager（用于会话控制请求）
        let session_manager = {
            let sm_lock = self.inner.session_manager.read().await;
            sm_lock.clone()
        };
        let plugin_manager = {
            let pm_lock = self.inner.plugin_manager.read().await;
            pm_lock.clone()
        };

        // 获取 app_handle（用于向前端发送事件）
        let app_handle = {
            let handle_lock = self.inner.app_handle.read().await;
            handle_lock.clone()
        };

        // 创建处理器实例
        let auth_handler = Arc::new(AuthHandler::new(
            pairing_service,
            qr_manager,
            app_handle.clone(),
        ));
        // 传入 session_manager 和 plugin_manager（用于会话控制请求）
        // 同时传入 app_handle（用于发送刷新事件到桌面端前端）
        let control_handler = Arc::new(SessionControlHandler::new(
            session_manager.clone(),
            plugin_manager,
            app_handle.clone(),
        ));
        let session_config_handler = Arc::new(SessionConfigHandler::new(db.clone()));
        // 传入 session_manager（用于终端输入写入 PTY）
        let terminal_handler = Arc::new(TerminalHandler::new(session_manager));

        // 创建 BusinessRouter（实现 MessageRouter trait）
        let (event_tx_sender, _) = broadcast::channel(AppConfig::global().channels.ws_event_capacity);
        let router = Arc::new(
            BusinessRouter::builder()
                .connection_manager(server.connection_manager().clone())
                .event_tx(event_tx_sender)
                .route("Auth", auth_handler)
                .route("SessionControl", control_handler)
                .route("SessionConfig", session_config_handler)
                .route("Terminal", terminal_handler)
                .build()
        );

        let ws_handler = DefaultMessageHandler::new(None, Some(router));
        server.set_handler(Arc::new(ws_handler)).await;

        // 启动服务器
        let server_clone = server.clone();
        tokio::spawn(async move {
            if let Err(e) = server_clone.start().await {
                tracing::error!("WebSocket server error: {}", e);
            }
        });

        // 等待服务器启动
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // 保存服务器引用和端口
        {
            let mut server_lock = self.inner.server.write().await;
            *server_lock = Some(server);
        }
        {
            let mut port_lock = self.inner.port.write().await;
            *port_lock = Some(port);
        }

        tracing::info!("WebSocketManager started on port {}", port);
        Ok(())
    }

    /// 获取服务器实例（用于注册事件源）
    pub async fn get_server(&self) -> Option<Arc<WsServer>> {
        let server = self.inner.server.read().await;
        server.clone()
    }

    /// 停止服务器
    pub async fn stop(&self) -> Result<()> {
        let server = {
            let mut server_lock = self.inner.server.write().await;
            server_lock.take()
        };

        if let Some(s) = server {
            s.stop().await?;
        }

        {
            let mut port_lock = self.inner.port.write().await;
            *port_lock = None;
        }

        tracing::info!("WebSocketManager stopped");
        Ok(())
    }

    /// 服务器是否运行中
    pub async fn is_running(&self) -> bool {
        let server = self.inner.server.read().await;
        if let Some(s) = &*server {
            s.is_running().await
        } else {
            false
        }
    }

    /// 获取服务器端口
    pub fn port(&self) -> Option<u16> {
        self.inner.port.blocking_read().clone()
    }

    /// 获取订阅管理器
    pub fn subscription_manager(&self) -> Arc<PtySubscriptionManager> {
        self.inner.subscription_manager.clone()
    }

    // ==================== Client Management APIs ====================

    /// 获取所有已连接客户端列表
    pub async fn list_clients(&self) -> Vec<ClientSummary> {
        let server = self.inner.server.read().await;
        if let Some(s) = &*server {
            let cm = s.connection_manager();
            let ids = cm.all_ids().await;
            let addr_to_client_id = self.inner.addr_to_client_id.read().await;
            let addr_to_device_name = self.inner.addr_to_device_name.read().await;
            let addr_to_connected_at = self.inner.addr_to_connected_at.read().await;

            // 先获取所有连接信息
            let mut connections = Vec::new();
            for id in ids {
                if let Some(info) = cm.get(id).await {
                    connections.push(info);
                }
            }

            connections
                .iter()
                .map(|info| {
                    let addr = info.addr;
                    let client_id = addr_to_client_id
                        .get(&addr)
                        .cloned()
                        .unwrap_or_else(|| addr.to_string());
                    let device_name = addr_to_device_name.get(&addr).cloned();
                    let connected_at = addr_to_connected_at
                        .get(&addr)
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
        let server = self.inner.server.read().await;
        if let Some(s) = &*server {
            let client_info = s.get_client(addr).await?;
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
        let server = self.inner.server.read().await;
        if let Some(s) = &*server {
            s.client_count().await
        } else {
            0
        }
    }

    /// 获取已认证客户端数量
    pub async fn authenticated_count(&self) -> usize {
        let server = self.inner.server.read().await;
        if let Some(s) = &*server {
            s.authenticated_count().await
        } else {
            0
        }
    }

    // ==================== Message Sending APIs ====================

    /// 向指定客户端发���消息（通过 client_id）
    pub async fn send_to_client(&self, client_id: &str, message: &BusinessMessage) -> Result<()> {
        let addr = {
            let client_id_to_addr = self.inner.client_id_to_addr.read().await;
            client_id_to_addr.get(client_id).copied()
        };

        if let Some(addr) = addr {
            self.send_to_addr(&addr, message).await
        } else {
            Err(AppError::WebSocket(format!("Client {} not found", client_id)))
        }
    }

    /// 向指定客户端发送文本（通过 client_id）
    pub async fn send_text_to_client(&self, client_id: &str, text: &str) -> Result<()> {
        let message = BusinessMessage::from_json(text)
            .map_err(|e| AppError::WebSocket(format!("Invalid message: {}", e)))?;
        self.send_to_client(client_id, &message).await
    }

    /// 向指定客户端发送消息（通过 SocketAddr）
    pub async fn send_to_addr(&self, addr: &SocketAddr, message: &BusinessMessage) -> Result<()> {
        let server = self.inner.server.read().await;
        if let Some(s) = &*server {
            let json = message.to_json()?;
            s.send_text_to(addr, &json).await
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

        let server = self.inner.server.read().await;
        if let Some(s) = &*server {
            if let Some(addr) = exclude_addr {
                s.broadcast_to_others(&addr, message).await?;
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
        let server = self.inner.server.read().await;
        if let Some(s) = &*server {
            s.broadcast(message).await
        } else {
            Err(AppError::WebSocket("Server not started".to_string()))
        }
    }

    /// 向所有客户端广播（包含未认证）
    pub async fn broadcast_all(&self, message: &BusinessMessage) -> Result<()> {
        self.broadcast(message).await
    }

    /// 向除指定设备外的所有已认证客户端广播（基于设备名称）
    ///
    /// 用于同步事件广播，排除触发操作的设备
    pub async fn broadcast_sync_to_others(
        &self,
        exclude_device_name: &str,
        message: &BusinessMessage,
    ) -> Result<()> {
        // 查找设备名称对应的地址
        let exclude_addr = {
            let addr_to_device_name = self.inner.addr_to_device_name.read().await;
            addr_to_device_name
                .iter()
                .find(|(_, name)| *name == exclude_device_name)
                .map(|(addr, _)| *addr)
        };

        let server = self.inner.server.read().await;
        if let Some(s) = &*server {
            if let Some(addr) = exclude_addr {
                s.broadcast_to_others(&addr, message).await?;
            } else {
                // 未找到设备，广播给所有客户端
                self.broadcast(message).await?;
            }
            Ok(())
        } else {
            Err(AppError::WebSocket("Server not started".to_string()))
        }
    }

    // ==================== Event Subscription ====================

    /// 订阅服务器事件
    pub fn subscribe(&self) -> broadcast::Receiver<WsServerEvent> {
        // 注意：这里需要获取 server 的锁，但在没有 server 的情况下返回一个空的 receiver
        let server = self.inner.server.blocking_read();
        if let Some(s) = &*server {
            s.subscribe()
        } else {
            let (tx, rx) = broadcast::channel(1);
            let _ = tx; // 避免未使用警告
            rx
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

    /// 获取设备名称（通过地址）
    pub async fn get_device_name_by_addr(&self, addr: &SocketAddr) -> Option<String> {
        let addr_to_device_name = self.inner.addr_to_device_name.read().await;
        addr_to_device_name.get(addr).cloned()
    }

    /// 更新客户端认证状态
    pub async fn set_authenticated(&self, addr: &SocketAddr, client_id: Option<String>) {
        let server = self.inner.server.read().await;
        if let Some(s) = &*server {
            let cid = client_id.clone();
            s.set_authenticated(addr, cid).await;

            if let Some(cid) = client_id {
                let mut client_id_to_addr = self.inner.client_id_to_addr.write().await;
                client_id_to_addr.insert(cid.clone(), *addr);

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

    /// 清理客户端连接数据（公开方法，供外部事件处理器调用）
    pub async fn cleanup_client_by_addr(&self, addr: SocketAddr) {
        // 获取 client_id
        let client_id = {
            let addr_to_client_id = self.inner.addr_to_client_id.read().await;
            addr_to_client_id.get(&addr).cloned()
        };

        // 清理映射
        if let Some(cid) = client_id.clone() {
            let mut client_id_to_addr = self.inner.client_id_to_addr.write().await;
            client_id_to_addr.remove(&cid);
        }

        {
            let mut addr_to_client_id = self.inner.addr_to_client_id.write().await;
            addr_to_client_id.remove(&addr);
        }

        {
            let mut addr_to_device_name = self.inner.addr_to_device_name.write().await;
            addr_to_device_name.remove(&addr);
        }

        {
            let mut addr_to_connected_at = self.inner.addr_to_connected_at.write().await;
            addr_to_connected_at.remove(&addr);
        }

        // 清理 GlobalOutputManager 中该客户端的所有订阅
        // 防止断开后仍尝试向已关闭的通道发送数据
        if let Some(cid) = client_id {
            use crate::desktop::session::GlobalOutputManager;
            let global_manager = GlobalOutputManager::global();
            global_manager.unsubscribe_all_for_client(&cid).await;
            tracing::info!("[WebSocketManager] Cleaned up all subscriptions for client {}", cid);
        }
    }
}

// ==================== Private Helper Methods ====================

impl WebSocketManager {
    /// 向指定地址发送消息
    async fn send_to_addr_internal(
        inner: &Arc<WsManagerInner>,
        addr: &SocketAddr,
        message: &BusinessMessage,
    ) -> Result<()> {
        let server = inner.server.read().await;
        if let Some(s) = &*server {
            let json = message.to_json()?;
            s.send_text_to(addr, &json).await
        } else {
            Err(AppError::WebSocket("Server not started".to_string()))
        }
    }
}