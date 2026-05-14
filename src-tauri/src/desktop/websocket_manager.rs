//! WebSocket Manager
//!
//! 单例模式的 WebSocket 服务器管理器
//! 提供移动端远程控制功能的便捷操作 API

use crate::desktop::server::message::Message as BusinessMessage;
use crate::shared::websocket::{
    ClientInfo as WsClientInfo, WsServer, WsServerConfig, WsServerEvent,
};
use crate::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// 客户端摘要（对外暴露的信息）
#[derive(Debug, Clone)]
pub struct ClientSummary {
    pub client_id: String,
    pub device_name: Option<String>,
    pub addr: String,
    pub authenticated: bool,
    pub connected_at: i64,
}

/// 业务消息处理器 trait
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
#[derive(Debug, Clone, Default)]
pub struct NoopBusinessHandler;

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
        static INSTANCE: WebSocketManager = WebSocketManager {
            inner: Arc::new(WsManagerInner::new()),
        };
        &INSTANCE
    }
}