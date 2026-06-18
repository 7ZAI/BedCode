//! WebSocket Manager
//!
//! 单例模式的服务器管理器
//! 使用 Actix Web 提供 HTTP REST API + WebSocket 终端
//! 客户端跟踪和同步广播通过 Actix WS actor 地址实现

use crate::desktop::pty::PtySubscriptionManager;
use crate::desktop::server::message::Message as BusinessMessage;
use crate::desktop::session::SessionManager;
use crate::desktop::plugin::PluginManager;
use crate::shared::system::error::AppError;
use crate::shared::auth::QrTokenManager;
use crate::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::RwLock;

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
    /// SocketAddr 到认证状态的映射
    addr_to_authenticated: RwLock<HashMap<SocketAddr, bool>>,
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
            subscription_manager: Arc::new(PtySubscriptionManager::new()),
            client_id_to_addr: RwLock::new(HashMap::new()),
            addr_to_client_id: RwLock::new(HashMap::new()),
            addr_to_device_name: RwLock::new(HashMap::new()),
            addr_to_connected_at: RwLock::new(HashMap::new()),
            addr_to_authenticated: RwLock::new(HashMap::new()),
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

        {
            let mut db_lock = self.inner.db.write().await;
            *db_lock = Some(db);
        }
        {
            let mut qr_lock = self.inner.qr_manager.write().await;
            *qr_lock = Some(qr_manager);
        }
        {
            let mut ps_lock = self.inner.pairing_service.write().await;
            *ps_lock = Some(pairing_service);
        }
        {
            let mut handle_lock = self.inner.app_handle.write().await;
            *handle_lock = Some(app_handle);
        }
        {
            let mut sm_lock = self.inner.session_manager.write().await;
            *sm_lock = Some(session_manager);
        }
        {
            let mut pm_lock = self.inner.plugin_manager.write().await;
            *pm_lock = Some(plugin_manager);
        }

        tracing::info!("WebSocketManager initialized");
        Ok(())
    }

    /// 启动 Actix Web 服务器（HTTP + WS 统一端口）
    pub async fn start(&self, port: u16) -> Result<()> {
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
                return Err(AppError::WebSocket(
                    "Server already running".to_string(),
                ));
            }
        }

        // 启动 Actix Web 服务器（HTTP REST + WS 终端）
        std::thread::spawn(move || {
            let rt = actix_rt::Runtime::new().expect("Failed to create Actix runtime");
            rt.block_on(async move {
                if let Err(e) = crate::desktop::server::app::start_http_server(port).await {
                    tracing::error!("Actix Web server error: {}", e);
                }
            });
        });

        // 短暂等待服务器启动
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        {
            let mut port_lock = self.inner.port.write().await;
            *port_lock = Some(port);
        }

        tracing::info!("Actix Web server (HTTP + WS) started on port {}", port);
        Ok(())
    }

    /// 停止服务器
    pub async fn stop(&self) -> Result<()> {
        // Actix Web 服务器在独立线程中运行，无法优雅停止
        // 但标记端口为空即可
        {
            let mut port_lock = self.inner.port.write().await;
            *port_lock = None;
        }
        tracing::info!("WebSocketManager stopped");
        Ok(())
    }

    /// 服务器是否运行中
    pub async fn is_running(&self) -> bool {
        let port = self.inner.port.read().await;
        port.is_some()
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
        let addr_to_client_id = self.inner.addr_to_client_id.read().await;
        let addr_to_device_name = self.inner.addr_to_device_name.read().await;
        let addr_to_connected_at = self.inner.addr_to_connected_at.read().await;
        let addr_to_authenticated = self.inner.addr_to_authenticated.read().await;

        addr_to_client_id
            .iter()
            .map(|(addr, client_id)| {
                ClientSummary {
                    client_id: client_id.clone(),
                    device_name: addr_to_device_name.get(addr).cloned(),
                    addr: addr.to_string(),
                    authenticated: addr_to_authenticated.get(addr).copied().unwrap_or(false),
                    connected_at: addr_to_connected_at
                        .get(addr)
                        .copied()
                        .unwrap_or_else(|| Utc::now().timestamp_millis()),
                }
            })
            .collect()
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
        let addr_to_client_id = self.inner.addr_to_client_id.read().await;
        let client_id = addr_to_client_id.get(addr).cloned()?;
        let addr_to_device_name = self.inner.addr_to_device_name.read().await;
        let addr_to_connected_at = self.inner.addr_to_connected_at.read().await;
        let addr_to_authenticated = self.inner.addr_to_authenticated.read().await;

        Some(ClientSummary {
            client_id,
            device_name: addr_to_device_name.get(addr).cloned(),
            addr: addr.to_string(),
            authenticated: addr_to_authenticated.get(addr).copied().unwrap_or(false),
            connected_at: addr_to_connected_at
                .get(addr)
                .copied()
                .unwrap_or_else(|| Utc::now().timestamp_millis()),
        })
    }

    /// 获取客户端数量
    pub async fn client_count(&self) -> usize {
        let addr_to_client_id = self.inner.addr_to_client_id.read().await;
        addr_to_client_id.len()
    }

    /// 获取已认证客户端数量
    pub async fn authenticated_count(&self) -> usize {
        let addr_to_authenticated = self.inner.addr_to_authenticated.read().await;
        addr_to_authenticated.values().filter(|&&v| v).count()
    }

    // ==================== Message Sending APIs ====================

    /// 向指定客户端发送消息（通过 client_id）
    ///
    /// 注意：Actix WS 模式下，消息发送通过 WS actor 内部的订阅机制实现
    /// 此方法用于同步事件广播，需要客户端通过 WS 订阅接收
    pub async fn send_to_client(&self, _client_id: &str, _message: &BusinessMessage) -> Result<()> {
        // TODO: 实现 Actix WS 消息发送（通过 actor 地址注册表）
        tracing::debug!("send_to_client: Actix WS broadcast not yet implemented");
        Ok(())
    }

    /// 向指定客户端发送文本（通过 client_id）
    pub async fn send_text_to_client(&self, _client_id: &str, _text: &str) -> Result<()> {
        tracing::debug!("send_text_to_client: Actix WS broadcast not yet implemented");
        Ok(())
    }

    /// 向指定客户端发送消息（通过 SocketAddr）
    pub async fn send_to_addr(&self, _addr: &SocketAddr, _message: &BusinessMessage) -> Result<()> {
        tracing::debug!("send_to_addr: Actix WS broadcast not yet implemented");
        Ok(())
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
        _exclude_client_id: &str,
        _message: &BusinessMessage,
    ) -> Result<()> {
        // TODO: 实现 Actix WS 广播
        Ok(())
    }

    /// 向所有已认证客户端广播
    pub async fn broadcast(&self, _message: &BusinessMessage) -> Result<()> {
        // TODO: 实现 Actix WS 广播
        Ok(())
    }

    /// 向所有客户端广播（包含未认证）
    pub async fn broadcast_all(&self, message: &BusinessMessage) -> Result<()> {
        self.broadcast(message).await
    }

    /// 向除指定设备外的所有已认证客户端广播（基于设备名称）
    pub async fn broadcast_sync_to_others(
        &self,
        _exclude_device_name: &str,
        _message: &BusinessMessage,
    ) -> Result<()> {
        // TODO: 实现 Actix WS 广播
        Ok(())
    }

    // ==================== Event Subscription ====================

    /// 订阅服务器事件（空实现，Actix WS 不使用此机制）
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<ServerEvent> {
        let (_, rx) = tokio::sync::broadcast::channel(1);
        rx
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
        if let Some(cid) = &client_id {
            let mut client_id_to_addr = self.inner.client_id_to_addr.write().await;
            client_id_to_addr.insert(cid.clone(), *addr);

            let mut addr_to_client_id = self.inner.addr_to_client_id.write().await;
            addr_to_client_id.insert(*addr, cid.clone());
        }

        let mut addr_to_authenticated = self.inner.addr_to_authenticated.write().await;
        addr_to_authenticated.insert(*addr, client_id.is_some());
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
        let client_id = {
            let addr_to_client_id = self.inner.addr_to_client_id.read().await;
            addr_to_client_id.get(&addr).cloned()
        };

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

        {
            let mut addr_to_authenticated = self.inner.addr_to_authenticated.write().await;
            addr_to_authenticated.remove(&addr);
        }

        if let Some(cid) = client_id {
            use crate::desktop::session::GlobalOutputManager;
            let global_manager = GlobalOutputManager::global();
            global_manager.unsubscribe_all_for_client(&cid).await;
            tracing::info!("[WebSocketManager] Cleaned up all subscriptions for client {}", cid);
        }
    }

    // ==================== Getters ====================

    /// 获取数据库实例
    pub async fn db(&self) -> Option<Arc<tokio::sync::Mutex<crate::shared::db::Database>>> {
        self.inner.db.read().await.clone()
    }

    /// 获取 QR Token 管理器
    pub async fn qr_manager(&self) -> Option<Arc<QrTokenManager>> {
        self.inner.qr_manager.read().await.clone()
    }

    /// 获取配对服务
    pub async fn pairing_service(&self) -> Option<Arc<crate::desktop::server::services::PairingService>> {
        self.inner.pairing_service.read().await.clone()
    }

    /// 获取 AppHandle
    pub async fn app_handle(&self) -> Option<Arc<AppHandle>> {
        self.inner.app_handle.read().await.clone()
    }

    /// 获取 SessionManager
    pub async fn session_manager(&self) -> Option<Arc<SessionManager>> {
        self.inner.session_manager.read().await.clone()
    }

    /// 获取 PluginManager
    pub async fn plugin_manager(&self) -> Option<Arc<PluginManager>> {
        self.inner.plugin_manager.read().await.clone()
    }
}

/// 服务器事件（简化版，Actix WS 不使用复杂事件机制）
#[derive(Debug, Clone)]
pub enum ServerEvent {
    Started,
    Stopped,
}
