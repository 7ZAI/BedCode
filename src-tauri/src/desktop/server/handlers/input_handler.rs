//! Input Handler - 输入消息处理器
//!
//! 处理 `Message::Input` 消息，委托给 input_service

use crate::desktop::server::message::Message as BusinessMessage;
use crate::shared::websocket::server::context::RouteContext;
use crate::desktop::server::router::handler::RouteHandler;
use crate::desktop::server::services::input_service::handle_input;
use crate::desktop::session::SessionManager;
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub struct InputHandler {
    session_manager: Option<Arc<SessionManager>>,
}

impl InputHandler {
    pub fn new(session_manager: Option<Arc<SessionManager>>) -> Self {
        Self { session_manager }
    }
}

#[async_trait]
impl RouteHandler for InputHandler {
    async fn handle(
        &self,
        message: BusinessMessage,
        _ctx: &RouteContext,
    ) -> Result<Option<BusinessMessage>> {
        let (session_id, payload, message_id, timestamp) = match message {
            BusinessMessage::Input {
                message_id,
                expect_response: _,
                session_id,
                timestamp,
                payload,
            } => (session_id, payload, message_id, timestamp),
            _ => return Ok(None),
        };

        handle_input(&session_id, payload, &self.session_manager).await
    }
}