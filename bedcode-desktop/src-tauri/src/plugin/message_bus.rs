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
            // dispatcher 仅在投递 WASM 订阅者时需要（两阶段初始化注入）；
            // 静态订阅者走 Rust callback，不依赖 dispatcher，不能因未注入而丢弃
            let dispatcher = {
                let guard = dispatcher_arc.read().await;
                guard.clone()
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
                        let Some(dispatcher) = &dispatcher else {
                            tracing::warn!(
                                "MessageBus: dispatcher not set, skipping WASM subscriber '{}'",
                                plugin_id
                            );
                            continue;
                        };
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
        /// 投递记录通道
        tx: Sender<(String, BusMessage)>,
    }

    impl MessageDispatcher for TestDispatcher {
        fn dispatch_to_wasm(&self, plugin_id: &str, msg: &BusMessage) -> anyhow::Result<()> {
            if self.fail.iter().any(|p| p == plugin_id) {
                return Err(anyhow::anyhow!("simulated dispatch failure for {}", plugin_id));
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
    fn wait_delivery<T>(
        rx: &Receiver<T>,
        timeout: Duration,
    ) -> Result<T, std::sync::mpsc::RecvTimeoutError> {
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

        let (plugin_id, msg) = wait_delivery(&rx, Duration::from_secs(2))
            .expect("WASM 订阅者应收到消息");
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
            tx,
        });
        bus.set_dispatcher(dispatcher).await;
        bus.subscribe_wasm("plugin-b", "topic:multi").await;
        bus.subscribe_wasm("plugin-c", "topic:multi").await;

        bus.publish("topic:multi", "plugin-a", serde_json::json!(1));

        let (plugin_id, _) = wait_delivery(&rx, Duration::from_secs(2))
            .expect("plugin-c 应收到（plugin-b 的失败不影响它）");
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

        let msg = wait_delivery(&rx, Duration::from_secs(1))
            .expect("静态订阅者走 Rust callback，不应依赖 WASM dispatcher");
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
}
