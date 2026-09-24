//! Terminal Service
//!
//! 处理终端相关消息（输入、输出等）

use crate::enums::{TerminalAction, TerminalPayload};
use crate::server::websocket::message::Message;
use crate::Result;

/// 处理终端输入消息
///
/// 票 11：`session_manager` 参数删除——它此前只作 `is_some()` 存在性门（会话真源早
/// 已迁插件登记域，宿主这一侧从不读它），留着会把「宿主还能直连会话」的表象带下去。
/// 输入一律经宿主窄转发层调插件互调 api（`session_gateway`）。
pub async fn handle_input(session_id: &str, payload: TerminalPayload) -> Result<Option<Message>> {
    // 会话真源在插件登记域（P1-b）：输入经宿主窄转发层调插件互调 api
    let host_ctx = || crate::system::app_context::AppContext::global().plugin_host().wasm_host_ctx();
    // 从 payload 中提取 action
    let (data, special_key) = match payload.action {
        TerminalAction::Input { data, special_key } => (data, special_key),
        _ => return Ok(None),
    };

    // 处理普通数据输入
    if !data.is_empty() {
        // 高频输入节流日志在插件侧提交行重建路径（防 TUI 高频输入刷屏），
        // 此处不再重复打，保留错误路径日志
        if let Err(e) = crate::utils::session_gateway::input(host_ctx(), session_id, &data).await {
            tracing::error!(
                "[TerminalService] Failed to write input to session {}: {}",
                session_id,
                e
            );
        }
    }

    // 处理特殊键输入（移动端 WS 路径）：票 06 下沉后宿主不再翻译——
    // 组合串经 `session_gateway::special_key` 转发插件，插件自译自写
    // （统一直写语义：特殊键绕过提交行重建，Ctrl+C 即 \x03 直进 pty）。
    if let Some(ref key_combo) = special_key {
        if let Err(e) =
            crate::utils::session_gateway::special_key(host_ctx(), session_id, &key_combo.to_str()).await
        {
            tracing::error!(
                "[TerminalService] Failed to write special key to session {}: {}",
                session_id,
                e
            );
        }
    }

    // 输入消息不需要响应，返回 None 由路由器自动发送 Ack（如果 expect_response=true）
    Ok(None)
}
