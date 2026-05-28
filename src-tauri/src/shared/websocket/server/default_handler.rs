//! Default Message Handler
//!
//! 默认消息处理器，实现 MessageHandler trait
//! 提供认证拦截、消息路由的标准处理流程

use crate::shared::model::message::Message;
use crate::shared::websocket::codec::{JsonCodec, MessageCodec};
use crate::shared::websocket::MessageHandler;
use crate::shared::websocket::server::auth_interceptor::AuthInterceptor;
use crate::shared::websocket::server::message_router::MessageRouter;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

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
                tracing::debug!("[DefaultMessageHandler] Binary message received from {}: {} bytes", addr, data.len());
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
                            let _ = sender.try_send(WsMsg::Text(error_msg.to_json().unwrap_or_default()));
                        }
                        return;
                    }
                };

                // 认证检查
                let authenticated_client_id = if let Some(auth) = &self.auth_interceptor {
                    match auth.authenticate(&ws_message, addr) {
                        Ok(Some(id)) => Some(id),
                        Ok(None) => {
                            let error_msg = Message::error("NOT_AUTHENTICATED", "Authentication required");
                            if let Some(sender) = sender {
                                let _ = sender.try_send(WsMsg::Text(error_msg.to_json().unwrap_or_default()));
                            }
                            return;
                        }
                        Err(e) => {
                            let error_msg = Message::error("AUTH_ERROR", &e);
                            if let Some(sender) = sender {
                                let _ = sender.try_send(WsMsg::Text(error_msg.to_json().unwrap_or_default()));
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