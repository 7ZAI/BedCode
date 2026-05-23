//! Authentication Interceptor
//!
//! 认证拦截器 trait 定义和实现

use crate::shared::websocket::message::WsMessage;
use std::net::SocketAddr;

/// 认证拦截器 trait
/// 业务层可以实现此 trait 来定义自己的认证逻辑
pub trait AuthInterceptor: Send + Sync {
    /// 认证检查
    ///
    /// # Arguments
    /// * `message` - 接收到的消息（已解码为 WsMessage）
    /// * `addr` - 客户端地址
    ///
    /// # Returns
    /// * `Ok(Some(client_id))` - 认证成功，返回客户端ID
    /// * `Ok(None)` - 认证失败/未认证，返回错误响应由框架处理
    /// * `Err(e)` - 认证过程中发生错误
    fn authenticate(&self, message: &WsMessage, addr: SocketAddr) -> Result<Option<String>, String>;

    /// 拦截器名称
    fn name(&self) -> &str;
}
