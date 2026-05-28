//! Message Router
//!
//! 消息路由器 trait 定义和实现

use crate::shared::model::message::Message;
use std::net::SocketAddr;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

/// 消息路由器 trait
/// 业务层可以实现此 trait 来定义自己的消息路由逻辑
pub trait MessageRouter: Send + Sync {
    /// 路由消息到具体处理器
    ///
    /// # Arguments
    /// * `message` - 已解码的业务消息
    /// * `addr` - 客户端地址
    /// * `client_id` - 认证后的客户端ID（若已认证）
    /// * `sender` - 用于发送响应消息的通道
    fn route(&self, message: &Message, addr: SocketAddr, client_id: Option<&str>, sender: Option<mpsc::Sender<WsMsg>>);

    /// 路由器名称
    fn name(&self) -> &str;
}
