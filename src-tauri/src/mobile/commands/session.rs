//! Mobile Session Commands
//!
//! 会话管理相关命令

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::Result;
use crate::mobile::SessionInfo;
use crate::mobile::remote::request::{SessionRequest, ResponseParser, timeouts, TerminalRequest, ConfigRequest};
use crate::mobile::managers::{get_connection_manager, get_session_manager};

/// 启动会话响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionResponse {
    pub session_id: String,
    pub session: Option<SessionInfo>,
}

/// 加载会话列表（从桌面端拉取真实会话）
#[tauri::command]
pub async fn ws_load_sessions(app_handle: AppHandle) -> Result<Vec<serde_json::Value>> {
    tracing::info!("[ws_load_sessions] Sending ListSessions request");
    let conn = get_connection_manager();

    let message = SessionRequest::list_sessions();
    let response = conn.send_and_wait_with_disconnect_handling(&app_handle, &message, timeouts::SESSION_CONTROL).await?;

    // 解析响应中的会话列表
    let list = ResponseParser::parse_session_list(&response)
        .unwrap_or_else(|| {
            tracing::warn!("[ws_load_sessions] Failed to parse response, returning empty");
            Vec::new()
        });

    tracing::info!("[ws_load_sessions] Response OK, {} sessions", list.len());
    Ok(list)
}

/// 订阅会话，开始接收该会话的输出
/// 使用 Message::Terminal(Subscribe) 消息，支持指定起始序号用于历史回放
#[tauri::command]
pub async fn ws_join_session(app_handle: AppHandle, session_id: String) -> Result<()> {
    tracing::info!("[ws_join_session] session_id={}", session_id);
    let conn = get_connection_manager();

    let message = TerminalRequest::subscribe(&session_id, None);
    conn.send_and_wait_with_disconnect_handling(&app_handle, &message, timeouts::TERMINAL_SUBSCRIBE).await?;
    tracing::info!("[ws_join_session] Subscribed to session successfully: {}", session_id);
    Ok(())
}

/// 取消订阅会话，停止接收该会话的输出
/// 使用 Message::Terminal(Unsubscribe) 消息
#[tauri::command]
pub async fn ws_leave_session(app_handle: AppHandle, session_id: String) -> Result<()> {
    tracing::info!("[ws_leave_session] session_id={}", session_id);
    let conn = get_connection_manager();

    let message = TerminalRequest::unsubscribe(&session_id);
    conn.send_and_wait_with_disconnect_handling(&app_handle, &message, timeouts::TERMINAL_SUBSCRIBE).await?;
    tracing::info!("[ws_leave_session] Unsubscribed from session successfully: {}", session_id);
    Ok(())
}

/// 带起始序号的订阅会话（用于断线重连后从断点继续）
///
/// - 首次订阅：`start_seq = None` 或 `0` → 从头接收所有历史
/// - 断线重连：使用之前记录的最大 index → 从断点继续接收
/// - 切换会话：使用当前缓冲区最大 index → 避免重复接收
#[tauri::command]
pub async fn ws_subscribe_session(app_handle: AppHandle, session_id: String, start_seq: Option<u64>) -> Result<()> {
    tracing::info!("[ws_subscribe_session] session_id={}, start_seq={:?}", session_id, start_seq);
    let conn = get_connection_manager();

    let message = TerminalRequest::subscribe(&session_id, start_seq);
    conn.send_and_wait_with_disconnect_handling(&app_handle, &message, timeouts::TERMINAL_SUBSCRIBE).await?;
    tracing::info!("[ws_subscribe_session] Subscribed to session with start_seq={:?}: {}", start_seq, session_id);
    Ok(())
}

/// 启动会话
#[tauri::command]
pub async fn ws_start_session(config_id: String, session_name: Option<String>) -> Result<StartSessionResponse> {
    let session_mgr = get_session_manager();
    let session_id = session_mgr.start_session(&config_id, session_name.as_deref()).await?;

    // 获取刚创建的会话信息
    let session = session_mgr.get_session_by_id(&session_id).await;

    Ok(StartSessionResponse {
        session_id,
        session,
    })
}

/// 停止会话
#[tauri::command]
pub async fn ws_stop_session(session_id: String) -> Result<()> {
    let session_mgr = get_session_manager();
    session_mgr.stop_session(&session_id).await
}

/// 删除会话
#[tauri::command]
pub async fn ws_remove_session(session_id: String) -> Result<()> {
    tracing::info!("[ws_remove_session] Entry: session_id={}", session_id);
    let session_mgr = get_session_manager();
    session_mgr.remove_session(&session_id).await?;
    tracing::info!("[ws_remove_session] Exit: returning Ok(()) for session_id={}", session_id);
    Ok(())
}

/// 获取会话配置列表
#[tauri::command]
pub async fn ws_load_session_configs(app_handle: AppHandle) -> Result<Vec<serde_json::Value>> {
    tracing::info!("[ws_load_session_configs] Sending ListSessionConfigs request");
    let conn = get_connection_manager();

    let message = ConfigRequest::list_session_configs();
    let response = conn.send_and_wait_with_disconnect_handling(&app_handle, &message, timeouts::CONFIG).await?;

    // 从响应中提取会话配置列表
    let configs = ResponseParser::parse_config_list(&response)
        .unwrap_or_else(|| {
            tracing::warn!("[ws_load_session_configs] Failed to parse response, returning empty");
            Vec::new()
        });

    tracing::info!("[ws_load_session_configs] Response OK, {} configs", configs.len());
    Ok(configs)
}