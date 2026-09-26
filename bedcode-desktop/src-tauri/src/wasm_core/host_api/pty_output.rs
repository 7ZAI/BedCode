//! host-pty 输出可用通知（P2：宿主限频唤醒）
//!
//! 会话下沉后宿主环只留游标拉取（ADR 0022 D3），插件侧最细只能靠 `host-timer` 秒级
//! 与前端 50/250 ms 轮询感知新输出。本模块补回「环有新字节」的**限频通知**，
//! 形态与 `pty:exit` 完全同通道（属主私有总线 topic，`<owner>::pty:output`）——
//! **不改 WIT / 不改 ABI**（ADR 0029 §7 的「宿主主动 publish / 限频唤醒」原方案）。
//!
//! 三条不可越过的性质（ADR 0022 D3 的拉取语义 + ADR 0029 §7）：
//!
//! 1. **数据面仍是拉取**：通知只带 `{ ptyId }`（不带字节），数据一律由插件按自己的
//!    游标 `ring-fetch` 取——不是 push 数据面，也没有 per-subscriber 窗口回流；
//! 2. **零背压**：`MessageBus::publish` 内部 spawn 独立投递任务 + 每订阅者有界队列
//!    （满则丢弃计数），写线程只做一次短锁判断，绝不等待消费者；
//! 3. **限频 + 合并**：同一句柄两次通知间隔 ≥ [`OUTPUT_NOTIFY_MIN_INTERVAL_MS`]，
//!    窗口内的产出**不排队**、直接合并丢弃——丢的只是「提前知道」的机会，不是数据。
//!
//! 正确的兜底始终在消费端：事件可丢（无订阅者 / 队列满 / 插件未激活 / 通知本身被
//! 合并），插件前端轮询 + `truncated` resync 仍是唯一正确性保证，本机制只买延迟。
//!
//! 装配形态：写侧装饰器 [`OutputNotifySink`] 包住 `PtyRingSink`（**先落环再通知**），
//! 由 [`super::pty::pty_spawn`] 组装；`pty/` 引擎模块保持零总线依赖。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bedcode_plugin_api::host::bus::owned_topic;

use crate::pty::PtyOutputSink;
use crate::wasm_core::bus::MessageBus;

// ==================== 事件名与限频常量 ====================

/// 输出可用事件名（topic = `<owner>::pty:output`；与 SDK `PTY_OUTPUT` 常量逐字一致，
/// 漂移锁见 `host_api/tests/pty.rs::output_event_name_matches_sdk_subscription_helper`）
pub(crate) const EVENT_OUTPUT: &str = "pty:output";

/// 通知限频间隔（ms）：同一句柄两次通知的最小间隔
///
/// 取值与插件前端快档轮询（`OUTPUT_PULL_INTERVAL_MS = 50`）同量级：连续产出期通知
/// 密度不超过既有轮询密度（不额外放大 invoke 频率）；而**空闲后的首个字节**因窗口
/// 早已过期而立即发布——这正是本机制要买到的延迟（慢档 250 ms → ≈0）。
pub(crate) const OUTPUT_NOTIFY_MIN_INTERVAL_MS: u64 = 50;

// ==================== 限频裁决（纯函数） ====================

/// 限频裁决：距上次通知不足 `min_interval` → 丢弃本次（合并）
///
/// 纯函数（时间由调用方注入），native 可测边界：首次必定放行；恰好等于间隔放行；
/// 间隔内丢弃；时钟回拨（`now < last`）按「间隔未到」处理（饱和差）。
fn allow_notify(last: Option<Instant>, now: Instant, min_interval: Duration) -> bool {
    match last {
        Some(last) => now.saturating_duration_since(last) >= min_interval,
        None => true,
    }
}

// ==================== 通知汇（写侧装饰器） ====================

/// 输出汇装饰器：先把字节落内层环，再按限频发布 `<owner>::pty:output`
///
/// 单一生产者（该句柄的读线程消费任务）调用，故限频状态只需一把短锁；
/// 锁中毒取回内部值继续（与 `PtyRingSink` 同口径：不因一次 panic 打断产出链）。
pub(crate) struct OutputNotifySink {
    inner: Arc<dyn PtyOutputSink>,
    /// 属主插件 ID（topic 命名空间 = 定向投递的唯一凭据）
    owner: String,
    /// 句柄（通知载荷；消费端据此反查会话）
    pty_id: String,
    bus: Arc<MessageBus>,
    min_interval: Duration,
    /// 上次通知时刻
    last_notified: Mutex<Option<Instant>>,
}

impl OutputNotifySink {
    /// 生产装配：限频取 [`OUTPUT_NOTIFY_MIN_INTERVAL_MS`]
    pub(crate) fn new(inner: Arc<dyn PtyOutputSink>, owner: String, pty_id: String, bus: Arc<MessageBus>) -> Self {
        Self::with_min_interval(
            inner,
            owner,
            pty_id,
            bus,
            Duration::from_millis(OUTPUT_NOTIFY_MIN_INTERVAL_MS),
        )
    }

