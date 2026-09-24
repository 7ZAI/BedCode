//! Global Event System
//!
//! 项目全局事件顶层抽象 + 统一发布入口
//! 所有模块的事件都应实现 [`AppEvent`]，并经 [`publish`] 进入事件匹配器

use std::fmt::{self, Debug};

use super::matcher::global_matcher;

/// 全局事件顶层 trait：约束 + 统一发送协议
///
/// 项目中所有事件类型都应实现此 trait，并只经 [`publish`] 进入广播面——
/// 事件发送不再有「专用 Sender + 处理器内业务 match」的旁路（会话事件下沉专项）。
pub trait AppEvent: Clone + Send + Sync + Debug {
    /// 触发源设备（WS 同步广播的「排除发起者」语义；非同步通道返回 `None`）
    ///
    /// 只做信封字段取值：返回的是**谁**触发的，不是「这是什么业务动作」。
    fn source_device(&self) -> Option<&str> {
        None
    }

    /// 可广播前置校验；`Err` → [`publish`] 显性失败，不进入广播
    ///
    /// 业务必填字段的自足性是**生产者**的责任（插件在 `broadcast_sync` 前保证），
    /// 实现者在这里只留「事件本身能否成形」的信封级兜底。
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }

    /// 转为出站同步线协议载荷；`None` = 本事件不走 `SyncData` 通道
    ///
    /// **故意不给默认实现**：新增事件类型必须显式回答「走不走同步通道」。
    /// 默认 `None` 会让「事件发了、没人广播、测试全绿」的静默失声成为默认形态。
    fn to_sync_payload(&self) -> Option<bedcode_plugin_api::wire::SyncPayload>;
}

/// [`publish`] 的失败面
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishError {
    /// 事件自校验未通过（载荷不自足，无法折成出站形状）
    Validation(String),
    /// 该事件类型没有注册事件源（装配缺失 / 启动早期）
    NoSource(&'static str),
    /// 事件源通道已关闭
    Channel(String),
}

impl fmt::Display for PublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PublishError::Validation(msg) => write!(f, "publish rejected: {msg}"),
            PublishError::NoSource(event) => {
                write!(f, "publish error: no event source registered for {event}")
            }
            PublishError::Channel(msg) => write!(f, "publish error: {msg}"),
        }
    }
}

impl std::error::Error for PublishError {}

