//! 终端域 Host Functions（PTY 输入注入）
//!
//! 逻辑层 `terminal_send`（权限校验 + 写入）供 core module 胶水
//! 与 Component Model 绑定（`wasm_runtime::component`）共用。

use super::memory::read_wasm_string_consume;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext, WasmPluginState};
use crate::plugin::permission::PERMISSION_TERMINAL_INPUT;

/// 逻辑层：向指定会话注入终端输入（权限校验 + 写入）
pub(crate) fn terminal_send(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
    data: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_TERMINAL_INPUT, "host_terminal_send") {
        return Err("permission denied".to_string());
    }

    let sm = host_ctx.session_manager.clone();
    block_on_async(sm.write_input(session_id, data))
        .map_err(|e| format!("write failed: {}", e))
}

/// 终端：发送输入
///
/// 参数：(session_id_ptr, session_id_len, data_ptr, data_len)
/// 返回：0 成功，-1 失败
pub(super) fn host_terminal_send(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sid_ptr: u32,
    sid_len: u32,
    data_ptr: u32,
    data_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let session_id = match read_wasm_string_consume(&mut caller, sid_ptr, sid_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_terminal_send: failed to read session_id");
            return -1;
        }
    };

    let data = match read_wasm_string_consume(&mut caller, data_ptr, data_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, session_id = %session_id, "host_terminal_send: failed to read data");
            return -1;
        }
    };

    match terminal_send(&host_ctx, &plugin_id, &session_id, &data) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, session_id = %session_id, "host_terminal_send: write failed");
            -1
        }
    }
}
