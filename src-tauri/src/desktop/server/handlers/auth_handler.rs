//! Auth Handler - 认证消息处理器
//!
//! 处理 `Message::Auth` 消息，包括配对流程和 JWT 验证

use crate::desktop::server::message::{AuthPayload, Message as BusinessMessage};
use crate::shared::websocket::server::context::RouteContext;
use crate::desktop::server::router::handler::RouteHandler;
use crate::desktop::server::services::auth_service::{handle_auth, handle_jwt_auth};
use crate::shared::auth::JwtService;
use crate::shared::enums::AuthStage;
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AuthHandler {
    pairing_service: Arc<crate::desktop::server::services::PairingService>,
    qr_manager: Arc<crate::shared::auth::QrTokenManager>,
    jwt_service: JwtService,
}

impl AuthHandler {
    pub fn new(
        pairing_service: Arc<crate::desktop::server::services::PairingService>,
        qr_manager: Arc<crate::shared::auth::QrTokenManager>,
    ) -> Self {
        Self {
            pairing_service,
            qr_manager,
            jwt_service: JwtService::new(),
        }
    }
}

#[async_trait]
impl RouteHandler for AuthHandler {
    async fn handle(
        &self,
        message: BusinessMessage,
        ctx: &RouteContext,
    ) -> Result<Option<BusinessMessage>> {
        let (message_id, session_id, timestamp, payload) = match message {
            BusinessMessage::Auth {
                message_id,
                expect_response: _,
                session_id,
                timestamp,
                payload,
            } => (message_id, session_id, timestamp, payload),
            _ => return Ok(None),
        };

        let ws_manager = crate::desktop::websocket_manager::WebSocketManager::global();

        match payload.stage {
            // 配对和 QR 连接流程
            AuthStage::RequestPairing | AuthStage::VerifyCode | AuthStage::QrConnect => {
                handle_auth(
                    payload,
                    message_id,
                    ctx.addr,
                    &self.pairing_service,
                    &self.qr_manager,
                    &self.jwt_service,
                    &ws_manager,
                    &None, // app_handle 暂不支持
                ).await
            }
            // JWT 验证
            AuthStage::Authenticated | _ => {
                handle_jwt_auth(
                    message_id,
                    session_id,
                    timestamp,
                    payload,
                    ctx.addr,
                    &self.jwt_service,
                    &None,
                ).await
            }
        }
    }
}