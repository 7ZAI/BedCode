//! Terminal Service
//!
//! 处理终端相关消息（输入、输出等）
//! 拦截 /bedcode 自定义命令用于控制会话模式

use crate::desktop::app_context::AppContext;
use crate::desktop::session::SessionManager;
use crate::desktop::server::message::Message;
use crate::shared::enums::{TerminalAction, TerminalPayload, KeyCombo};
use crate::Result;
use std::sync::Arc;

/// BedCode 自定义命令前缀
const BEDCODE_CMD_PREFIX: &str = "/bedcode ";

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

    // 拦截 /bedcode 自定义命令
    if !data.is_empty() && data.starts_with(BEDCODE_CMD_PREFIX) {
        if let Some(result) = handle_bedcode_command(session_id, &data).await {
            return Ok(result);
        }
    }

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

/// 处理 /bedcode 自定义命令
///
/// 支持的命令：
/// - `/bedcode auto` — 开启自动授权模式
/// - `/bedcode manual` — 关闭自动授权模式
///
/// 命令被拦截后不传递到 PTY，直接返回操作结果
async fn handle_bedcode_command(session_id: &str, data: &str) -> Option<Option<Message>> {
    let arg = data[BEDCODE_CMD_PREFIX.len()..].trim();

    let ctx = AppContext::global();
    let plugin_manager = ctx.plugin_manager();

    match arg {
        "auto" => {
            plugin_manager.set_auto_mode(session_id, true).await;
            tracing::info!(
                "[TerminalService] /bedcode auto: session_id={} auto_approve enabled",
                session_id
            );
            // 命令已处理，不传递到 PTY
            Some(None)
        }
        "manual" => {
            plugin_manager.set_auto_mode(session_id, false).await;
            tracing::info!(
                "[TerminalService] /bedcode manual: session_id={} auto_approve disabled",
                session_id
            );
            Some(None)
        }
        _ => {
            // 未知 /bedcode 子命令，不拦截，传递到 PTY
            tracing::debug!(
                "[TerminalService] Unknown /bedcode command: {}, passing through",
                arg
            );
            None
        }
    }
}
