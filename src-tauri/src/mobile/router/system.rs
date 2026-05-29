//! System Router - 系统消息路由处理器

use async_trait::async_trait;

use crate::shared::model::message::Message;
use crate::Result;

use super::{ClientRouteContext, ClientRouteHandler, MobileEvent};

/// 系统消息路由器
///
/// 处理 ServerClosed、Error、Ack 消息
pub struct SystemRouter;

#[async_trait]
impl ClientRouteHandler for SystemRouter {
    async fn handle(&self, message: Message, ctx: &ClientRouteContext) -> Result<Option<Message>> {
        match message {
            Message::ServerClosed { reason, .. } => {
                tracing::info!("[SystemRouter] ServerClosed: {}", reason);
                ctx.emit(MobileEvent::ServerClosed { reason });
            }
            Message::Error { message, code, .. } => {
                let msg = if !message.is_empty() { message } else { code };
                tracing::warn!("[SystemRouter] Error: {}", msg);
                ctx.emit(MobileEvent::Error { message: msg });
            }
            Message::Ack { request_id, .. } => {
                tracing::debug!("[SystemRouter] Ack: request_id={}", request_id);
                ctx.emit(MobileEvent::Ack { request_id });
            }
            _ => {}
        }
        Ok(None)
    }

    fn name(&self) -> &str {
        "SystemRouter"
    }
}

impl Default for SystemRouter {
    fn default() -> Self {
        Self
    }
}
