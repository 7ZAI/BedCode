//! Desktop Server Module
//!
//! 提供移动端远程控制功能的 WebSocket 服务

// 模块声明 - 使用目录名.rs模式
pub mod message;
pub mod client_info;
pub mod connection_types;
pub mod services;
pub mod handlers;
pub mod router;

// 重新导出所有公开类型
pub use message::*;
pub use client_info::ClientInfo;
pub use connection_types::*;
pub use handlers::ControlAction;

// WebSocket Server 实现
use crate::shared::websocket::{WsServer, WsServerConfig};
use crate::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tauri::AppHandle;

/// WebSocket Server 结构体
/// 使用 shared::websocket::WsServer 并添加业务逻辑
pub struct WebSocketServer {
    /// 底层 WsServer
    inner: WsServer,
    /// App Handle
    app_handle: RwLock<Option<Arc<AppHandle>>>,
}

impl WebSocketServer {
    /// 创建新的 WebSocket Server
    pub fn new(
        port: u16,
        _session_manager: Arc<crate::desktop::session::SessionManager>,
        _plugin_manager: Arc<crate::desktop::plugin::PluginManager>,
        _db: Arc<tokio::sync::Mutex<crate::shared::db::Database>>,
        _pairing_service: Arc<crate::desktop::server::services::PairingService>,
        _qr_manager: Arc<crate::shared::auth::QrTokenManager>,
    ) -> Self {
        let config = WsServerConfig {
            port,
            max_connections: 0,
            heartbeat_interval_secs: 30,
            heartbeat_timeout_secs: 90,
            message_queue_size: 256,
            ip_filter: crate::shared::websocket::IpFilter::default(),
            response_handler: None,
        };

        let ws_server = WsServer::new(config);

        Self {
            inner: ws_server,
            app_handle: RwLock::new(None),
        }
    }

    /// 设置 App Handle
    pub fn set_app_handle(&mut self, handle: Arc<AppHandle>) {
        let mut app_handle = self.app_handle.blocking_write();
        *app_handle = Some(handle);
    }

    /// 启动服务器
    pub async fn start(&self) -> Result<()> {
        self.inner.start().await
    }

    /// 获取已连接设备列表
    pub async fn get_connected_devices(&self) -> Vec<crate::desktop::server::connection_types::DeviceConnectionInfo> {
        // 从 WsServer 获取客户端信息
        let clients = self.inner.clients().read().await;
        let mut devices = Vec::new();

        for (addr, info) in clients.iter() {
            devices.push(crate::desktop::server::connection_types::DeviceConnectionInfo {
                addr: addr.to_string(),
                device_id: info.client_id.clone().unwrap_or_else(|| addr.to_string()),
                session_count: 0,
            });
        }

        devices
    }
}