    /// 可注入间隔的装配（单测用：避免真实 sleep 的时序脆弱用例）
    pub(crate) fn with_min_interval(
        inner: Arc<dyn PtyOutputSink>,
        owner: String,
        pty_id: String,
        bus: Arc<MessageBus>,
        min_interval: Duration,
    ) -> Self {
        Self {
            inner,
            owner,
            pty_id,
            bus,
            min_interval,
            last_notified: Mutex::new(None),
        }
    }

    /// 发布一次可用通知（非阻塞：总线内部 spawn 投递任务；无 runtime 上下文时丢弃 + warn）
    fn notify_if_due(&self) {
        let now = Instant::now();
        // 限频裁决在锁内完成（写侧单一生产者，短锁）：窗口内（含时钟回拨）不发布、
        // 也不推进 last——合并的只是「提前知道」的机会，数据字节仍正常落环。
        let due = {
            let mut last = self.last_notified.lock().unwrap_or_else(|e| e.into_inner());
            if allow_notify(*last, now, self.min_interval) {
                *last = Some(now);
                true
            } else {
                false
            }
        };
        if !due {
            return;
        }
        // 锁外发布：投递任务与本句柄后续产出互不阻塞
        self.bus.publish(
            &owned_topic(&self.owner, EVENT_OUTPUT),
            "host",
            serde_json::json!({ "ptyId": self.pty_id }),
        );
    }
}

#[async_trait]
impl PtyOutputSink for OutputNotifySink {
    async fn on_bytes(&self, bytes: Vec<u8>, timestamp_ms: i64) {
        let has_bytes = !bytes.is_empty();
        // 先落环：通知绝不先于数据可见（消费端收到即拉必能拉到东西，除非已被淘汰）
        self.inner.on_bytes(bytes, timestamp_ms).await;
        if has_bytes {
            self.notify_if_due();
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::bus::BusMessageHandler;
    use std::sync::mpsc::{Receiver, TryRecvError};

    /// 计数汇（测试替身）：只记投递到的字节块，不涉及环
    struct CountingSink {
        chunks: Mutex<Vec<Vec<u8>>>,
    }

    impl CountingSink {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                chunks: Mutex::new(Vec::new()),
            })
        }

