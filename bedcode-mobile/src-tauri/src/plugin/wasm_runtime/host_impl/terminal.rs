//! host_terminal_send — 终端输入

use crate::connection::request::TerminalRequest;
use crate::state::get_connection_manager;
use super::super::WasmPluginState;
use super::support::{guarded_host_call, has_permission, read_wasm_string};

/// 终端：发送输入（通过 WebSocket 转发到桌面端）
pub(crate) fn host_terminal_send(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sid_ptr: u32,
    sid_len: u32,
    data_ptr: u32,
    data_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_TERMINAL_INPUT) {
        tracing::warn!(plugin_id = %plugin_id, "host_terminal_send: permission denied (terminal:input)");
        return -1;
    }

    let session_id = match read_wasm_string(&mut caller, sid_ptr, sid_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_terminal_send: failed to read session_id");
            return -1;
        }
    };

    let data = match read_wasm_string(&mut caller, data_ptr, data_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_terminal_send: failed to read data");
            return -1;
        }
    };

    // 通过 ConnectionManager WebSocket 转发到桌面端
    let conn = get_connection_manager();
    let message = TerminalRequest::input(&session_id, &data, None);
    let handle = caller.data().runtime_handle.clone();

    match guarded_host_call(
        &plugin_id,
        "host_terminal_send",
        Err(crate::AppError::Internal("host_terminal_send panicked".to_string())),
        || tokio::task::block_in_place(|| handle.block_on(conn.send(&message))),
    ) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, session_id = %session_id, "host_terminal_send: WebSocket send failed");
            -1
        }
    }
}
