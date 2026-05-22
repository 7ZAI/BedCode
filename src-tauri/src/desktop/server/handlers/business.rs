//! Business Message Handler
//!
//! 桌面端业务消息处理入口
//! 负责认证拦截和消息路由

use crate::desktop::plugin::PluginManager;
use crate::desktop::server::connection_types::AuthPayload;
use crate::desktop::server::message::Message as BusinessMessage;
use crate::desktop::server::services::session_control::handle_control_message;
use crate::desktop::server::services::auth_service::handle_jwt_auth;
use crate::desktop::server::services::input_service::handle_input;
use crate::shared::auth::qr_token::QrTokenManager;
use crate::desktop::session::SessionManager;
use crate::desktop::websocket_manager::BusinessHandler;
use crate::shared::auth::JwtService;
use crate::shared::db::Database;
use crate::shared::enums::AuthStage;
use crate::desktop::server::services::PairingService;
use crate::Result;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{AppHandle, Emitter};

/// 业务消息处理器
/// 负责：
/// 1. 认证拦截 - 所有消息需要认证
/// 2. JWT 验证 - 对未认证客户端进行 JWT 验证
/// 3. 配对处理 - RequestPairing / VerifyCode 配对流程
/// 4. 消息路由 - 将消息转发给不同的业务 handler
pub struct BusinessMessageHandler {
    /// JWT 服务
    jwt_service: JwtService,
    /// 数据库
    db: Arc<Mutex<Database>>,
    /// 配对服务
    pairing_service: Arc<PairingService>,
    /// QR 令牌管理器
    qr_manager: Arc<QrTokenManager>,
    /// 会话管理器
    session_manager: Option<Arc<SessionManager>>,
    /// 插件管理器
    plugin_manager: Option<Arc<PluginManager>>,
    /// App Handle（用于发送事件）
    app_handle: Option<Arc<AppHandle>>,
}

impl BusinessMessageHandler {
    /// 创建新的业务消息处理器
    pub fn new(
        db: Arc<Mutex<Database>>,
        pairing_service: Arc<PairingService>,
        qr_manager: Arc<QrTokenManager>,
        session_manager: Option<Arc<SessionManager>>,
        plugin_manager: Option<Arc<PluginManager>>,
        app_handle: Option<Arc<AppHandle>>,
    ) -> Self {
        Self {
            jwt_service: JwtService::new(),
            db,
            pairing_service,
            qr_manager,
            session_manager,
            plugin_manager,
            app_handle,
        }
    }

    /// 处理业务消息
    pub async fn handle_message(
        &self,
        message: BusinessMessage,
        addr: SocketAddr,
    ) -> Result<Option<BusinessMessage>> {
        // 使用 WebSocketManager 检查认证状态
        let ws_manager = crate::desktop::websocket_manager::WebSocketManager::global();
        let client = ws_manager.get_client_by_addr(&addr).await;
        let is_authenticated = client.map(|c| c.authenticated).unwrap_or(false);

        // 如果已认证，直接处理消息
        if is_authenticated {
            return self.dispatch_message(message, addr).await;
        }

        // 未认证，只允许 Auth 消息通过
        match message {
            BusinessMessage::Auth {
                message_id,
                session_id,
                timestamp,
                payload,
            } => {
                match payload.stage {
                    // 配对和 QR 连接流程 → 委托给 auth_service::handle_auth
                    AuthStage::RequestPairing | AuthStage::VerifyCode | AuthStage::QrConnect => {
                        crate::desktop::server::services::auth_service::handle_auth(
                            payload,
                            message_id,
                            addr,
                            &self.db,
                            &self.pairing_service,
                            &self.qr_manager,
                            &self.jwt_service,
                            &ws_manager,
                            &self.app_handle,
                        ).await
                    }
                    // 已认证 JWT Token → 走 JWT 验证
                    AuthStage::Authenticated => {
                        handle_jwt_auth(
                            message_id,
                            session_id,
                            timestamp,
                            payload,
                            addr,
                            &self.jwt_service,
                            &self.app_handle,
                        ).await
                    }
                    _ => {
                        handle_jwt_auth(
                            message_id,
                            session_id,
                            timestamp,
                            payload,
                            addr,
                            &self.jwt_service,
                            &self.app_handle,
                        ).await
                    }
                }
            }
            _ => {
                // 其他消息返回未认证错误
                tracing::warn!("Unauthenticated message from {}: {:?}", addr, message);
                Ok(Some(BusinessMessage::error(
                    "NOT_AUTHENTICATED",
                    "Please authenticate first by sending an Auth message with valid JWT token",
                )))
            }
        }
    }

