//! 消息总线域 Host Functions（插件间 Topic 发布/订阅）

use super::memory::read_wasm_string_consume;
use crate::plugin::wasm_runtime::{block_on_async, WasmPluginState};

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

    let payload: serde_json::Value = match serde_json::from_str(&payload_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, topic = %topic, "host_bus_publish: invalid JSON payload");
            return -1;
        }
    };

    let bus = host_ctx.message_bus.clone();
    bus.publish(&topic, &plugin_id, payload);
    0
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

    let bus = host_ctx.message_bus.clone();
    block_on_async(bus.subscribe_wasm(&plugin_id, &topic));
    0
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

    let bus = host_ctx.message_bus.clone();
    block_on_async(bus.unsubscribe(&plugin_id, &topic));
    0
}
