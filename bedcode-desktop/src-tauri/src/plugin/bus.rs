//! 消息总线
//!
//! 插件间 Topic 消息总线 — 发布/订阅模式通信
//! 通过 MessageDispatcher trait 解耦与 PluginHost 的循环引用

use crate::plugin::monitor::{MetricsRegistry, PluginMetrics};
use bedcode_plugin_api::BusMessage;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::RwLock;

/// 消息载荷格式（v11）：订阅方声明偏好，格式不匹配的投递被拒绝
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadFormat {
    /// JSON 文本（现状，默认；`publish` 载荷）
    Json,
    /// 二进制字节列（零 JSON 编解码，可传非 UTF-8 与大载荷；`publish-binary` 载荷）
    Binary,
}

/// 每订阅者有界队列容量（v11 背压）：慢订阅者不阻塞发布方，
/// 队列满时新消息丢弃 + warn（plugin_id/topic 结构化字段）+ 丢弃计数进 core-monitor
const SUBSCRIBER_QUEUE_CAPACITY: usize = 64;

// ==================== MessageDispatcher Trait ====================

/// WS 帧投递请求（ABI v14 `events-ws` 可选导出域）
///
/// 与总线消息平行：WS 帧天然文本/二进制双形态且需保序，经 JSON 总线必然
/// base64 膨胀，故走独立导出回调（spec §2.2 D2）；状态事件仍走总线 topic。
#[derive(Debug, Clone)]
pub enum WsFrameDispatch {
    /// 客户端域：连接句柄 + 帧类型（"text" / "binary"）+ 载荷
    /// （text 为 UTF-8 字节，零 JSON 转义）
    Client {
        handle: String,
        kind: String,
        payload: Vec<u8>,
    },
    /// 服务端域：端点句柄 + 对端 client-id + 帧类型 + 载荷
    EndpointClient {
        endpoint_id: String,
        client_id: String,
        kind: String,
        payload: Vec<u8>,
    },
}

/// 消息投递器 — MessageBus 通过此 trait 将消息投递给插件
///
/// 由 PluginHost 实现，避免 MessageBus 与 PluginHost 循环引用
pub trait MessageDispatcher: Send + Sync + 'static {
    /// 投递消息给 WASM 插件（调用 __bedcode_on_message）
    fn dispatch_to_wasm(&self, plugin_id: &str, msg: &BusMessage) -> anyhow::Result<()>;
    /// 检查插件是否已激活
    fn is_activated(&self, plugin_id: &str) -> bool;

    /// 投递 WS 帧给插件的 `events-ws` 可选导出（ABI v14）
    ///
    /// 返回 `Ok(true)` = 已投递；`Ok(false)` = 插件未导出该接口
    /// （调用方按 spec §2.2 降级：丢弃 + 首次 warn + 计数，宿主不缓存）；
    /// `Err` = 投递失败（trap / 实例不可用）。
    ///
    /// 默认实现返回 `Ok(false)`：非生产投递器（测试替身）未接 WS 通道时
    /// 视为「无导出」，不影响既有实现与用例。
    fn dispatch_ws_frame(&self, _plugin_id: &str, _frame: &WsFrameDispatch) -> anyhow::Result<bool> {
        Ok(false)
    }
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
    Wasm {
        plugin_id: String,
        /// 订阅方声明的载荷格式偏好（`subscribe` = Json / `subscribe-binary` = Binary）
        format: PayloadFormat,
        /// 每订阅者有界队列发送端（发布方 try_send；满则丢弃计数进监控）
        tx: mpsc::Sender<BusMessage>,
        /// 丢弃/格式拒绝计数句柄（core-monitor 插件维度）
        metrics: Arc<PluginMetrics>,
    },
    /// 静态注册插件订阅者 — 通过 Rust callback 投递（handler 在消费任务内独占）
    Static {
        plugin_id: String,
        format: PayloadFormat,
        tx: mpsc::Sender<BusMessage>,
    },
}

impl BusSubscriber {
    /// 订阅者插件 ID（发布侧跳过发送者自身 / 日志）
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

    /// 丢弃/格式拒绝计数句柄（WASM 订阅者进 core-monitor；静态订阅者无句柄 → None）
    fn metrics(&self) -> Option<Arc<PluginMetrics>> {
        match self {
            BusSubscriber::Wasm { metrics, .. } => Some(metrics.clone()),
            BusSubscriber::Static { .. } => None,
        }
    }

