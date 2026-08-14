//! 消息总线
//!
//! 插件间 Topic 消息总线 — 发布/订阅模式通信
//! 通过 MessageDispatcher trait 解耦与 PluginManager 的循环引用
//!
//! 投递模型：`publish()`（同步，WASM host function 调用）只做
//! 「快照订阅列表（短锁）→ 投递任务入队」，实际投递由 `set_dispatcher`
//! 时启动的投递 worker 任务串行完成。这样发布方不会在持锁状态下阻塞
//! 等待订阅者，避免「执行 WASM → 发布 → 投递 → 重入取锁」的死锁环。
//! 代价：全局投递串行，慢插件的 on_bus_message 会推迟后续投递。

use async_trait::async_trait;
use bedcode_plugin_api_mobile::BusMessage;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tokio::sync::Mutex;

// ==================== MessageDispatcher Trait ====================

/// 消息投递器 — MessageBus 通过此 trait 将消息投递给插件
///
/// 由 PluginManager 实现，避免 MessageBus 与 PluginManager 循环引用
#[async_trait]
pub trait MessageDispatcher: Send + Sync + 'static {
    /// 投递消息给 WASM 插件（调用组件契约 events.on-bus-message）
    async fn dispatch_to_wasm(&self, plugin_id: &str, msg: &BusMessage) -> anyhow::Result<()>;
    /// 检查插件是否已激活
    async fn is_activated(&self, plugin_id: &str) -> bool;
}

// ==================== BusMessageHandler Trait ====================

/// 消息处理器 trait — 静态注册插件实现此 trait 接收总线消息
pub trait BusMessageHandler: Send + Sync + 'static {
    fn on_message(&self, msg: &BusMessage) -> anyhow::Result<()>;
}

// ==================== BusSubscriber ====================

/// 订阅者（Arc handler 使订阅列表可廉价快照克隆）
#[derive(Clone)]
pub enum BusSubscriber {
    /// WASM 插件订阅者 — 通过 MessageDispatcher 投递
    Wasm { plugin_id: String },
    /// 静态注册插件订阅者 — 通过 Rust callback 投递
    Static {
        plugin_id: String,
        handler: Arc<dyn BusMessageHandler>,
    },
}

impl BusSubscriber {
    fn plugin_id(&self) -> &str {
        match self {
            BusSubscriber::Wasm { plugin_id } => plugin_id,
            BusSubscriber::Static { plugin_id, .. } => plugin_id,
        }
    }
}

// ==================== DeliveryJob ====================

/// 投递任务：发布时刻的订阅者快照 + 消息体
struct DeliveryJob {
    /// 已过滤发送者自己
    subs: Vec<BusSubscriber>,
    msg: BusMessage,
}

// ==================== MessageBus ====================

/// 消息总线（宿主侧，全局共享）
pub struct MessageBus {
    /// topic → 订阅者列表
    ///
    /// std RwLock：快照临界区为纯同步短操作（get/clone/iter），
    /// 使同步上下文（WASM host fn publish）可直接取锁，无需 block_on
    subscribers: Arc<RwLock<HashMap<String, Vec<BusSubscriber>>>>,
    /// 消息投递器（由 PluginManager 注入，两阶段初始化）
    dispatcher: Arc<RwLock<Option<Arc<dyn MessageDispatcher>>>>,
    /// 投递任务发送端（publish 入队）
    delivery_tx: mpsc::UnboundedSender<DeliveryJob>,
    /// 投递 worker 接收端（set_dispatcher 时 take 一次并启动 worker）
    delivery_rx: Mutex<Option<mpsc::UnboundedReceiver<DeliveryJob>>>,
}

