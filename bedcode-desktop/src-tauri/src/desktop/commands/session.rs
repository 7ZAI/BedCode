//! Session Commands

use crate::desktop::session::SessionManager;
use crate::Result;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn start_session(
    session_manager: State<'_, Arc<SessionManager>>,
    config_id: String,
) -> Result<String> {
    tracing::info!("start_session called with config_id: {}", config_id);
    let result = session_manager.create_session(&config_id).await;
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
) -> Result<()> {
    tracing::info!("start_existing_session called with session_id: {}", session_id);
    let result = session_manager.start_existing_session(&session_id).await;
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
) -> Result<Vec<crate::desktop::session::SessionInfo>> {
    Ok(session_manager.list_sessions().await)
}

#[tauri::command]
pub async fn get_session(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
) -> Result<Option<crate::desktop::session::SessionInfo>> {
    Ok(session_manager.get_session(&session_id).await)
}

#[tauri::command]
pub async fn kill_session(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
) -> Result<()> {
    session_manager.kill_session(&session_id).await
}

#[tauri::command]
pub async fn delete_session(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
) -> Result<()> {
    session_manager.remove_session(&session_id).await
}

#[tauri::command]
pub async fn restart_session(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
) -> Result<String> {
    session_manager.restart_session(&session_id).await
}

#[tauri::command]
pub async fn resize_session(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<()> {
    session_manager.resize_session(&session_id, cols, rows).await
}

#[tauri::command]
pub async fn get_session_output_history(
    _session_manager: State<'_, Arc<SessionManager>>,
    _session_id: String,
) -> Result<Vec<crate::desktop::model::PtyOutputEvent>> {
    // TODO: 实现从 PTY 会话获取历史输出
    Ok(vec![])
}
