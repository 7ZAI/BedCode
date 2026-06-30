//! Client Default Message Handler
//!
//! 客户端默认消息处理器，实现 MessageHandler trait
//! 使用编解码器解码消息，再委托给单个 MessageRouter 处理

use crate::mobile::websocket_client::router::MessageRouter;
use crate::mobile::websocket_client::codec::{JsonCodec, MessageCodec};
use crate::mobile::websocket_client::MessageHandler;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

/// 客户端默认消息处理器
///
/// 处理流程：
/// 1. 使用 codec 解码 WebSocket 消息为 Message
/// 2. 委托给单个 MessageRouter 处理
pub struct ClientDefaultMessageHandler {
    codec: Arc<dyn MessageCodec>,
    router: Option<Arc<dyn MessageRouter>>,
}

impl ClientDefaultMessageHandler {
    /// 创建默认消息处理器（使用 JsonCodec）
    pub fn new() -> Self {
        Self {
            codec: Arc::new(JsonCodec::new()),
            router: None,
        }
    }

    /// Builder 风格：设置编解码器
    pub fn with_codec(mut self, codec: Arc<dyn MessageCodec>) -> Self {
        self.codec = codec;
        self
    }

    /// Builder 风格：设置消息路由器
    pub fn with_router(mut self, router: Arc<dyn MessageRouter>) -> Self {
        self.router = Some(router);
        self
    }
}

impl Default for ClientDefaultMessageHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageHandler for ClientDefaultMessageHandler {
    fn handle(
        &self,
        raw_message: WsMsg,
        _addr: SocketAddr,
        _client_id: Option<&str>,
        _sender: Option<mpsc::Sender<WsMsg>>,
    ) {
        tracing::info!("[ClientDefaultMessageHandler] handle() called");

        // 使用 codec 解码消息
        let message = match self.codec.decode(raw_message) {
            Ok(Some(msg)) => {
                tracing::info!("[ClientDefaultMessageHandler] Decoded message: type={:?}", msg.message_type());
                msg
            }
            Ok(None) => {
                // 协议层消息（Ping/Pong/Frame）不需要处理
                return;
            }
            Err(e) => {
                tracing::warn!("[ClientDefaultMessageHandler] Codec decode error: {}", e);
                return;
            }
        };

        // 委托给 router 处理
        if let Some(router) = &self.router {
            tracing::info!("[ClientDefaultMessageHandler] Calling router.route()");
            let router = router.clone();
            tokio::spawn(async move {
                if let Err(e) = router.route(message).await {
                    tracing::error!("[ClientDefaultMessageHandler] Router error: {}", e);
                }
            });
        } else {
            tracing::warn!("[ClientDefaultMessageHandler] No router configured, message dropped");
        }
    }
}
