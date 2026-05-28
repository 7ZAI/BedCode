//! Session Control Handler - 会话控制消息处理器
//!
//! 处理 `Message::SessionControl` 消息，委托给 session_control 服务

use crate::desktop::server::message::Message as BusinessMessage;
use crate::shared::websocket::server::context::RouteContext;
use crate::desktop::server::router::handler::RouteHandler;
use crate::desktop::server::services::session_control::handle_control_message;
use crate::desktop::session::SessionManager;
use crate::desktop::plugin::PluginManager;
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub struct SessionControlHandler {
    session_manager: Option<Arc<SessionManager>>,
    plugin_manager: Option<Arc<PluginManager>>,
}

impl SessionControlHandler {
    pub fn new(
        session_manager: Option<Arc<SessionManager>>,
        plugin_manager: Option<Arc<PluginManager>>,
    ) -> Self {
        Self {
            session_manager,
            plugin_manager,
        }
    }
}

#[async_trait]
impl RouteHandler for SessionControlHandler {
    async fn handle(
        &self,
        message: BusinessMessage,
        ctx: &RouteContext,
    ) -> Result<Option<BusinessMessage>> {
        let (message_id, session_id, timestamp, action) = match message {
            BusinessMessage::SessionControl {
                message_id,
                expect_response: _,
                session_id,
                timestamp,
                payload,
                ..
            } => (message_id, session_id, timestamp, payload.action),
            _ => return Ok(None),
        };

        handle_control_message(
            message_id,
            session_id,
            timestamp,
            action,
            &self.session_manager,
            &self.plugin_manager,
            ctx.addr,
        ).await
    }
}