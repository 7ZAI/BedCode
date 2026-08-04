//! 消息总线
//!
//! 插件间 Topic 消息总线 — 发布/订阅模式通信
//! 通过 MessageDispatcher trait 解耦与 PluginHost 的循环引用

use bedcode_plugin_api::BusMessage;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ==================== MessageDispatcher Trait ====================

/// 消息投递器 — MessageBus 通过此 trait 将消息投递给插件
///
/// 由 PluginHost 实现，避免 MessageBus 与 PluginHost 循环引用
pub trait MessageDispatcher: Send + Sync + 'static {
    /// 投递消息给 WASM 插件（调用 __bedcode_on_message）
    fn dispatch_to_wasm(&self, plugin_id: &str, msg: &BusMessage) -> anyhow::Result<()>;
    /// 检查插件是否已激活
    fn is_activated(&self, plugin_id: &str) -> bool;
}

// ==================== BusMessageHandler Trait ====================

/// 消息处理器 trait — 静态注册插件实现此 trait 接收总线消息
pub trait BusMessageHandler: Send + Sync + 'static {
    fn on_message(&self, msg: &BusMessage) -> anyhow::Result<()>;
}

// ==================== BusSubscriber ====================

/// 订阅者
pub enum BusSubscriber {
    /// WASM 插件订阅者 — 通过 MessageDispatcher 投递
    Wasm { plugin_id: String },
    /// 静态注册插件订阅者 — 通过 Rust callback 投递
    Static {
        plugin_id: String,
        handler: Box<dyn BusMessageHandler>,
    },
}

// ==================== MessageBus ====================

/// 消息总线（宿主侧，全局共享）
pub struct MessageBus {
    /// topic → 订阅者列表
    subscribers: Arc<RwLock<HashMap<String, Vec<BusSubscriber>>>>,
    /// 消息投递器（由 PluginHost 注入，两阶段初始化）
    dispatcher: Arc<RwLock<Option<Arc<dyn MessageDispatcher>>>>,
}

