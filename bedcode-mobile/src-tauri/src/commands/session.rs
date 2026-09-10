//! Session Commands
//!
//! 会话管理相关命令

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::connection::request::{timeouts, ConfigRequest, ResponseParser, SessionRequest, TerminalRequest};
use crate::session::SessionInfo;
use crate::state::{get_connection_manager, get_global_token, get_session_manager};
use crate::Result;

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
    let response = conn
        .send_and_wait_with_disconnect_handling(&app_handle, &message, timeouts::SESSION_CONTROL)
        .await?;

    // 解析响应中的会话列表
    let list = ResponseParser::parse_session_list(&response).unwrap_or_else(|| {
        tracing::warn!("[ws_load_sessions] Failed to parse response, returning empty");
        Vec::new()
    });

    tracing::info!("[ws_load_sessions] Response OK, {} sessions", list.len());
    Ok(list)
}

/// 加入会话（09 后仅作订阅确认/日志用；输出链路已改前端直连）
/// 使用 Message::Terminal(Subscribe) 消息
#[tauri::command]
pub async fn ws_join_session(app_handle: AppHandle, session_id: String) -> Result<()> {
    tracing::info!("[ws_join_session] session_id={}", session_id);
    let conn = get_connection_manager();

    let message = TerminalRequest::subscribe(&session_id, None);
    conn.send_and_wait_with_disconnect_handling(&app_handle, &message, timeouts::TERMINAL_SUBSCRIBE)
        .await?;
    tracing::info!("[ws_join_session] Subscribed to session successfully: {}", session_id);
    Ok(())
}

/// 终端 WS 直连信息（09：移动端前端直连桌面端终端会话路由）
///
/// 返回完整 URL 与当前 JWT；JWT 由 Rust 持有，经此 invoke 提供给前端建连，
/// 禁止落前端存储（与 get_ws_token 同一令牌，D3）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalWsInfo {
    /// 完整 WebSocket URL（含会话路径）
    pub url: String,
    /// 当前持有的 JWT（一次性消费，禁止落前端存储）
    pub token: String,
}

/// 获取终端会话 WS 连接信息
#[tauri::command]
pub async fn get_terminal_ws_info(session_id: String) -> Result<TerminalWsInfo> {
    let conn = get_connection_manager();
    let target = conn
        .get_target()
        .await
        .ok_or_else(|| crate::AppError::Auth("No target device".to_string()))?;
    Ok(TerminalWsInfo {
        url: format!(
            "ws://{}:{}{}/{}",
            target.address,
            target.port,
            crate::system::constants::connection::WS_TERMINAL_SESSION_PATH,
            session_id
        ),
        token: get_global_token(),
    })
}

/// 启动会话
#[tauri::command]
pub async fn ws_start_session(config_id: String, session_name: Option<String>) -> Result<StartSessionResponse> {
    let session_mgr = get_session_manager();
    let session_id = session_mgr.start_session(&config_id, session_name.as_deref()).await?;

    // 获取刚创建的会话信息
    let session = session_mgr.get_session_by_id(&session_id).await;

    Ok(StartSessionResponse { session_id, session })
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
    tracing::info!(
        "[ws_remove_session] Exit: returning Ok(()) for session_id={}",
        session_id
    );
    Ok(())
}

/// 获取会话配置列表
#[tauri::command]
pub async fn ws_load_session_configs(app_handle: AppHandle) -> Result<Vec<serde_json::Value>> {
    tracing::info!("[ws_load_session_configs] Sending ListSessionConfigs request");
    let conn = get_connection_manager();

    let message = ConfigRequest::list_session_configs();
    let response = conn
        .send_and_wait_with_disconnect_handling(&app_handle, &message, timeouts::CONFIG)
        .await?;

    // 从响应中提取会话配置列表
    let configs = ResponseParser::parse_config_list(&response).unwrap_or_else(|| {
        tracing::warn!("[ws_load_session_configs] Failed to parse response, returning empty");
        Vec::new()
    });

    tracing::info!("[ws_load_session_configs] Response OK, {} configs", configs.len());
    Ok(configs)
}
