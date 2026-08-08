//! 终端域宿主实现（PTY 输入注入）
//!
//! `terminal_send`（权限校验 + 写入）供 Component Model 绑定
//! （`wasm_runtime::component`）调用。

use crate::plugin::permission::PERMISSION_TERMINAL_INPUT;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};

/// 向指定会话注入终端输入（权限校验 + 写入）
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
