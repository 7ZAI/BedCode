//! Message Router
//!
//! 消息路由器 trait 定义和实现

use crate::shared::websocket::message::{HandlerResult, WsMessage};
use std::net::SocketAddr;

/// 消息路由器 trait
/// 业务层可以实现此 trait 来定义自己的消息路由逻辑
pub trait MessageRouter: Send + Sync {
    /// 路由消息到具体处理器
    ///
    /// # Arguments
    /// * `message` - 已解码的业务消息
    /// * `addr` - 客户端地址
    /// * `client_id` - 认证后的客户端ID（若已认证）
    ///
    /// # Returns
    /// * `Ok(Some(response))` - 返回响应消息
    /// * `Ok(None)` - 不返回响应
    /// * `Err(e)` - 处理失败
    fn route(&self, message: &WsMessage, addr: SocketAddr, client_id: Option<&str>) -> HandlerResult;

    /// 路由器名称
    fn name(&self) -> &str;
}
