//! Router Module - Message Router Trait
//!
//! 消息路由器 trait 定义，桌面端和移动端都实现此 trait

use crate::model::message::Message;
use crate::Result;
use async_trait::async_trait;

/// 消息路由器 trait
///
/// 业务层实现此 trait 来定义消息路由逻辑。
/// 桌面端: BusinessRouter 实现此 trait
/// 移动端: ClientBusinessRouter 实现此 trait
#[async_trait]
pub trait MessageRouter: Send + Sync {
    /// 路由消息到具体处理器
    ///
    /// # Returns
    /// - `Ok(Some(Message))` - 返回响应消息
    /// - `Ok(None)` - 无需响应
    /// - `Err(e)` - 处理错误
    async fn route(&self, message: Message) -> Result<Option<Message>>;

    /// 路由器名称
    fn name(&self) -> &str;
}
