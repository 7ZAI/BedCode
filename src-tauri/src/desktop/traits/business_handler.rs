//! Business Handler Trait
//!
//! 业务处理器 trait 定义

use crate::shared::websocket::{ClientInfo, HandlerResult, WsClient, WsMessage};

pub trait BusinessHandler: Send + Sync {
    fn handle(&self, client: &WsClient, msg: WsMessage) -> HandlerResult;
}