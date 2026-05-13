//! Business Message Handler
//!
//! 实现 shared/websocket 的 MessageHandler trait
//! 桥接 shared 底层连接管理和 desktop 业务逻辑

use crate::desktop::plugin::PluginManager;
use crate::desktop::session::SessionManager;
use crate::desktop::websocket::message::Message;
use crate::shared::auth::PairingService;
use crate::shared::auth::QrTokenManager;
use crate::shared::db::Database;
use crate::shared::websocket::message::WsMessage;
use crate::shared::websocket::server::{ClientInfo, HandlerResult, MessageHandler};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{AppHandle, Emitter};

/// 业务消息处理器
///
/// 包装现有的 handlers 逻辑，实现 shared/websocket 的 MessageHandler trait
pub struct BusinessMessageHandler {
    session_manager: Arc<SessionManager>,
    plugin_manager: Arc<PluginManager>,
    db: Arc<Mutex<Database>>,
    pairing_service: Arc<PairingService>,
    qr_manager: Arc<QrTokenManager>,
    /// Tauri AppHandle 用于发送事件
    app_handle: Option<Arc<AppHandle>>,
}

impl BusinessMessageHandler {
    /// 创建新的业务消息处理器
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_manager: Arc<SessionManager>,
        plugin_manager: Arc<PluginManager>,
        db: Arc<Mutex<Database>>,
        pairing_service: Arc<PairingService>,
        qr_manager: Arc<QrTokenManager>,
        app_handle: Option<Arc<AppHandle>>,
    ) -> Self {
        Self {
            session_manager,
            plugin_manager,
            db,
            pairing_service,
            qr_manager,
            app_handle,
        }
    }
}

impl MessageHandler for BusinessMessageHandler {
    fn handle_text(&self, message: &WsMessage, _addr: SocketAddr, _client_info: &ClientInfo) -> HandlerResult {
        // 将 shared 的 WsMessage 转换为 desktop 的 Message
        match message {
            WsMessage::Text { payload, .. } => {
                // 尝试解析为业务消息
                match Message::from_json(&payload.content) {
                    Ok(business_msg) => {
                        tracing::debug!("Received business message: {:?}", business_msg.message_id());
                        Ok(None)
                    }
                    Err(_) => {
                        Ok(None)
                    }
                }
            }
            WsMessage::Binary { payload, .. } => {
                let data = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    &payload.data,
                ).unwrap_or_default();
                tracing::debug!("Received binary message: {} bytes", data.len());
                Ok(None)
            }
            _ => Ok(None)
        }
    }

    fn on_authenticated(&self, addr: SocketAddr, client_id: &str) {
        tracing::info!("Client {} authenticated as {}", addr, client_id);
        if let Some(ref handle) = self.app_handle {
            let event = crate::desktop::websocket::message::DeviceConnectionEvent {
                addr: addr.to_string(),
                device_id: client_id.to_string(),
                device_name: None,
                event: "authenticated".to_string(),
            };
            let _ = handle.emit("device-connected", &event);
        }
    }

    fn on_disconnected(&self, addr: SocketAddr, client_id: Option<&str>) {
        tracing::info!("Client {} disconnected, client_id: {:?}", addr, client_id);
        if let Some(ref handle) = self.app_handle {
            let event = crate::desktop::websocket::message::DeviceConnectionEvent {
                addr: addr.to_string(),
                device_id: client_id.unwrap_or("").to_string(),
                device_name: None,
                event: "disconnected".to_string(),
            };
            let _ = handle.emit("device-disconnected", &event);
        }
    }
}