    /// 消息路由 - 将消息分发到不同的 handler
    async fn dispatch_message(
        &self,
        message: BusinessMessage,
        addr: SocketAddr,
    ) -> Result<Option<BusinessMessage>> {
        match message {
            BusinessMessage::Auth { .. } => {
                // 已认证客户端发送的 Auth 消息，忽略或返回错误
                Ok(None)
            }
            BusinessMessage::Control {
                message_id,
                session_id,
                timestamp,
                payload,
            } => {
                // 委托给 session_control 服务统一处理
                handle_control_message(
                    message_id,
                    session_id,
                    timestamp,
                    payload.action,
                    &self.session_manager,
                    &self.plugin_manager,
                    &self.db,
                    addr,
                ).await
            }
            BusinessMessage::Input {
                session_id,
                payload,
                message_id,
                timestamp,
            } => {
                // 委托给 input_service 处理
                handle_input(
                    &session_id,
                    payload,
                    &self.session_manager,
                ).await
            }
            BusinessMessage::Output { .. } => {
                // 服务端不需要处理 Output 消息
                Ok(None)
            }
            BusinessMessage::Heartbeat { .. } => {
                // 心跳消息直接响应
                Ok(Some(BusinessMessage::heartbeat()))
            }
            BusinessMessage::Subscribe {
                message_id,
                session_id,
                start_seq,
            } => {
                // 处理订阅请求
                let ws_manager = crate::desktop::websocket_manager::WebSocketManager::global();
                let subscription_manager = ws_manager.subscription_manager();

                // 获取客户端标识
                let client = ws_manager.get_client_by_addr(&addr).await;
                let client_id = client.map(|c| c.client_id.clone()).unwrap_or_else(|| addr.to_string());

                // 调用订阅管理器处理订阅
                match subscription_manager.subscribe(client_id.clone(), session_id.clone(), start_seq).await {
                    Ok(response) => {
                        tracing::info!("Client subscribed to session: {} (client: {})", session_id, client_id);
                        Ok(Some(BusinessMessage::SubscribeResponse {
                            message_id,
                            session_id,
                            current_max_seq: response.current_max_seq,
                            history_count: response.history_count,
                        }))
                    }
                    Err(e) => {
                        tracing::error!("Subscribe failed: {}", e);
                        Ok(Some(BusinessMessage::error("SUBSCRIBE_FAILED", &e)))
                    }
                }
            }
            BusinessMessage::Unsubscribe {
                message_id,
                session_id,
            } => {
                // 处理取消订阅请求
                let ws_manager = crate::desktop::websocket_manager::WebSocketManager::global();
                let subscription_manager = ws_manager.subscription_manager();

                // 获取客户端标识
                let client = ws_manager.get_client_by_addr(&addr).await;
                let client_id = client.map(|c| c.client_id.clone()).unwrap_or_else(|| addr.to_string());

                // 调用订阅管理器处理取消订阅
                match subscription_manager.unsubscribe(&client_id, &session_id).await {
                    Ok(_) => {
                        tracing::info!("Client unsubscribed from session: {} (client: {})", session_id, client_id);
                        Ok(Some(BusinessMessage::UnsubscribeResponse {
                            message_id,
                            session_id,
                        }))
                    }
                    Err(e) => {
                        tracing::error!("Unsubscribe failed: {}", e);
                        Ok(Some(BusinessMessage::error("UNSUBSCRIBE_FAILED", &e)))
                    }
                }
            }
            _ => Ok(None),
        }
    }
}

// Implement BusinessHandler to be compatible with WebSocketManager
#[async_trait]
impl BusinessHandler for BusinessMessageHandler {
    async fn handle_message(
        &self,
        msg: BusinessMessage,
        client_id: &str,
    ) -> Result<Option<BusinessMessage>> {
        // Parse client_id as SocketAddr
        if let Ok(addr) = client_id.parse::<SocketAddr>() {
            self.handle_message(msg, addr).await
        } else {
            // If client_id is not a valid SocketAddr, try looking up in WebSocketManager
            let ws_manager = crate::desktop::websocket_manager::WebSocketManager::global();
            if let Some(client) = ws_manager.get_client(client_id).await {
                if let Ok(addr) = client.addr.parse::<SocketAddr>() {
                    return self.handle_message(msg, addr).await;
                }
            }
            tracing::warn!("Invalid client_id format: {} (not found in client map)", client_id);
            Ok(None)
        }
    }

    fn on_connected(&self, _client_id: &str, _device_name: Option<String>) {}

    fn on_disconnected(&self, _client_id: &str) {}
}