    /// 入队（队列满 / 已关闭时返回 TrySendError）
    fn try_send(&self, msg: BusMessage) -> Result<(), TrySendError<BusMessage>> {
        match self {
            BusSubscriber::Wasm { tx, .. } | BusSubscriber::Static { tx, .. } => tx.try_send(msg),
        }
    }
}

// ==================== MessageBus ====================

/// 消息总线（宿主侧，全局共享）
pub struct MessageBus {
    /// topic → 订阅者列表
    subscribers: Arc<RwLock<HashMap<String, Vec<BusSubscriber>>>>,
    /// 消息投递器（由 PluginHost 注入，两阶段初始化）
    dispatcher: Arc<RwLock<Option<Arc<dyn MessageDispatcher>>>>,
    /// core-monitor 注册表（订阅者丢弃/格式拒绝计数；PluginHost 初始化时注入）
    monitor: Arc<RwLock<Option<Arc<MetricsRegistry>>>>,
}

impl MessageBus {
    /// 创建消息总线（dispatcher / monitor 延迟注入）
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            dispatcher: Arc::new(RwLock::new(None)),
            monitor: Arc::new(RwLock::new(None)),
        }
    }

    /// 注入消息投递器（PluginHost 构造完成后调用一次）
    pub async fn set_dispatcher(&self, dispatcher: Arc<dyn MessageDispatcher>) {
        let mut d = self.dispatcher.write().await;
        *d = Some(dispatcher);
    }

    /// 取当前投递器（未注入 → None：两阶段初始化的中间态）
    ///
    /// 供**非总线路径**的定向投递使用：host-websocket（ABI v14）的 `events-ws`
    /// 帧回灌不经 topic 订阅，直接按属主寻址投给插件实例
    pub async fn dispatcher(&self) -> Option<Arc<dyn MessageDispatcher>> {
        self.dispatcher.read().await.clone()
    }

    /// 注入 core-monitor 注册表（订阅者队列满丢弃 / 格式不匹配拒绝计数落点；
    /// 生产路径由 PluginHost::init_message_bus 在首次订阅前注入）
    pub async fn set_monitor(&self, monitor: Arc<MetricsRegistry>) {
        let mut m = self.monitor.write().await;
        *m = Some(monitor);
    }

    /// 发布 JSON 消息
    ///
    /// 异步入队给所有订阅了该 topic 的插件（不投递给发送者自己；格式偏好
    /// 不匹配的订阅者被拒绝，不进队列）。实际投递由每订阅者消费任务异步完成，
    /// 慢订阅者只阻塞自己的队列（背压丢弃），不阻塞发布方与其他订阅者。
    ///
    /// 从同步 host function 上下文调用时，spawn 独立任务入队，避免
    /// block_on_async 嵌套（消费任务内部的同步↔异步桥接已在独立任务中）。
    pub fn publish(&self, topic: &str, sender: &str, payload: serde_json::Value) {
        self.dispatch_publish(topic, sender, PayloadFormat::Json, payload, None);
    }

    /// 发布二进制消息（v11）：字节列原样透传，零 JSON 编解码，
    /// 可传非 UTF-8 与大载荷（MB 级）。仅二进制格式偏好的订阅者接收；
    /// JSON 偏好订阅者被拒绝（格式不匹配）。
    pub fn publish_binary(&self, topic: &str, sender: &str, payload: Vec<u8>) {
        self.dispatch_publish(
            topic,
            sender,
            PayloadFormat::Binary,
            serde_json::Value::Null,
            Some(payload),
        );
    }

    /// 发布公共路径：格式检查 → 每订阅者有界队列入队（满则丢弃 + 计数进监控）
    fn dispatch_publish(
        &self,
        topic: &str,
        sender: &str,
        format: PayloadFormat,
        payload: serde_json::Value,
        payload_binary: Option<Vec<u8>>,
    ) {
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

            let subscribers = subscribers_arc.read().await;
            let Some(subs) = subscribers.get(&topic) else {
                tracing::debug!("MessageBus: no subscribers for topic '{}', message dropped", topic);
                return;
            };

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

            let mut enqueued = 0;
            let mut rejected = 0;
            for sub in subs.iter() {
                if sub.plugin_id() == &sender {
                    continue;
                }
                // 格式偏好不匹配：拒绝投递（不进队列）+ 计数进监控 + warn
                if sub.format() != format {
                    if let Some(m) = sub.metrics() {
                        m.record_bus_format_rejected();
                    }
                    tracing::warn!(
                        plugin_id = %sub.plugin_id(),
                        topic = %topic,
                        format = ?format,
                        "MessageBus: subscriber format mismatch, message rejected"
                    );
                    rejected += 1;
                    continue;
                }
                // 每订阅者有界队列：try_send 失败即丢弃（背压保护），
                // 丢弃计数进 core-monitor（plugin 维度）
                match sub.try_send(msg.clone()) {
                    Ok(()) => enqueued += 1,
                    Err(TrySendError::Full(_)) => {
                        if let Some(m) = sub.metrics() {
                            m.record_bus_dropped();
                        }
                        tracing::warn!(
                            plugin_id = %sub.plugin_id(),
                            topic = %topic,
                            "MessageBus: subscriber queue full, message dropped"
                        );
                    }
                    Err(TrySendError::Closed(_)) => {
                        // 订阅者已移除/停用（队列关闭）：消息丢弃，不计数
                        tracing::debug!(
                            plugin_id = %sub.plugin_id(),
                            topic = %topic,
                            "MessageBus: subscriber queue closed, message dropped"
                        );
                    }
                }
            }

            tracing::debug!(
                "MessageBus: published topic='{}' sender='{}' enqueued={}/{} rejected={}",
                topic,
                sender,
                enqueued,
                subs.len(),
                rejected
            );
        });
    }

    /// 订阅 topic（WASM 插件，JSON 格式偏好，默认路径零回归）
    pub async fn subscribe_wasm(&self, plugin_id: &str, topic: &str) {
        self.subscribe_wasm_with_format(plugin_id, topic, PayloadFormat::Json)
            .await;
    }

    /// 以二进制格式偏好订阅 topic（v11）：只接收 `publish-binary` 投递，
    /// JSON 消息对其按格式不匹配拒绝（宿主按订阅者声明的格式过滤）
    pub async fn subscribe_wasm_binary(&self, plugin_id: &str, topic: &str) {
        self.subscribe_wasm_with_format(plugin_id, topic, PayloadFormat::Binary)
            .await;
    }

    /// 订阅公共路径：建每订阅者有界队列 + spawn 消费任务（recv → dispatch）
    async fn subscribe_wasm_with_format(&self, plugin_id: &str, topic: &str, format: PayloadFormat) {
        let mut subscribers = self.subscribers.write().await;
        let subs = subscribers.entry(topic.to_string()).or_default();
        // 避免重复订阅
        if subs
            .iter()
            .any(|s| matches!(s, BusSubscriber::Wasm { plugin_id: pid, .. } if pid == plugin_id))
        {
            tracing::debug!(plugin_id = %plugin_id, topic = %topic, "MessageBus: plugin already subscribed");
            return;
        }
        // 丢弃/格式拒绝计数句柄：core-monitor 插件维度；monitor 未注入
        // （测试/初始化前）时用临时句柄，计数不落监控——生产路径 monitor
        // 注入（PluginHost::init_message_bus）先于任何订阅，不受影响
        let metrics = {
            let m = self.monitor.read().await;
            m.as_ref()
                .map(|r| r.plugin(plugin_id))
                .unwrap_or_else(|| Arc::new(PluginMetrics::default()))
        };
        // 每订阅者有界队列：发布方 try_send（满则丢弃计数），消费任务串行投递，
        // 慢订阅者只阻塞自己，不阻塞发布方与其他订阅者（背压隔离）
        let (tx, mut rx) = mpsc::channel(SUBSCRIBER_QUEUE_CAPACITY);
        let dispatcher = self.dispatcher.clone();
        let pid = plugin_id.to_string();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                // dispatcher 两阶段注入：消费时动态读取（与旧 publish 路径同语义）
                let dispatcher = dispatcher.read().await.clone();
                let Some(dispatcher) = dispatcher else {
                    tracing::warn!(plugin_id = %pid, "MessageBus: dispatcher not set, WASM message skipped");
                    continue;
                };
                if !dispatcher.is_activated(&pid) {
                    tracing::warn!(plugin_id = %pid, "MessageBus: subscriber not activated, skipping");
                    continue;
                }
                if let Err(e) = dispatcher.dispatch_to_wasm(&pid, &msg) {
                    tracing::error!(plugin_id = %pid, error = %e, "MessageBus: dispatch to WASM plugin failed");
                }
            }
        });
        subs.push(BusSubscriber::Wasm {
            plugin_id: plugin_id.to_string(),
            format,
            tx,
            metrics,
        });
        tracing::info!(plugin_id = %plugin_id, topic = %topic, "MessageBus: plugin subscribed");
    }

    /// 订阅 topic（静态注册插件，JSON 格式偏好）
    pub async fn subscribe_static(&self, plugin_id: &str, topic: &str, handler: Box<dyn BusMessageHandler>) {
        let mut subscribers = self.subscribers.write().await;
        let subs = subscribers.entry(topic.to_string()).or_default();
        // 每订阅者有界队列 + 消费任务（handler 在任务内独占调用）
        let (tx, mut rx) = mpsc::channel(SUBSCRIBER_QUEUE_CAPACITY);
        let pid = plugin_id.to_string();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Err(e) = handler.on_message(&msg) {
                    tracing::error!(plugin_id = %pid, error = %e, "MessageBus: handler for static plugin failed");
                }
            }
        });
        subs.push(BusSubscriber::Static {
            plugin_id: plugin_id.to_string(),
            format: PayloadFormat::Json,
            tx,
        });
        tracing::info!(plugin_id = %plugin_id, topic = %topic, "MessageBus: static plugin subscribed");
    }

    /// 取消插件对指定 topic 的订阅
    ///
    /// 移除订阅者即 drop 其队列发送端，消费任务随 channel 关闭自动退出
    pub async fn unsubscribe(&self, plugin_id: &str, topic: &str) {
        let mut subscribers = self.subscribers.write().await;
        if let Some(subs) = subscribers.get_mut(topic) {
            let before = subs.len();
            subs.retain(|s| match s {
                BusSubscriber::Wasm { plugin_id: pid, .. } => pid != plugin_id,
                BusSubscriber::Static { plugin_id: pid, .. } => pid != plugin_id,
            });
            if subs.len() < before {
                tracing::info!(plugin_id = %plugin_id, topic = %topic, "MessageBus: plugin unsubscribed");
            }
        }
    }

    /// 某 topic 当前订阅者条数（诊断用：回复道/定向 topic 的订阅泄漏与收敛核对）
    pub async fn subscriber_count(&self, topic: &str) -> usize {
        self.subscribers
            .read()
            .await
            .get(topic)
            .map(|subs| subs.len())
            .unwrap_or(0)
    }

    /// 移除插件的所有订阅（停用时调用）
    ///
    /// 移除订阅者即 drop 其队列发送端，消费任务随 channel 关闭自动退出
    pub async fn remove_all_subscriptions(&self, plugin_id: &str) {
        let mut subscribers = self.subscribers.write().await;
        for (topic, subs) in subscribers.iter_mut() {
            let before = subs.len();
            subs.retain(|s| match s {
                BusSubscriber::Wasm { plugin_id: pid, .. } => pid != plugin_id,
                BusSubscriber::Static { plugin_id: pid, .. } => pid != plugin_id,
            });
            if subs.len() < before {
                tracing::debug!(plugin_id = %plugin_id, topic = %topic, "MessageBus: removed plugin subscription");
            }
        }
        // 清理空 topic
        subscribers.retain(|_, subs| !subs.is_empty());
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Receiver, Sender};
    use std::time::Duration;

    /// 测试用消息投递器 — 记录投递到 std mpsc 通道
    ///
    /// dispatch_to_wasm 是同步方法（在 spawn 的投递任务中调用），
    /// 测试线程用 recv_timeout 等待断言，避免依赖 sleep 猜测时序
    struct TestDispatcher {
        /// 视为已激活的插件 ID 集合（is_activated 按此判断）
        activated: Vec<String>,
        /// 模拟 dispatch 失败的插件 ID 集合（验证单个失败不阻塞其他订阅者）
        fail: Vec<String>,
        /// 每条投递的模拟耗时（制造队列积压，测背压丢弃；零 = 不延迟）
        delay: Duration,
        /// 投递记录通道
        tx: Sender<(String, BusMessage)>,
    }

    impl MessageDispatcher for TestDispatcher {
        fn dispatch_to_wasm(&self, plugin_id: &str, msg: &BusMessage) -> anyhow::Result<()> {
            if self.fail.iter().any(|p| p == plugin_id) {
                return Err(anyhow::anyhow!("simulated dispatch failure for {}", plugin_id));
            }
            if !self.delay.is_zero() {
                std::thread::sleep(self.delay);
            }
            self.tx.send((plugin_id.to_string(), msg.clone()))?;
            Ok(())
        }

        fn is_activated(&self, plugin_id: &str) -> bool {
            self.activated.iter().any(|p| p == plugin_id)
        }
    }

    /// 测试用静态订阅者 — 收到的消息转发到通道
    struct TestHandler {
        tx: Sender<BusMessage>,
    }

    impl BusMessageHandler for TestHandler {
        fn on_message(&self, msg: &BusMessage) -> anyhow::Result<()> {
            self.tx.send(msg.clone())?;
            Ok(())
        }
    }

    /// 构造测试 dispatcher（activated 之外的插件一律视为未激活）
    fn test_dispatcher(activated: &[&str]) -> (Arc<dyn MessageDispatcher>, Receiver<(String, BusMessage)>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (
            Arc::new(TestDispatcher {
                activated: activated.iter().map(|s| s.to_string()).collect(),
                fail: Vec::new(),
                delay: Duration::ZERO,
                tx,
            }),
            rx,
        )
    }

    /// 构造测试静态订阅者
    fn test_handler() -> (Box<dyn BusMessageHandler>, Receiver<BusMessage>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (Box::new(TestHandler { tx }), rx)
    }

    /// 等待投递结果，超时视为未投递
    fn wait_delivery<T>(rx: &Receiver<T>, timeout: Duration) -> Result<T, std::sync::mpsc::RecvTimeoutError> {
        rx.recv_timeout(timeout)
    }

    // ==================== 发布与投递 ====================

    /// 无任何订阅者时 publish 不 panic，消息静默丢弃
    #[tokio::test(flavor = "multi_thread")]
    async fn test_publish_no_subscribers_no_panic() {
        let bus = MessageBus::new();
        bus.publish("topic:no-sub", "sender-a", serde_json::json!({"v": 1}));
        // 给 spawn 的投递任务一点执行时间，验证不崩溃
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    /// 在无 runtime 上下文中 publish 直接丢弃消息，不 panic
    ///
    /// 同步 host function 之外调用（如测试线程无 tokio runtime）时，
    /// try_current 失败走警告分支
    #[test]
    fn test_publish_outside_runtime_drops_without_panic() {
        let bus = MessageBus::new();
        bus.publish("topic:x", "sender-a", serde_json::json!(1));
    }

    /// dispatcher 未注入时（两阶段初始化的中间态）消息被丢弃而非 panic
    #[tokio::test(flavor = "multi_thread")]
    async fn test_publish_without_dispatcher_drops_message() {
        let bus = MessageBus::new();
        bus.subscribe_wasm("plugin-b", "topic:demo").await;
        bus.publish("topic:demo", "sender-a", serde_json::json!({"v": 1}));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    /// WASM 订阅者收到完整 BusMessage：topic/sender/payload 原样透传，时间戳非 0
    #[tokio::test(flavor = "multi_thread")]
    async fn test_wasm_subscriber_receives_published_message() {
        let bus = MessageBus::new();
        let (dispatcher, rx) = test_dispatcher(&["plugin-b"]);
        bus.set_dispatcher(dispatcher).await;
        bus.subscribe_wasm("plugin-b", "task:status-changed").await;

        let payload = serde_json::json!({"taskId": "t-1", "status": "running"});
        bus.publish("task:status-changed", "plugin-a", payload.clone());

        let (plugin_id, msg) = wait_delivery(&rx, Duration::from_secs(2)).expect("WASM 订阅者应收到消息");
        assert_eq!(plugin_id, "plugin-b");
        assert_eq!(msg.topic, "task:status-changed");
        assert_eq!(msg.sender, "plugin-a");
        assert_eq!(msg.payload, payload);
        assert!(msg.timestamp > 0, "时间戳应为当前毫秒，非 0");
    }

    /// 消息不投递给发送者自己，但其他订阅者正常收到
    #[tokio::test(flavor = "multi_thread")]
    async fn test_publish_does_not_deliver_to_sender() {
        let bus = MessageBus::new();
        let (dispatcher, rx) = test_dispatcher(&["plugin-a", "plugin-b"]);
        bus.set_dispatcher(dispatcher).await;
        bus.subscribe_wasm("plugin-a", "topic:echo").await;
        bus.subscribe_wasm("plugin-b", "topic:echo").await;

        bus.publish("topic:echo", "plugin-a", serde_json::json!(1));

        let (plugin_id, _) = wait_delivery(&rx, Duration::from_secs(2)).expect("plugin-b 应收到");
        assert_eq!(plugin_id, "plugin-b");
        // 发送者 plugin-a 被跳过，通道不应再有第二条消息
        assert!(wait_delivery(&rx, Duration::from_millis(300)).is_err());
    }

    /// topic 不匹配的发布不投递
    #[tokio::test(flavor = "multi_thread")]
    async fn test_publish_topic_mismatch_not_delivered() {
        let bus = MessageBus::new();
        let (dispatcher, rx) = test_dispatcher(&["plugin-b"]);
        bus.set_dispatcher(dispatcher).await;
        bus.subscribe_wasm("plugin-b", "topic:a").await;

        bus.publish("topic:b", "plugin-a", serde_json::json!(1));
        assert!(wait_delivery(&rx, Duration::from_millis(300)).is_err());
    }

    /// 同一插件重复订阅同一 topic 只投递一次（subscribe_wasm 去重）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_duplicate_wasm_subscribe_delivers_once() {
        let bus = MessageBus::new();
        let (dispatcher, rx) = test_dispatcher(&["plugin-b"]);
        bus.set_dispatcher(dispatcher).await;
        bus.subscribe_wasm("plugin-b", "topic:dup").await;
        bus.subscribe_wasm("plugin-b", "topic:dup").await;

        bus.publish("topic:dup", "plugin-a", serde_json::json!(1));

        let (plugin_id, _) = wait_delivery(&rx, Duration::from_secs(2)).expect("应收到一条消息");
        assert_eq!(plugin_id, "plugin-b");
        assert!(wait_delivery(&rx, Duration::from_millis(300)).is_err());
    }

    /// 未激活的 WASM 订阅者被跳过，已激活的订阅者正常收到
    #[tokio::test(flavor = "multi_thread")]
    async fn test_inactive_wasm_subscriber_skipped() {
        let bus = MessageBus::new();
        let (dispatcher, rx) = test_dispatcher(&["plugin-c"]);
        bus.set_dispatcher(dispatcher).await;
        bus.subscribe_wasm("plugin-b", "topic:act").await;
        bus.subscribe_wasm("plugin-c", "topic:act").await;

        bus.publish("topic:act", "plugin-a", serde_json::json!(1));

        let (plugin_id, _) = wait_delivery(&rx, Duration::from_secs(2)).expect("激活的订阅者应收到");
        assert_eq!(plugin_id, "plugin-c");
        assert!(wait_delivery(&rx, Duration::from_millis(300)).is_err());
    }

    /// 单个插件 dispatch 失败（如 WASM 运行时错误）不阻塞其他订阅者
    #[tokio::test(flavor = "multi_thread")]
    async fn test_dispatch_error_does_not_block_other_subscribers() {
        let bus = MessageBus::new();
        let (tx, rx) = std::sync::mpsc::channel();
        let dispatcher: Arc<dyn MessageDispatcher> = Arc::new(TestDispatcher {
            activated: vec!["plugin-b".to_string(), "plugin-c".to_string()],
            fail: vec!["plugin-b".to_string()],
            delay: Duration::ZERO,
            tx,
        });
        bus.set_dispatcher(dispatcher).await;
        bus.subscribe_wasm("plugin-b", "topic:multi").await;
        bus.subscribe_wasm("plugin-c", "topic:multi").await;

        bus.publish("topic:multi", "plugin-a", serde_json::json!(1));

        let (plugin_id, _) =
            wait_delivery(&rx, Duration::from_secs(2)).expect("plugin-c 应收到（plugin-b 的失败不影响它）");
        assert_eq!(plugin_id, "plugin-c");
    }

    // ==================== 静态订阅者 ====================

    /// 静态订阅者通过 Rust callback 收到消息（dispatcher 未注入时也不受影响，见下方测试）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_static_subscriber_receives_message() {
        let bus = MessageBus::new();
        // 生产环境 PluginHost 构造完成后必注入 dispatcher，静态投递路径不依赖它
        let (dispatcher, _rx_d) = test_dispatcher(&[]);
        bus.set_dispatcher(dispatcher).await;
        let (handler, rx) = test_handler();
        bus.subscribe_static("plugin-c", "topic:static", handler).await;

        bus.publish("topic:static", "plugin-a", serde_json::json!({"n": 42}));

        let msg = wait_delivery(&rx, Duration::from_secs(2)).expect("静态订阅者应收到消息");
        assert_eq!(msg.topic, "topic:static");
        assert_eq!(msg.sender, "plugin-a");
        assert_eq!(msg.payload, serde_json::json!({"n": 42}));
        assert!(msg.timestamp > 0);
    }

    /// 静态订阅者不依赖 WASM dispatcher：未注入时静态消息仍投递，WASM 订阅者被跳过
    #[tokio::test(flavor = "multi_thread")]
    async fn test_static_subscriber_receives_without_dispatcher() {
        let bus = MessageBus::new();
        let (handler, rx) = test_handler();
        bus.subscribe_static("plugin-c", "topic:static", handler).await;
        // 同时注册一个 WASM 订阅者，验证未注入 dispatcher 时被跳过而不是阻塞静态投递
        bus.subscribe_wasm("plugin-b", "topic:static").await;

        bus.publish("topic:static", "plugin-a", serde_json::json!(1));

        let msg =
            wait_delivery(&rx, Duration::from_secs(1)).expect("静态订阅者走 Rust callback，不应依赖 WASM dispatcher");
        assert_eq!(msg.sender, "plugin-a");
    }

    // ==================== 订阅增删 ====================

    /// unsubscribe 同时移除 WASM 与静态订阅者；对不存在的 topic 调用不 panic
    #[tokio::test(flavor = "multi_thread")]
    async fn test_unsubscribe_removes_wasm_and_static() {
        let bus = MessageBus::new();
        let (dispatcher, rx) = test_dispatcher(&["plugin-b"]);
        bus.set_dispatcher(dispatcher).await;
        bus.subscribe_wasm("plugin-b", "topic:unsub").await;
        let (handler, _rx_h) = test_handler();
        bus.subscribe_static("plugin-c", "topic:unsub", handler).await;

        bus.unsubscribe("plugin-b", "topic:unsub").await;
        bus.unsubscribe("plugin-b", "topic:not-exist").await;

        bus.publish("topic:unsub", "plugin-a", serde_json::json!(1));
        assert!(wait_delivery(&rx, Duration::from_millis(300)).is_err());
    }

    /// remove_all_subscriptions 清空插件全部订阅并回收空 topic；其他插件不受影响
    #[tokio::test(flavor = "multi_thread")]
    async fn test_remove_all_subscriptions_cleans_topics() {
        let bus = MessageBus::new();
        let (dispatcher, rx) = test_dispatcher(&["plugin-b", "plugin-c"]);
        bus.set_dispatcher(dispatcher).await;
        bus.subscribe_wasm("plugin-b", "topic:x").await;
        bus.subscribe_wasm("plugin-b", "topic:y").await;
        bus.subscribe_wasm("plugin-c", "topic:y").await;

        bus.remove_all_subscriptions("plugin-b").await;

        bus.publish("topic:x", "plugin-a", serde_json::json!(1));
        bus.publish("topic:y", "plugin-a", serde_json::json!(2));

        // topic:x 应因无订阅者被清空；topic:y 仍投递给 plugin-c
        let (plugin_id, msg) = wait_delivery(&rx, Duration::from_secs(2)).expect("plugin-c 应收到 topic:y");
        assert_eq!(plugin_id, "plugin-c");
        assert_eq!(msg.topic, "topic:y");
        assert!(wait_delivery(&rx, Duration::from_millis(300)).is_err());
    }

    // ==================== v11：二进制载荷与背压 ====================

    /// 二进制 roundtrip：非 UTF-8 字节与 MB 级大载荷收发字节一致（零 JSON 编解码）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_binary_roundtrip_non_utf8_and_large_payload() {
        let bus = MessageBus::new();
        let (dispatcher, rx) = test_dispatcher(&["plugin-b"]);
        bus.set_dispatcher(dispatcher).await;
        bus.subscribe_wasm_binary("plugin-b", "blob:transfer").await;

        // 非 UTF-8 字节（含 0xFF/0xFE/0xC3 0x28 等非法 UTF-8 序列）
        let non_utf8: Vec<u8> = vec![0x00, 0xFF, 0xFE, 0x80, 0x41, 0xC3, 0x28];
        bus.publish_binary("blob:transfer", "plugin-a", non_utf8.clone());
        let (plugin_id, msg) = wait_delivery(&rx, Duration::from_secs(2)).expect("二进制订阅者应收到非 UTF-8 载荷");
        assert_eq!(plugin_id, "plugin-b");
        assert_eq!(
            msg.payload_binary.as_ref(),
            Some(&non_utf8),
            "非 UTF-8 字节必须原样透传"
        );
        assert_eq!(msg.payload, serde_json::Value::Null, "二进制消息的 JSON 字段恒为 Null");

        // MB 级大载荷（2MB，模 251 校验字节一致性）
        let large: Vec<u8> = (0..2 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        bus.publish_binary("blob:transfer", "plugin-a", large.clone());
        let (_, msg) = wait_delivery(&rx, Duration::from_secs(2)).expect("MB 级大载荷应收到");
        assert_eq!(msg.payload_binary.as_ref(), Some(&large), "大载荷字节必须一致");
    }

    /// JSON 格式偏好的订阅者收不到二进制消息：拒绝 + warn + 拒绝计数进监控
    #[tokio::test(flavor = "multi_thread")]
    async fn test_json_subscriber_rejects_binary_message() {
        let bus = MessageBus::new();
        let monitor = Arc::new(MetricsRegistry::new());
        bus.set_monitor(monitor.clone()).await;
        let (dispatcher, rx) = test_dispatcher(&["plugin-b"]);
        bus.set_dispatcher(dispatcher).await;
        // 默认 subscribe_wasm = JSON 格式偏好
        bus.subscribe_wasm("plugin-b", "topic:mixed").await;

        bus.publish_binary("topic:mixed", "plugin-a", vec![1, 2, 3]);

        // 格式不匹配：拒绝投递（不进队列），dispatcher 无投递记录
        assert!(
            wait_delivery(&rx, Duration::from_millis(300)).is_err(),
            "JSON 订阅者不得收到二进制消息"
        );
        // 拒绝计数进监控（dropped 不受影响）
        let p = &monitor.snapshot()["plugins"]["plugin-b"]["bus"];
        assert_eq!(p["format_rejected"], 1, "格式拒绝应计数");
        assert_eq!(p["dropped"], 0);
    }

    /// 二进制格式偏好的订阅者收不到 JSON 消息（对称拒绝 + 对称计数）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_binary_subscriber_rejects_json_message() {
        let bus = MessageBus::new();
        let monitor = Arc::new(MetricsRegistry::new());
        bus.set_monitor(monitor.clone()).await;
        let (dispatcher, rx) = test_dispatcher(&["plugin-b"]);
        bus.set_dispatcher(dispatcher).await;
        bus.subscribe_wasm_binary("plugin-b", "topic:mixed").await;

        bus.publish("topic:mixed", "plugin-a", serde_json::json!({ "v": 1 }));

        assert!(
            wait_delivery(&rx, Duration::from_millis(300)).is_err(),
            "二进制订阅者不得收到 JSON 消息"
        );
        // 对称方向同样进 format_rejected 计数（与 JSON 订阅者拒二进制一致）
        let p = &monitor.snapshot()["plugins"]["plugin-b"]["bus"];
        assert_eq!(p["format_rejected"], 1, "二进制订阅者拒绝 JSON 同样须计数");
        assert_eq!(p["dropped"], 0);
    }

    /// 队列满丢弃（背压保护）：慢订阅者队列满时消息丢弃、丢弃计数进监控，
    /// 且守恒律成立——每条消息要么被投递要么被计数丢弃
    #[tokio::test(flavor = "multi_thread")]
    async fn test_queue_full_drops_with_monitor_count() {
        const TOTAL: usize = 70;
        const DELAY_MS: u64 = 5; // 消费任务每条投递耗时：制造队列积压

        let bus = MessageBus::new();
        let monitor = Arc::new(MetricsRegistry::new());
        bus.set_monitor(monitor.clone()).await;
        let (tx, rx) = std::sync::mpsc::channel();
        let dispatcher: Arc<dyn MessageDispatcher> = Arc::new(TestDispatcher {
            activated: vec!["plugin-b".to_string()],
            fail: Vec::new(),
            delay: Duration::from_millis(DELAY_MS),
            tx,
        });
        bus.set_dispatcher(dispatcher).await;
        bus.subscribe_wasm("plugin-b", "topic:flood").await;

        // 等消费任务就绪（订阅 spawn 完成）后瞬时灌入 TOTAL 条
        tokio::time::sleep(Duration::from_millis(50)).await;
        for i in 0..TOTAL {
            bus.publish("topic:flood", "plugin-a", serde_json::json!(i));
        }

        // 收齐全部投递（每条 5ms，总耗时约 350ms + 超时余量）
        let mut delivered = 0;
        while rx.recv_timeout(Duration::from_millis(100)).is_ok() {
            delivered += 1;
        }
        let dropped = monitor.snapshot()["plugins"]["plugin-b"]["bus"]["dropped"]
            .as_u64()
            .unwrap_or(0) as usize;

        assert_eq!(
            delivered + dropped,
            TOTAL,
            "守恒：投递 {} + 丢弃 {} == 总数 {}",
            delivered,
            dropped,
            TOTAL
        );
        assert!(
            dropped > 0,
            "慢消费者场景必须出现队列满丢弃（容量 {} < 总数 {}）",
            SUBSCRIBER_QUEUE_CAPACITY,
            TOTAL
        );
    }
}
