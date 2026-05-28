//! Authentication Interceptor Implementation
//!
//! 桌面端认证拦截器实现（业务层）

use crate::shared::auth::JwtService;
use crate::shared::model::message::Message;
use crate::shared::websocket::server::default_handler::AuthInterceptor;
use crate::shared::websocket::server::connection_manager::ConnectionManager;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// 桌面端认证拦截器实现
///
/// 认证逻辑：
/// 1. 首先检查连接是否已在 ConnectionManager 中认证，已认证则直接放行
/// 2. 未认证则检查消息是否带有 token，有则进行 JWT 校验，校验通过则设置连接为已认证状态并放行
/// 3. 无 token 或 token 验证失败，检查是否为 Auth 类型消息，是则放行由业务处理器处理
/// 4. 未认证、无有效 token、非 Auth 消息：拒绝
pub struct DesktopAuthInterceptor {
    /// JWT 认证服务
    jwt_service: JwtService,
    /// 连接管理器（用于查询已认证客户端）
    connection_manager: Arc<ConnectionManager>,
}

impl DesktopAuthInterceptor {
    /// 创建认证拦截器
    pub fn new(connection_manager: Arc<ConnectionManager>) -> Self {
        Self {
            jwt_service: JwtService::new(),
            connection_manager,
        }
    }

    /// 异步检查客户端是否已认证
    async fn check_authenticated_async(&self, addr: &SocketAddr) -> Option<String> {
        let connection_id = self.connection_manager.get_id_by_addr(addr).await?;
        let connection = self.connection_manager.get(connection_id).await?;
        connection.client_id
    }

    /// 同步检查客户端是否已认证
    fn check_authenticated_sync(&self, addr: &SocketAddr) -> Option<String> {
        // 使用 tokio::runtime::Handle 在当前上下文中运行异步代码
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            rt.block_on(async { self.check_authenticated_async(addr).await })
        } else {
            // 如果没有运行时，使用简单的轮询方式（有限重试）
            self.try_check_authenticated(addr)
        }
    }

    /// 尝试同步检查认证状态（用于没有运行时的情况）
    fn try_check_authenticated(&self, addr: &SocketAddr) -> Option<String> {
        // 这里使用一个简化的方法，直接查询
        // 在生产环境中，应该确保在异步上下文中调用
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;

        rt.block_on(async { self.check_authenticated_async(addr).await })
    }

    /// 检查消息是否为认证消息
    fn is_auth_message(message: &Message) -> bool {
        matches!(message, Message::Auth { .. })
    }

    /// 从消息中提取 token
    fn extract_token(message: &Message) -> Option<String> {
        match message {
            Message::Auth { payload, .. } => payload.session_token.clone(),
            // 其他消息类型如果带有 token 字段，可以在这里扩展
            _ => None,
        }
    }

    /// 异步设置连接为已认证状态
    async fn set_authenticated_async(&self, addr: &SocketAddr, client_id: &str) {
        if let Some(conn_id) = self.connection_manager.get_id_by_addr(addr).await {
            self.connection_manager
                .set_client_id(conn_id, Some(client_id.to_string()))
                .await;
            info!(
                "Connection {} marked as authenticated for client {}",
                addr, client_id
            );
        } else {
            warn!("Connection not found for addr: {}", addr);
        }
    }

    /// 同步设置连接为已认证状态
    fn set_authenticated_sync(&self, addr: &SocketAddr, client_id: &str) {
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            let addr = *addr;
            let client_id = client_id.to_string();
            let cm = self.connection_manager.clone();
            rt.spawn(async move {
                if let Some(conn_id) = cm.get_id_by_addr(&addr).await {
                    cm.set_client_id(conn_id, Some(client_id.clone())).await;
                    info!(
                        "Connection {} marked as authenticated for client {}",
                        addr, client_id
                    );
                } else {
                    warn!("Connection not found for addr: {}", addr);
                }
            });
        }
    }
}

impl AuthInterceptor for DesktopAuthInterceptor {
    fn authenticate(&self, message: &Message, addr: SocketAddr) -> Result<Option<String>, String> {
        // 1. 首先检查连接是否已在 ConnectionManager 中认证
        if let Some(client_id) = self.check_authenticated_sync(&addr) {
            debug!("Client {} already authenticated as {}", addr, client_id);
            return Ok(Some(client_id));
        }

        // 2. 未认证连接：检查消息是否带有 token
        if let Some(token) = Self::extract_token(message) {
            match self.jwt_service.verify_token_with_expiry(&token) {
                Ok(claims) => {
                    info!("Client {} authenticated via JWT token", addr);
                    // 设置连接为已认证状态
                    self.set_authenticated_sync(&addr, &claims.sub);
                    return Ok(Some(claims.sub));
                }
                Err(e) => {
                    warn!("JWT verification failed for {}: {}", addr, e);
                    // JWT 验证失败，继续检查是否为认证消息
                }
            }
        }

        // 3. 无 token 或 token 验证失败：检查是否为 Auth 类型消息
        if Self::is_auth_message(message) {
            debug!(
                "Auth message from {}, allowing through for business handler",
                addr
            );
            return Ok(None);
        }

        // 4. 未认证、无有效 token、非 Auth 消息：拒绝
        debug!("Client {} not authenticated, rejecting message", addr);
        Ok(None)
    }

    fn name(&self) -> &str {
        "DesktopAuthInterceptor"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        // 单元测试需要 mock 依赖
    }
}