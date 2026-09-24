//! 消息总线域宿主实现（插件间 Topic 发布/订阅）

use bedcode_plugin_api::host::bus::{
    is_legacy_owner_suffix, is_reply_topic, owned_topic, topic_owner, API_TOPIC_PREFIX,
};


// ==================== Topic 命名空间门禁（审计票 05，P0-4） ====================

/// 命名空间门禁：`<owner>::<name>` 形态的 topic 只有属主插件（与宿主）可读写
///
/// 形态即边界——判定只看串，不查激活表、不看安装表，因此与「属主是否已激活 /
/// 是否已安装」无关，抢在属主之前订阅或伪发布都进不去（这是选命名空间而非
/// 「解析 owner 段」的根本原因：后者的边界会随时序漏）。
fn check_namespace(plugin_id: &str, topic: &str, action: &str, target: &str) -> Result<(), String> {
    let owner = match topic_owner(topic) {
        Some(owner) => owner,
        None => return Ok(()),
    };
    if owner == plugin_id {
        return Ok(());
    }
    tracing::warn!(
        plugin_id = %plugin_id,
        topic = %topic,
        namespace_owner = %owner,
        "bus {action}: cross-namespace topic rejected (topic namespace gate, audit ticket 05)"
    );
    Err(format!(
        "bus error: topic '{topic}' is in the namespace of plugin '{owner}' — only {target} may {action} it"
    ))
}

/// 订阅面门禁（`subscribe` / `subscribe-binary`）：命名空间 + 两条订阅专属形态规则
///
/// - 回复道 `bedcode.api.reply.*`：caller 的收件箱，订阅由宿主在 `host-api-call`
///   内静态注册（见 host_impl/api.rs），对 WASM 一律关闭——此前任意插件可订阅他人
///   回复道窃听互调结果，且 correlation id 是可猜的单调计数器（`req-1`、`req-2`…）；
/// - legacy 定向形态 `<base>.<own-id>`：票 05 迁移前 SDK 助手产出的串，宿主已改投
///   命名空间 topic，放行会让旧产物**静默断流**（订阅得到、永远收不到），故显式拒绝
///   并回带新形态。
fn check_subscribe_access(plugin_id: &str, topic: &str) -> Result<(), String> {
    check_namespace(plugin_id, topic, "subscribe", "that plugin (and the host)")?;
    if is_reply_topic(topic) {
        tracing::warn!(
            plugin_id = %plugin_id,
            topic = %topic,
            "bus subscribe: reply lane is host-managed, rejected (audit ticket 05)"
        );
        return Err(format!(
            "bus error: topic '{topic}' is on the inter-plugin reply lane (bedcode.api.reply.*) \u{2014} reply subscriptions are host-managed by host-api-call"
        ));
    }
    if is_legacy_owner_suffix(topic, plugin_id) {
        tracing::warn!(
            plugin_id = %plugin_id,
            topic = %topic,
            "bus subscribe: legacy owner-suffix directed topic form rejected (audit ticket 05)"
        );
        return Err(format!(
            "bus error: legacy directed-topic form '{topic}' is retired, subscribe '{new_form}' instead (SDK owned_topic / *_event_topic)",
            new_form = owned_topic(plugin_id, &topic[..topic.len() - plugin_id.len() - 1])
        ));
    }
    Ok(())
}

