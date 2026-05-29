//! Auth Router - 认证消息路由处理器

use async_trait::async_trait;

use crate::shared::model::message::Message;
use crate::shared::enums::auth::AuthStage;
use crate::Result;

use super::{ClientRouteContext, ClientRouteHandler, MobileEvent};

/// 认证消息路由器
pub struct AuthRouter;

#[async_trait]
impl ClientRouteHandler for AuthRouter {
    async fn handle(&self, message: Message, ctx: &ClientRouteContext) -> Result<Option<Message>> {
        if let Message::Auth { payload, .. } = message {
            match payload.stage {
                AuthStage::Authenticated => {
                    if let (Some(device_id), Some(session_token)) = (payload.device_id, payload.session_token) {
                        tracing::info!("[AuthRouter] Authenticated: device_id={}", device_id);
                        ctx.emit(MobileEvent::AuthSuccess {
                            device_id,
                            session_token,
                        });
                    }
                }
                AuthStage::VerifyCode => {
                    tracing::info!("[AuthRouter] PairingVerified");
                    ctx.emit(MobileEvent::PairingVerified);
                }
                AuthStage::Failed => {
                    let reason = payload.error.unwrap_or_else(|| "Authentication failed".to_string());
                    tracing::warn!("[AuthRouter] AuthFailed: {}", reason);
                    ctx.emit(MobileEvent::AuthFailed { reason });
                }
                _ => {}
            }
        }
        Ok(None)
    }

    fn name(&self) -> &str {
        "AuthRouter"
    }
}

impl Default for AuthRouter {
    fn default() -> Self {
        Self
    }
}
