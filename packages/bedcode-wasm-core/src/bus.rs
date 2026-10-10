//! 消息总线
//!
//! 插件间 Topic 消息总线 — 发布/订阅模式通信
//! 通过 MessageDispatcher trait 解耦与 PluginHost 的循环引用

use crate::monitor::{MetricsRegistry, PluginMetrics};
use async_trait::async_trait;
// 双端 SDK 的同名 wire 副本（形状逐字一致，server-base wire/drift_lock 钉住）：
// 机制面载荷类型按形态取对应 SDK（票 06 批次 03；真源统一留票 07）。
// **crate 内引用一律走 `crate::bus::BusMessage`**（此处 pub use 是单点切换），
// 禁止直引 SDK 路径（否则双形态签名漂移）
#[cfg(feature = "desktop-host")]
pub use bedcode_plugin_api::BusMessage;
#[cfg(feature = "mobile-host")]
pub use bedcode_plugin_api_mobile::BusMessage;
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
/// 由 PluginHost 实现，避免 MessageBus 与 PluginHost 循环引用。
///
/// **投递形态按宿主分支分叉（票 06 批次 03）**：桌面宿主的投递器实现是同步
/// 调用（PluginHost 直驱实例调用）；移动装配面（fork 票 17）投递器 async 化
/// （`dispatch_to_wasm` / `is_activated` 为 async fn，host fn 上下文经
/// block_on 桥进入）——同一 trait 双形态签名，实现方按形态对应。
#[cfg(feature = "mobile-host")]
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

