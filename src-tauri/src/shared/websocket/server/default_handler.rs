//! Default Message Handler
//!
//! 默认消息处理器，实现 MessageHandler trait
//! 提供认证拦截、消息路由的标准处理流程

use crate::shared::model::message::Message;
use crate::shared::websocket::codec::{JsonCodec, MessageCodec};
use crate::shared::websocket::MessageHandler;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

// ==================== Trait Definitions ====================

/// 认证拦截器 trait
///
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

/// 消息路由器 trait
///
/// 业务层可以实现此 trait 来定义自己的消息路由逻辑
pub trait MessageRouter: Send + Sync {
    /// 路由消息到具体处理器
    ///
    /// # Arguments
    /// * `message` - 已解码的业务消息
    /// * `addr` - 客户端地址
    /// * `client_id` - 认证后的客户端ID（若已认证）
    /// * `sender` - 用于发送响应消息的通道
    fn route(
        &self,
        message: &Message,
        addr: SocketAddr,
        client_id: Option<&str>,
        sender: Option<mpsc::Sender<WsMsg>>,
    );

    /// 路由器名称
    fn name(&self) -> &str;
}

// ==================== Default Message Handler ====================

/// 默认消息处理器
///
/// 按照标准的处理流程处理 WebSocket 消息：
/// 1. 通过认证拦截器进行身份验证
/// 2. 将已认证的消息路由到业务处理器
pub struct DefaultMessageHandler {
    codec: Arc<dyn MessageCodec>,
    auth_interceptor: Option<Arc<dyn AuthInterceptor>>,
    router: Option<Arc<dyn MessageRouter>>,
}

impl DefaultMessageHandler {
    /// 创建默认消息处理器（使用 JsonCodec）
    pub fn new(
        auth_interceptor: Option<Arc<dyn AuthInterceptor>>,
        router: Option<Arc<dyn MessageRouter>>,
    ) -> Self {
        Self {
            codec: Arc::new(JsonCodec::new()),
            auth_interceptor,
            router,
        }
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
        sender: Option<mpsc::Sender<WsMsg>>,
    ) {
        // 按消息类型分别处理
        match raw_message {
            WsMsg::Binary(data) => {
                // Binary 类型只记录日志，不做处理
                tracing::debug!(
                    "[DefaultMessageHandler] Binary message received from {}: {} bytes",
                    addr,
                    data.len()
                );
                return;
            }
            WsMsg::Text(text) => {
                // 使用 codec 解码 Text 消息
                let ws_message = match self.codec.decode(WsMsg::Text(text)) {
                    Ok(Some(msg)) => msg,
                    Ok(None) => {
                        return;
                    }
                    Err(e) => {
                        let error_msg = Message::error("DECODE_ERROR", &e.to_string());
                        if let Some(sender) = sender {
                            let _ = sender
                                .try_send(WsMsg::Text(error_msg.to_json().unwrap_or_default()));
                        }
                        return;
                    }
                };

                // 认证检查
                let authenticated_client_id = if let Some(auth) = &self.auth_interceptor {
                    match auth.authenticate(&ws_message, addr) {
                        Ok(Some(id)) => Some(id),
                        Ok(None) => {
                            let error_msg =
                                Message::error("NOT_AUTHENTICATED", "Authentication required");
                            if let Some(sender) = sender {
                                let _ = sender
                                    .try_send(WsMsg::Text(error_msg.to_json().unwrap_or_default()));
                            }
                            return;
                        }
                        Err(e) => {
                            let error_msg = Message::error("AUTH_ERROR", &e);
                            if let Some(sender) = sender {
                                let _ = sender
                                    .try_send(WsMsg::Text(error_msg.to_json().unwrap_or_default()));
                            }
                            return;
                        }
                    }
                } else {
                    client_id.map(|s| s.to_string())
                };

                // 路由到业务处理器
                if let Some(router) = &self.router {
                    router.route(&ws_message, addr, authenticated_client_id.as_deref(), sender);
                }
            }
            _ => {}
        }
    }
}