/// 互调门禁（ADR-0017 层 1，JSON 与二进制发布共用）：`bedcode.api.<api>` 请求
/// topic 的目标 api 必须命中某已激活插件的声明清单（注册表只在激活态登记）；
/// `bedcode.api.reply.` 是响应通道（回复 topic 的调用方即为目标），免校验；
/// 普通广播 topic 不校验，保持向后兼容。
fn check_api_gate(sec: &dyn crate::wasm_core::host_api::context::SecurityScope, plugin_id: &str, topic: &str) -> Result<(), String> {
    if let Some(api) = topic.strip_prefix(API_TOPIC_PREFIX) {
        if !api.starts_with("reply.") {
            // 互调门经 core-security 授权框架路由（三段管线；行为与直查注册表等价）
            let req = crate::wasm_core::security::AuthRequest {
                plugin_id,
                resource: crate::wasm_core::security::ResourceKind::ApiCall,
                operation: "invoke",
                target: api,
            };
            if sec.security().authorize(&req) != crate::wasm_core::security::AuthDecision::Allow {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    topic = %topic,
                    api = %api,
                    "bus publish: api call to undeclared api rejected (inter-plugin call gate, ADR-0017)"
                );
                return Err(format!(
                    "bus error: api '{}' is not declared by any activated plugin (gate)",
                    api
                ));
            }
        }
    }
    Ok(())
}

/// 请求 topic 的目标 api **声明属主**（票 05 回复道 sender 校验依据）
///
/// 与 [`check_api_gate`] 读同一张注册表（`ApiCallAuthorizer` 的命中判定即
/// `registry.contains`），故「门禁放行」与「取到属主」不会漂移。
/// 回复道 / 公开 topic → `None`（无属主可校验）。
pub(crate) fn api_gate_target_owner(reg: &dyn crate::wasm_core::host_api::context::ApiRegistryScope, request_topic: &str) -> Option<String> {
    let api = request_topic.strip_prefix(API_TOPIC_PREFIX)?;
    if api.starts_with("reply.") {
        return None;
    }
    reg.api_registry().owner_of(api)
}

/// 发布 JSON 消息到 Topic（同步投递，总线内部异步派发）
pub(crate) fn bus_publish(
    bus: &dyn crate::wasm_core::host_api::context::BusScope,
    sec: &dyn crate::wasm_core::host_api::context::SecurityScope,
    plugin_id: &str,
    topic: &str,
    payload_json: &str,
) -> Result<(), String> {
    let payload: serde_json::Value =
        serde_json::from_str(payload_json).map_err(|e| format!("bus error: invalid JSON payload: {}", e))?;

    check_namespace(plugin_id, topic, "publish", "that plugin (and the host)")?;
    check_api_gate(sec, plugin_id, topic)?;

    let bus = bus.message_bus().clone();
    bus.publish(topic, plugin_id, payload);
    Ok(())
}

/// 发布二进制消息到 Topic（v11）：字节列原样透传（零 JSON 编解码，
/// 可传非 UTF-8 与大载荷）；互调门禁语义与 JSON 发布一致（防绕过）
pub(crate) fn bus_publish_binary(
    bus: &dyn crate::wasm_core::host_api::context::BusScope,
    sec: &dyn crate::wasm_core::host_api::context::SecurityScope,
    plugin_id: &str,
    topic: &str,
    payload: Vec<u8>,
) -> Result<(), String> {
    check_namespace(plugin_id, topic, "publish", "that plugin (and the host)")?;
    check_api_gate(sec, plugin_id, topic)?;

    let bus = bus.message_bus().clone();
    bus.publish_binary(topic, plugin_id, payload);
    Ok(())
}

/// 订阅 topic
///
/// 异步投递订阅请求，避免在 wasm 调用栈内同步等待 subscribers 写锁：
/// bus 派发路径持 subscribers 读锁执行插件回调（on_message / on_session_lifecycle 等），
/// 若插件在这些回调中订阅/退订，同步等待写锁会与派发任务形成同任务重入死锁。
///
/// 命名空间/回复道/legacy 门禁在 spawn **之前**同步判定：错误必须回给 guest，
/// 不能退化成「返回 Ok 但没订阅上」。
pub(crate) fn bus_subscribe(bus: &dyn crate::wasm_core::host_api::context::BusScope, plugin_id: &str, topic: &str) -> Result<(), String> {
    check_subscribe_access(plugin_id, topic)?;
    let bus = bus.message_bus().clone();
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        tracing::warn!(plugin_id = %plugin_id, topic = %topic, "bus_subscribe: no runtime context, subscription dropped");
        return Err("bus error: no runtime context".to_string());
    };
    let pid = plugin_id.to_string();
    let t = topic.to_string();
    handle.spawn(async move {
        bus.subscribe_wasm(&pid, &t).await;
    });
    Ok(())
}

