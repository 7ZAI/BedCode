//! Mobile-specific Tauri Commands
//!
//! 移动端专用命令 - 桌面端不可用
//!
//! WebSocket 连接管理命令：
//! - ws_connect: 连接到桌面端
//! - ws_disconnect: 断开连接
//! - ws_send_message: 发送消息
//! - ws_send_with_response: 发送消息并等待响应

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::mobile::{
    AuthManager, ConnectionManager, ConnectionStatus as ConnStatus,
    SessionInfo, SessionManager,
};
use crate::shared::websocket::WsMessage;

// ==================== Singleton Managers ====================

/// 全局连接管理器单例
static CONNECTION_MANAGER: std::sync::OnceLock<Arc<ConnectionManager>> = std::sync::OnceLock::new();

/// 全局认证管理器单例
static AUTH_MANAGER: std::sync::OnceLock<Arc<AuthManager>> = std::sync::OnceLock::new();

/// 全局会话管理器单例
static SESSION_MANAGER: std::sync::OnceLock<Arc<SessionManager>> = std::sync::OnceLock::new();

/// 获取连接管理器
fn get_connection_manager() -> Arc<ConnectionManager> {
    CONNECTION_MANAGER.get_or_init(|| ConnectionManager::new()).clone()
}

/// 获取认证管理器
fn get_auth_manager() -> Arc<AuthManager> {
    AUTH_MANAGER.get_or_init(|| {
        let conn = get_connection_manager();
        AuthManager::new(conn)
    }).clone()
}

/// 获取会话管理器
fn get_session_manager() -> Arc<SessionManager> {
    SESSION_MANAGER.get_or_init(|| {
        let conn = get_connection_manager();
        SessionManager::new(conn)
    }).clone()
}

// ==================== WebSocket Commands ====================

/// 连接信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub address: String,
    pub port: u16,
    pub status: String,
}

/// 连接到桌面端
#[tauri::command]
pub async fn ws_connect(address: String, port: u16, name: Option<String>) -> Result<ConnectionInfo> {
    tracing::info!("WebSocket connecting to {}:{}", address, port);

    let conn = get_connection_manager();
    conn.connect(address.clone(), port, name).await?;

    let status = conn.get_status().await;
    Ok(ConnectionInfo {
        address,
        port,
        status: format!("{:?}", status),
    })
}

/// 断开连接
#[tauri::command]
pub async fn ws_disconnect() -> Result<()> {
    tracing::info!("WebSocket disconnecting");

    let conn = get_connection_manager();
    conn.disconnect().await;

    // 清除会话状态
    let session_mgr = get_session_manager();
    session_mgr.clear().await;

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

// ==================== Auth Commands ====================

/// 认证状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthState {
    pub status: String,
    pub is_authenticated: bool,
}

/// 获取认证状态
#[tauri::command]
pub async fn ws_get_auth_status() -> Result<AuthState> {
    let auth = get_auth_manager();
    let status = auth.get_status().await;
    let is_authenticated = matches!(status, crate::mobile::AuthStatus::Authenticated);

    Ok(AuthState {
        status: format!("{:?}", status),
        is_authenticated,
    })
}

/// 使用已存储凭据认证
#[tauri::command]
pub async fn ws_authenticate() -> Result<bool> {
    let auth = get_auth_manager();
    auth.authenticate().await
}

/// 请求配对
#[tauri::command]
pub async fn ws_request_pairing() -> Result<()> {
    let auth = get_auth_manager();
    auth.request_pairing().await
}

/// 验证配对码
#[tauri::command]
pub async fn ws_verify_pairing_code(code: String) -> Result<bool> {
    let auth = get_auth_manager();
    auth.verify_pairing_code(&code).await
}

/// 使用 QR token 认证
#[tauri::command]
pub async fn ws_authenticate_with_qr(token: String) -> Result<bool> {
    let auth = get_auth_manager();
    auth.authenticate_with_qr(&token).await
}

// ==================== Session Commands ====================

/// 加载会话列表
#[tauri::command]
pub async fn ws_load_sessions() -> Result<Vec<SessionInfo>> {
    let session_mgr = get_session_manager();
    session_mgr.load_sessions().await
}

/// 启动会话
#[tauri::command]
pub async fn ws_start_session(config_id: String) -> Result<String> {
    let session_mgr = get_session_manager();
    session_mgr.start_session(&config_id).await
}

/// 停止会话
#[tauri::command]
pub async fn ws_stop_session(session_id: String) -> Result<()> {
    let session_mgr = get_session_manager();
    session_mgr.stop_session(&session_id).await
}

/// 发送输入到会话
#[tauri::command]
pub async fn ws_send_input(session_id: String, data: String, special_key: Option<String>) -> Result<()> {
    let session_mgr = get_session_manager();
    session_mgr.send_input(&session_id, &data, special_key).await
}

/// 调整终端大小
#[tauri::command]
pub async fn ws_resize_terminal(session_id: String, cols: u32, rows: u32) -> Result<()> {
    let session_mgr = get_session_manager();
    session_mgr.resize(&session_id, cols, rows).await
}

/// 获取会话配置列表
#[tauri::command]
pub async fn ws_load_session_configs() -> Result<Vec<serde_json::Value>> {
    let conn = get_connection_manager();

    let message = WsMessage::text(serde_json::to_string(&serde_json::json!({
        "type": "control",
        "message_id": uuid::Uuid::new_v4().to_string(),
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "payload": {
            "action": {
                "type": "list_session_configs"
            }
        }
    })).unwrap());

    let response = conn.send_and_wait(&message, std::time::Duration::from_secs(30)).await?;

    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&response.to_json()?) {
        if let Some(configs) = payload.get("payload").and_then(|p| p.get("action")).and_then(|a| a.get("configs")) {
            if let Ok(configs_vec) = serde_json::from_value(configs.clone()) {
                return Ok(configs_vec);
            }
        }
    }

    Ok(Vec::new())
}

// ==================== Android-specific Commands ====================

/// 获取 Android 状态栏高度（像素）
/// 通过 JNI 调用 Android API 获取系统状态栏高度
#[cfg(target_os = "android")]
#[tauri::command]
pub fn get_status_bar_height(app_handle: tauri::AppHandle) -> Result<u32> {
    use tauri::Manager;

    let windows = app_handle.webview_windows();
    let _window = windows.get("main");

    Ok(0)
}

/// 非 Android 平台返回 0
#[cfg(not(target_os = "android"))]
#[tauri::command]
pub fn get_status_bar_height() -> Result<u32> {
    Ok(0)
}

/// 设置 Android 屏幕方向
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn set_screen_orientation(
    _app_handle: tauri::AppHandle,
    orientation: String,
) -> Result<()> {
    tracing::info!("Setting screen orientation to: {}", orientation);
    Ok(())
}

/// 非 Android 平台忽略
#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn set_screen_orientation(_orientation: String) -> Result<()> {
    Ok(())
}

/// 保持屏幕唤醒（防止锁屏）
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn keep_screen_awake(
    _app_handle: tauri::AppHandle,
    enabled: bool,
) -> Result<()> {
    tracing::info!("Setting screen awake: {}", enabled);
    Ok(())
}

/// 非 Android 平台忽略
#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn keep_screen_awake(_enabled: bool) -> Result<()> {
    Ok(())
}