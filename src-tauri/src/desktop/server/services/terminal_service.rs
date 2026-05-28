//! Terminal Service
//!
//! 处理终端相关消息（输入、输出等）

use crate::desktop::session::SessionManager;
use crate::desktop::server::message::Message;
use crate::shared::enums::{TerminalAction, TerminalPayload, SpecialKey::*};
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
        if let Some(ref key) = special_key {
            let key_bytes = match key {
                Tab => "\t",
                Enter => "\r",
                Escape => "\x1b",
                CtrlC => "\x03",
                CtrlD => "\x04",
                CtrlL => "\x0c",
                CtrlZ => "\x1a",
                ArrowUp => "\x1b[A",
                ArrowDown => "\x1b[B",
                ArrowLeft => "\x1b[D",
                ArrowRight => "\x1b[C",
                Backspace => "\x7f",
            };
            tracing::debug!(
                "[TerminalService] writing special_key key={:?} bytes={:?}",
                key,
                key_bytes
            );
            if let Err(e) = sm.write_input(session_id, key_bytes).await {
                tracing::error!(
                    "[TerminalService] Failed to write special key to session {}: {}",
                    session_id,
                    e
                );
            }
        }
    } else {
        tracing::warn!(
            "[TerminalService] session_manager is None, cannot handle Input message for session {}",
            session_id
        );
    }

    // 返回 Input 确认响应，让 send_and_wait 能收到匹配的 ACK
    Ok(Some(Message::input(session_id, "", None)))
}