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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::Mutex;

/// 消息载荷格式（v9）：订阅方声明偏好，格式不匹配的投递被拒绝（不进队列）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadFormat {
    /// JSON 文本（现状，默认；`publish` 载荷）
    Json,
    /// 二进制字节列（零 JSON 编解码，可传非 UTF-8 与大载荷；`publish-binary` 载荷）
    Binary,
}

/// 投递队列容量（v9 背压）：worker 消费慢时队列满则丢弃 + 计数（内部计数，
/// 移动端无 core-monitor；桌面端同语义计数进监控模块）
const BUS_QUEUE_CAPACITY: usize = 64;

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
    Wasm {
        plugin_id: String,
        /// 订阅方声明的载荷格式偏好（`subscribe` = Json / `subscribe-binary` = Binary）
        format: PayloadFormat,
    },
    /// 静态注册插件订阅者 — 通过 Rust callback 投递
    Static {
        plugin_id: String,
        handler: Arc<dyn BusMessageHandler>,
        format: PayloadFormat,
    },
}

impl BusSubscriber {
    fn plugin_id(&self) -> &str {
        match self {
            BusSubscriber::Wasm { plugin_id, .. } => plugin_id,
            BusSubscriber::Static { plugin_id, .. } => plugin_id,
        }
    }

