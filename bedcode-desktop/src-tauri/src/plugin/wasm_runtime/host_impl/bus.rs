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
