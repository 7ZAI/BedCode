//! Terminal Service
//!
//! 处理终端相关消息（输入、输出等）

use crate::session::SessionManager;
use crate::server::message::Message;
use crate::enums::{TerminalAction, TerminalPayload};
use crate::Result;
use std::sync::Arc;

/// 处理终端输入消息
pub async fn handle_input(
    session_id: &str,
    payload: TerminalPayload,
    session_manager: &Option<Arc<SessionManager>>,
) -> Result<Option<Message>> {
    // 从 payload 中提取 action
    let (data, special_key) = match payload.action {
        TerminalAction::Input { data, special_key } => (data, special_key),
        _ => return Ok(None),
    };

    if let Some(ref sm) = session_manager {
        // 处理普通数据输入
        if !data.is_empty() {
            tracing::debug!(
                "[TerminalService] writing data to session {}, data_len={}",
                session_id,
                data.len()
            );
            if let Err(e) = sm.write_input(session_id, &data).await {
                tracing::error!(
                    "[TerminalService] Failed to write input to session {}: {}",
                    session_id,
                    e
                );
            }
        }

        // 处理特殊键输入
        if let Some(ref key_combo) = special_key {
            match key_combo.to_pty_bytes() {
                Some(key_bytes) => {
                    tracing::debug!(
                        "[TerminalService] writing key_combo={} bytes={:?}",
                        key_combo.to_str(),
                        key_bytes
                    );
                    if let Err(e) = sm.write_input(session_id, &String::from_utf8_lossy(&key_bytes)).await {
                        tracing::error!(
                            "[TerminalService] Failed to write special key to session {}: {}",
                            session_id,
                            e
                        );
                    }
                }
                None => {
                    tracing::warn!(
                        "[TerminalService] unsupported key combo: {}",
                        key_combo.to_str()
                    );
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
