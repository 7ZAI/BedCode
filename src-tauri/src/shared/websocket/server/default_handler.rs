//! Default Message Handler
//!
//! 默认消息处理器，实现 MessageHandler trait
//! 提供编解码、认证拦截、消息路由的标准处理流程

use crate::shared::websocket::codec::MessageCodec;
use crate::shared::websocket::message::{WsMessage, MessageHandler};
use crate::shared::websocket::server::auth_interceptor::AuthInterceptor;
use crate::shared::websocket::server::message_router::MessageRouter;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

/// 默认消息处理器
///
/// 按照标准的处理流程处理 WebSocket 消息：
/// 1. 使用 codec 解码原始消息
/// 2. 通过认证拦截器进行身份验证
/// 3. 将已认证的消息路由到业务处理器
pub struct DefaultMessageHandler {
    codec: Arc<dyn MessageCodec>,
    auth_interceptor: Option<Arc<dyn AuthInterceptor>>,
    router: Option<Arc<dyn MessageRouter>>,
}

impl DefaultMessageHandler {
    /// 创建默认消息处理器
    ///
    /// # Arguments
    /// * `codec` - 消息编解码器，必须提供
    /// * `auth_interceptor` - 认证拦截器，可选
    /// * `router` - 消息路由器，可选（但无路由器时 handle 会返回错误）
    pub fn new(
        codec: Arc<dyn MessageCodec>,
        auth_interceptor: Option<Arc<dyn AuthInterceptor>>,
        router: Option<Arc<dyn MessageRouter>>,
    ) -> Self {
        Self {
            codec,
            auth_interceptor,
            router,
        }
    }

    /// Builder 风格：设置编解码器
    pub fn with_codec(mut self, codec: Arc<dyn MessageCodec>) -> Self {
        self.codec = codec;
        self
    }

    /// Builder 风格：设置认证拦截器
    pub fn with_auth_interceptor(mut self, auth: Arc<dyn AuthInterceptor>) -> Self {
        self.auth_interceptor = Some(auth);
        self
    }

    /// Builder 风格：设置消息路由器
    pub fn with_router(mut self, router: Arc<dyn MessageRouter>) -> Self {
        self.router = Some(router);
        self
    }
}

impl MessageHandler for DefaultMessageHandler {
    fn handle(
        &self,
        raw_message: WsMsg,
        addr: SocketAddr,
        client_id: Option<&str>,
    ) -> std::result::Result<Option<WsMessage>, String> {
        // 1. 使用 codec 解码原始消息
        let ws_message = match self.codec.decode(raw_message) {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                // 非 Text/Binary 消息（如 Ping/Pong），由框架处理，不需要业务层响应
                return Ok(None);
            }
            Err(e) => {
                // 解码失败，返回错误响应让客户端知道消息格式有问题
                let error_msg = WsMessage::error("DECODE_ERROR", e.to_string());
                return Ok(Some(error_msg));
            }
        };

        // 2. 如果有认证拦截器，进行认证检查
        // 未认证的客户端必须在业务路由之前被拦截，防止未授权访问
        let authenticated_client_id = if let Some(auth) = &self.auth_interceptor {
            match auth.authenticate(&ws_message, addr) {
                Ok(Some(id)) => Some(id),
                Ok(None) => {
                    // 认证失败：客户端未提供有效凭证，返回未认证错误
                    let error_msg = WsMessage::error(
                        "NOT_AUTHENTICATED",
                        "Authentication required",
                    );
                    return Ok(Some(error_msg));
                }
                Err(e) => {
                    // 认证过程出错（如 token 过期、格式错误等）
                    let error_msg = WsMessage::error("AUTH_ERROR", e);
                    return Ok(Some(error_msg));
                }
            }
        } else {
            // 没有配置认证拦截器，使用外部传入的 client_id
            // 这意味着服务端不要求认证，或认证由其他层处理
            client_id.map(|s| s.to_string())
        };

        // 3. 调用路由器处理消息
        // 路由器负责将消息分发到具体的业务处理器
        match &self.router {
            Some(router) => {
                router.route(&ws_message, addr, authenticated_client_id.as_deref())
            }
            None => {
                // 没有配置路由器，无法处理消息
                Err("Router not configured".to_string())
            }
        }
    }
}
