//! Mobile Terminal Commands
//!
//! 终端输入命令

use tauri::AppHandle;

use crate::Result;
use crate::connection::request::{TerminalRequest, SessionRequest};
use crate::state::get_connection_manager;

/// 发送输入到会话（带确认模式）
/// 等待桌面端确认收到输入后再返回，确保消息已被处理
#[tauri::command]
pub async fn ws_send_input_async(
    app_handle: AppHandle,
    session_id: String,
    data: String,
    special_key: Option<String>,
) -> Result<()> {
    let conn = get_connection_manager();

    // 裁剪尾部换行
    let trimmed_data = if special_key.as_deref() == Some("enter") {
        data.trim_end_matches('\n').trim_end_matches('\r').to_string()
    } else {
        data
    };

    // 解析特殊按键
    let special_key_enum = special_key.as_ref().and_then(|k| TerminalRequest::parse_special_key(k));

    let message = TerminalRequest::input(&session_id, &trimmed_data, special_key_enum);

    // 使用带断开处理的 send_and_wait
    // 设置 5 秒超时，终端输入应该快速响应
    let timeout = std::time::Duration::from_secs(5);
    conn.send_and_wait_with_disconnect_handling(&app_handle, &message, timeout).await?;

    Ok(())
}

/// 发送消息（不等待响应）
/// 通用接口，接受 JSON 格式的消息
#[tauri::command]
pub async fn ws_send_message(
    app_handle: AppHandle,
    message_type: String,
    payload: serde_json::Value,
) -> Result<()> {
    let conn = get_connection_manager();

    // 尝试将 payload 转换为对应的 Message 类型
    let result = convert_json_to_message(&message_type, payload);
    match result {
        Some(message) => conn.send_with_disconnect_handling(&app_handle, &message).await,
        None => Err(crate::AppError::Parse(format!("Unsupported message type: {}", message_type)))
    }
}

/// 发送消息并等待响应
#[tauri::command]
pub async fn ws_send_and_wait(
    app_handle: AppHandle,
    message_type: String,
    payload: serde_json::Value,
    timeout_secs: Option<u64>,
) -> Result<serde_json::Value> {
    let conn = get_connection_manager();
    let timeout = std::time::Duration::from_secs(timeout_secs.unwrap_or(30));

    // 尝试将 payload 转换为对应的 Message 类型
    let message = match convert_json_to_message(&message_type, payload) {
        Some(m) => m,
        None => return Err(crate::AppError::Parse(format!("Unsupported message type: {}", message_type))),
    };

    let response = conn.send_and_wait_with_disconnect_handling(&app_handle, &message, timeout).await?;
    let json_str = response.to_json()?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| crate::AppError::Parse(e.to_string()))?;

    Ok(parsed)
}

/// 将 JSON payload 转换为 Message 类型（带响应期望）
/// 用于 ws_send_and_wait，确保所有消息都设置 expect_response: true
fn convert_json_to_message(message_type: &str, payload: serde_json::Value) -> Option<crate::model::message::Message> {
    use crate::model::message::Message;
    use crate::enums::control::{SessionControlAction, SessionConfigAction};

    match message_type {
        "session_control" | "control" => {
            let action_type = payload.get("action")
                .and_then(|a| a.get("type"))
                .and_then(|t| t.as_str())?;

            let action = match action_type {
                "list_sessions" => SessionControlAction::ListSessions,
                "start_session" => {
                    let config_id = payload.get("action")
                        .and_then(|a| a.get("config_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    SessionControlAction::StartSession { config_id }
                }
                "stop_session" => {
                    let session_id = payload.get("action")
                        .and_then(|a| a.get("session_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    SessionControlAction::StopSession { session_id }
                }
                "remove_session" => {
                    let session_id = payload.get("action")
                        .and_then(|a| a.get("session_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    SessionControlAction::RemoveSession { session_id }
                }
                "resize_session" => {
                    let session_id = payload.get("action")
                        .and_then(|a| a.get("session_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let cols = payload.get("action")
                        .and_then(|a| a.get("cols"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(80) as u16;
                    let rows = payload.get("action")
                        .and_then(|a| a.get("rows"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(24) as u16;
                    SessionControlAction::ResizeSession { session_id, cols, rows }
                }
                _ => return None,
            };
            // 使用 _with_response 版本确保 expect_response: true
            Some(Message::session_control_with_response(action, None))
        }
        "session_config" => {
            let action_type = payload.get("action")
                .and_then(|a| a.get("type"))
                .and_then(|t| t.as_str())?;

            let action = match action_type {
                "list_session_configs" => SessionConfigAction::ListSessionConfigs,
                "list_quick_actions" => SessionConfigAction::ListQuickActions,
                _ => return None,
            };
            // 使用 _with_response 版本确保 expect_response: true
            Some(Message::session_config_with_response(action, None))
        }
        "input" => {
            let session_id = payload.get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let data = payload.get("payload")
                .and_then(|p| p.get("data"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // input 不需要响应，保持原样
            Some(Message::input(session_id, data, None))
        }
        "subscribe" => {
            let session_id = payload.get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let start_seq = payload.get("start_seq")
                .and_then(|v| v.as_u64());
            // 使用 _with_response 版本确保 expect_response: true
            Some(Message::subscribe_with_response(session_id, start_seq))
        }
        "unsubscribe" => {
            let session_id = payload.get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // 使用 _with_response 版本确保 expect_response: true
            Some(Message::unsubscribe_with_response(session_id))
        }
        _ => None,
    }
}

/// 调整终端大小
///
/// 将移动端终端的实际尺寸 (cols, rows) 通过 WebSocket 发送到桌面端。
/// 桌面端收到后更新 PTY 尺寸，使输出按移动端屏幕宽度排版，
/// 避免因宽度不匹配导致 \r 光标定位错乱、多行输出堆叠等问题。
#[tauri::command]
pub async fn ws_resize_terminal(
    app_handle: AppHandle,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<()> {
    tracing::debug!("[ws_resize_terminal] session_id={}, cols={}, rows={}", session_id, cols, rows);
    let conn = get_connection_manager();

    let message = SessionRequest::resize_session(&session_id, cols as u16, rows as u16);

    conn.send_with_disconnect_handling(&app_handle, &message).await
}
