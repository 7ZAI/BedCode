//! Desktop-only Tauri Commands
//!
//! 桌面端专用命令 - 移动端不可用

use crate::Result;
use std::sync::Arc;
use tauri::State;

// ==================== WSL Commands ====================

/// 获取已安装的 WSL 发行版
#[tauri::command]
pub async fn list_wsl_distributions() -> Result<Vec<crate::desktop::pty::WslDistro>> {
    crate::desktop::pty::list_distributions()
}

/// 检查 WSL 是否可用
#[tauri::command]
pub fn is_wsl_available() -> bool {
    crate::desktop::pty::is_wsl_available()
}

// ==================== Tmux Commands ====================

/// 获取 Tmux 会话列表
#[tauri::command]
pub async fn list_tmux_sessions() -> Result<Vec<crate::desktop::pty::TmuxSession>> {
    crate::desktop::pty::list_sessions()
}

/// 检查 Tmux 是否可用
#[tauri::command]
pub fn is_tmux_available() -> bool {
    crate::desktop::pty::is_tmux_available()
}

/// 创建 Tmux 会话
#[tauri::command]
pub async fn create_tmux_session(name: String, command: Option<String>) -> Result<()> {
    crate::desktop::pty::create_session(&name, command.as_deref())
}

// ==================== Session Commands ====================

/// 启动会话
#[tauri::command]
pub async fn start_session(
    session_manager: State<'_, Arc<crate::desktop::session::SessionManager>>,
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

/// 获取会话列表
#[tauri::command]
pub async fn list_sessions(
    session_manager: State<'_, Arc<crate::desktop::session::SessionManager>>,
) -> Result<Vec<crate::desktop::session::SessionInfo>> {
    Ok(session_manager.list_sessions().await)
}

/// 获取单个会话信息
#[tauri::command]
pub async fn get_session(
    session_manager: State<'_, Arc<crate::desktop::session::SessionManager>>,
    session_id: String,
) -> Result<Option<crate::desktop::session::SessionInfo>> {
    Ok(session_manager.get_session(&session_id).await)
}

/// 终止会话
#[tauri::command]
pub async fn kill_session(
    session_manager: State<'_, Arc<crate::desktop::session::SessionManager>>,
    session_id: String,
) -> Result<()> {
    session_manager.kill_session(&session_id).await
}

/// 删除会话
#[tauri::command]
pub async fn delete_session(
    session_manager: State<'_, Arc<crate::desktop::session::SessionManager>>,
    session_id: String,
) -> Result<()> {
    session_manager.remove_session(&session_id).await
}

/// 重启会话
#[tauri::command]
pub async fn restart_session(
    session_manager: State<'_, Arc<crate::desktop::session::SessionManager>>,
    session_id: String,
) -> Result<String> {
    session_manager.restart_session(&session_id).await
}

/// 调整会话终端大小
#[tauri::command]
pub async fn resize_session(
    session_manager: State<'_, Arc<crate::desktop::session::SessionManager>>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<()> {
    session_manager.resize_session(&session_id, cols, rows).await
}

// ==================== PTY Input Commands ====================

/// 输入数据到会话
#[tauri::command]
pub async fn write_to_session(
    session_manager: State<'_, Arc<crate::desktop::session::SessionManager>>,
    session_id: String,
    data: String,
) -> Result<()> {
    session_manager.write_input(&session_id, &data).await
}

/// 发送特殊键
#[tauri::command]
pub async fn send_special_key(
    session_manager: State<'_, Arc<crate::desktop::session::SessionManager>>,
    session_id: String,
    key: String,
) -> Result<()> {
    session_manager.send_special_key(&session_id, &key).await
}

// ==================== Device Connection Commands ====================

/// 获取当前 WebSocket 已连接的设备列表
#[tauri::command]
pub async fn get_connected_devices(
    ws_server: State<'_, Arc<crate::desktop::server::WebSocketServer>>,
) -> Result<Vec<crate::desktop::server::DeviceConnectionInfo>> {
    Ok(ws_server.get_connected_devices().await)
}