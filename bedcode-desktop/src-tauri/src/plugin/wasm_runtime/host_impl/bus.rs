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