        fn chunks(&self) -> Vec<Vec<u8>> {
            self.chunks.lock().unwrap_or_else(|e| e.into_inner()).clone()
        }
    }

    #[async_trait]
    impl PtyOutputSink for CountingSink {
        async fn on_bytes(&self, bytes: Vec<u8>, _timestamp_ms: i64) {
            self.chunks.lock().unwrap_or_else(|e| e.into_inner()).push(bytes);
        }
    }

    /// 记录总线投递（topic / sender / payload），形态沿用 host_api/tests/pty.rs 的订阅替身
    struct NotifyRecorder {
        tx: std::sync::mpsc::Sender<serde_json::Value>,
    }

    impl BusMessageHandler for NotifyRecorder {
        fn on_message(&self, msg: &bedcode_plugin_api::BusMessage) -> anyhow::Result<()> {
            let _ = self.tx.send(serde_json::json!({
                "topic": msg.topic,
                "sender": msg.sender,
                "payload": msg.payload,
            }));
            Ok(())
        }
    }

    /// 在宿主总线上静态订阅一个 topic（等价插件 activate 期的 `bus_subscribe`）
    async fn subscribe(bus: &Arc<MessageBus>, topic: &str) -> Receiver<serde_json::Value> {
        let (tx, rx) = std::sync::mpsc::channel();
        bus.subscribe_static("com.bedcode.notify-test", topic, Box::new(NotifyRecorder { tx }))
            .await;
        rx
    }

    /// 等一条投递（投递任务异步，超时返回 None）
    async fn wait_event(rx: &Receiver<serde_json::Value>) -> Option<serde_json::Value> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match rx.try_recv() {
                Ok(event) => return Some(event),
                Err(TryRecvError::Disconnected) => return None,
                Err(TryRecvError::Empty) if Instant::now() >= deadline => return None,
                Err(TryRecvError::Empty) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    }

    // ==================== 限频裁决边界 ====================

    /// 首次通知（无历史）必定放行
    #[test]
    fn allow_notify_first_call_passes() {
        let now = Instant::now();
        assert!(allow_notify(None, now, Duration::from_millis(50)));
    }

    /// 间隔内 → 丢弃（合并语义：不排队）
    #[test]
    fn allow_notify_within_interval_is_dropped() {
        let now = Instant::now();
        let last = now.checked_sub(Duration::from_millis(49)).expect("instant 可回退");
        assert!(!allow_notify(Some(last), now, Duration::from_millis(50)));
    }

    /// 恰好等于间隔 → 放行（`>=` 边界，别写成 `>`）
    #[test]
    fn allow_notify_at_interval_boundary_passes() {
        let now = Instant::now();
        let last = now.checked_sub(Duration::from_millis(50)).expect("instant 可回退");
        assert!(allow_notify(Some(last), now, Duration::from_millis(50)));
    }

    /// 超过间隔 → 放行（空闲后的首个字节即此路径：立即通知）
    #[test]
    fn allow_notify_after_interval_passes() {
        let now = Instant::now();
        let last = now.checked_sub(Duration::from_secs(5)).expect("instant 可回退");
        assert!(allow_notify(Some(last), now, Duration::from_millis(50)));
    }

    /// 时钟回拨（`now < last`）→ 按间隔未到处理（饱和差，不 panic）
    #[test]
    fn allow_notify_with_clock_going_backwards_is_dropped() {
        let now = Instant::now();
        let last = now + Duration::from_secs(1);
        assert!(!allow_notify(Some(last), now, Duration::from_millis(50)));
    }

    // ==================== 装饰器行为（真实总线投递） ====================

    /// 限频生效：窗口内多次产出只发一条通知，且**字节一个不少地转发给内层汇**
    #[tokio::test(flavor = "multi_thread")]
    async fn notify_is_rate_limited_while_bytes_are_forwarded() {
        let bus = Arc::new(MessageBus::new());
        let topic = owned_topic("com.bedcode.owner", EVENT_OUTPUT);
        let rx = subscribe(&bus, &topic).await;

        let inner = CountingSink::new();
        // 显式标注 trait 对象：只在 let 绑定处做 unsize 转换，参数位不再二次推断
        let inner_sink: Arc<dyn PtyOutputSink> = inner.clone();
        // 间隔取 1 小时：窗口内 5 次产出必须只通知 1 次（确定性，不依赖 sleep）
        let sink = OutputNotifySink::with_min_interval(
            inner_sink,
            "com.bedcode.owner".to_string(),
            "pty-1".to_string(),
            Arc::clone(&bus),
            Duration::from_secs(3600),
        );
        for tag in 0u8..5 {
            sink.on_bytes(vec![tag; 4], 1000).await;
        }

        let event = wait_event(&rx).await.expect("首次产出必须通知");
        assert_eq!(event["payload"]["ptyId"], "pty-1");
        assert!(rx.try_recv().is_err(), "限频窗口内的其余 4 次产出不得再发通知");
        assert_eq!(
            inner.chunks(),
            (0u8..5).map(|tag| vec![tag; 4]).collect::<Vec<_>>(),
            "通知限频不得影响字节转发（先落环，再通知）"
        );
    }

    /// 通知的 topic / sender / 载荷形状（消费端按 SDK 助手订阅即命中）
    #[tokio::test(flavor = "multi_thread")]
    async fn notify_topic_and_payload_are_owner_scoped() {
        let bus = Arc::new(MessageBus::new());
        let owner = "com.bedcode.owner-x";
        // 订阅侧刻意走 SDK 助手：宿主发布形状若与插件订阅形状分叉即收不到
        let topic = bedcode_plugin_api::host::pty_event_topic(bedcode_plugin_api::host::PTY_OUTPUT, owner);
        let rx = subscribe(&bus, &topic).await;

        let sink = OutputNotifySink::new(
            CountingSink::new(),
            owner.to_string(),
            "pty-9".to_string(),
            Arc::clone(&bus),
        );
        sink.on_bytes(b"x".to_vec(), 1000).await;

        let event = wait_event(&rx).await.expect("产出必须通知");
        assert_eq!(event["topic"], topic, "topic 形状 = <owner>::pty:output");
        assert_eq!(event["sender"], "host");
        assert_eq!(
            event["payload"],
            serde_json::json!({ "ptyId": "pty-9" }),
            "载荷只带句柄（字节仍由 ring-fetch 拉取）"
        );
    }

    /// 空投递不通知（环本身也忽略空段：无新字节可拉，通知只会让消费端白跑一轮）
    #[tokio::test(flavor = "multi_thread")]
    async fn empty_push_does_not_notify() {
        let bus = Arc::new(MessageBus::new());
        let topic =
            bedcode_plugin_api::host::pty_event_topic(bedcode_plugin_api::host::PTY_OUTPUT, "com.bedcode.owner-empty");
        let rx = subscribe(&bus, &topic).await;

        let sink = OutputNotifySink::new(
            CountingSink::new(),
            "com.bedcode.owner-empty".to_string(),
            "pty-e".to_string(),
            Arc::clone(&bus),
        );
        sink.on_bytes(Vec::new(), 1000).await;
        // 真产出一次作为哨兵：投递通道可用时，只有哨兵那一条能到
        sink.on_bytes(b"y".to_vec(), 1001).await;

        let event = wait_event(&rx).await.expect("哨兵产出必须通知");
        assert_eq!(event["payload"]["ptyId"], "pty-e");
        assert!(rx.try_recv().is_err(), "空投递不得产生第二条通知");
    }
}
