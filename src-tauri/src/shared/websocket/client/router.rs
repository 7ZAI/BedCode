//! Router Module - Message Routing
//!
//! 职责：接收消息分发，基于消息类型或订阅者模式

use crate::shared::websocket::message::{WsMessage};
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::debug;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

/// 消息路由事件
#[derive(Debug, Clone)]
pub enum RouterEvent {
    /// 消息路由完成
    Routed {
        message_id: Option<String>,
        message_type: WsMsg,
    },
    /// 路由失败
    RouteFailed {
        error: String,
    },
}

/// 消息处理器 trait（用于处理特定类型的消息）
#[async_trait]
pub trait MessageRouter: Send + Sync {
    /// 处理接收到的消息
    async fn handle(&self, message: WsMessage) -> Result<Option<WsMessage>>;

    /// 处理器名称
    fn name(&self) -> &str;
}

/// 默认路由器（不做任何处理，仅转发事件）
#[derive(Debug, Clone, Default)]
pub struct DefaultRouter;

#[async_trait]
impl MessageRouter for DefaultRouter {
    async fn handle(&self, _message: WsMessage) -> Result<Option<WsMessage>> {
        Ok(None)
    }

    fn name(&self) -> &str {
        "DefaultRouter"
    }
}

/// 路由器配置
#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// 是否启用自动Ack
    pub auto_ack: bool,
    /// 是否启用消息解析
    pub parse_messages: bool,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            auto_ack: true,
            parse_messages: true,
        }
    }
}

/// 消息路由器
pub struct MessageRouterManager {
    config: RouterConfig,
    /// 处理器列表（按优先级排序）
    handlers: RwLock<Vec<Arc<dyn MessageRouter>>>,
    /// 事件广播器
    event_tx: broadcast::Sender<RouterEvent>,
    /// 待响应的回调（message_id -> callback）
    pending_callbacks: RwLock<std::collections::HashMap<String, Arc<dyn Fn(WsMessage) + Send + Sync + 'static>>>,
}

impl MessageRouterManager {
    /// 创建新的路由器管理器
    pub fn new(config: RouterConfig) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(1024);
        Arc::new(Self {
            config,
            handlers: RwLock::new(Vec::new()),
            event_tx,
            pending_callbacks: RwLock::new(std::collections::HashMap::new()),
        })
    }

    /// 创建默认配置的路由器
    pub fn with_default_config() -> Arc<Self> {
        Self::new(RouterConfig::default())
    }

    /// 获取配置
    pub fn config(&self) -> &RouterConfig {
        &self.config
    }

    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<RouterEvent> {
        self.event_tx.subscribe()
    }

    /// 添加处理器
    pub async fn add_handler(&self, handler: Arc<dyn MessageRouter>) {
        let mut handlers = self.handlers.write().await;
        handlers.push(handler);
    }

    /// 移除处理器（按名称）
    pub async fn remove_handler(&self, name: &str) {
        let mut handlers = self.handlers.write().await;
        handlers.retain(|h| h.name() != name);
    }

    /// 路由消息
    pub async fn route(&self, message: WsMessage) -> Result<Option<WsMessage>> {
        let message_id = message.message_id().map(|s| s.to_string());
        let message_type = message.message_type();

        debug!("[Router] Routing message: type={:?}, id={:?}", message_type, message_id);

        // 1. 检查是否有待处理的回调（基于 message_id）
        if let Some(ref msg_id) = message_id {
            let callback = {
                let mut callbacks = self.pending_callbacks.write().await;
                callbacks.remove(msg_id)
            };

            if let Some(cb) = callback {
                cb(message.clone());
                return Ok(None);
            }
        }

        // 2. 依次调用处理器
        let handlers = self.handlers.read().await;
        for handler in handlers.iter() {
            match handler.handle(message.clone()).await {
                Ok(Some(response)) => return Ok(Some(response)),
                Ok(None) => continue,
                Err(_) => continue,
            }
        }

        Ok(None)
    }

    /// 注册回调（用于 send_with_callback）
    pub async fn register_callback(
        &self,
        message_id: String,
        callback: Arc<dyn Fn(WsMessage) + Send + Sync + 'static>,
    ) {
        let mut callbacks = self.pending_callbacks.write().await;
        callbacks.insert(message_id, callback);
    }

    /// 移除回调
    pub async fn remove_callback(&self, message_id: &str) {
        let mut callbacks = self.pending_callbacks.write().await;
        callbacks.remove(message_id);
    }
}

impl Default for MessageRouterManager {
    fn default() -> Self {
        Self {
            config: RouterConfig::default(),
            handlers: RwLock::new(Vec::new()),
            event_tx: broadcast::channel(1024).0,
            pending_callbacks: RwLock::new(std::collections::HashMap::new()),
        }
    }
}