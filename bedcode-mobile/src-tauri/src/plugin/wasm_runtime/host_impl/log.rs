//! host_log_* — 插件日志

use super::super::WasmPluginState;
use super::support::{guarded_host_call, read_wasm_string};

pub(crate) fn host_log_info(mut caller: wasmtime::Caller<'_, WasmPluginState>, msg_ptr: u32, msg_len: u32) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::info!("[plugin:{}] {}", plugin_id, message);
}

pub(crate) fn host_log_debug(mut caller: wasmtime::Caller<'_, WasmPluginState>, msg_ptr: u32, msg_len: u32) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::debug!("[plugin:{}] {}", plugin_id, message);
}

pub(crate) fn host_log_warn(mut caller: wasmtime::Caller<'_, WasmPluginState>, msg_ptr: u32, msg_len: u32) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::warn!("[plugin:{}] {}", plugin_id, message);
}

pub(crate) fn host_log_error(mut caller: wasmtime::Caller<'_, WasmPluginState>, msg_ptr: u32, msg_len: u32) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::error!("[plugin:{}] {}", plugin_id, message);
}

// ==================== File System Host Functions ====================