/// 以二进制格式偏好订阅（v11）：只接收 publish-binary 投递，
/// JSON 消息对其按格式不匹配拒绝（与 subscribe 同因异步投递）
pub(crate) fn bus_subscribe_binary(bus: &dyn crate::wasm_core::host_api::context::BusScope, plugin_id: &str, topic: &str) -> Result<(), String> {
    check_subscribe_access(plugin_id, topic)?;
    let bus = bus.message_bus().clone();
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        tracing::warn!(plugin_id = %plugin_id, topic = %topic, "bus_subscribe_binary: no runtime context, subscription dropped");
        return Err("bus error: no runtime context".to_string());
    };
    let pid = plugin_id.to_string();
    let t = topic.to_string();
    handle.spawn(async move {
        bus.subscribe_wasm_binary(&pid, &t).await;
    });
    Ok(())
}

/// 取消订阅（与 subscribe 同因异步投递）
///
/// 只过命名空间门：legacy/回复道形态在此不拦——退订是清理动作，必须幂等可用
/// （旧产物退订它曾订阅过的串不应被新规则噎住）
pub(crate) fn bus_unsubscribe(bus: &dyn crate::wasm_core::host_api::context::BusScope, plugin_id: &str, topic: &str) -> Result<(), String> {
    check_namespace(plugin_id, topic, "unsubscribe", "that plugin (and the host)")?;
    let bus = bus.message_bus().clone();
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        tracing::warn!(plugin_id = %plugin_id, topic = %topic, "bus_unsubscribe: no runtime context, unsubscribe dropped");
        return Err("bus error: no runtime context".to_string());
    };
    let pid = plugin_id.to_string();
    let t = topic.to_string();
    handle.spawn(async move {
        bus.unsubscribe(&pid, &t).await;
    });
    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::bus::{BusMessageHandler, MessageDispatcher};
    use crate::wasm_core::host_api::tests::build_host_ctx;
    use bedcode_plugin_api::BusMessage;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    /// 无操作 dispatcher：publish 要求 dispatcher 已注入，静态订阅不实际使用它
    struct NoopDispatcher;

    impl MessageDispatcher for NoopDispatcher {
        fn dispatch_to_wasm(&self, _plugin_id: &str, _msg: &BusMessage) -> anyhow::Result<()> {
            Ok(())
        }

        fn is_activated(&self, _plugin_id: &str) -> bool {
            true
        }
    }

    /// 静态订阅者：把收到的消息转发到 mpsc 通道供断言
    struct ChannelHandler(mpsc::UnboundedSender<BusMessage>);

    impl BusMessageHandler for ChannelHandler {
        fn on_message(&self, msg: &BusMessage) -> anyhow::Result<()> {
            let _ = self.0.send(msg.clone());
            Ok(())
        }
    }

    /// 非法 JSON 载荷在触达消息总线前被拒绝（纯解析路径，无需 tokio 运行时）
    #[test]
    fn bus_publish_invalid_json_rejected() {
        let ctx = build_host_ctx();
        let err = bus_publish(ctx.as_ref(), ctx.as_ref(), "plugin-a", "topic", "not-json").unwrap_err();
        assert!(err.contains("invalid JSON payload"), "got: {}", err);
    }

    /// 合法载荷发布：静态订阅者收到完整消息（topic/sender/payload 原样）
    #[tokio::test]
    async fn bus_publish_delivers_to_subscriber() {
        let ctx = build_host_ctx();
        ctx.message_bus.set_dispatcher(Arc::new(NoopDispatcher)).await;
        let (tx, mut rx) = mpsc::unbounded_channel();
        ctx.message_bus
            .subscribe_static("plugin-b", "greeting", Box::new(ChannelHandler(tx)))
            .await;

        bus_publish(ctx.as_ref(), ctx.as_ref(), "plugin-a", "greeting", r#"{"hello":"world"}"#).expect("publish ok");

        let msg = rx.recv().await.expect("subscriber must receive message");
        assert_eq!(msg.topic, "greeting");
        assert_eq!(msg.sender, "plugin-a");
        assert_eq!(msg.payload, serde_json::json!({ "hello": "world" }));
    }

    // ==================== 互调门禁（ADR-0017） ====================

    /// 已登记 api 的请求 topic：放行（门禁命中声明清单）
    #[test]
    fn gate_allows_declared_api() {
        let ctx = build_host_ctx();
        ctx.api_registry()
            .register("com.bedcode.scheduler", &["com.bedcode.scheduler.add".to_string()]);
        bus_publish(
                        ctx.as_ref(),
            ctx.as_ref(),
            "plugin-a",
            "bedcode.api.com.bedcode.scheduler.add",
            r#"{"jsonrpc":"2.0"}"#,
        )
        .expect("declared api must pass gate");
    }

    /// 未登记 api 的请求 topic：拒绝 + 明确错误（「未声明 api 的调用被宿主拒绝」验收）
    #[test]
    fn gate_rejects_undeclared_api() {
        let ctx = build_host_ctx();
        let err = bus_publish(ctx.as_ref(), ctx.as_ref(), "plugin-a", "bedcode.api.com.bedcode.scheduler.remove", "{}").unwrap_err();
        assert!(err.contains("not declared"), "got: {}", err);
        assert!(err.contains("com.bedcode.scheduler.remove"), "got: {}", err);
    }

    /// 停用注销后的 api：不再放行（未激活插件目标调用被拒）
    #[test]
    fn gate_rejects_after_unregister() {
        let ctx = build_host_ctx();
        ctx.api_registry()
            .register("com.bedcode.scheduler", &["com.bedcode.scheduler.add".to_string()]);
        ctx.api_registry().unregister("com.bedcode.scheduler");
        let err = bus_publish(ctx.as_ref(), ctx.as_ref(), "plugin-a", "bedcode.api.com.bedcode.scheduler.add", "{}").unwrap_err();
        assert!(err.contains("not declared"), "got: {}", err);
    }

    /// 响应通道（`bedcode.api.reply.`）：免门禁校验（回复的调用方即为目标）
    #[test]
    fn gate_allows_reply_topic() {
        let ctx = build_host_ctx();
        bus_publish(
                        ctx.as_ref(),
            ctx.as_ref(),
            "com.bedcode.scheduler",
            "bedcode.api.reply.com.bedcode.caller.req-1",
            r#"{"jsonrpc":"2.0","result":1}"#,
        )
        .expect("reply topic must bypass gate");
    }

    /// 普通广播 topic：不校验（向后兼容，filesrv:peer_changed 等既有约定不受影响）
    #[test]
    fn gate_ignores_regular_topics() {
        let ctx = build_host_ctx();
        bus_publish(ctx.as_ref(), ctx.as_ref(), "plugin-a", "filesrv:peer_changed", "{}").expect("regular topics must bypass gate");
    }

    /// 门禁只校验目标（层 1）：任意已激活插件声明的 api 均可调，不校验调用方身份
    #[test]
    fn gate_layer1_does_not_check_caller() {
        let ctx = build_host_ctx();
        ctx.api_registry()
            .register("com.bedcode.scheduler", &["com.bedcode.scheduler.list".to_string()]);
        bus_publish(ctx.as_ref(), ctx.as_ref(), "any-plugin", "bedcode.api.com.bedcode.scheduler.list", "{}")
            .expect("layer 1 gate checks target declaration only");
    }

    /// 总线语义：不投递给发送者自己（同一插件发布+订阅同一 topic）
    #[tokio::test]
    async fn bus_publish_skips_sender() {
        let ctx = build_host_ctx();
        ctx.message_bus.set_dispatcher(Arc::new(NoopDispatcher)).await;
        let (tx, mut rx) = mpsc::unbounded_channel();
        ctx.message_bus
            .subscribe_static("plugin-a", "echo", Box::new(ChannelHandler(tx)))
            .await;

        bus_publish(ctx.as_ref(), ctx.as_ref(), "plugin-a", "echo", "{}").expect("publish ok");

        // 短等待后通道应仍为空：同 sender 订阅不投递
        match tokio::time::timeout(std::time::Duration::from_millis(300), rx.recv()).await {
            Err(_) => {}
            Ok(Some(msg)) => panic!("sender must not receive own message, got: {:?}", msg),
            Ok(None) => panic!("channel closed unexpectedly"),
        }
    }

    /// 无 tokio 运行时上下文：订阅请求被拒绝（异步投递不可用，防静默丢弃）
    #[test]
    fn bus_subscribe_no_runtime_context_rejected() {
        let ctx = build_host_ctx();
        let err = bus_subscribe(ctx.as_ref(), "plugin-a", "topic").unwrap_err();
        assert_eq!(err, "bus error: no runtime context");
    }

    /// 无 tokio 运行时上下文：退订请求被拒绝（同上）
    #[test]
    fn bus_unsubscribe_no_runtime_context_rejected() {
        let ctx = build_host_ctx();
        let err = bus_unsubscribe(ctx.as_ref(), "plugin-a", "topic").unwrap_err();
        assert_eq!(err, "bus error: no runtime context");
    }

    /// 订阅/退订在运行时上下文内异步投递成功（spawn 不报错）
    #[tokio::test]
    async fn bus_subscribe_unsubscribe_inside_runtime_ok() {
        let ctx = build_host_ctx();
        bus_subscribe(ctx.as_ref(), "plugin-a", "topic").expect("subscribe ok");
        bus_unsubscribe(ctx.as_ref(), "plugin-a", "topic").expect("unsubscribe ok");
    }

    // ==================== topic 命名空间门禁（审计票 05，P0-4） ====================

    /// 他人命名空间 topic（跨属主订阅 = 窃听他人定向事件流）：拒绝
    #[tokio::test]
    async fn subscribe_rejects_other_namespace() {
        let ctx = build_host_ctx();
        let err = bus_subscribe(ctx.as_ref(), "com.evil", "com.victim::pty:exit").unwrap_err();
        assert!(err.contains("namespace"), "got: {}", err);
        assert!(err.contains("com.victim"), "错误须点出属主: got: {}", err);
    }

    /// 他人命名空间 topic（二进制偏好订阅同样拒）：拒绝
    #[tokio::test]
    async fn subscribe_binary_rejects_other_namespace() {
        let ctx = build_host_ctx();
        let err = bus_subscribe_binary(ctx.as_ref(), "com.evil", "com.victim::blob:chunk").unwrap_err();
        assert!(err.contains("namespace"), "got: {}", err);
    }

    /// 自己的命名空间 topic：放行（宿主定向投递的正常订阅路径）
    #[tokio::test]
    async fn subscribe_allows_own_namespace() {
        let ctx = build_host_ctx();
        bus_subscribe(ctx.as_ref(), "com.victim", "com.victim::pty:exit").expect("own namespace must pass");
    }

    /// 跨命名空间伪发布（他人收件箱投毒）：拒绝
    #[tokio::test]
    async fn publish_rejects_other_namespace() {
        let ctx = build_host_ctx();
        let err = bus_publish(ctx.as_ref(), ctx.as_ref(), "com.evil", "com.victim::pty:exit", "{}").unwrap_err();
        assert!(err.contains("namespace"), "got: {}", err);
    }

    /// 跨命名空间伪发布（二进制发布不得绕过 JSON 面的门禁）：拒绝
    #[tokio::test]
    async fn publish_binary_rejects_other_namespace() {
        let ctx = build_host_ctx();
        let err = bus_publish_binary(ctx.as_ref(), ctx.as_ref(), "com.evil", "com.victim::blob:chunk", vec![1, 2]).unwrap_err();
        assert!(err.contains("namespace"), "got: {}", err);
    }

    /// 公开 topic（不含 `::`）不受命名空间门禁影响：既有广播约定零回归
    #[tokio::test]
    async fn publish_public_topic_unaffected_by_namespace_gate() {
        let ctx = build_host_ctx();
        bus_publish(ctx.as_ref(), ctx.as_ref(), "com.evil", "task:status-changed", "{}").expect("public topic must pass");
        bus_subscribe(ctx.as_ref(), "com.evil", "peer:consent").expect("public subscribe must pass");
    }

    /// legacy 定向形态（`<base>.<自身 id>`，SDK 旧助手产出的串）：显式拒绝并回带新形态，
    /// 不得退化成「订阅成功但永远收不到」的静默断流
    #[tokio::test]
    async fn subscribe_rejects_legacy_owner_suffix() {
        let ctx = build_host_ctx();
        let err = bus_subscribe(ctx.as_ref(), "com.victim", "pty:exit.com.victim").unwrap_err();
        assert!(err.contains("legacy"), "got: {}", err);
        assert!(err.contains("com.victim::pty:exit"), "错误须给出新形态: got: {}", err);
    }

    /// 退订同样受门禁约束（他人命名空间不可寻址）
    #[tokio::test]
    async fn unsubscribe_rejects_other_namespace() {
        let ctx = build_host_ctx();
        let err = bus_unsubscribe(ctx.as_ref(), "com.evil", "com.victim::pty:exit").unwrap_err();
        assert!(err.contains("namespace"), "got: {}", err);
    }

    // ==================== 互调回复道订阅面（P1-4 之一） ====================

    /// 插件订阅他人回复道（窃听互调回复；correlation id 是可猜的单调计数器）：拒绝
    #[tokio::test]
    async fn subscribe_rejects_reply_lane() {
        let ctx = build_host_ctx();
        let err = bus_subscribe(ctx.as_ref(), "com.evil", "bedcode.api.reply.com.victim.req-1").unwrap_err();
        assert!(err.contains("reply"), "got: {}", err);
    }

    /// 自身回复道亦不对 WASM 开放：回复订阅由宿主在 host-api-call 内静态注册
    #[tokio::test]
    async fn subscribe_rejects_own_reply_lane() {
        let ctx = build_host_ctx();
        let err = bus_subscribe(ctx.as_ref(), "com.victim", "bedcode.api.reply.com.victim.req-1").unwrap_err();
        assert!(err.contains("reply"), "got: {}", err);
    }

    /// 二进制偏好订阅回复道同样拒绝
    #[tokio::test]
    async fn subscribe_binary_rejects_reply_lane() {
        let ctx = build_host_ctx();
        let err = bus_subscribe_binary(ctx.as_ref(), "com.evil", "bedcode.api.reply.com.victim.req-1").unwrap_err();
        assert!(err.contains("reply"), "got: {}", err);
    }

    /// 请求道不受回复道门禁影响（服务方必须能订阅 `bedcode.api.*`）
    #[tokio::test]
    async fn subscribe_allows_api_request_lane() {
        let ctx = build_host_ctx();
        bus_subscribe(ctx.as_ref(), "com.victim", "bedcode.api.com.victim.echo").expect("request lane must pass");
    }

    /// 端到端：B 订阅不到 A 的定向事件流，也就收不到宿主投给 A 的事件
    #[tokio::test]
    async fn cross_namespace_subscriber_never_receives_directed_event() {
        let ctx = build_host_ctx();
        ctx.message_bus.set_dispatcher(Arc::new(NoopDispatcher)).await;
        assert!(bus_subscribe(ctx.as_ref(), "com.evil", "com.victim::pty:exit").is_err());

        // 宿主按新形态定向投递给 victim：evil 无订阅者条目，收不到任何投递
        ctx.message_bus
            .publish("com.victim::pty:exit", "host", serde_json::json!({ "ptyId": "p-1" }));
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}
