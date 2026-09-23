//! Terminal Service
//!
//! 处理终端相关消息（输入、输出等）

use crate::enums::{TerminalAction, TerminalPayload};
use crate::server::websocket::message::Message;
use crate::session::SessionManager;
use crate::Result;
use std::sync::Arc;

/// 处理终端输入消息
pub async fn handle_input(
    session_id: &str,
    payload: TerminalPayload,
    session_manager: &Option<Arc<SessionManager>>,
) -> Result<Option<Message>> {
    // 会话真源在插件登记域（P1-b）：输入经宿主窄转发层调插件互调 api
    let host_ctx = || crate::system::app_context::AppContext::global().plugin_host().wasm_host_ctx();
    // 从 payload 中提取 action
    let (data, special_key) = match payload.action {
        TerminalAction::Input { data, special_key } => (data, special_key),
        _ => return Ok(None),
    };

    if session_manager.is_some() {
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

        // 处理特殊键输入（移动端 WS 路径：与旧 `SessionManager::write_input`
        // 一致走**普通输入**通道——提交行重建会把控制字节中性化，不产生内容
        // 也不触发提交，见 input_line 的 test_ctrl_c_from_mobile_ws_path_neutralized）
        if let Some(ref key_combo) = special_key {
            match key_combo.to_pty_bytes() {
                Some(key_bytes) => {
                    // 按键序列是字节级契约：非 UTF-8 不得 lossy 替换（会向 PTY 写入
                    // U+FFFD 垃圾字节），显式告警并丢弃该按键。当前按键集合
                    // （ASCII 控制符 / CSI 序列）均为合法 UTF-8，异常仅来自未来扩展
                    let key_text = match String::from_utf8(key_bytes) {
                        Ok(text) => text,
                        Err(e) => {
                            tracing::warn!(
                                session_id = %session_id,
                                key = %key_combo.to_str(),
                                error = %e,
                                "[TerminalService] special key bytes not valid UTF-8, dropped"
                            );
                            return Ok(None);
                        }
                    };
                    if let Err(e) =
                        crate::utils::session_gateway::input(host_ctx(), session_id, &key_text).await
                    {
                        tracing::error!(
                            "[TerminalService] Failed to write special key to session {}: {}",
                            session_id,
                            e
                        );
                    }
                }
                None => {
                    tracing::warn!("[TerminalService] unsupported key combo: {}", key_combo.to_str());
                }
            }
        }
    } else {
        tracing::warn!(
            "[TerminalService] session_manager is None, cannot handle Input message for session {}",
            session_id
        );
    }

    // 输入消息不需要响应，返回 None 由路由器自动发送 Ack（如果 expect_response=true）
    Ok(None)
}