impl MessageBus {
    /// 创建消息总线（dispatcher 延迟注入，投递 worker 随 set_dispatcher 启动）
    pub fn new() -> Self {
        let (delivery_tx, delivery_rx) = mpsc::unbounded_channel();
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            dispatcher: Arc::new(RwLock::new(None)),
            delivery_tx,
            delivery_rx: Mutex::new(Some(delivery_rx)),
        }
    }

    /// 注入消息投递器（PluginManager 构造完成后调用一次）
    ///
    /// 同时启动投递 worker 任务（必须在 Tokio 运行时上下文中调用）。
    /// worker 启动前发布的消息缓存在队列中，启动后立即投递。
    pub async fn set_dispatcher(&self, dispatcher: Arc<dyn MessageDispatcher>) {
        *self.dispatcher.write().unwrap() = Some(dispatcher);

        // 启动投递 worker（仅一次，take 后为 None）
        if let Some(mut rx) = self.delivery_rx.lock().await.take() {
            let dispatcher_slot = self.dispatcher.clone();
            tokio::spawn(async move {
                while let Some(job) = rx.recv().await {
                    let disp = dispatcher_slot.read().unwrap().clone();
                    let Some(disp) = disp else {
                        tracing::warn!("MessageBus: dispatcher not set, message dropped");
                        continue;
                    };
                    deliver_job(disp.as_ref(), job).await;
                }
            });
        }
    }

    /// 发布消息
    ///
    /// 快照订阅者（std 锁短临界区）→ 过滤发送者 → 入队由 worker 异步投递。
    /// 同步调用方（WASM host function）不会被订阅者的执行阻塞。
    pub fn publish(&self, topic: &str, sender: &str, payload: serde_json::Value) {
        // 快照该 topic 的订阅者并过滤发送者自己；守卫快照完成即 drop
        let subs: Vec<BusSubscriber> = {
            let map = self.subscribers.read().unwrap();
            map.get(topic)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|s| s.plugin_id() != sender)
                .collect()
        };

        if subs.is_empty() {
            tracing::debug!("MessageBus: no subscribers for topic '{}', message dropped", topic);
            return;
        }

        let msg = BusMessage {
            topic: topic.to_string(),
            sender: sender.to_string(),
            payload,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };

        if let Err(e) = self.delivery_tx.send(DeliveryJob { subs, msg }) {
            tracing::error!(
                "MessageBus: delivery worker unavailable, topic '{}' message dropped: {}",
                topic, e
            );
        }
    }

    /// 订阅 topic（WASM 插件）
    pub async fn subscribe_wasm(&self, plugin_id: &str, topic: &str) {
        let mut subscribers = self.subscribers.write().unwrap();
        let subs = subscribers.entry(topic.to_string()).or_default();
        if subs.iter().any(|s| matches!(s, BusSubscriber::Wasm { plugin_id: pid } if pid == plugin_id)) {
            tracing::debug!("MessageBus: plugin '{}' already subscribed to '{}'", plugin_id, topic);
            return;
        }
        subs.push(BusSubscriber::Wasm { plugin_id: plugin_id.to_string() });
        tracing::info!("MessageBus: plugin '{}' subscribed to '{}'", plugin_id, topic);
    }

    /// 订阅 topic（静态注册插件）
    pub async fn subscribe_static(
        &self,
        plugin_id: &str,
        topic: &str,
        handler: Arc<dyn BusMessageHandler>,
    ) {
        let mut subscribers = self.subscribers.write().unwrap();
        let subs = subscribers.entry(topic.to_string()).or_default();
        subs.push(BusSubscriber::Static {
            plugin_id: plugin_id.to_string(),
            handler,
        });
        tracing::info!("MessageBus: static plugin '{}' subscribed to '{}'", plugin_id, topic);
    }

    /// 取消插件对指定 topic 的订阅
    pub async fn unsubscribe(&self, plugin_id: &str, topic: &str) {
        let mut subscribers = self.subscribers.write().unwrap();
        if let Some(subs) = subscribers.get_mut(topic) {
            let before = subs.len();
            subs.retain(|s| s.plugin_id() != plugin_id);
            if subs.len() < before {
                tracing::info!("MessageBus: plugin '{}' unsubscribed from '{}'", plugin_id, topic);
            }
        }
    }

    /// 移除插件的所有订阅（停用时调用）
    ///
    /// 已入队但尚未投递的消息中若包含该插件，投递时按快照投递；
    /// WASM 实例已移除时 dispatcher 会丢弃并告警。
    pub async fn remove_all_subscriptions(&self, plugin_id: &str) {
        let mut subscribers = self.subscribers.write().unwrap();
        for (topic, subs) in subscribers.iter_mut() {
            let before = subs.len();
            subs.retain(|s| s.plugin_id() != plugin_id);
            if subs.len() < before {
                tracing::debug!("MessageBus: removed plugin '{}' from topic '{}'", plugin_id, topic);
            }
        }
        subscribers.retain(|_, subs| !subs.is_empty());
    }
}

/// 投递单个任务（投递 worker 任务内执行）
async fn deliver_job(disp: &dyn MessageDispatcher, job: DeliveryJob) {
    let mut delivered = 0;
    for sub in job.subs.iter() {
        match sub {
            BusSubscriber::Wasm { plugin_id } => {
                if !disp.is_activated(plugin_id).await {
                    tracing::warn!("MessageBus: subscriber '{}' not activated, skipping", plugin_id);
                    continue;
                }
                if let Err(e) = disp.dispatch_to_wasm(plugin_id, &job.msg).await {
                    tracing::error!("MessageBus: dispatch to WASM plugin '{}' failed: {}", plugin_id, e);
                } else {
                    delivered += 1;
                }
            }
            BusSubscriber::Static { plugin_id, handler } => {
                if let Err(e) = handler.on_message(&job.msg) {
                    tracing::error!("MessageBus: handler for static plugin '{}' failed: {}", plugin_id, e);
                } else {
                    delivered += 1;
                }
            }
        }
    }

    tracing::debug!(
        "MessageBus: published topic='{}' sender='{}' delivered={}/{}",
        job.msg.topic, job.msg.sender, delivered, job.subs.len()
    );
}
