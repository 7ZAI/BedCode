//! Heartbeat Handler - 心跳消息处理器
//!
//! 处理 `Message::Heartbeat` 消息，直接返回心跳响应

use crate::desktop::server::message::Message as BusinessMessage;
use crate::desktop::server::router::context::RouteContext;
use crate::desktop::server::router::handler::RouteHandler;
use crate::Result;
use async_trait::async_trait;

pub struct HeartbeatHandler;

impl HeartbeatHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RouteHandler for HeartbeatHandler {
    async fn handle(
        &self,
        message: BusinessMessage,
        _ctx: &RouteContext,
    ) -> Result<Option<BusinessMessage>> {
        match message {
            BusinessMessage::Heartbeat { .. } => {
                // 心跳消息直接响应
                Ok(Some(BusinessMessage::heartbeat()))
            }
            _ => Ok(None),
        }
    }
}