//! 消息总线域宿主实现（插件间 Topic 发布/订阅）

use crate::plugin::wasm_runtime::WasmHostContext;

/// 发布消息到 Topic（同步投递，总线内部异步派发）
pub(crate) fn bus_publish(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    topic: &str,
    payload_json: &str,
) -> Result<(), String> {
    let payload: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|e| format!("bus error: invalid JSON payload: {}", e))?;

    // 互调门禁（ADR-0017 层 1）：`bedcode.api.<api>` 请求 topic 的目标 api
    // 必须命中某已激活插件的声明清单（注册表只在激活态登记）；
    // `bedcode.api.reply.` 是响应通道（回复 topic 的调用方即为目标），免校验；
    // 普通广播 topic 不校验，保持向后兼容。
    if let Some(api) = topic.strip_prefix("bedcode.api.") {
        if !api.starts_with("reply.") && !host_ctx.api_registry().contains(api) {
            tracing::warn!(
                plugin_id = %plugin_id,
                topic = %topic,
                api = %api,
                "bus_publish: api call to undeclared api rejected (inter-plugin call gate, ADR-0017)"
            );
            return Err(format!(
                "bus error: api '{}' is not declared by any activated plugin (gate)",
                api
            ));
        }
    }

    let bus = host_ctx.message_bus.clone();
    bus.publish(topic, plugin_id, payload);
    Ok(())
}

/// 订阅 topic
///
/// 异步投递订阅请求，避免在 wasm 调用栈内同步等待 subscribers 写锁：
/// bus 派发路径持 subscribers 读锁执行插件回调（on_message / on_session_lifecycle 等），
/// 若插件在这些回调中订阅/退订，同步等待写锁会与派发任务形成同任务重入死锁。
pub(crate) fn bus_subscribe(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    topic: &str,
) -> Result<(), String> {
    let bus = host_ctx.message_bus.clone();
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

/// 取消订阅（与 subscribe 同因异步投递）
pub(crate) fn bus_unsubscribe(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    topic: &str,
) -> Result<(), String> {
    let bus = host_ctx.message_bus.clone();
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
    use crate::plugin::message_bus::{BusMessageHandler, MessageDispatcher};
    use crate::plugin::wasm_runtime::host_impl::tests::build_host_ctx;
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
        let err = bus_publish(&ctx, "plugin-a", "topic", "not-json").unwrap_err();
        assert!(err.contains("invalid JSON payload"), "got: {}", err);
    }

    /// 合法载荷发布：静态订阅者收到完整消息（topic/sender/payload 原样）
    #[tokio::test]
    async fn bus_publish_delivers_to_subscriber() {
        let ctx = build_host_ctx();
        ctx.message_bus
            .set_dispatcher(Arc::new(NoopDispatcher))
            .await;
        let (tx, mut rx) = mpsc::unbounded_channel();
        ctx.message_bus
            .subscribe_static("plugin-b", "greeting", Box::new(ChannelHandler(tx)))
            .await;

        bus_publish(&ctx, "plugin-a", "greeting", r#"{"hello":"world"}"#).expect("publish ok");

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
            &ctx,
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
        let err = bus_publish(
            &ctx,
            "plugin-a",
            "bedcode.api.com.bedcode.scheduler.remove",
            "{}",
        )
        .unwrap_err();
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
        let err = bus_publish(
            &ctx,
            "plugin-a",
            "bedcode.api.com.bedcode.scheduler.add",
            "{}",
        )
        .unwrap_err();
        assert!(err.contains("not declared"), "got: {}", err);
    }

    /// 响应通道（`bedcode.api.reply.`）：免门禁校验（回复的调用方即为目标）
    #[test]
    fn gate_allows_reply_topic() {
        let ctx = build_host_ctx();
        bus_publish(
            &ctx,
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
        bus_publish(&ctx, "plugin-a", "filesrv:peer_changed", "{}")
            .expect("regular topics must bypass gate");
    }

    /// 门禁只校验目标（层 1）：任意已激活插件声明的 api 均可调，不校验调用方身份
    #[test]
    fn gate_layer1_does_not_check_caller() {
        let ctx = build_host_ctx();
        ctx.api_registry()
            .register("com.bedcode.scheduler", &["com.bedcode.scheduler.list".to_string()]);
        bus_publish(
            &ctx,
            "any-plugin",
            "bedcode.api.com.bedcode.scheduler.list",
            "{}",
        )
        .expect("layer 1 gate checks target declaration only");
    }

    /// 总线语义：不投递给发送者自己（同一插件发布+订阅同一 topic）
    #[tokio::test]
    async fn bus_publish_skips_sender() {
        let ctx = build_host_ctx();
        ctx.message_bus
            .set_dispatcher(Arc::new(NoopDispatcher))
            .await;
        let (tx, mut rx) = mpsc::unbounded_channel();
        ctx.message_bus
            .subscribe_static("plugin-a", "echo", Box::new(ChannelHandler(tx)))
            .await;

        bus_publish(&ctx, "plugin-a", "echo", "{}").expect("publish ok");

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
        let err = bus_subscribe(&ctx, "plugin-a", "topic").unwrap_err();
        assert_eq!(err, "bus error: no runtime context");
    }

    /// 无 tokio 运行时上下文：退订请求被拒绝（同上）
    #[test]
    fn bus_unsubscribe_no_runtime_context_rejected() {
        let ctx = build_host_ctx();
        let err = bus_unsubscribe(&ctx, "plugin-a", "topic").unwrap_err();
        assert_eq!(err, "bus error: no runtime context");
    }

    /// 订阅/退订在运行时上下文内异步投递成功（spawn 不报错）
    #[tokio::test]
    async fn bus_subscribe_unsubscribe_inside_runtime_ok() {
        let ctx = build_host_ctx();
        bus_subscribe(&ctx, "plugin-a", "topic").expect("subscribe ok");
        bus_unsubscribe(&ctx, "plugin-a", "topic").expect("unsubscribe ok");
    }
}
