//! Session Commands

use crate::session::{RendererSource, ResizeOutcome, SessionManager};
use crate::Result;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn start_session(
    session_manager: State<'_, Arc<SessionManager>>,
    config_id: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<String> {
    tracing::info!("start_session called with config_id: {}", config_id);
    // 桌面端启动：携带本端终端组件默认网格作为 PTY 初始尺寸
    let initial_size = match (cols, rows) {
        (Some(c), Some(r)) if c > 0 && r > 0 => Some((c, r)),
        _ => None,
    };
    let result = session_manager
        .create_session_with_source(&config_id, None, initial_size)
        .await;
    match result {
        Ok(id) => {
            tracing::info!("Session created successfully: {}", id);
            Ok(id)
        }
        Err(e) => {
            tracing::error!("Failed to create session: {}", e);
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn create_session_no_start(
    session_manager: State<'_, Arc<SessionManager>>,
    config_id: String,
) -> Result<String> {
    tracing::info!("create_session_no_start called with config_id: {}", config_id);
    let result = session_manager.create_session_no_start(&config_id).await;
    match result {
        Ok(id) => {
            tracing::info!("Session created (not started) successfully: {}", id);
            Ok(id)
        }
        Err(e) => {
            tracing::error!("Failed to create session (not started): {}", e);
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn start_existing_session(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<()> {
    tracing::info!("start_existing_session called with session_id: {}", session_id);
    // 两阶段启动第二阶段：spawn 前按请求端尺寸调整 PTY
    let initial_size = match (cols, rows) {
        (Some(c), Some(r)) if c > 0 && r > 0 => Some((c, r)),
        _ => None,
    };
    let result = session_manager
        .start_existing_session(&session_id, initial_size)
        .await;
    match result {
        Ok(_) => {
            tracing::info!("Session started successfully: {}", session_id);
            Ok(())
        }
        Err(e) => {
            tracing::error!("Failed to start session: {}", e);
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn list_sessions(
    session_manager: State<'_, Arc<SessionManager>>,
) -> Result<Vec<crate::session::SessionInfo>> {
    Ok(session_manager.list_sessions().await)
}

#[tauri::command]
pub async fn get_session(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
) -> Result<Option<crate::session::SessionInfo>> {
    Ok(session_manager.get_session(&session_id).await)
}

#[tauri::command]
pub async fn kill_session(session_manager: State<'_, Arc<SessionManager>>, session_id: String) -> Result<()> {
    session_manager.kill_session(&session_id).await
}

#[tauri::command]
pub async fn delete_session(session_manager: State<'_, Arc<SessionManager>>, session_id: String) -> Result<()> {
    session_manager.remove_session(&session_id).await
}

#[tauri::command]
pub async fn restart_session(session_manager: State<'_, Arc<SessionManager>>, session_id: String) -> Result<String> {
    session_manager.restart_session(&session_id).await
}

/// 调整会话终端大小（桌面本地路径，正统渲染端身份恒为 Desktop）
///
/// force 置位表示覆盖确认已通过（前端弹窗确认后重发）；返回 ResizeOutcome
/// 供前端判断是否需要弹窗确认（NeedsConfirmation 时未应用任何改动）。
#[tauri::command]
pub async fn resize_session(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
    cols: u16,
    rows: u16,
    force: Option<bool>,
) -> Result<ResizeOutcome> {
    session_manager
        .resize_session(&session_id, cols, rows, RendererSource::Desktop, force.unwrap_or(false))
        .await
}
