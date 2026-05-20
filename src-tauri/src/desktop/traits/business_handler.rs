//! Business Handler Trait
//!
//! 业务处理器 trait 定义

use crate::desktop::websocket_manager::{HandlerResult, WsMessage};
use crate::shared::websocket::client::WsClient;
use crate::shared::websocket::types::ClientInfo;

pub trait BusinessHandler: Send + Sync {
    fn handle(&self, client: &WsClient, msg: WsMessage) -> HandlerResult;
}