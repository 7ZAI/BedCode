//! PTY Input Commands

use crate::desktop::session::SessionManager;
use crate::Result;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn write_to_session(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
    data: String,
) -> Result<()> {
    session_manager.write_input(&session_id, &data).await
}

#[tauri::command]
pub async fn send_special_key(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
    key: String,
) -> Result<()> {
    session_manager.send_special_key(&session_id, &key).await
}
