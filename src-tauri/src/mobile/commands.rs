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
use anyhow::Context;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tracing;

use crate::Result;
use crate::mobile::{
    AuthCredentials, AuthManager, ConnectionManager,
    SessionInfo, SessionManager,
    MobileEvent,
};
use crate::shared::system::error_boundary::spawn_with_error_boundary;
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

/// 输出事件转发标志（只启动一次）
static OUTPUT_FORWARDING_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
    let _ = app_handle.emit("ws_connecting", serde_json::json!({
        "address": address,
        "port": port,
    }));
    tracing::info!("Emitted ws_connecting event");

    // 启动输出事件转发（仅一次），将 MobileEvent::Output 转发为 Tauri ws_output 事件
    if !OUTPUT_FORWARDING_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let conn_fwd = get_connection_manager();
        let mut event_rx = conn_fwd.handler().subscribe();
        let app_clone = app_handle.clone();
        spawn_with_error_boundary("output_forwarder", async move {
            tracing::info!("[OutputForwarder] Started forwarding output events");
            while let Ok(event) = event_rx.recv().await {
                if let MobileEvent::Output { session_id, data, is_waiting } = event {
                    let _ = app_clone.emit("ws_output", serde_json::json!({
                        "session_id": session_id,
                        "data": data,
                        "is_waiting": is_waiting,
                    }));
                }
            }
            tracing::warn!("[OutputForwarder] Event channel closed");
        });
    }

    let conn = get_connection_manager();
    tracing::info!("Calling conn.connect()...");

    match conn.connect(app_handle.clone(), address.clone(), port, name).await {
        Ok(_) => {
            tracing::info!("conn.connect() returned Ok");
        }
        Err(e) => {
            tracing::error!("conn.connect() returned error: {}", e);
            let _ = app_handle.emit("ws_error", serde_json::json!({
                "message": format!("Connection failed: {}", e)
            }));
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
    let _ = app_handle.emit("ws_disconnected", serde_json::json!({
        "reason": "User initiated disconnect"
    }));

    // 清除会话状态 - 使用公共方法
    let session_mgr = get_session_manager();
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

// ==================== Reconnection Commands ====================

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

/// 使用 JWT token 认证（重连时使用已存储的 session_token）
#[tauri::command]
pub async fn ws_authenticate(app_handle: AppHandle, session_token: String) -> Result<bool> {
    tracing::info!("[ws_authenticate] called, token length={}", session_token.len());
    let auth = get_auth_manager();
    let result = auth.authenticate_with_token(&session_token).await?;

    if result {
        let _ = app_handle.emit("ws_auth_success", ());
        let _ = app_handle.emit("ws_paired", ());
    }

    Ok(result)
}

/// 请求配对
#[tauri::command]
pub async fn ws_request_pairing(app_handle: AppHandle) -> Result<()> {
    eprintln!("[ws_request_pairing] COMMAND ENTERED!");
    tracing::info!("[ws_request_pairing] command entered");

    let auth = get_auth_manager();
    tracing::info!("[ws_request_pairing] got auth manager, calling request_pairing...");
    match auth.request_pairing().await {
        Ok(()) => {
            tracing::info!("[ws_request_pairing] request_pairing OK, emitting event");
            let _ = app_handle.emit("ws_pairing_request", ());
            Ok(())
        }
        Err(e) => {
            tracing::error!("[ws_request_pairing] request_pairing failed: {}", e);
            Err(e)
        }
    }
}

/// 验证配对码，成功后返回凭据（含 JWT token）
#[tauri::command]
pub async fn ws_verify_pairing_code(app_handle: AppHandle, code: String) -> Result<Option<crate::mobile::auth::AuthCredentials>> {
    let auth = get_auth_manager();
    let result = auth.verify_pairing_code(&code).await?;

    if result {
        let _ = app_handle.emit("ws_pairing_verified", ());
        let _ = app_handle.emit("ws_paired", ());
        // 返回存储的凭据，前端持久化到 localStorage
        Ok(auth.get_credentials().await)
    } else {
        let _ = app_handle.emit("ws_auth_failed", serde_json::json!({
            "reason": "Pairing verification failed"
        }));
        Ok(None)
    }
}

/// 使用 QR token 认证
#[tauri::command]
pub async fn ws_authenticate_with_qr(app_handle: AppHandle, token: String) -> Result<Option<AuthCredentials>> {
    let auth = get_auth_manager();
    let result = auth.authenticate_with_qr(&token).await?;

    if result {
        let _ = app_handle.emit("ws_pairing_verified", ());
        let _ = app_handle.emit("ws_paired", ());
        return Ok(auth.get_credentials().await);
    }

    Ok(None)
}

// ==================== Session Commands ====================

/// 加载会话列表（从桌面端拉取真实会话）
#[tauri::command]
pub async fn ws_load_sessions() -> Result<Vec<serde_json::Value>> {
    tracing::info!("[ws_load_sessions] Sending ListSessions request");
    let conn = get_connection_manager();
    let message = crate::shared::websocket::WsMessage::text(serde_json::to_string(&serde_json::json!({
        "type": "control",
        "message_id": uuid::Uuid::new_v4().to_string(),
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "payload": {
            "action": {
                "type": "list_sessions"
            }
        }
    })).unwrap());

    let response = conn.send_and_wait(&message, std::time::Duration::from_secs(15)).await?;

    if let crate::shared::websocket::WsMessage::Text { payload: text_payload, .. } = &response {
        if let Ok(inner) = serde_json::from_str::<serde_json::Value>(&text_payload.content) {
            if let Some(sessions) = inner.get("payload")
                .and_then(|p| p.get("action"))
                .and_then(|a| a.get("sessions"))
            {
                let count = sessions.as_array().map(|a| a.len()).unwrap_or(0);
                tracing::info!("[ws_load_sessions] Response OK, {} sessions", count);
                if let Ok(list) = serde_json::from_value(sessions.clone()) {
                    return Ok(list);
                }
            }
        }
    }

    Ok(Vec::new())
}

/// 启动会话
#[tauri::command]
pub async fn ws_start_session(config_id: String, session_name: Option<String>) -> Result<String> {
    let session_mgr = get_session_manager();
    session_mgr.start_session(&config_id, session_name.as_deref()).await
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
    let session_mgr = get_session_manager();
    session_mgr.remove_session(&session_id).await
}

/// 发送输入到会话
#[tauri::command]
pub async fn ws_send_input(session_id: String, data: String, special_key: Option<String>) -> Result<()> {
    tracing::info!("[ws_send_input] session_id={}, data_len={}, has_special_key={}",
        session_id, data.len(), special_key.is_some());
    let conn = get_connection_manager();

    // 裁剪尾部换行，避免 data 末尾已含换行时与 special_key=Enter 重复执行
    let trimmed_data = if special_key.as_deref() == Some("enter") {
        data.trim_end_matches('\n').trim_end_matches('\r').to_string()
    } else {
        data
    };

    let payload = serde_json::json!({
        "type": "input",
        "message_id": uuid::Uuid::new_v4().to_string(),
        "session_id": session_id,
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "payload": {
            "data": trimmed_data,
            "special_key": special_key,
        },
    });
    let msg_json = serde_json::to_string(&payload).unwrap_or_default();
    let preview_len = msg_json.len().min(200);
    tracing::info!("[ws_send_input] sending message: {}", &msg_json[..preview_len]);
    let message = crate::shared::websocket::WsMessage::text(serde_json::to_string(&payload).unwrap());

    // 使用 send_and_wait 等待桌面端确认，确保输入已送达
    conn.send_and_wait(&message, std::time::Duration::from_secs(5)).await
        .with_context(|| format!("Input not acknowledged by desktop (session={})", &session_id[..session_id.len().min(16)]))
        .map_err(|e| {
            tracing::error!("[ws_send_input] failed: {:?}", e);
            crate::AppError::WebSocket(e.to_string())
        })?;
    tracing::info!("[ws_send_input] desktop ACK received");
    Ok(())
}

/// 发送消息（不等待响应）
#[tauri::command]
pub async fn ws_send_message(message_type: String, payload: serde_json::Value) -> Result<()> {
    let conn = get_connection_manager();
    let message = WsMessage::text(serde_json::to_string(&serde_json::json!({
        "type": message_type,
        "message_id": uuid::Uuid::new_v4().to_string(),
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "payload": payload,
    })).unwrap());

    conn.send(&message).await
}

/// 发送消息并等待响应
#[tauri::command]
pub async fn ws_send_and_wait(
    message_type: String,
    payload: serde_json::Value,
    timeout_secs: Option<u64>,
) -> Result<serde_json::Value> {
    let conn = get_connection_manager();
    let timeout = std::time::Duration::from_secs(timeout_secs.unwrap_or(30));

    let message = WsMessage::text(serde_json::to_string(&serde_json::json!({
        "type": message_type,
        "message_id": uuid::Uuid::new_v4().to_string(),
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "payload": payload,
    })).unwrap());

    let response = conn.send_and_wait(&message, timeout).await?;
    let json_str = response.to_json()?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| crate::AppError::Parse(e.to_string()))?;

    Ok(parsed)
}

/// 调整终端大小
///
/// 将移动端终端的实际尺寸 (cols, rows) 通过 WebSocket 发送到桌面端。
/// 桌面端收到后更新 PTY 尺寸，使输出按移动端屏幕宽度排版，
/// 避免因宽度不匹配导致 \r 光标定位错乱、多行输出堆叠等问题。
#[tauri::command]
pub async fn ws_resize_terminal(session_id: String, cols: u32, rows: u32) -> Result<()> {
    tracing::info!("[ws_resize_terminal] session_id={}, cols={}, rows={}", session_id, cols, rows);
    let conn = get_connection_manager();
    let payload = serde_json::json!({
        "type": "control",
        "message_id": uuid::Uuid::new_v4().to_string(),
        "session_id": session_id,
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "payload": {
            "action": {
                "type": "resize_session",
                "session_id": session_id,
                "cols": cols,
                "rows": rows,
            }
        },
    });
    let message = crate::shared::websocket::WsMessage::text(
        serde_json::to_string(&payload).unwrap()
    );
    conn.send(&message).await
}

/// 获取会话配置列表
#[tauri::command]
pub async fn ws_load_session_configs() -> Result<Vec<serde_json::Value>> {
    tracing::info!("[ws_load_session_configs] Sending ListSessionConfigs request");
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

    // 从 WsMessage 的 payload.content 中提取业务响应 JSON
    if let WsMessage::Text { payload: text_payload, .. } = &response {
        if let Ok(inner) = serde_json::from_str::<serde_json::Value>(&text_payload.content) {
            if let Some(configs) = inner.get("payload").and_then(|p| p.get("action")).and_then(|a| a.get("configs")) {
                let count = configs.as_array().map(|a| a.len()).unwrap_or(0);
                tracing::info!("[ws_load_session_configs] Response OK, {} configs", count);
                if let Ok(configs_vec) = serde_json::from_value(configs.clone()) {
                    return Ok(configs_vec);
                }
            }
        }
    }

    tracing::warn!("[ws_load_session_configs] Failed to parse response, returning empty");
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