//! host_bus_* — 消息总线

use super::super::WasmPluginState;
use super::support::{guarded_host_call, has_permission, read_wasm_string};

/// 消息总线：发布消息
pub(crate) fn host_bus_publish(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    topic_ptr: u32,
    topic_len: u32,
    payload_ptr: u32,
    payload_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_BUS) {
        tracing::warn!(plugin_id = %plugin_id, "host_bus_publish: permission denied (bus)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let topic = match read_wasm_string(&mut caller, topic_ptr, topic_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_bus_publish: failed to read topic");
            return -1;
        }
    };

    let payload_str = match read_wasm_string(&mut caller, payload_ptr, payload_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, topic = %topic, "host_bus_publish: failed to read payload");
            return -1;
        }
    };

    let payload: serde_json::Value = match serde_json::from_str(&payload_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, plugin_id = %plugin_id, topic = %topic, "host_bus_publish: invalid JSON payload, using raw string");
            serde_json::Value::String(payload_str)
        }
    };

    host_ctx.message_bus.publish(&topic, &plugin_id, payload);
    0
}


/// 消息总线：订阅 topic
pub(crate) fn host_bus_subscribe(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    topic_ptr: u32,
    topic_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_BUS) {
        tracing::warn!(plugin_id = %plugin_id, "host_bus_subscribe: permission denied (bus)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let topic = match read_wasm_string(&mut caller, topic_ptr, topic_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_bus_subscribe: failed to read topic");
            return -1;
        }
    };

    let bus = host_ctx.message_bus.clone();
    let handle = caller.data().runtime_handle.clone();
    guarded_host_call(&plugin_id, "host_bus_subscribe", (), || {
        tokio::task::block_in_place(|| handle.block_on(bus.subscribe_wasm(&plugin_id, &topic)))
    });
    0
}


/// 消息总线：取消订阅
pub(crate) fn host_bus_unsubscribe(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    topic_ptr: u32,
    topic_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_BUS) {
        tracing::warn!(plugin_id = %plugin_id, "host_bus_unsubscribe: permission denied (bus)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let topic = match read_wasm_string(&mut caller, topic_ptr, topic_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_bus_unsubscribe: failed to read topic");
            return -1;
        }
    };

    let bus = host_ctx.message_bus.clone();
    let handle = caller.data().runtime_handle.clone();
    guarded_host_call(&plugin_id, "host_bus_unsubscribe", (), || {
        tokio::task::block_in_place(|| handle.block_on(bus.unsubscribe(&plugin_id, &topic)))
    });
    0
}