#[cfg(feature = "desktop-host")]
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
                // 投递调用形态随 trait 分叉（票 06 批次 03：移动 async / 桌面同步）
                #[cfg(feature = "mobile-host")]
                {
                    if !dispatcher.is_activated(&pid).await {
                        tracing::warn!(plugin_id = %pid, "MessageBus: subscriber not activated, skipping");
                        continue;
                    }
                    if let Err(e) = dispatcher.dispatch_to_wasm(&pid, &msg).await {
                        tracing::error!(plugin_id = %pid, error = %e, "MessageBus: dispatch to WASM plugin failed");
                        continue;
                    }
                }
                #[cfg(feature = "desktop-host")]
                {
                    if !dispatcher.is_activated(&pid) {
                        tracing::warn!(plugin_id = %pid, "MessageBus: subscriber not activated, skipping");
                        continue;
                    }
                    if let Err(e) = dispatcher.dispatch_to_wasm(&pid, &msg) {
                        tracing::error!(plugin_id = %pid, error = %e, "MessageBus: dispatch to WASM plugin failed");
                        continue;
                    }
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

// ==================== HostBusPort（wasm-core-whole-crate 票 M11，从 lib server/ports_impl.rs 抽入） ====================
//
// 桌面装配面（票 06 批次 03）：HostBusPort 打包 MessageBus + 能力域 WsPorts 帧投递，
// 依赖 `bedcode-server-websocket`（desktop-host optional 依赖）——移动形态无 WS
// 服务端域，整段不编译（mobile 装配面无此端口）。

/// 插件消息总线 + WS 帧投递（`bus::MessageBus` + 能力域 `deliver_endpoint_frame` 包装）
///
/// 原属 lib 的 `server/ports_impl.rs`（包 `MessageBus`——crate 属物），随整核抽出
/// 迁入本 crate；lib 的 `server/ports_impl.rs` 经 `pub use` 垫片再导出，`assemble()`
/// 本体留 lib（组合根唯一性）。
#[cfg(feature = "desktop-host")]
pub struct HostBusPort {
    binding: BusBinding,
}

/// 总线绑定形态：装配期钉死 / 逐次调用解析（late-bound）
#[cfg(feature = "desktop-host")]
enum BusBinding {
    /// 装配期即持有真实总线（bootstrap 之后装配的端口走这一支）
    Fixed {
        bus: Arc<MessageBus>,
        /// 帧投递用的能力域端口视图（**构造一次**、不逐帧分配）：能力域的帧投递函数
        /// 收 `&Arc<dyn WsPorts>`，故此处持一份绑定到本总线的窄端口（无权限管理器）。
        ///
        /// **由构造方喂入**（票 02 批次 03）：内核不再自建该端口——它是能力域 adapter
        /// 的产物（`WsPorts` 的宿主实现），本模块只接收。
        ws_ports: Arc<dyn bedcode_server_websocket::plugin_binding::ports::WsPorts>,
    },
    /// 逐次调用解析当前总线（装配可能早于总线就位，见 [`HostBusPort::late_bound`]）
    ///
    /// 第二分量 = 帧投递端口工厂：late-bound 形态必须能按**当下的**总线现造窄端口
    /// （端口与总线必须同源，见 [`HostBusPort::ws_ports`]）。
    LateBound(
        Arc<dyn Fn() -> Arc<MessageBus> + Send + Sync>,
        Arc<dyn Fn(&Arc<MessageBus>) -> Arc<dyn bedcode_server_websocket::plugin_binding::ports::WsPorts> + Send + Sync>,
    ),
}

#[cfg(feature = "desktop-host")]
impl HostBusPort {
    /// 钉死构造：`ws_ports` 由**构造方**给出（绑定到 `bus` 的窄端口）
    ///
    /// 为什么必须由构造方给：该端口是能力域 adapter 的产物，内核不构造它（票 02
    /// 批次 03）；且它必须绑定到**本实例的**那条总线——多上下文场景下端口错绑会把
    /// 帧投进别的实例。
    pub fn new(
        bus: Arc<MessageBus>,
        ws_ports: Arc<dyn bedcode_server_websocket::plugin_binding::ports::WsPorts>,
    ) -> Self {
        Self {
            binding: BusBinding::Fixed { bus, ws_ports },
        }
    }

    /// late-bound 构造：每次调用经 `resolve` 取当前总线，经 `ws_ports_for` 现造窄端口
    ///
    /// 为什么需要这一支：端口装配**可能早于真实总线就位**——宿主组合根的顺序是
    /// 「建 PluginHost（内部即激活插件，激活期 guest 会立刻调 host-* 原语）→ 注册
    /// AppContext → 装端口」，而总线在 `PluginHost` 构造时创建。装配期把总线钉死
    /// 的话，提前装配出来的那套端口会永远指向占位总线（插件订阅永收不到消息）；
    /// late-bound 让「早装的」与「晚装的」在总线就位后行为一致。
    pub fn late_bound<F>(resolve: F, ws_ports_for: Arc<dyn Fn(&Arc<MessageBus>) -> Arc<dyn bedcode_server_websocket::plugin_binding::ports::WsPorts> + Send + Sync>) -> Self
    where
        F: Fn() -> Arc<MessageBus> + Send + Sync + 'static,
    {
        Self {
            binding: BusBinding::LateBound(Arc::new(resolve), ws_ports_for),
        }
    }

    fn bus(&self) -> Arc<MessageBus> {
        match &self.binding {
            BusBinding::Fixed { bus, .. } => bus.clone(),
            BusBinding::LateBound(resolve, _) => resolve(),
        }
    }

    /// 帧投递窄端口：钉死形态复用构造期那一份（不逐帧分配）；late-bound 形态
    /// 按**当下的**总线现造（每帧一个 `Arc`，相对 JSON 编码可忽略）
    fn ws_ports(&self) -> Arc<dyn bedcode_server_websocket::plugin_binding::ports::WsPorts> {
        match &self.binding {
            BusBinding::Fixed { ws_ports, .. } => ws_ports.clone(),
            BusBinding::LateBound(_, ws_ports_for) => ws_ports_for(&self.bus()),
        }
    }
}

// ==================== 总线绑定的帧投递窄端口（总线侧 plumbing） ====================

/// 绑定到某条总线的帧投递窄端口（**总线侧 plumbing，不是能力域适配器**）
///
/// ## 为什么住在总线侧（票 02 批次 03 的裁决 ①）
///
/// 它要的两件事都由**总线**提供：往这条总线投 `events-ws` 帧、以及造一个 `BusPort`
/// 给端点登记。它不需要宿主上下文，也**不做权限判定**（`check_permission` 恒 `false`
/// ——与迁移前 `HostWsPorts::from_bus` 的 fail-safe 口径逐字一致）。因此它的语义是
/// 「总线接到能力域 trait 上的适配器」，属机制，留在内核；能力域的**完整端口**
/// （带权限管理器 / 实例绑定）在宿主 adapter `src-tauri/src/plugin/ws.rs`。
///
/// ## 谁在用
///
/// - [`HostBusPort`] 的帧回灌路径（`ws_ports()` 未注入时的兜底不存在——注入由构造方
///   提供；本类型是**内核侧构造方**用的那一份）；
/// - `manager/host/register.rs` 的 WS 端点登记（需要 `bus_port()`）。
#[cfg(feature = "desktop-host")]
pub(crate) struct BusBoundWsPorts {
    bus: Arc<MessageBus>,
}

#[cfg(feature = "desktop-host")]
impl BusBoundWsPorts {
    /// 绑定到给定总线（窄端口：无宿主上下文 ⇒ 权限门恒拒）
    pub(crate) fn new(bus: Arc<MessageBus>) -> Self {
        Self { bus }
    }
}

#[cfg(feature = "desktop-host")]
impl bedcode_server_websocket::plugin_binding::ports::WsPorts for BusBoundWsPorts {
    fn check_permission(&self, _plugin_id: &str, _permission: &str, _api: &str) -> bool {
        // 窄端口没有权限管理器 ⇒ 拒绝（fail-safe）。消费它的两条路径（帧回灌 / 端点登记）
        // 本就不做权限判定，真正的权限门在能力域原语入口与完整端口上。
        false
    }

    fn publish(&self, topic: &str, payload: serde_json::Value) {
        self.bus.publish(topic, "host", payload);
    }

    fn bus_port(&self) -> Arc<dyn bedcode_server_base::ports::BusPort> {
        Arc::new(HostBusPort::new(
            Arc::clone(&self.bus),
            Arc::new(Self::new(Arc::clone(&self.bus))),
        ))
    }

    fn dispatch_frame(
        &self,
        plugin_id: &str,
        target: bedcode_server_websocket::plugin_binding::ports::WsFrameTarget<'_>,
        kind: &str,
        payload: Vec<u8>,
    ) -> bedcode_server_websocket::plugin_binding::ports::FrameDispatch {
        use bedcode_server_websocket::plugin_binding::ports::FrameDispatch;
        let Some(dispatcher) = crate::runtime_util::block_on_async({
            let bus = Arc::clone(&self.bus);
            async move { bus.dispatcher().await }
        }) else {
            return FrameDispatch::Unavailable;
        };
        let frame = match target {
            bedcode_server_websocket::plugin_binding::ports::WsFrameTarget::Client(handle) => {
                WsFrameDispatch::Client {
                    handle: handle.to_string(),
                    kind: kind.to_string(),
                    payload,
                }
            }
            bedcode_server_websocket::plugin_binding::ports::WsFrameTarget::EndpointClient {
                endpoint_id,
                client_id,
            } => WsFrameDispatch::EndpointClient {
                endpoint_id: endpoint_id.to_string(),
                client_id: client_id.to_string(),
                kind: kind.to_string(),
                payload,
            },
        };
        match dispatcher.dispatch_ws_frame(plugin_id, &frame) {
            Ok(true) => FrameDispatch::Delivered,
            Ok(false) => FrameDispatch::NotExported,
            Err(e) => FrameDispatch::Failed(e.to_string()),
        }
    }

    fn block_on_any(
        &self,
        fut: bedcode_server_websocket::plugin_binding::ports::BoxedBlocked,
    ) -> Box<dyn std::any::Any + Send> {
        crate::runtime_util::block_on_async(fut)
    }
}

/// base 侧 `BusMessageHandler` → 本 crate 侧 `BusMessageHandler` 适配
///
/// P5 常量下沉后 base trait 载荷形状收 `bedcode_server_base::wire::BusMessage`
/// （本 crate 总线内部流转保持 SDK `BusMessage`——机制层类型选择，spec §8）；
/// 两形状逐字一致由 base 侧 `wire::drift_lock` 钉死，此处做值转换（字段逐个
/// 克隆，零语义变化）。
struct WasmHandlerAdapter(Box<dyn bedcode_server_base::ports::BusMessageHandler>);

impl BusMessageHandler for WasmHandlerAdapter {
    fn on_message(&self, msg: &BusMessage) -> anyhow::Result<()> {
        self.0.on_message(&bedcode_server_base::wire::BusMessage {
            topic: msg.topic.clone(),
            sender: msg.sender.clone(),
            payload: msg.payload.clone(),
            payload_binary: msg.payload_binary.clone(),
            timestamp: msg.timestamp,
        })
    }
}

#[async_trait]
#[cfg(feature = "desktop-host")]
impl bedcode_server_base::ports::BusPort for HostBusPort {
    fn publish(&self, topic: &str, sender: &str, payload: serde_json::Value) {
        self.bus().publish(topic, sender, payload);
    }

    fn publish_binary(&self, topic: &str, sender: &str, payload: Vec<u8>) {
        self.bus().publish_binary(topic, sender, payload);
    }

    async fn subscribe_static(&self, subscriber: &str, topic: &str, handler: Box<dyn bedcode_server_base::ports::BusMessageHandler>) {
        self.bus()
            .subscribe_static(subscriber, topic, Box::new(WasmHandlerAdapter(handler)))
            .await;
    }

    async fn deliver_endpoint_frame(
        &self,
        owner: &str,
        endpoint_id: &str,
        client_id: &str,
        kind: &str,
        payload: Vec<u8>,
    ) {
        // 能力域已迁入 `bedcode_server_websocket::plugin_binding`（wasm-core-lib-split 票 04）
        bedcode_server_websocket::plugin_binding::deliver_endpoint_frame(
            &self.ws_ports(),
            owner,
            endpoint_id,
            client_id,
            kind,
            payload,
        )
        .await;
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Receiver, Sender};

    /// 测试用帧投递端口工厂：真实能力域窄端口（绑定传入的总线、无权限管理器）
    ///
    /// 生产路径这份由宿主 adapter 喂入（票 02 批次 03）；内核测试二进制里 adapter 仍在，
    /// 直接复用其 `from_bus` 构造。
    fn test_ws_ports_factory() -> Arc<
        dyn Fn(&Arc<MessageBus>) -> Arc<dyn bedcode_server_websocket::plugin_binding::ports::WsPorts>
            + Send
            + Sync,
    > {
        Arc::new(|bus: &Arc<MessageBus>| {
            Arc::new(crate::bus::BusBoundWsPorts::new(Arc::clone(bus)))
        })
    }
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

    // 2026-10-08 补：BusPort 端口路径（HostBusPort 经 WasmHandlerAdapter 收 base trait）
    // 需要 base 形状的 handler；TestHandler 直接双实现。
    // （在途 BusBinding 重构新增的 late_bound 用例此前从未编译过——wasm-core 测试
    // 自整核抽出后未跑，修复仅为解除全量测试的编译阻塞）
    // P5 常量下沉：base trait 载荷形状收 base 自持副本（`wire::BusMessage`），
    // 转发进 SDK 形状的通道前做值转换（形状一致由 base 侧漂移锁钉死）。
    impl bedcode_server_base::ports::BusMessageHandler for TestHandler {
        fn on_message(&self, msg: &bedcode_server_base::wire::BusMessage) -> anyhow::Result<()> {
            self.tx.send(BusMessage {
                topic: msg.topic.clone(),
                sender: msg.sender.clone(),
                payload: msg.payload.clone(),
                payload_binary: msg.payload_binary.clone(),
                timestamp: msg.timestamp,
            })?;
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

    /// 构造测试静态订阅者（wasm-core 形状，MessageBus 直连路径）
    fn test_handler() -> (Box<dyn BusMessageHandler>, Receiver<BusMessage>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (Box::new(TestHandler { tx }), rx)
    }

    /// 构造测试静态订阅者（base 形状，HostBusPort 端口路径；2026-10-08 补）
    fn test_base_handler() -> (Box<dyn bedcode_server_base::ports::BusMessageHandler>, Receiver<BusMessage>) {
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

    // ==================== HostBusPort 装配形态（钉死 vs late-bound） ====================

    /// 可切换的总线槽（模拟「端口先装配、真实总线后就位」的宿主启动顺序）
    fn switchable_bus(initial: Arc<MessageBus>) -> Arc<std::sync::Mutex<Arc<MessageBus>>> {
        Arc::new(std::sync::Mutex::new(initial))
    }

    /// late-bound 正例 + 切换：装配后把解析目标从 A 切到 B，两次 publish 必须各投其当前目标
    ///
    /// 杀死「构造期捕获总线」的写法（那条写法下第二次仍投 A，本用例红）
    #[tokio::test(flavor = "multi_thread")]
    async fn late_bound_bus_port_publishes_to_current_bus_each_call() {
        use bedcode_server_base::ports::BusPort as _;

        let bus_a = Arc::new(MessageBus::new());
        let bus_b = Arc::new(MessageBus::new());
        let (handler_a, rx_a) = test_handler();
        let (handler_b, rx_b) = test_handler();
        bus_a.subscribe_static("sub", "topic:late", handler_a).await;
        bus_b.subscribe_static("sub", "topic:late", handler_b).await;

        let slot = switchable_bus(bus_a.clone());
        let port_slot = slot.clone();
        let port = HostBusPort::late_bound(
            move || port_slot.lock().expect("bus slot lock").clone(),
            test_ws_ports_factory(),
        );

        // 第一次：仍解析到 A
        port.publish("topic:late", "sender-x", serde_json::json!({"phase": 1}));
        let first = wait_delivery(&rx_a, Duration::from_secs(2)).expect("切换前应投递到 A");
        assert_eq!(first.payload, serde_json::json!({"phase": 1}));

        // 宿主完成注册：解析目标切到 B（这就是真实总线就位那一刻）
        *slot.lock().expect("bus slot lock") = bus_b.clone();
        port.publish("topic:late", "sender-x", serde_json::json!({"phase": 2}));

        let second = wait_delivery(&rx_b, Duration::from_secs(2)).expect("切换后应投递到 B");
        assert_eq!(second.payload, serde_json::json!({"phase": 2}));
        assert!(
            wait_delivery(&rx_a, Duration::from_millis(300)).is_err(),
            "切换后的消息不得回投旧总线（否则等于构造期捕获）"
        );
    }

    /// late-bound 反例：静态订阅注册在**调用当时**的总线上，切目标后旧总线收不到订阅
    #[tokio::test(flavor = "multi_thread")]
    async fn late_bound_bus_port_registers_static_subscriber_on_current_bus() {
        use bedcode_server_base::ports::BusPort as _;

        let bus_a = Arc::new(MessageBus::new());
        let bus_b = Arc::new(MessageBus::new());
        let slot = switchable_bus(bus_a.clone());
        let port_slot = slot.clone();
        let port = HostBusPort::late_bound(
            move || port_slot.lock().expect("bus slot lock").clone(),
            test_ws_ports_factory(),
        );

        let (handler, rx) = test_base_handler();
        port.subscribe_static("sub", "topic:sub", handler).await;
        assert_eq!(bus_a.subscriber_count("topic:sub").await, 1, "订阅应落在当时解析到的总线 A 上");

        // 切到 B 后重发一次同名订阅：应叠加在 B 上（A 上仍只有 1 个）
        *slot.lock().expect("bus slot lock") = bus_b.clone();
        let (handler2, rx2) = test_base_handler();
        port.subscribe_static("sub2", "topic:sub", handler2).await;
        assert_eq!(bus_b.subscriber_count("topic:sub").await, 1, "切换后的订阅应落在 B 上");
        assert_eq!(bus_a.subscriber_count("topic:sub").await, 1, "A 上不应出现切换后的订阅");

        // 反向验证：投 B 能收到，A 收不到
        bus_b.publish("topic:sub", "sender-y", serde_json::json!({"to": "b"}));
        let msg = wait_delivery(&rx2, Duration::from_secs(2)).expect("B 上的订阅者应收到消息");
        assert_eq!(msg.payload, serde_json::json!({"to": "b"}));
        assert!(wait_delivery(&rx, Duration::from_millis(300)).is_err(), "A 上的订阅者不应收到投给 B 的消息");
    }

    /// late-bound 二进制面：`publish_binary` 同样走当前解析目标
    #[tokio::test(flavor = "multi_thread")]
    async fn late_bound_bus_port_publish_binary_to_current_bus() {
        use bedcode_server_base::ports::BusPort as _;

        let bus_a = Arc::new(MessageBus::new());
        let bus_b = Arc::new(MessageBus::new());
        let (dispatcher_a, rx_da) = test_dispatcher(&["plugin-b"]);
        bus_a.set_dispatcher(dispatcher_a).await;
        let (dispatcher_b, rx_db) = test_dispatcher(&["plugin-b"]);
        bus_b.set_dispatcher(dispatcher_b).await;
        bus_a.subscribe_wasm_binary("plugin-b", "blob:late").await;
        bus_b.subscribe_wasm_binary("plugin-b", "blob:late").await;

        let slot = switchable_bus(bus_a.clone());
        let port_slot = slot.clone();
        let port = HostBusPort::late_bound(
            move || port_slot.lock().expect("bus slot lock").clone(),
            test_ws_ports_factory(),
        );

        *slot.lock().expect("bus slot lock") = bus_b.clone();
        let bytes: Vec<u8> = vec![0x00, 0xFF, 0x80];
        port.publish_binary("blob:late", "sender-z", bytes.clone());

        let (_plugin, msg) = wait_delivery(&rx_db, Duration::from_secs(2)).expect("二进制应投递到当前总线 B");
        assert_eq!(msg.payload_binary.as_ref(), Some(&bytes), "非 UTF-8 字节必须原样透传到当前总线");
        assert!(wait_delivery(&rx_da, Duration::from_millis(300)).is_err(), "旧总线不应收到二进制消息");
    }

    /// 钉死形态回归：`new()` 仍只投给构造时给定的那条总线（无头 harness / 单测装配面）
    #[tokio::test(flavor = "multi_thread")]
    async fn fixed_bus_port_publishes_only_to_its_own_bus() {
        use bedcode_server_base::ports::BusPort as _;

        let bus_a = Arc::new(MessageBus::new());
        let bus_b = Arc::new(MessageBus::new());
        let (handler_a, rx_a) = test_handler();
        let (handler_b, rx_b) = test_handler();
        bus_a.subscribe_static("sub", "topic:fixed", handler_a).await;
        bus_b.subscribe_static("sub", "topic:fixed", handler_b).await;

        let port = HostBusPort::new(
            bus_a.clone(),
            Arc::new(crate::bus::BusBoundWsPorts::new(bus_a.clone())),
        );
        port.publish("topic:fixed", "sender-x", serde_json::json!({"n": 1}));

        let msg = wait_delivery(&rx_a, Duration::from_secs(2)).expect("钉死形态应投给构造时那条总线");
        assert_eq!(msg.payload, serde_json::json!({"n": 1}));
        assert!(wait_delivery(&rx_b, Duration::from_millis(300)).is_err(), "钉死形态不得投给其他总线");
    }
}
