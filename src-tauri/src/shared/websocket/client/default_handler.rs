//! Client Default Message Handler
//!
//! 客户端默认消息处理器，实现 MessageHandler trait
//! 解码消息并委托给 MessageRouter 处理

use crate::shared::model::message::Message;
use crate::shared::websocket::client::router::MessageRouter;
use crate::shared::websocket::MessageHandler;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

/// 客户端默认消息处理器
///
/// 处理流程：
/// 1. 解码 JSON 为 Message
/// 2. 委托给 MessageRouter 处理
pub struct ClientDefaultMessageHandler {
    router: Option<Arc<dyn MessageRouter>>,
}

impl ClientDefaultMessageHandler {
    pub fn new() -> Self {
        Self { router: None }
    }

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
        // 只处理 Text 类型
        let text = match raw_message {
            WsMsg::Text(text) => text,
            WsMsg::Binary(data) => {
                tracing::debug!("[ClientDefaultMessageHandler] Binary received: {} bytes", data.len());
                return;
            }
            _ => return,
        };

        // 解析为 Message
        let message = match Message::from_json(&text) {
            Ok(msg) => msg,
            Err(e) => {
                tracing::warn!("[ClientDefaultMessageHandler] Failed to parse message: {}", e);
                return;
            }
        };

        // 委托给 router 处理
        if let Some(router) = &self.router {
            let router = router.clone();
            tokio::spawn(async move {
                if let Err(e) = router.handle(message).await {
                    tracing::error!("[ClientDefaultMessageHandler] Router error: {}", e);
                }
            });
        }
    }
}
