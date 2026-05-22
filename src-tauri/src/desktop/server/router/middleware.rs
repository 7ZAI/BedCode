//! Middleware - 中间件系统
//!
//! 提供洋葱模型中间件执行机制：
//!
//! 请求流向： 外层中间件 → 中层中间件 → 内层中间件 → 处理器
//! 响应流向： 处理器     → 内层中间件 → 中层中间件 → 外层中间件
//!
//! 每个中间件可以在 before（调用 next 前）和 after（next 返回后）插入逻辑。

use crate::desktop::server::message::Message;
use crate::desktop::server::router::context::RouteContext;
use crate::desktop::server::router::handler::RouteHandler;
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;

/// 中间件 trait
///
/// 实现者负责决定是否将消息传递给下一层（通过调用 next.run）。
/// 通过拦截 next 的返回值，可以实现 after 逻辑。
#[async_trait]
pub trait BusinessMiddleware: Send + Sync {
    /// 处理消息
    ///
    /// - `message`: 当前消息（中间件可修改或替换）
    /// - `ctx`: 连接上下文
    /// - `next`: 下一层（中间件链剩余部分或最终处理器）
    async fn process(
        &self,
        message: Message,
        ctx: &RouteContext,
        next: Next<'_>,
    ) -> Result<Option<Message>>;
}

/// 下一层执行器
///
/// 内部持有中间件链的剩余部分和最终处理器。
/// 当调用 `run` 时，如果还有中间件，则递归执行；否则调用处理器。
pub struct Next<'a> {
    middlewares: &'a [Arc<dyn BusinessMiddleware>],
    handler: &'a dyn RouteHandler,
}

impl<'a> Next<'a> {
    pub fn new(
        middlewares: &'a [Arc<dyn BusinessMiddleware>],
        handler: &'a dyn RouteHandler,
    ) -> Self {
        Self {
            middlewares,
            handler,
        }
    }

    /// 执行下一层
    ///
    /// 如果还有中间件，移出第一个并调用其 process；
    /// 否则直接调用最终处理器的 handle。
    pub async fn run(self, message: Message, ctx: &RouteContext) -> Result<Option<Message>> {
        if let Some((mid, rest)) = self.middlewares.split_first() {
            mid.process(message, ctx, Next::new(rest, self.handler)).await
        } else {
            self.handler.handle(message, ctx).await
        }
    }
}