impl MessageBus {
    /// 创建消息总线（dispatcher 延迟注入）
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            dispatcher: Arc::new(RwLock::new(None)),
        }
    }

    /// 注入消息投递器（PluginHost 构造完成后调用一次）
    pub async fn set_dispatcher(&self, dispatcher: Arc<dyn MessageDispatcher>) {
        let mut d = self.dispatcher.write().await;
        *d = Some(dispatcher);
    }

    /// 发布消息
    ///
    /// 异步投递给所有订阅了该 topic 的插件（不投递给发送者自己）。
    ///
    /// 从同步 host function 上下文调用时，spawn 独立任务投递，
    /// 避免 block_on_async 嵌套（dispatch_to_wasm 内部的同步↔异步桥接
    /// 已在独立任务中，不再与发布方形成嵌套阻塞）
    pub fn publish(&self, topic: &str, sender: &str, payload: serde_json::Value) {
        let dispatcher_arc = self.dispatcher.clone();
        let subscribers_arc = self.subscribers.clone();
        let topic_owned = topic.to_string();
        let sender_owned = sender.to_string();

        // host function 与 PluginHost 均在 runtime 上下文内调用，try_current 理论上不会失败
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            tracing::warn!(topic = %topic, "MessageBus: no runtime context, message dropped");
            return;
        };

        handle.spawn(async move {
            let topic = topic_owned;
            let sender = sender_owned;
            let dispatcher = {
                let guard = dispatcher_arc.read().await;
                guard.clone()
            };
            let Some(dispatcher) = dispatcher else {
                tracing::warn!("MessageBus: dispatcher not set, message dropped");
                return;
            };

            let subscribers = subscribers_arc.read().await;
            let Some(subs) = subscribers.get(&topic) else {
                tracing::debug!("MessageBus: no subscribers for topic '{}', message dropped", topic);
                return;
            };

            let msg = BusMessage {
                topic: topic.to_string(),
                sender: sender.to_string(),
                payload,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            };

            let mut delivered = 0;
            for sub in subs.iter() {
                match sub {
                    BusSubscriber::Wasm { plugin_id } => {
                        if plugin_id == &sender {
                            continue;
                        }
                        if !dispatcher.is_activated(plugin_id) {
                            tracing::warn!(
                                "MessageBus: subscriber '{}' not  activated, skipping",
                                plugin_id
                            );
                            continue;
                        }
                        if let Err(e) = dispatcher.dispatch_to_wasm(plugin_id, &msg) {
                            tracing::error!(
                                "MessageBus: dispatch to WASM plugin '{}' failed: {}",
                                plugin_id,
                                e
                            );
                        } else {
                            delivered += 1;
                        }
                    }
                    BusSubscriber::Static { plugin_id, handler } => {
                        if plugin_id == &sender {
                            continue;
                        }
                        if let Err(e) = handler.on_message(&msg) {
                            tracing::error!(
                                "MessageBus: handler for static plugin '{}' failed: {}",
                                plugin_id,
                                e
                            );
                        } else {
                            delivered += 1;
                        }
                    }
                }
            }

            tracing::debug!(
                "MessageBus: published topic='{}' sender='{}' delivered={}/{}",
                topic,
                sender,
                delivered,
                subs.len()
            );
        });
    }

    /// 订阅 topic（WASM 插件）
    pub async fn subscribe_wasm(&self, plugin_id: &str, topic: &str) {
        let mut subscribers = self.subscribers.write().await;
        let subs = subscribers.entry(topic.to_string()).or_default();
        // 避免重复订阅
        if subs.iter().any(|s| matches!(s, BusSubscriber::Wasm { plugin_id: pid } if pid == plugin_id)) {
            tracing::debug!("MessageBus: plugin '{}' already subscribed to '{}'", plugin_id, topic);
            return;
        }
        subs.push(BusSubscriber::Wasm {
            plugin_id: plugin_id.to_string(),
        });
        tracing::info!("MessageBus: plugin '{}' subscribed to '{}'", plugin_id, topic);
    }

    /// 订阅 topic（静态注册插件）
    pub async fn subscribe_static(
        &self,
        plugin_id: &str,
        topic: &str,
        handler: Box<dyn BusMessageHandler>,
    ) {
        let mut subscribers = self.subscribers.write().await;
        let subs = subscribers.entry(topic.to_string()).or_default();
        subs.push(BusSubscriber::Static {
            plugin_id: plugin_id.to_string(),
            handler,
        });
        tracing::info!("MessageBus: static plugin '{}' subscribed to '{}'", plugin_id, topic);
    }

    /// 取消插件对指定 topic 的订阅
    pub async fn unsubscribe(&self, plugin_id: &str, topic: &str) {
        let mut subscribers = self.subscribers.write().await;
        if let Some(subs) = subscribers.get_mut(topic) {
            let before = subs.len();
            subs.retain(|s| match s {
                BusSubscriber::Wasm { plugin_id: pid } => pid != plugin_id,
                BusSubscriber::Static { plugin_id: pid, .. } => pid != plugin_id,
            });
            if subs.len() < before {
                tracing::info!(
                    "MessageBus: plugin '{}' unsubscribed from '{}'",
                    plugin_id,
                    topic
                );
            }
        }
    }

    /// 移除插件的所有订阅（停用时调用）
    pub async fn remove_all_subscriptions(&self, plugin_id: &str) {
        let mut subscribers = self.subscribers.write().await;
        for (topic, subs) in subscribers.iter_mut() {
            let before = subs.len();
            subs.retain(|s| match s {
                BusSubscriber::Wasm { plugin_id: pid } => pid != plugin_id,
                BusSubscriber::Static { plugin_id: pid, .. } => pid != plugin_id,
            });
            if subs.len() < before {
                tracing::debug!(
                    "MessageBus: removed plugin '{}' from topic '{}'",
                    plugin_id,
                    topic
                );
            }
        }
        // 清理空 topic
        subscribers.retain(|_, subs| !subs.is_empty());
    }
}
