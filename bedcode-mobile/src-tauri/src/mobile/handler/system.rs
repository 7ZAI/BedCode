//! System Handler - 系统消息处理器
//!
//! 处理 ServerClosed、Error、Ack 消息

use async_trait::async_trait;

use crate::shared::model::message::Message;
use crate::Result;

use crate::mobile::router::{ClientRouteContext, MobileEvent, ClientRouteHandler};

/// 系统消息处理器
pub struct SystemHandler;

#[async_trait]
impl ClientRouteHandler for SystemHandler {
    async fn handle(&self, message: Message, ctx: &ClientRouteContext) -> Result<Option<Message>> {
        match message {
            Message::ServerClosed { reason, .. } => {
                tracing::info!("[SystemHandler] ServerClosed: {}", reason);
                ctx.emit(MobileEvent::ServerClosed { reason });
            }
            Message::Error { message, code, .. } => {
                let msg = if !message.is_empty() { message } else { code };
                tracing::warn!("[SystemHandler] Error: {}", msg);
                ctx.emit(MobileEvent::Error { message: msg });
            }
            Message::Ack { request_id, .. } => {
                tracing::debug!("[SystemHandler] Ack: request_id={}", request_id);
                ctx.emit(MobileEvent::Ack { request_id });
            }
            _ => {}
        }
        Ok(None)
    }

    fn name(&self) -> &str {
        "SystemHandler"
    }
}

impl Default for SystemHandler {
    fn default() -> Self {
        Self
    }
}