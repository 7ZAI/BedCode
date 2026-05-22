//! Middlewares - 常用中间件实现
//!
//! 提供开箱即用的中间件：
//! - `LoggingMiddleware`: 请求/响应 日志记录
//! - `AuthMiddleware`: 认证拦截

use crate::desktop::server::message::Message;
use crate::desktop::server::router::context::RouteContext;
use crate::desktop::server::router::middleware::{BusinessMiddleware, Next};
use crate::Result;
use async_trait::async_trait;
use std::time::Instant;
use tracing::{debug, info, warn};

// ==================== Logging Middleware ====================

/// 日志中间件
///
/// 记录每个请求的入站消息类型、客户端信息、处理耗时。
pub struct LoggingMiddleware;

#[async_trait]
impl BusinessMiddleware for LoggingMiddleware {
    async fn process(
        &self,
        message: Message,
        ctx: &RouteContext,
        next: Next<'_>,
    ) -> Result<Option<Message>> {
        let msg_type = message_type_str(&message);
        let start = Instant::now();

        debug!(
            "[LoggingMiddleware] >>> {} from client={} (conn_id={})",
            msg_type,
            ctx.client_id,
            ctx.connection_id
        );

        let result = next.run(message, ctx).await;

        let elapsed = start.elapsed();
        match &result {
            Ok(Some(response)) => {
                let resp_type = message_type_str(response);
                info!(
                    "[LoggingMiddleware] {} -> {} in {:?} (client={})",
                    msg_type,
                    resp_type,
                    elapsed,
                    ctx.client_id
                );
            }
            Ok(None) => {
                debug!(
                    "[LoggingMiddleware] {} handled (no response) in {:?} (client={})",
                    msg_type,
                    elapsed,
                    ctx.client_id
                );
            }
            Err(e) => {
                warn!(
                    "[LoggingMiddleware] {} failed in {:?} (client={}): {}",
                    msg_type,
                    elapsed,
                    ctx.client_id,
                    e
                );
            }
        }

        result
    }
}

// ==================== Auth Middleware ====================

/// 认证中间件
///
/// 拦截未认证连接的非认证消息，要求客户端先完成认证。
/// 允许 `Message::Auth` 类型消息通过，其他消息直接返回未认证错误。
pub struct AuthMiddleware;

#[async_trait]
impl BusinessMiddleware for AuthMiddleware {
    async fn process(
        &self,
        message: Message,
        ctx: &RouteContext,
        next: Next<'_>,
    ) -> Result<Option<Message>> {
        // 获取当前连接认证状态
        let is_authenticated = match ctx.connection_manager.get(ctx.connection_id).await {
            Some(conn) => conn.authenticated,
            None => {
                warn!("[AuthMiddleware] Connection {} not found", ctx.connection_id);
                return Ok(None);
            }
        };

        // 已认证直接放行
        if is_authenticated {
            return next.run(message, ctx).await;
        }

        // 未认证，只允许 Auth 消息通过
        match message {
            Message::Auth { .. } => next.run(message, ctx).await,
            _ => {
                warn!(
                    "[AuthMiddleware] Rejected unauthenticated {} from client={}",
                    message_type_str(&message),
                    ctx.client_id
                );
                Ok(Some(Message::error(
                    "NOT_AUTHENTICATED",
                    "Please authenticate first by sending an Auth message with valid JWT token",
                )))
            }
        }
    }
}

// ==================== Helper ====================

/// 获取消息类型的可读字符串（用于日志）
fn message_type_str(msg: &Message) -> String {
    match msg {
        Message::Auth { .. } => "auth".to_string(),
        Message::Control { .. } => "control".to_string(),
        Message::Input { .. } => "input".to_string(),
        Message::Output { .. } => "output".to_string(),
        Message::Heartbeat { .. } => "heartbeat".to_string(),
        Message::Subscribe { .. } => "subscribe".to_string(),
        Message::Unsubscribe { .. } => "unsubscribe".to_string(),
        Message::ServerClosed { .. } => "server_closed".to_string(),
        Message::ClientDisconnected { .. } => "client_disconnected".to_string(),
        Message::SessionEvent { .. } => "session_event".to_string(),
        Message::SubscribeResponse { .. } => "subscribe_response".to_string(),
        Message::UnsubscribeResponse { .. } => "unsubscribe_response".to_string(),
        Message::Error { .. } => "error".to_string(),
    }
}
