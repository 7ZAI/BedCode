//! Authentication Interceptor
//!
//! 认证拦截器 trait 定义和实现

use crate::shared::auth::JwtService;
use crate::shared::model::message::Message;
use crate::shared::websocket::server::connection_manager::ConnectionManager;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// 认证拦截器 trait
/// 业务层可以实现此 trait 来定义自己的认证逻辑
pub trait AuthInterceptor: Send + Sync {
    /// 认证检查
    ///
    /// # Arguments
    /// * `message` - 接收到的消息（已解码为 Message）
    /// * `addr` - 客户端地址
    ///
    /// # Returns
    /// * `Ok(Some(client_id))` - 认证成功，返回客户端ID
    /// * `Ok(None)` - 认证失败/未认证，返回错误响应由框架处理
    /// * `Err(e)` - 认证过程中发生错误
    fn authenticate(&self, message: &Message, addr: SocketAddr) -> Result<Option<String>, String>;

    /// 拦截器名称
    fn name(&self) -> &str;
}

/// 认证拦截器实现
///
/// 认证逻辑：
/// 1. 认证消息（Auth）直接放行，由业务处理器完成完整的认证流程
/// 2. 非认证消息检查 ConnectionManager 中是否已认证
/// 3. 已认证客户端放行，未认证则拒绝
pub struct AuthInterceptorImpl {
    /// JWT 认证服务
    jwt_service: JwtService,
    /// 连接管理器（用于查询已认证客户端）
    connection_manager: Arc<ConnectionManager>,
}

impl AuthInterceptorImpl {
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
            rt.block_on(async {
                self.check_authenticated_async(addr).await
            })
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

        rt.block_on(async {
            self.check_authenticated_async(addr).await
        })
    }

    /// 检查消息是否为认证消息
    fn is_auth_message(message: &Message) -> bool {
        matches!(message, Message::Auth { .. })
    }
}

impl AuthInterceptor for AuthInterceptorImpl {
    fn authenticate(&self, message: &Message, addr: SocketAddr) -> Result<Option<String>, String> {
        // 1. 认证消息直接放行，由业务处理器完成认证流程
        if Self::is_auth_message(message) {
            debug!("Auth message from {}, allowing through for business handler", addr);
            return Ok(None);
        }

        // 2. 非认证消息：检查是否已认证
        match self.check_authenticated_sync(&addr) {
            Some(client_id) => {
                debug!("Client {} authenticated as {}", addr, client_id);
                Ok(Some(client_id))
            }
            None => {
                // 3. 未认证，尝试快速验证 JWT token（降级处理）
                if let Message::Auth { payload, .. } = message {
                    if let Some(token) = &payload.session_token {
                        match self.jwt_service.verify_token_with_expiry(token) {
                            Ok(claims) => {
                                info!("Client {} authenticated via message JWT", addr);
                                return Ok(Some(claims.sub));
                            }
                            Err(e) => {
                                warn!("JWT verification failed for {}: {}", addr, e);
                            }
                        }
                    }
                }

                // 4. 未认证且无有效 token
                debug!("Client {} not authenticated, rejecting message", addr);
                Ok(None)
            }
        }
    }

    fn name(&self) -> &str {
        "AuthInterceptorImpl"
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