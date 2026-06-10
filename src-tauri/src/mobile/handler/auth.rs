//! Auth Handler - 认证消息处理器

use async_trait::async_trait;

use crate::shared::model::message::Message;
use crate::shared::enums::auth::AuthStage;
use crate::Result;

use crate::mobile::router::{ClientRouteContext, MobileEvent, ClientRouteHandler};

/// 认证消息处理器
pub struct AuthHandler;

#[async_trait]
impl ClientRouteHandler for AuthHandler {
    async fn handle(&self, message: Message, ctx: &ClientRouteContext) -> Result<Option<Message>> {
        if let Message::Auth { payload, .. } = message {
            match payload.stage {
                AuthStage::Authenticated => {
                    if let Some(session_token) = payload.session_token {
                        tracing::info!("[AuthHandler] Authenticated");
                        ctx.emit(MobileEvent::AuthSuccess {
                            session_token,
                        });
                    }
                }
                AuthStage::VerifyCode => {
                    tracing::info!("[AuthHandler] PairingVerified");
                    ctx.emit(MobileEvent::PairingVerified);
                }
                AuthStage::Failed => {
                    let reason = payload.error.unwrap_or_else(|| "Authentication failed".to_string());
                    tracing::warn!("[AuthHandler] AuthFailed: {}", reason);
                    ctx.emit(MobileEvent::AuthFailed { reason });
                }
                _ => {}
            }
        }
        Ok(None)
    }

    fn name(&self) -> &str {
        "AuthHandler"
    }
}

impl Default for AuthHandler {
    fn default() -> Self {
        Self
    }
}