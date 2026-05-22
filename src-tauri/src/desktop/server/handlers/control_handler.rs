//! Control Handler - 控制消息处理器
//!
//! 处理 `Message::Control` 消息，委托给 session_control 服务

use crate::desktop::server::message::Message as BusinessMessage;
use crate::desktop::server::router::context::RouteContext;
use crate::desktop::server::router::handler::RouteHandler;
use crate::desktop::server::services::session_control::handle_control_message;
use crate::desktop::session::SessionManager;
use crate::desktop::plugin::PluginManager;
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ControlHandler {
    session_manager: Option<Arc<SessionManager>>,
    plugin_manager: Option<Arc<PluginManager>>,
    db: Arc<Mutex<crate::shared::db::Database>>,
}

impl ControlHandler {
    pub fn new(
        session_manager: Option<Arc<SessionManager>>,
        plugin_manager: Option<Arc<PluginManager>>,
        db: Arc<Mutex<crate::shared::db::Database>>,
    ) -> Self {
        Self {
            session_manager,
            plugin_manager,
            db,
        }
    }
}

#[async_trait]
impl RouteHandler for ControlHandler {
    async fn handle(
        &self,
        message: BusinessMessage,
        ctx: &RouteContext,
    ) -> Result<Option<BusinessMessage>> {
        let (message_id, session_id, timestamp, action) = match message {
            BusinessMessage::Control {
                message_id,
                session_id,
                timestamp,
                payload,
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
            &self.db,
            ctx.addr,
        ).await
    }
}