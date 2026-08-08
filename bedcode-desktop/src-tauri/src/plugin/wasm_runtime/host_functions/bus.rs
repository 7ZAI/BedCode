//! 消息总线域 Host Functions（插件间 Topic 发布/订阅）

use super::memory::read_wasm_string_consume;
use crate::plugin::wasm_runtime::{WasmHostContext, WasmPluginState};

// ==================== 逻辑层（core 胶水与 Component Model 绑定共用） ====================

/// 逻辑层：发布消息到 Topic（同步投递，总线内部异步派发）
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

/// 逻辑层：订阅 topic
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

/// 逻辑层：取消订阅（与 subscribe 同因异步投递）
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

// ==================== Host Functions（core module 胶水） ====================

/// 消息总线：发布消息
///
/// 参数：(topic_ptr, topic_len, payload_ptr, payload_len)
/// 返回：0 成功，-1 失败
pub(super) fn host_bus_publish(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    topic_ptr: u32,
    topic_len: u32,
    payload_ptr: u32,
    payload_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let topic = match read_wasm_string_consume(&mut caller, topic_ptr, topic_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_bus_publish: failed to read topic");
            return -1;
        }
    };

    let payload_str = match read_wasm_string_consume(&mut caller, payload_ptr, payload_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, topic = %topic, "host_bus_publish: failed to read payload");
            return -1;
        }
    };

    match bus_publish(&host_ctx, &plugin_id, &topic, &payload_str) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, topic = %topic, "host_bus_publish: publish failed");
            -1
        }
    }
}

/// 消息总线：订阅 topic
///
/// 参数：(topic_ptr, topic_len)
/// 返回：0 成功，-1 失败
pub(super) fn host_bus_subscribe(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    topic_ptr: u32,
    topic_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let topic = match read_wasm_string_consume(&mut caller, topic_ptr, topic_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_bus_subscribe: failed to read topic");
            return -1;
        }
    };

    match bus_subscribe(&host_ctx, &plugin_id, &topic) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, topic = %topic, "host_bus_subscribe: subscribe failed");
            -1
        }
    }
}

/// 消息总线：取消订阅
///
/// 参数：(topic_ptr, topic_len)
/// 返回：0 成功，-1 失败
pub(super) fn host_bus_unsubscribe(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    topic_ptr: u32,
    topic_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let topic = match read_wasm_string_consume(&mut caller, topic_ptr, topic_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_bus_unsubscribe: failed to read topic");
            return -1;
        }
    };

    match bus_unsubscribe(&host_ctx, &plugin_id, &topic) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, topic = %topic, "host_bus_unsubscribe: unsubscribe failed");
            -1
        }
    }
}
