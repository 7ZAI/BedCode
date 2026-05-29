//! Mobile Connection Commands
//!
//! WebSocket 连接管理命令

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::Result;
use crate::mobile::ConnectionManager;
use crate::mobile::events;

/// 全局连接管理器单例
static CONNECTION_MANAGER: std::sync::OnceLock<Arc<ConnectionManager>> = std::sync::OnceLock::new();

/// 获取连接管理器
pub fn get_connection_manager() -> Arc<ConnectionManager> {
    CONNECTION_MANAGER.get_or_init(|| ConnectionManager::new()).clone()
}

/// 连接信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub address: String,
    pub port: u16,
    pub status: String,
}

/// 连接到桌面端
#[tauri::command]
pub async fn ws_connect(
    app_handle: AppHandle,
    address: String,
    port: u16,
    name: Option<String>,
) -> Result<ConnectionInfo> {
    eprintln!("[ws_connect] START - address={}, port={}, name={:?}", address, port, name);
    tracing::info!("WebSocket connecting to {}:{}", address, port);

    // 发射连接开始事件
    events::emit_connecting(&app_handle, &address, port);

    // 启动事件转发任务（仅一次）
    events::start_event_forwarding(app_handle.clone());

    let conn = get_connection_manager();
    tracing::info!("Calling conn.connect()...");

    match conn.connect(app_handle.clone(), address.clone(), port, name).await {
        Ok(_) => {
            tracing::info!("conn.connect() returned Ok");
        }
        Err(e) => {
            tracing::error!("conn.connect() returned error: {}", e);
            events::emit_error(&app_handle, &format!("Connection failed: {}", e));
            return Err(e);
        }
    }

    let status = conn.get_status().await;
    tracing::info!("Connection status: {:?}", status);

    Ok(ConnectionInfo {
        address,
        port,
        status: format!("{:?}", status),
    })
}

/// 断开连接
#[tauri::command]
pub async fn ws_disconnect(app_handle: AppHandle) -> Result<()> {
    tracing::info!("WebSocket disconnecting");

    let conn = get_connection_manager();
    conn.disconnect().await;

    // 发射断开连接事件
    events::emit_disconnected(&app_handle, "User initiated disconnect");

    // 清除会话状态 - 使用公共方法
    let session_mgr = crate::mobile::commands::session::get_session_manager();
    // 停止活跃会话
    if let Some(session) = session_mgr.get_active_session().await {
        let _ = session_mgr.stop_session(&session.id).await;
    }

    Ok(())
}

/// 获取连接状态
#[tauri::command]
pub async fn ws_get_status() -> Result<String> {
    let conn = get_connection_manager();
    let status = conn.get_status().await;
    Ok(format!("{:?}", status))
}

/// 检查是否已连接
#[tauri::command]
pub async fn ws_is_connected() -> Result<bool> {
    let conn = get_connection_manager();
    Ok(conn.is_connected().await)
}

/// 重新连接（断线重连）
#[tauri::command]
pub async fn ws_reconnect(
    app_handle: AppHandle,
    session_token: Option<String>,
) -> Result<()> {
    tracing::info!("[ws_reconnect] session_token: {:?}", session_token.as_ref().map(|t| format!("len={}", t.len())));

    let manager = get_connection_manager();

    // 检查是否已连接
    if manager.is_connected().await {
        tracing::info!("Already connected, skipping reconnect");
        return Ok(());
    }

    // 调用重连
    manager.reconnect(app_handle, session_token).await
}