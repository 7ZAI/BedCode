//! host_terminal_send — 终端输入（逻辑层）

use crate::connection::request::TerminalRequest;
use crate::state::get_connection_manager;
use super::super::WasmPluginState;
use super::support::guarded_host_call;

/// 逻辑层：向指定终端会话发送输入（经 WebSocket 转发到桌面端）
pub(crate) fn terminal_send(
    state: &WasmPluginState,
    session_id: &str,
    data: &str,
) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_TERMINAL_INPUT)
    {
        return Err("permission denied: terminal:input".to_string());
    }

    // 通过 ConnectionManager WebSocket 转发到桌面端
    let conn = get_connection_manager();
    let message = TerminalRequest::input(session_id, data, None);
    guarded_host_call(
        &state.plugin_id,
        "host_terminal_send",
        Err(crate::AppError::Internal("host_terminal_send panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                state.runtime_handle.block_on(conn.send(&message))
            })
        },
    )
    .map_err(|e| format!("WebSocket send failed: {}", e))
}
