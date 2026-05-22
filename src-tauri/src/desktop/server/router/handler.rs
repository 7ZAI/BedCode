//! Route Handler - 路由处理器 trait
//!
//! 定义业务处理器的标准接口

use crate::desktop::server::message::Message;
use crate::desktop::server::router::context::RouteContext;
use crate::Result;
use async_trait::async_trait;

/// 业务消息处理器 trait
///
/// 每个处理器负责处理一种或多种消息类型。
/// 返回 Some(Message) 表示需要向发起方发送响应；
/// 返回 None 表示无需响应（如 fire-and-forget）。
#[async_trait]
pub trait RouteHandler: Send + Sync {
    /// 处理消息
    async fn handle(
        &self,
        message: Message,
        ctx: &RouteContext,
    ) -> Result<Option<Message>>;
}

/// 便捷类型别名：存放到注册表中的处理器引用
pub type BoxedHandler = Arc<dyn RouteHandler>;

use std::sync::Arc;
