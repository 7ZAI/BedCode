//! Business Message Handler
//!
//! 桌面端业务消息处理入口
//! 负责认证拦截和消息路由

use crate::desktop::server::connection_types::AuthPayload;
use crate::desktop::server::message::Message as BusinessMessage;
use crate::desktop::server::services::session_control::handle_control;
use crate::desktop::server::services::qr_token_service::QrTokenService;
use crate::desktop::websocket_manager::BusinessHandler;
use crate::shared::auth::JwtService;
use crate::shared::auth::PairingService;
use crate::shared::db::Database;
use crate::shared::enums::AuthStage;
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
    qr_manager: Arc<QrTokenService>,
    /// App Handle（用于发送事件）
    app_handle: Option<Arc<AppHandle>>,
}

impl BusinessMessageHandler {
    /// 创建新的业务消息处理器
    pub fn new(
        db: Arc<Mutex<Database>>,
        pairing_service: Arc<PairingService>,
        qr_manager: Arc<QrTokenService>,
        app_handle: Option<Arc<AppHandle>>,
    ) -> Self {
        Self {
            jwt_service: JwtService::new(),
            db,
            pairing_service,
            qr_manager,
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
                    // 配对和 QR 连接流程 → 委托给 services/auth.rs::handle_auth
                    AuthStage::RequestPairing | AuthStage::VerifyCode | AuthStage::QrConnect => {
                        crate::desktop::server::services::auth::handle_auth(
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
                        self.handle_jwt_auth(message_id, session_id, timestamp, payload, addr)
                            .await
                    }
                    _ => {
                        self.handle_jwt_auth(message_id, session_id, timestamp, payload, addr)
                            .await
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

    /// 处理 JWT 认证
    async fn handle_jwt_auth(
        &self,
        message_id: String,
        session_id: Option<String>,
        timestamp: i64,
        payload: AuthPayload,
        addr: SocketAddr,
    ) -> Result<Option<BusinessMessage>> {
        // 从 payload 中获取 JWT token
        let token = match &payload.session_token {
            Some(t) if !t.is_empty() => t.clone(),
            _ => {
                // 没有 token，返回认证失败
                return Ok(Some(BusinessMessage::Auth {
                    message_id,
                    session_id,
                    timestamp,
                    payload: AuthPayload {
                        stage: crate::desktop::server::connection_types::AuthStage::Failed,
                        error: Some("No JWT token provided".to_string()),
                        ..Default::default()
                    },
                }));
            }
        };

        // 验证 JWT
        match self.jwt_service.verify_token_with_expiry(&token) {
            Ok(claims) => {
                // JWT 验证成功，使用 WebSocketManager 设置客户端为已认证
                let ws_manager = crate::desktop::websocket_manager::WebSocketManager::global();
                ws_manager.set_authenticated(&addr, Some(claims.sub.clone())).await;
                if let Some(name) = &claims.device_name {
                    ws_manager.set_device_name(&addr, Some(name.clone())).await;
                }

                // 发送认证成功事件给前端
                if let Some(handle) = &self.app_handle {
                    let event = crate::desktop::server::connection_types::DeviceConnectionEvent {
                        addr: addr.to_string(),
                        device_id: claims.sub.clone(),
                        device_name: claims.device_name.clone(),
                        event: "authenticated".to_string(),
                    };
                    let _ = handle.emit("device-connected", &event);
                }

                tracing::info!(
                    "Client {} authenticated via JWT (device: {})",
                    addr,
                    claims.sub
                );

                // 返回认证成功响应
                Ok(Some(BusinessMessage::Auth {
                    message_id,
                    session_id,
                    timestamp,
                    payload: AuthPayload {
                        stage: crate::desktop::server::connection_types::AuthStage::Authenticated,
                        device_id: Some(claims.sub),
                        device_name: claims.device_name,
                        device_fingerprint: claims.fingerprint,
                        session_token: Some(token),
                        error: None,
                        ..Default::default()
                    },
                }))
            }
            Err(e) => {
                tracing::warn!("JWT verification failed for {}: {}", addr, e);

                // 返回认证失败响应
                let error_msg = match e {
                    crate::shared::auth::JwtError::TokenExpired => "JWT token expired, please re-authenticate",
                    _ => "Invalid JWT token",
                };

                Ok(Some(BusinessMessage::Auth {
                    message_id,
                    session_id,
                    timestamp,
                    payload: AuthPayload {
                        stage: crate::desktop::server::connection_types::AuthStage::Failed,
                        error: Some(error_msg.to_string()),
                        ..Default::default()
                    },
                }))
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
                message_id: _,
                session_id: _,
                timestamp: _,
                payload: _,
            } => {
                // TODO: 实现会话控制功能，需要注入 SessionManager 和 PluginManager
                tracing::debug!("Control message received - not implemented yet");
                Ok(None)
            }
            BusinessMessage::Input {
                message_id,
                session_id,
                timestamp,
                payload,
            } => {
                // 处理输入消息，转发到 PTY
                // TODO: 实现输入转发
                tracing::debug!("Input message for session {}: {:?}", session_id, payload);
                Ok(None)
            }
            BusinessMessage::Output { .. } => {
                // 服务端不需要处理 Output 消息
                Ok(None)
            }
            BusinessMessage::Heartbeat { .. } => {
                // 心跳消息直接响应
                Ok(Some(BusinessMessage::heartbeat()))
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
            // If client_id is not a valid SocketAddr, try as identifier
            tracing::warn!("Invalid client_id format: {}", client_id);
            Ok(None)
        }
    }

    fn on_connected(&self, _client_id: &str, _device_name: Option<String>) {}

    fn on_disconnected(&self, _client_id: &str) {}
}