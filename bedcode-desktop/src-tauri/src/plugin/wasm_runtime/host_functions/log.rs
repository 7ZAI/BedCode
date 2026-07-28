//! 日志域 Host Functions（转发到宿主 tracing，附加 plugin_id 前缀）

use super::memory::read_wasm_string_consume;
use crate::plugin::wasm_runtime::WasmPluginState;

/// 日志：info 级别
pub(super) fn host_log_info(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string_consume(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::info!("[plugin:{}] {}", plugin_id, message);
}

/// 日志：debug 级别
pub(super) fn host_log_debug(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string_consume(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::debug!("[plugin:{}] {}", plugin_id, message);
}

/// 日志：warn 级别
pub(super) fn host_log_warn(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string_consume(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::warn!("[plugin:{}] {}", plugin_id, message);
}

/// 日志：error 级别
pub(super) fn host_log_error(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string_consume(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::error!("[plugin:{}] {}", plugin_id, message);
}
