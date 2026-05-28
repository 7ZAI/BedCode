//! Client Default Message Handler
//!
//! 客户端默认消息处理器，实现 MessageHandler trait
//! 处理接收到的消息，包括回调和路由

use crate::shared::model::message::Message;
use crate::shared::websocket::client::router::MessageRouter;
use crate::shared::websocket::client::WsClientEvent;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

/// 客户端默认消息处理器
///
/// 处理流程：
/// 1. 检查是否为响应消息（有 message_id），如果是则通过回调处理
/// 2. 否则通过路由器路由到业务处理器
pub struct ClientDefaultMessageHandler {
    router: Option<Arc<dyn MessageRouter>>,
    event_tx: Option<broadcast::Sender<WsClientEvent>>,
}

impl ClientDefaultMessageHandler {
    pub fn new(router: Option<Arc<dyn MessageRouter>>) -> Self {
        Self { router, event_tx: None }
    }

    pub fn with_router(mut self, router: Arc<dyn MessageRouter>) -> Self {
        self.router = Some(router);
        self
    }

    /// 设置事件发送器（用于 send_and_wait 响应匹配）
    pub fn with_event_tx(mut self, event_tx: broadcast::Sender<WsClientEvent>) -> Self {
        self.event_tx = Some(event_tx);
        self
    }
}

impl crate::shared::websocket::MessageHandler for ClientDefaultMessageHandler {
    fn handle(
        &self,
        raw_message: WsMsg,
        _addr: SocketAddr,
        _client_id: Option<&str>,
        sender: Option<mpsc::Sender<WsMsg>>,
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
        let ws_message = match Message::from_json(&text) {
            Ok(msg) => msg,
            Err(e) => {
                tracing::warn!("[ClientDefaultMessageHandler] Failed to parse message: {}", e);
                return;
            }
        };

        // 获取 message_id 用于响应匹配
        let message_id = ws_message.message_id().map(|s| s.to_string());

        // 发送 TextMessage 事件到 event_tx（用于 send_and_wait 响应匹配）
        if let Some(ref event_tx) = self.event_tx {
            let _ = event_tx.send(WsClientEvent::TextMessage {
                message_id: message_id.clone(),
                content: text.clone(),
            });
        }

        // 检查是否为响应消息（有 message_id）
        if message_id.is_some() {
            // 这是一个响应，交给路由器处理回调
            if let Some(router) = &self.router {
                let router = router.clone();
                let rt = tokio::runtime::Handle::current();
                rt.spawn(async move {
                    router.handle(ws_message).await;
                    // 注意：客户端响应不需要通过 sender 发送回服务器
                    // 回调由业务层处理
                });
            }
        } else {
            // 这是一个推送消息（服务器主动发送），交给路由器处理
            if let Some(router) = &self.router {
                let router = router.clone();
                let sender = sender.clone();
                let rt = tokio::runtime::Handle::current();
                rt.spawn(async move {
                    match router.handle(ws_message).await {
                        Ok(Some(response)) => {
                            // 如果业务层返回响应，发送到服务器
                            if let Some(sender) = sender {
                                let _ = sender.try_send(WsMsg::Text(response.to_json().unwrap_or_default()));
                            }
                        }
                        Ok(None) | Err(_) => {}
                    }
                });
            }
        }
    }
}