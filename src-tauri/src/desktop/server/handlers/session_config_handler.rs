//! Session Config Handler - 会话配置消息处理器
//!
//! 处理 `Message::SessionConfig` 中的会话配置相关操作

use crate::desktop::server::message::{SessionConfigAction, Message as BusinessMessage};
use crate::shared::websocket::server::context::RouteContext;
use crate::desktop::server::router::handler::RouteHandler;
use crate::desktop::server::services::session_config;
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Session Config Handler
pub struct SessionConfigHandler {
    db: Arc<Mutex<crate::shared::db::Database>>,
}

impl SessionConfigHandler {
    pub fn new(db: Arc<Mutex<crate::shared::db::Database>>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl RouteHandler for SessionConfigHandler {
    async fn handle(
        &self,
        message: BusinessMessage,
        _ctx: &RouteContext,
    ) -> Result<Option<BusinessMessage>> {
        match message {
            BusinessMessage::SessionConfig {
                message_id,
                expect_response: _,
                session_id: _,
                timestamp: _,
                payload,
                ..
            } => {
                match payload.action {
                    SessionConfigAction::SessionConfigList { .. } => {
                        session_config::list_session_configs(message_id, &self.db).await
                    }
                    _ => Ok(None),
                }
            }
            _ => Ok(None),
        }
    }
}