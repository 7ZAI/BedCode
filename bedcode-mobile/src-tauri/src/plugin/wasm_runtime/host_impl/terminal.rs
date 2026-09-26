//! host_terminal_send — 终端输入（逻辑层，票 04：迁 HTTP）

use super::super::WasmPluginState;
use super::support::guarded_host_call;
use crate::auth::http::resolve_base_url;
use crate::session::http::SessionHttpClient;
use crate::state::get_connection_manager;

/// 逻辑层：向指定终端会话发送输入（经 HTTP `POST /api/sessions/{id}/input`
/// 直达桌面端；旧 WS `Message` 信封链路已随票 04 退役）
pub(crate) fn terminal_send(state: &WasmPluginState, session_id: &str, data: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_TERMINAL_INPUT)
    {
        return Err("permission denied: terminal:input".to_string());
    }

    // 经会话控制 HTTP 客户端发送（JWT 注入 / 桌面信封解析；与命令层同路径）
    let http = SessionHttpClient::new();
    guarded_host_call(
        &state.plugin_id,
        "host_terminal_send",
        Err(crate::AppError::Internal("host_terminal_send panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                state.runtime_handle.block_on(async {
                    let conn = get_connection_manager();
                    let base_url = resolve_base_url(&conn).await?;
                    http.send_input(&base_url, session_id, data, None).await
                })
            })
        },
    )
    .map_err(|e| format!("HTTP input send failed: {}", e))
}
