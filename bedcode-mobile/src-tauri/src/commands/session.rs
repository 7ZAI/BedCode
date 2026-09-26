//! Session Commands
//!
//! 会话管理相关命令（票 04：控制面迁 HTTP）
//!
//! `ws_load_sessions` / `ws_load_session_configs` / `ws_join_session` /
//! `get_terminal_ws_info` 已随 WS `Message` 信封退役删除——前端会话列表/配置
//! 改经 `useHttpApi` 直调桌面 HTTP（`GET /api/sessions` / `GET /api/configs`），
//! 终端订阅改走票 05 的终端 WS 新协议（`terminal_link`）。本文件只保留
//! 起停删三命令，后端实现为 HTTP（`SessionManager` 经 `session::http`）。

use serde::{Deserialize, Serialize};

use crate::session::SessionInfo;
use crate::state::get_session_manager;
use crate::Result;

/// 启动会话响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionResponse {
    pub session_id: String,
    pub session: Option<SessionInfo>,
}

/// 启动会话（HTTP `POST /api/sessions/start`）
#[tauri::command]
pub async fn ws_start_session(config_id: String, session_name: Option<String>) -> Result<StartSessionResponse> {
    let session_mgr = get_session_manager();
    let session_id = session_mgr.start_session(&config_id, session_name.as_deref()).await?;

    // 获取刚创建的会话信息
    let session = session_mgr.get_session_by_id(&session_id).await;

    Ok(StartSessionResponse { session_id, session })
}

/// 停止会话（HTTP `POST /api/sessions/{id}/stop`）
#[tauri::command]
pub async fn ws_stop_session(session_id: String) -> Result<()> {
    let session_mgr = get_session_manager();
    session_mgr.stop_session(&session_id).await
}

/// 删除会话（HTTP `DELETE /api/sessions/{id}/remove`）
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
