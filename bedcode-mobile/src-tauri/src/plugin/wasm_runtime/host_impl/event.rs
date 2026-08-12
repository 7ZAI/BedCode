//! host_emit_event — 事件推送

use tauri::Emitter;
use super::super::WasmPluginState;
use super::support::{guarded_host_call, read_wasm_string};

/// 事件：向前端发送
pub(crate) fn host_emit_event(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    name_ptr: u32,
    name_len: u32,
    payload_ptr: u32,
    payload_len: u32,
) {
    let event_name = match read_wasm_string(&mut caller, name_ptr, name_len) {
        Some(s) => s,
        None => {
            tracing::error!("host_emit_event: failed to read event_name");
            return;
        }
    };

    let payload_str = match read_wasm_string(&mut caller, payload_ptr, payload_len) {
        Some(s) => s,
        None => {
            tracing::error!(event = %event_name, "host_emit_event: failed to read payload");
            return;
        }
    };

    let json_payload: serde_json::Value = match serde_json::from_str(&payload_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, event = %event_name, "host_emit_event: invalid JSON payload, using raw string");
            serde_json::Value::String(payload_str)
        }
    };

    let host_ctx = caller.data().host_ctx.clone();
    if let Err(e) = host_ctx.app_handle.emit(&event_name, json_payload) {
        tracing::error!(error = %e, event = %event_name, "host_emit_event: emit failed");
    }
}