/// 统一发布入口：插件事件与宿主内部事件共用同一条路
///
/// 顺序固定为 **校验 → 查源 → 投递**：
/// - 校验失败即 `Err` 返回调用方，绝不「先投出去再说」（防止空载荷推送）
/// - 无事件源即 `Err`：静默 `Ok` 等于把「线还在、数据永远是空」的断链留给下游
pub async fn publish<E: AppEvent + Clone + 'static>(event: E) -> Result<(), PublishError> {
    if let Err(e) = event.validate() {
        tracing::warn!(event = std::any::type_name::<E>(), error = %e, "[events] 发布前校验未通过，事件未进入广播");
        return Err(PublishError::Validation(e));
    }

    let matcher = global_matcher();
    if !matcher.has_source::<E>().await {
        return Err(PublishError::NoSource(std::any::type_name::<E>()));
    }
    matcher
        .publish(event)
        .await
        .map_err(|e| PublishError::Channel(e.to_string()))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use bedcode_plugin_api::wire::SyncPayload;
    use std::sync::{Arc, Mutex};
    use tokio::sync::broadcast;

    /// 带信封字段的测试事件：一个类型同时覆盖 source_device / validate / to_sync_payload
    #[derive(Debug, Clone, PartialEq)]
    struct ProbeEvent {
        device: String,
        broken: bool,
    }

    impl AppEvent for ProbeEvent {
        fn source_device(&self) -> Option<&str> {
            (!self.device.is_empty()).then(|| self.device.as_str())
        }

        fn validate(&self) -> Result<(), String> {
            if self.broken {
                Err("probe payload incomplete".to_string())
            } else {
                Ok(())
            }
        }

        fn to_sync_payload(&self) -> Option<SyncPayload> {
            if self.broken {
                return None;
            }
            Some(SyncPayload::SessionRemoved {
                session_id: self.device.clone(),
                session_name: String::new(),
            })
        }
    }

    /// trait 的三个方法各按契约作答（正例 + 反例）
    #[test]
    fn probe_event_trait_methods_follow_contract() {
        let ok = ProbeEvent { device: "d1".into(), broken: false };
        assert_eq!(ok.source_device(), Some("d1"));
        assert_eq!(ok.validate(), Ok(()));
        assert!(matches!(
            ok.to_sync_payload(),
            Some(SyncPayload::SessionRemoved { .. })
        ));

        let local = ProbeEvent { device: String::new(), broken: false };
        assert_eq!(local.source_device(), None, "空设备名不构成排除");

        let bad = ProbeEvent { device: "d1".into(), broken: true };
        assert_eq!(bad.validate(), Err("probe payload incomplete".to_string()));
        assert!(bad.to_sync_payload().is_none());
    }

    /// 默认实现：不写这三个方法的事件 = 无源设备、免校验、不走同步通道
    #[test]
    fn trait_defaults_are_envelope_neutral() {
        #[derive(Debug, Clone)]
        struct Plain;
        impl AppEvent for Plain {
            fn to_sync_payload(&self) -> Option<SyncPayload> {
                None
            }
        }
        assert_eq!(Plain.source_device(), None);
        assert_eq!(Plain.validate(), Ok(()));
        assert!(Plain.to_sync_payload().is_none());
    }

    /// 未注册事件源时 `publish` 必须显性失败
    ///
    /// 静默 `Ok` 等于「事件发了、没人收、测试全绿」的断链——与 `EventMatcher::publish`
    /// 的底层语义（无源即丢弃）相反，故统一入口自己把这一格补上。
    #[tokio::test]
    async fn publish_without_source_is_an_error() {
        #[derive(Debug, Clone)]
        struct Orphan;
        impl AppEvent for Orphan {
            fn to_sync_payload(&self) -> Option<SyncPayload> {
                None
            }
        }
        let err = publish(Orphan).await.expect_err("无事件源必须报错");
        assert!(
            matches!(err, PublishError::NoSource(name) if name.contains("Orphan")),
            "错误必须是点名类型名的 NoSource，实际: {err:?}"
        );
    }

    /// 校验失败即拒发：不投进通道，处理器一次都收不到
    #[tokio::test]
    async fn publish_rejects_invalid_event_before_delivery() {
        let (tx, mut rx) = broadcast::channel::<ProbeEvent>(8);
        super::global_matcher().register_source::<ProbeEvent>(tx).await;

        let err = publish(ProbeEvent { device: "d1".into(), broken: true })
            .await
            .expect_err("校验失败必须报错");
        assert!(
            matches!(&err, PublishError::Validation(msg) if msg == "probe payload incomplete"),
            "实际: {err:?}"
        );
        assert_eq!(err.to_string(), "publish rejected: probe payload incomplete");
        assert_eq!(
            rx.try_recv().err(),
            Some(broadcast::error::TryRecvError::Empty),
            "被拒的事件不得进入通道"
        );

        super::global_matcher().unregister_source::<ProbeEvent>().await;
        assert!(!super::global_matcher().has_source::<ProbeEvent>().await);
    }

    /// 正常路径：经统一入口投递，处理器收到同一事件
    ///
    /// 事件类型与 `ProbeEvent` 分开：`global_matcher` 按 TypeId 存事件源，两个用例
    /// 共用一个类型会互相把对方的源摘掉（并发跑即假红）。
    #[tokio::test]
    async fn publish_delivers_to_registered_handler() {
        use crate::events::matcher::EventHandler;

        #[derive(Debug, Clone, PartialEq)]
        struct Delivered(u32);
        impl AppEvent for Delivered {
            fn to_sync_payload(&self) -> Option<SyncPayload> {
                None
            }
        }

        #[derive(Clone)]
        struct Collector(Arc<Mutex<Vec<Delivered>>>);
        impl EventHandler<Delivered> for Collector {
            fn handle(&self, event: Delivered) {
                self.0.lock().unwrap().push(event);
            }
        }

        let (tx, _rx) = broadcast::channel::<Delivered>(8);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let matcher = super::global_matcher();
        matcher.register_source::<Delivered>(tx).await;
        matcher
            .register::<Delivered>(Arc::new(Collector(seen.clone())))
            .await;

        publish(Delivered(7)).await.expect("已注册事件源应投递成功");

        for _ in 0..100 {
            if !seen.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(*seen.lock().unwrap(), vec![Delivered(7)], "处理器应收到同一事件");

        matcher.unregister_handlers::<Delivered>().await;
        matcher.unregister_source::<Delivered>().await;
    }

    /// 错误面自带操作上下文（AGENTS §6 禁止裸 `?` 透传）：三种失败都能被调用方点名定位
    #[test]
    fn publish_errors_are_self_describing() {
        let cases = [
            (
                PublishError::Validation("missing session".into()),
                "publish rejected: missing session",
            ),
            (
                PublishError::NoSource("bedcode_lib::events::HostSyncEvent"),
                "publish error: no event source registered for bedcode_lib::events::HostSyncEvent",
            ),
            (
                PublishError::Channel("broadcast channel closed".into()),
                "publish error: broadcast channel closed",
            ),
        ];
        for (err, expected) in cases {
            assert_eq!(err.to_string(), expected, "{err:?} 的展示文本应可定位失败环节");
        }
    }
}
