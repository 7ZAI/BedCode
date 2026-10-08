//! 消息总线
//!
//! 插件间 Topic 消息总线 — 发布/订阅模式通信
//! 通过 MessageDispatcher trait 解耦与 PluginHost 的循环引用

use crate::monitor::{MetricsRegistry, PluginMetrics};
use async_trait::async_trait;
use bedcode_plugin_api_mobile::BusMessage;
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
#[async_trait::async_trait]
pub trait MessageDispatcher: Send + Sync + 'static {
    /// 投递消息给 WASM 插件（调用 __bedcode_on_message）
    async fn dispatch_to_wasm(&self, plugin_id: &str, msg: &BusMessage) -> anyhow::Result<()>;
    /// 检查插件是否已激活
    async fn is_activated(&self, plugin_id: &str) -> bool;

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
                // 无订阅者 → 丢弃。这是**按消息触发**的分支，与下面的成功投递同属
                // 热路径，逐条打 debug 同样会成风暴（实测 task:status-changed 这类
                // 「bus + emit + ws 三通道」topic 在无跨插件消费者时每次事件都刷一行）。
                // 发布意图由插件侧自己的日志留痕；本锁见 hot_path_logging_test.rs。
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
                    continue;
                }
                // 每订阅者有界队列：try_send 失败即丢弃（背压保护），
                // 丢弃计数进 core-monitor（plugin 维度）
                match sub.try_send(msg.clone()) {
                    Ok(()) => {}
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
                        // 订阅者已移除/停用（队列关闭）：消息丢弃、不计数、不打日志
                        // ——停用路径上队列关闭与消息到达同频发生，逐条打日志即风暴
                    }
                }
            }
            // 成功入队的路径**不落任何日志**：publish 是按消息触发的热路径，
            // 高频数据面 topic（PTY 输出通知 ≥50ms/句柄、WS 事件流）会把它打成
            // 日志风暴——实测单次会话 11 分钟刷 9585 行、占开发日志 52% 字节，且
            // 每次内容都是 `enqueued=1/1 rejected=0` 的零信息重复。投递健康度由
            // core-monitor 的 `bus.dropped` / `bus.format_rejected` 计数承担
            // （本函数异常分支各自 warn），正常投递无需逐条留痕。
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
                if !dispatcher.is_activated(&pid).await {
                    tracing::warn!(plugin_id = %pid, "MessageBus: subscriber not activated, skipping");
                    continue;
                }
                if let Err(e) = dispatcher.dispatch_to_wasm(&pid, &msg).await {
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

