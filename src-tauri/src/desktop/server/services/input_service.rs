//! Input Service
//!
//! 处理终端输入消息

use crate::desktop::session::SessionManager;
use crate::desktop::server::message::{InputPayload, Message};
use crate::shared::enums::SpecialKey::*;
use crate::Result;
use std::sync::Arc;

/// 处理 Input 消息
pub async fn handle_input(
    session_id: &str,
    payload: InputPayload,
    session_manager: &Option<Arc<SessionManager>>,
) -> Result<Option<Message>> {
    if let Some(ref sm) = session_manager {
        // 处理普通数据输入
        if !payload.data.is_empty() {
            tracing::debug!(
                "[InputService] writing data to session {}, data_len={}",
                session_id,
                payload.data.len()
            );
            if let Err(e) = sm.write_input(session_id, &payload.data).await {
                tracing::error!(
                    "[InputService] Failed to write input to session {}: {}",
                    session_id,
                    e
                );
            }
        }

        // 处理特殊键输入
        if let Some(ref key) = payload.special_key {
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
                "[InputService] writing special_key key={:?} bytes={:?}",
                key,
                key_bytes
            );
            if let Err(e) = sm.write_input(session_id, key_bytes).await {
                tracing::error!(
                    "[InputService] Failed to write special key to session {}: {}",
                    session_id,
                    e
                );
            }
        }
    } else {
        tracing::warn!(
            "[InputService] session_manager is None, cannot handle Input message for session {}",
            session_id
        );
    }

    // 返回 Input 确认响应，让 send_and_wait 能收到匹配的 ACK
    Ok(Some(Message::Input {
        message_id: String::new(), // 会被 websocket_manager.rs 替换为原始 message_id
        expect_response: false,
        session_id: session_id.to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        payload: InputPayload {
            data: String::new(),
            special_key: None,
        },
    }))
}