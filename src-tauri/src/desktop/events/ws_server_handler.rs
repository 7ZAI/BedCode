//! WebSocket Server Event Handler
//!
//! 处理 WsServerEvent，包括连接、断开、心跳超时等事件

use crate::shared::event::handler::EventHandler;
use crate::shared::websocket::WsServerEvent;
use crate::desktop::session::GlobalOutputManager;
use crate::desktop::websocket_manager::WebSocketManager;
use chrono::Utc;
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// WebSocket 服务器事件处理器
///
/// 处理客户端连接、断开、心跳超时等事件
pub struct WsServerEventHandler {
    ws_manager: &'static WebSocketManager,
    app_handle: Option<Arc<AppHandle>>,
}

impl WsServerEventHandler {
    /// 创建新的事件处理器
    pub fn new(
        ws_manager: &'static WebSocketManager,
        app_handle: Option<Arc<AppHandle>>,
    ) -> Self {
        Self {
            ws_manager,
            app_handle,
        }
    }

    /// 异步处理事件
    async fn process_event(&self, event: WsServerEvent) {
        match event {
            WsServerEvent::ClientConnected { addr, client_id } => {
                self.handle_client_connected(addr, client_id).await;
            }
            WsServerEvent::ClientDisconnected { addr, client_id, reason } => {
                self.handle_client_disconnected(addr, client_id, reason).await;
            }
            WsServerEvent::HeartbeatTimeout { addr, client_id } => {
                self.handle_heartbeat_timeout(addr, client_id).await;
            }
            WsServerEvent::AuthSuccess { addr, client_id } => {
                self.handle_auth_success(addr, client_id).await;
            }
            _ => {}
        }
    }

    /// 处理客户端连接
    async fn handle_client_connected(&self, addr: SocketAddr, client_id: Option<String>) {
        tracing::info!("[WsServerEventHandler] Client connected: {}", addr);

        // 向前端发送设备连接事件
        if let Some(ref client_id) = client_id {
            self.emit_device_event("device-connected", client_id, addr).await;
        }
    }

    /// 处理客户端断开连接
    async fn handle_client_disconnected(
        &self,
        addr: SocketAddr,
        client_id: Option<String>,
        reason: Option<String>,
    ) {
        tracing::info!(
            "[WsServerEventHandler] Client disconnected: {}, reason: {:?}",
            addr, reason
        );

        // 清理客户端数据
        self.cleanup_client(&addr).await;

        // 向前端发送设备断开事件
        if let Some(ref client_id) = client_id {
            self.emit_device_event("device-disconnected", client_id, addr).await;
        }
    }

    /// 处理心跳超时
    async fn handle_heartbeat_timeout(&self, addr: SocketAddr, client_id: Option<String>) {
        tracing::warn!(
            "[WsServerEventHandler] Heartbeat timeout for client: {} ({:?})",
            addr, client_id
        );

        // 向前端发送心跳超时事件
        if let Some(ref client_id) = client_id {
            self.emit_device_event("device-heartbeat-timeout", client_id, addr).await;
        }
    }

    /// 处理认证成功
    async fn handle_auth_success(&self, addr: SocketAddr, client_id: String) {
        tracing::info!(
            "[WsServerEventHandler] Client authenticated: {} ({})",
            addr, client_id
        );

        // 向前端发送认证成功事件
        self.emit_device_event("device-authenticated", &client_id, addr).await;
    }

    /// 清理客户端连接数据
    async fn cleanup_client(&self, addr: &SocketAddr) {
        // 获取 client_id
        let client_id = self.ws_manager.get_client_by_addr(addr).await
            .map(|c| c.client_id);

        // 清理 WebSocketManager 中的映射
        self.ws_manager.cleanup_client_by_addr(*addr).await;

        // 清理 GlobalOutputManager 中该客户端的所有订阅
        // 防止断开后仍尝试向已关闭的通道发送数据
        if let Some(cid) = client_id {
            let global_manager = GlobalOutputManager::global();
            global_manager.unsubscribe_all_for_client(&cid).await;
            tracing::info!(
                "[WsServerEventHandler] Cleaned up all subscriptions for client {}",
                cid
            );
        }
    }

    /// 向前端发送设备事件
    async fn emit_device_event(&self, event_name: &str, client_id: &str, addr: SocketAddr) {
        if let Some(ref app_handle) = self.app_handle {
            let payload = serde_json::json!({
                "clientId": client_id,
                "addr": addr.to_string(),
                "timestamp": Utc::now().timestamp_millis(),
            });
            if let Err(e) = app_handle.emit(event_name, &payload) {
                tracing::error!(
                    "[WsServerEventHandler] Failed to emit event '{}': {}",
                    event_name, e
                );
            }
        }
    }
}

impl EventHandler<WsServerEvent> for WsServerEventHandler {
    fn handle(&self, event: WsServerEvent) {
        let ws_manager = self.ws_manager;
        let app_handle = self.app_handle.clone();

        tokio::spawn(async move {
            let handler = WsServerEventHandler {
                ws_manager,
                app_handle,
            };
            handler.process_event(event).await;
        });
    }
}