    /// 订阅方声明的载荷格式偏好
    fn format(&self) -> PayloadFormat {
        match self {
            BusSubscriber::Wasm { format, .. } => *format,
            BusSubscriber::Static { format, .. } => *format,
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
    /// 投递任务发送端（publish 入队；有界，满则丢弃 + 计数）
    delivery_tx: mpsc::Sender<DeliveryJob>,
    /// 投递 worker 接收端（set_dispatcher 时 take 一次并启动 worker）
    delivery_rx: Mutex<Option<mpsc::Receiver<DeliveryJob>>>,
    /// 队列满丢弃计数（v9 背压；桌面端同语义进 core-monitor）
    dropped_total: AtomicU64,
    /// 格式不匹配拒绝计数（v9）
    format_rejected_total: AtomicU64,
}

impl MessageBus {
    /// 创建消息总线（dispatcher 延迟注入，投递 worker 随 set_dispatcher 启动）
    pub fn new() -> Self {
        let (delivery_tx, delivery_rx) = mpsc::channel(BUS_QUEUE_CAPACITY);
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            dispatcher: Arc::new(RwLock::new(None)),
            delivery_tx,
            delivery_rx: Mutex::new(Some(delivery_rx)),
            dropped_total: AtomicU64::new(0),
            format_rejected_total: AtomicU64::new(0),
        }
    }

    /// 队列满丢弃计数（v9 背压观测；测试/诊断用）
    pub fn dropped_total(&self) -> u64 {
        self.dropped_total.load(Ordering::Relaxed)
    }

    /// 格式不匹配拒绝计数（v9）
    pub fn format_rejected_total(&self) -> u64 {
        self.format_rejected_total.load(Ordering::Relaxed)
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

    /// 发布 JSON 消息
    ///
    /// 快照订阅者（std 锁短临界区）→ 过滤发送者自身 + 格式偏好不匹配者
    /// （拒绝 + warn + 计数）→ 有界队列入队，由 worker 异步投递。
    /// 同步调用方（WASM host function）不会被订阅者的执行阻塞；
    /// 队列满时新消息丢弃（背压保护）+ 计数
    pub fn publish(&self, topic: &str, sender: &str, payload: serde_json::Value) {
        self.publish_inner(topic, sender, PayloadFormat::Json, payload, None);
    }

    /// 发布二进制消息（v9）：字节列原样透传，零 JSON 编解码，
    /// 可传非 UTF-8 与大载荷（MB 级）。仅二进制格式偏好的订阅者接收
    pub fn publish_binary(&self, topic: &str, sender: &str, payload: Vec<u8>) {
        self.publish_inner(
            topic,
            sender,
            PayloadFormat::Binary,
            serde_json::Value::Null,
            Some(payload),
        );
    }

    /// 发布公共路径：快照订阅者 → 过滤发送者/格式 → 有界队列入队
    fn publish_inner(
        &self,
        topic: &str,
        sender: &str,
        format: PayloadFormat,
        payload: serde_json::Value,
        payload_binary: Option<Vec<u8>>,
    ) {
        // 快照该 topic 的订阅者并过滤发送者自己与格式偏好不匹配者；
        // 守卫快照完成即 drop（格式不匹配拒绝发生在发布侧，不进队列）
        let subs: Vec<BusSubscriber> = {
            let map = self.subscribers.read().unwrap();
            map.get(topic)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|s| {
                    if s.plugin_id() == sender {
                        return false;
                    }
                    if s.format() != format {
                        self.format_rejected_total.fetch_add(1, Ordering::Relaxed);
                        tracing::warn!(
                            plugin_id = %s.plugin_id(),
                            topic = %topic,
                            format = ?format,
                            "MessageBus: subscriber format mismatch, message rejected"
                        );
                        return false;
                    }
                    true
                })
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
            payload_binary,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };

        // 有界队列（v9 背压）：worker 消费慢时队列满则丢弃 + 计数 + warn
        match self.delivery_tx.try_send(DeliveryJob { subs, msg }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    topic = %topic,
                    "MessageBus: delivery queue full, message dropped (backpressure)"
                );
            }
            Err(TrySendError::Closed(_)) => {
                tracing::error!(
                    "MessageBus: delivery worker unavailable, topic '{}' message dropped",
                    topic
                );
            }
        }
    }

    /// 订阅 topic（WASM 插件，JSON 格式偏好，默认路径零回归）
    pub async fn subscribe_wasm(&self, plugin_id: &str, topic: &str) {
        let mut subscribers = self.subscribers.write().unwrap();
        let subs = subscribers.entry(topic.to_string()).or_default();
        if subs
            .iter()
            .any(|s| matches!(s, BusSubscriber::Wasm { plugin_id: pid, .. } if pid == plugin_id))
        {
            tracing::debug!("MessageBus: plugin '{}' already subscribed to '{}'", plugin_id, topic);
            return;
        }
        subs.push(BusSubscriber::Wasm {
            plugin_id: plugin_id.to_string(),
            format: PayloadFormat::Json,
        });
        tracing::info!("MessageBus: plugin '{}' subscribed to '{}'", plugin_id, topic);
    }

    /// 以二进制格式偏好订阅（v9）：只接收 publish-binary 投递，
    /// JSON 消息对其按格式不匹配拒绝
    pub async fn subscribe_wasm_binary(&self, plugin_id: &str, topic: &str) {
        let mut subscribers = self.subscribers.write().unwrap();
        let subs = subscribers.entry(topic.to_string()).or_default();
        if subs
            .iter()
            .any(|s| matches!(s, BusSubscriber::Wasm { plugin_id: pid, .. } if pid == plugin_id))
        {
            tracing::debug!("MessageBus: plugin '{}' already subscribed to '{}'", plugin_id, topic);
            return;
        }
        subs.push(BusSubscriber::Wasm {
            plugin_id: plugin_id.to_string(),
            format: PayloadFormat::Binary,
        });
        tracing::info!("MessageBus: plugin '{}' subscribed binary to '{}'", plugin_id, topic);
    }

    /// 订阅 topic（静态注册插件，JSON 格式偏好）
    pub async fn subscribe_static(&self, plugin_id: &str, topic: &str, handler: Arc<dyn BusMessageHandler>) {
        let mut subscribers = self.subscribers.write().unwrap();
        let subs = subscribers.entry(topic.to_string()).or_default();
        subs.push(BusSubscriber::Static {
            plugin_id: plugin_id.to_string(),
            handler,
            format: PayloadFormat::Json,
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
            BusSubscriber::Wasm { plugin_id, .. } => {
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
            BusSubscriber::Static { plugin_id, handler, .. } => {
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
        job.msg.topic,
        job.msg.sender,
        delivered,
        job.subs.len()
    );
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// 测试投递器：把消息转发到 mpsc 通道供断言（is_activated 恒真）
    struct ChannelDispatcher(mpsc::UnboundedSender<BusMessage>);

    #[async_trait]
    impl MessageDispatcher for ChannelDispatcher {
        async fn dispatch_to_wasm(&self, _plugin_id: &str, msg: &BusMessage) -> anyhow::Result<()> {
            let _ = self.0.send(msg.clone());
            Ok(())
        }

        async fn is_activated(&self, _plugin_id: &str) -> bool {
            true
        }
    }

    /// 慢投递器：每条投递 sleep，制造队列积压（背压丢弃测试用）
    struct SlowDispatcher(mpsc::UnboundedSender<BusMessage>, Duration);

    #[async_trait]
    impl MessageDispatcher for SlowDispatcher {
        async fn dispatch_to_wasm(&self, _plugin_id: &str, msg: &BusMessage) -> anyhow::Result<()> {
            tokio::time::sleep(self.1).await;
            let _ = self.0.send(msg.clone());
            Ok(())
        }

        async fn is_activated(&self, _plugin_id: &str) -> bool {
            true
        }
    }

    /// v9 二进制 roundtrip：非 UTF-8 字节原样透传（零 JSON 编解码）
    #[tokio::test]
    async fn binary_roundtrip_non_utf8() {
        let bus = Arc::new(MessageBus::new());
        let (tx, mut rx) = mpsc::unbounded_channel();
        bus.set_dispatcher(Arc::new(ChannelDispatcher(tx))).await;
        bus.subscribe_wasm_binary("plugin-b", "blob:x").await;

        let bytes: Vec<u8> = vec![0x00, 0xFF, 0xFE, 0x80, 0x41, 0xC3, 0x28];
        bus.publish_binary("blob:x", "plugin-a", bytes.clone());

        let msg = rx.recv().await.expect("二进制订阅者应收到");
        assert_eq!(msg.payload_binary.as_ref(), Some(&bytes), "非 UTF-8 字节必须原样透传");
        assert_eq!(msg.payload, serde_json::Value::Null);
    }

    /// JSON 格式偏好的订阅者收不到二进制消息：拒绝 + 计数（对称：二进制偏好
    /// 订阅者同样收不到 JSON）
    #[tokio::test]
    async fn format_mismatch_rejected_and_counted() {
        let bus = Arc::new(MessageBus::new());
        let (tx, mut rx) = mpsc::unbounded_channel();
        bus.set_dispatcher(Arc::new(ChannelDispatcher(tx))).await;
        bus.subscribe_wasm("plugin-b", "topic:mixed").await;

        bus.publish_binary("topic:mixed", "plugin-a", vec![1, 2, 3]);
        // 拒绝发生在发布侧（不进队列），worker 无投递
        assert!(
            tokio::time::timeout(Duration::from_millis(200), rx.recv())
                .await
                .is_err(),
            "JSON 订阅者不得收到二进制消息"
        );
        assert_eq!(bus.format_rejected_total(), 1);
        assert_eq!(bus.dropped_total(), 0);

        // 对称：二进制偏好订阅者收不到 JSON
        bus.subscribe_wasm_binary("plugin-b", "topic:mixed2").await;
        bus.publish("topic:mixed2", "plugin-a", serde_json::json!({ "v": 1 }));
        assert!(
            tokio::time::timeout(Duration::from_millis(200), rx.recv())
                .await
                .is_err(),
            "二进制订阅者不得收到 JSON 消息"
        );
        assert_eq!(bus.format_rejected_total(), 2);
    }

    /// 队列满丢弃（背压保护）：慢 worker 消费时队列满则丢弃 + 计数
    #[tokio::test]
    async fn queue_full_drops_with_count() {
        let bus = Arc::new(MessageBus::new());
        let (tx, _rx) = mpsc::unbounded_channel();
        bus.set_dispatcher(Arc::new(SlowDispatcher(tx, Duration::from_millis(5))))
            .await;
        bus.subscribe_wasm("plugin-b", "topic:flood").await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        for i in 0..70 {
            bus.publish("topic:flood", "plugin-a", serde_json::json!(i));
        }

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            bus.dropped_total() > 0,
            "慢 worker 场景必须出现队列满丢弃（容量 {} < 70）",
            BUS_QUEUE_CAPACITY
        );
    }
}
