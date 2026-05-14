//! Business Message Handler
//!
//! 桌面端业务消息处理入口
//! 负责认证拦截和消息路由

use crate::desktop::server::connection_types::AuthPayload;
use crate::desktop::server::message::Message as BusinessMessage;
use crate::desktop::server::services::session_control::handle_control;
use crate::shared::auth::JwtService;
use crate::shared::db::Database;
use crate::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tauri::{AppHandle, Emitter};

/// 业务消息处理器
/// 负责：
/// 1. 认证拦截 - 所有消息需要认证
/// 2. JWT 验证 - 对未认证客户端进行 JWT 验证
/// 3. 消息路由 - 将消息转发给不同的业务 handler
pub struct BusinessMessageHandler {
    /// JWT 服务
    jwt_service: JwtService,
    /// 数据库
    db: Arc<Mutex<Database>>,
    /// 客户端信息（用于获取认证状态）
    clients: Arc<RwLock<HashMap<SocketAddr, crate::desktop::server::ClientInfo>>>,
    /// App Handle（用于发送事件）
    app_handle: Option<Arc<AppHandle>>,
}

impl BusinessMessageHandler {
    /// 创建新的业务消息处理器
    pub fn new(
        db: Arc<Mutex<Database>>,
        clients: Arc<RwLock<HashMap<SocketAddr, crate::desktop::server::ClientInfo>>>,
        app_handle: Option<Arc<AppHandle>>,
    ) -> Self {
        Self {
            jwt_service: JwtService::new(),
            db,
            clients,
            app_handle,
        }
    }

    /// 处理业务消息
    pub async fn handle_message(
        &self,
        message: BusinessMessage,
        addr: SocketAddr,
    ) -> Result<Option<BusinessMessage>> {
        // 检查认证状态
        let is_authenticated = {
            let clients = self.clients.read().await;
            clients
                .get(&addr)
                .map(|c| c.authenticated)
                .unwrap_or(false)
        };

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
                // 尝试 JWT 认证
                self.handle_jwt_auth(message_id, session_id, timestamp, payload, addr)
                    .await
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
                // JWT 验证成功，设置客户端为已认证
                {
                    let mut clients = self.clients.write().await;
                    if let Some(client) = clients.get_mut(&addr) {
                        client.authenticated = true;
                        client.device_id = Some(claims.sub.clone());
                        client.device_name = claims.device_name.clone();
                    }
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
                message_id,
                session_id,
                timestamp,
                payload,
            } => {
                handle_control(
                    payload.action,
                    message_id,
                    // TODO: 从 dependency injection 获取
                    &Arc::new(crate::desktop::session::SessionManager::new()),
                    &Arc::new(crate::desktop::plugin::PluginManager::new()),
                    &self.db,
                    &self.clients,
                    addr,
                    None,
                )
                .await
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