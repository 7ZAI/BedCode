//! Client Default Message Handler
//!
//! 客户端默认消息处理器，实现 MessageHandler trait
//! 使用编解码器解码消息，再委托给单个 MessageRouter 处理

use crate::connection::client_router::MessageRouter;
use crate::connection::codec::{JsonCodec, MessageCodec};
use crate::connection::MessageHandler;
use crate::handler::PluginEventRouter;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

/// 客户端默认消息处理器
///
/// 处理流程：
/// 1. 插件事件帧优先（`{"type":"event",...}`，票 03）：命中即闭环返回
/// 2. 其余帧使用 codec 解码 WebSocket 消息为 Message
/// 3. 委托给单个 MessageRouter 处理
pub struct ClientDefaultMessageHandler {
    codec: Arc<dyn MessageCodec>,
    router: Option<Arc<dyn MessageRouter>>,
    /// 插件事件帧路由（`session-control` 常驻事件连接专用；其它连接为 None）
    plugin_event: Option<Arc<PluginEventRouter>>,
}

impl ClientDefaultMessageHandler {
    /// 创建默认消息处理器（使用 JsonCodec）
    pub fn new() -> Self {
        Self {
            codec: Arc::new(JsonCodec::new()),
            router: None,
            plugin_event: None,
        }
    }

    /// Builder 风格：设置编解码器
    pub fn with_codec(mut self, codec: Arc<dyn MessageCodec>) -> Self {
        self.codec = codec;
        self
    }

    /// Builder 风格：挂插件事件帧路由（常驻事件通道连接）
    pub fn with_plugin_event(mut self, plugin_event: Arc<PluginEventRouter>) -> Self {
        self.plugin_event = Some(plugin_event);
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
        tracing::debug!("[ClientDefaultMessageHandler] handle() called");

        // 插件事件帧优先（票 03）：`{"type":"event",...}` 不是 `Message` 信封，
        // 交给 codec 必然解码失败刷 warn——命中即在此闭环（已路由或按契约丢弃）
        if let (WsMsg::Text(text), Some(plugin_event)) = (&raw_message, &self.plugin_event) {
            if plugin_event.route_text(text) {
                return;
            }
        }

        // 使用 codec 解码消息
        let message = match self.codec.decode(raw_message) {
            Ok(Some(msg)) => {
                tracing::debug!(
                    "[ClientDefaultMessageHandler] Decoded message: type={:?}",
                    msg.message_type()
                );
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
            tracing::debug!("[ClientDefaultMessageHandler] Calling router.route()");
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
