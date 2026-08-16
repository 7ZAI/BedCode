//! host_bus_* — 消息总线（逻辑层）
//!
//! 逻辑层函数（值传递）供组件 trait impl（wasm_runtime/component.rs）调用；
//! core 形态的 func_wrap 胶水（内存搬运 + 状态码映射）已随 09 清理删除。

use super::super::WasmPluginState;
use super::support::guarded_host_call;

/// 逻辑层：发布消息
pub(crate) fn bus_publish(
    state: &WasmPluginState,
    topic: &str,
    payload_str: &str,
) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_BUS)
    {
        return Err("permission denied: bus".to_string());
    }

    let payload: serde_json::Value = match serde_json::from_str(payload_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, plugin_id = %state.plugin_id, topic = %topic, "host_bus_publish: invalid JSON payload, using raw string");
            serde_json::Value::String(payload_str.to_string())
        }
    };

    state.host_ctx.message_bus.publish(topic, &state.plugin_id, payload);
    Ok(())
}

/// 逻辑层：订阅 topic
pub(crate) fn bus_subscribe(state: &WasmPluginState, topic: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_BUS)
    {
        return Err("permission denied: bus".to_string());
    }
    guarded_host_call(&state.plugin_id, "host_bus_subscribe", (), || {
        tokio::task::block_in_place(|| {
            state.runtime_handle.block_on(
                state.host_ctx.message_bus.subscribe_wasm(&state.plugin_id, topic),
            )
        })
    });
    Ok(())
}

/// 逻辑层：取消订阅
pub(crate) fn bus_unsubscribe(state: &WasmPluginState, topic: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_BUS)
    {
        return Err("permission denied: bus".to_string());
    }
    guarded_host_call(&state.plugin_id, "host_bus_unsubscribe", (), || {
        tokio::task::block_in_place(|| {
            state
                .runtime_handle
                .block_on(state.host_ctx.message_bus.unsubscribe(&state.plugin_id, topic))
        })
    });
    Ok(())
}
