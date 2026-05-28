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
    MobileEvent, get_terminal_manager,
};
use crate::shared::system::error_boundary::spawn_with_error_boundary;
use crate::shared::model::message::Message;
use crate::shared::enums::control::SessionControlAction;
use crate::shared::enums::special_key::SpecialKey;

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
    // 同时将数据写入 TerminalBuffer（由 Rust 后端管理缓冲区）
    if !OUTPUT_FORWARDING_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let conn_fwd = get_connection_manager();
        let mut event_rx = conn_fwd.handler().subscribe();
        let app_clone = app_handle.clone();
        let terminal_mgr = get_terminal_manager();
        spawn_with_error_boundary("output_forwarder", async move {
            tracing::info!("[OutputForwarder] Started forwarding output events");
            while let Ok(event) = event_rx.recv().await {
                if let MobileEvent::Output { session_id, data, is_waiting, index: global_index } = event {
                    tracing::debug!("[MobileCommands] Output event received: session_id={}, data_len={}, global_index={}", session_id, data.len(), global_index);
                    // 1. 写入 TerminalBuffer（Rust 后端管理缓冲区，负责 Base64 解码和字节限制）
                    // 注意：这里的 index 应该使用桌面端传来的全局索引，而不是缓冲区自己的索引
                    terminal_mgr.write_output_with_index(&session_id, data.clone(), is_waiting, global_index).await;
                    tracing::debug!("[MobileCommands] Written to buffer with global_index={}", global_index);

                    // 2. 转发解码后的数据给前端（前端不再需要 Base64 解码）
                    let decoded_data = base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        &data,
                    ).unwrap_or_default();
                    let decoded_str = String::from_utf8_lossy(&decoded_data).to_string();

                    let emit_result = app_clone.emit("ws_output", serde_json::json!({
                        "session_id": session_id,
                        "data": decoded_str,
                        "is_waiting": is_waiting,
                        "index": global_index,
                    }));
                    if let Err(e) = emit_result {
                        tracing::error!("[MobileCommands] Failed to emit ws_output: {}", e);
                    } else {
                        tracing::debug!("[MobileCommands] Emitted ws_output: session_id={}", session_id);
                    }
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
    use crate::shared::enums::control::SessionControlAction;

    tracing::info!("[ws_load_sessions] Sending ListSessions request");
    let conn = get_connection_manager();
    let message = Message::session_control(SessionControlAction::ListSessions, None);

    let response = conn.send_and_wait(&message, std::time::Duration::from_secs(15)).await?;

    // 解析 SessionControl 响应中的会话列表
    if let Message::SessionControl { payload, .. } = &response {
        if let SessionControlAction::SessionList { sessions } = &payload.action {
            let count = sessions.len();
            tracing::info!("[ws_load_sessions] Response OK, {} sessions", count);
            // 转换为 serde_json::Value
            let list = serde_json::to_value(sessions)?;
            return Ok(list);
        }
    }

    Ok(Vec::new())
}

/// 订阅会话，开始接收该会话的输出
/// 使用桌面端的 Message::Subscribe 消息，支持指定起始序号用于历史回放
#[tauri::command]
pub async fn ws_join_session(session_id: String) -> Result<()> {
    tracing::info!("[ws_join_session] session_id={}", session_id);
    let conn = get_connection_manager();

    // 使用 Message::Subscribe 消息类型
    let message = Message::subscribe(&session_id, None);

    conn.send_and_wait(&message, std::time::Duration::from_secs(10)).await?;
    tracing::info!("[ws_join_session] Subscribed to session successfully: {}", session_id);
    Ok(())
}

/// 取消订阅会话，停止接收该会话的输出
/// 使用桌面端的 Message::Unsubscribe 消息
#[tauri::command]
pub async fn ws_leave_session(session_id: String) -> Result<()> {
    tracing::info!("[ws_leave_session] session_id={}", session_id);
    let conn = get_connection_manager();

    // 使用 Message::Unsubscribe 消息类型
    let message = Message::unsubscribe(&session_id);

    conn.send_and_wait(&message, std::time::Duration::from_secs(10)).await?;
    tracing::info!("[ws_leave_session] Unsubscribed from session successfully: {}", session_id);
    Ok(())
}

/// 带起始序号的订阅会话（用于断线重连后从断点继续）
///
/// - 首次订阅：`start_seq = None` 或 `0` → 从头接收所有历史
/// - 断线重连：使用之前记录的最大 index → 从断点继续接收
/// - 切换会话：使用当前缓冲区最大 index → 避免重复接收
#[tauri::command]
pub async fn ws_subscribe_session(session_id: String, start_seq: Option<u64>) -> Result<()> {
    tracing::info!("[ws_subscribe_session] session_id={}, start_seq={:?}", session_id, start_seq);
    let conn = get_connection_manager();

    // 使用 Message::Subscribe，带可选的起始序号
    let message = Message::subscribe(&session_id, start_seq);

    conn.send_and_wait(&message, std::time::Duration::from_secs(10)).await?;
    tracing::info!("[ws_subscribe_session] Subscribed to session with start_seq={:?}: {}", start_seq, session_id);
    Ok(())
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

/// 发送输入到会话（异步模式，不等待服务端确认）
/// 实现真正的终端输入体验：发送后立即返回，不阻塞 UI
#[tauri::command]
pub async fn ws_send_input_async(session_id: String, data: String, special_key: Option<String>) -> Result<()> {
    tracing::debug!("[ws_send_input_async] session_id={}, data_len={}, has_special_key={}",
        session_id, data.len(), special_key.is_some());
    let conn = get_connection_manager();

    // 裁剪尾部换行
    let trimmed_data = if special_key.as_deref() == Some("enter") {
        data.trim_end_matches('\n').trim_end_matches('\r').to_string()
    } else {
        data
    };

    // 使用 Message::input 构造输入消息
    let special_key_enum = special_key.as_ref().and_then(|k| {
        match k.as_str() {
            "enter" => Some(SpecialKey::Enter),
            "ctrl_c" => Some(SpecialKey::CtrlC),
            "ctrl_d" => Some(SpecialKey::CtrlD),
            "ctrl_z" => Some(SpecialKey::CtrlZ),
            "tab" => Some(SpecialKey::Tab),
            "esc" | "escape" => Some(SpecialKey::Escape),
            "backspace" => Some(SpecialKey::Backspace),
            "delete" => Some(SpecialKey::Delete),
            "up" => Some(SpecialKey::ArrowUp),
            "down" => Some(SpecialKey::ArrowDown),
            "left" => Some(SpecialKey::ArrowLeft),
            "right" => Some(SpecialKey::ArrowRight),
            "home" => Some(SpecialKey::Home),
            "end" => Some(SpecialKey::End),
            "page_up" => Some(SpecialKey::PageUp),
            "page_down" => Some(SpecialKey::PageDown),
            _ => None,
        }
    });

    let message = Message::input(&session_id, &trimmed_data, special_key_enum);

    // fire-and-forget：只确保发送到缓冲区，不等待服务端 ACK
    // 这样用户体验像真正的终端：输入后立即返回
    conn.send(&message).await?;

    tracing::debug!("[ws_send_input_async] sent to buffer (no ACK waiting)");
    Ok(())
}

/// 发送消息（不等待响应）
/// 通用接口，接受 JSON 格式的消息
#[tauri::command]
pub async fn ws_send_message(message_type: String, payload: serde_json::Value) -> Result<()> {
    let conn = get_connection_manager();

    // 尝试将 payload 转换为对应的 Message 类型
    let result = convert_json_to_message(&message_type, payload);
    match result {
        Some(message) => conn.send(&message).await,
        None => Err(crate::AppError::Parse(format!("Unsupported message type: {}", message_type)))
    }
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

    // 尝试将 payload 转换为对应的 Message 类型
    let message = match convert_json_to_message(&message_type, payload) {
        Some(m) => m,
        None => return Err(crate::AppError::Parse(format!("Unsupported message type: {}", message_type))),
    };

    let response = conn.send_and_wait(&message, timeout).await?;
    let json_str = response.to_json()?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| crate::AppError::Parse(e.to_string()))?;

    Ok(parsed)
}

/// 将 JSON payload 转换为 Message 类型
fn convert_json_to_message(message_type: &str, payload: serde_json::Value) -> Option<Message> {
    match message_type {
        "session_control" | "control" => {
            let action_type = payload.get("action")
                .and_then(|a| a.get("type"))
                .and_then(|t| t.as_str())?;

            let action = match action_type {
                "list_sessions" => SessionControlAction::ListSessions,
                "start_session" => {
                    let config_id = payload.get("action")
                        .and_then(|a| a.get("config_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    SessionControlAction::StartSession { config_id }
                }
                "stop_session" => {
                    let session_id = payload.get("action")
                        .and_then(|a| a.get("session_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    SessionControlAction::StopSession { session_id }
                }
                "remove_session" => {
                    let session_id = payload.get("action")
                        .and_then(|a| a.get("session_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    SessionControlAction::RemoveSession { session_id }
                }
                "resize_session" => {
                    let session_id = payload.get("action")
                        .and_then(|a| a.get("session_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let cols = payload.get("action")
                        .and_then(|a| a.get("cols"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(80) as u16;
                    let rows = payload.get("action")
                        .and_then(|a| a.get("rows"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(24) as u16;
                    SessionControlAction::ResizeSession { session_id, cols, rows }
                }
                _ => return None,
            };
            Some(Message::session_control(action, None))
        }
        "session_config" => {
            use crate::shared::enums::control::SessionConfigAction;

            let action_type = payload.get("action")
                .and_then(|a| a.get("type"))
                .and_then(|t| t.as_str())?;

            let action = match action_type {
                "list_session_configs" => SessionConfigAction::ListSessionConfigs,
                "list_quick_actions" => SessionConfigAction::ListQuickActions,
                _ => return None,
            };
            Some(Message::session_config(action, None))
        }
        "input" => {
            let session_id = payload.get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let data = payload.get("payload")
                .and_then(|p| p.get("data"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Some(Message::input(session_id, data, None))
        }
        "subscribe" => {
            let session_id = payload.get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let start_seq = payload.get("start_seq")
                .and_then(|v| v.as_u64());
            Some(Message::subscribe(session_id, start_seq))
        }
        "unsubscribe" => {
            let session_id = payload.get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Some(Message::unsubscribe(session_id))
        }
        _ => None,
    }
}

/// 调整终端大小
///
/// 将移动端终端的实际尺寸 (cols, rows) 通过 WebSocket 发送到桌面端。
/// 桌面端收到后更新 PTY 尺寸，使输出按移动端屏幕宽度排版，
/// 避免因宽度不匹配导致 \r 光标定位错乱、多行输出堆叠等问题。
#[tauri::command]
pub async fn ws_resize_terminal(session_id: String, cols: u32, rows: u32) -> Result<()> {
    use crate::shared::enums::control::SessionControlAction;

    tracing::info!("[ws_resize_terminal] session_id={}, cols={}, rows={}", session_id, cols, rows);
    let conn = get_connection_manager();

    let action = SessionControlAction::ResizeSession {
        session_id: session_id.clone(),
        cols: cols as u16,
        rows: rows as u16,
    };
    let message = Message::session_control(action, Some(&session_id));

    conn.send(&message).await
}

/// 获取会话配置列表
#[tauri::command]
pub async fn ws_load_session_configs() -> Result<Vec<serde_json::Value>> {
    use crate::shared::enums::control::SessionConfigAction;

    tracing::info!("[ws_load_session_configs] Sending ListSessionConfigs request");
    let conn = get_connection_manager();

    let message = Message::session_config(SessionConfigAction::ListSessionConfigs, None);

    let response = conn.send_and_wait(&message, std::time::Duration::from_secs(30)).await?;

    // 从 Message::SessionConfig 响应中提取会话配置列表
    if let Message::SessionConfig { payload, .. } = &response {
        if let SessionConfigAction::SessionConfigList { configs } = &payload.action {
            let count = configs.len();
            tracing::info!("[ws_load_session_configs] Response OK, {} configs", count);
            let configs_vec = serde_json::to_value(configs)?;
            return Ok(configs_vec);
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

// ==================== Terminal Commands ====================

/// 获取会话的完整输出历史（用于首次连接或断线重连后恢复数据）
#[tauri::command]
pub async fn ws_get_terminal_history(session_id: String) -> Result<crate::mobile::TerminalHistory> {
    let terminal_mgr = get_terminal_manager();
    let history = terminal_mgr.get_history(&session_id).await;
    tracing::debug!(
        "[ws_get_terminal_history] session_id={}, events_count={}, current_index={}",
        session_id,
        history.events.len(),
        history.current_index
    );
    Ok(history)
}

/// 订阅终端（记录当前索引位置，用于增量获取）
#[tauri::command]
pub async fn ws_subscribe_terminal(session_id: String) -> Result<usize> {
    let terminal_mgr = get_terminal_manager();
    let index = terminal_mgr.subscribe(&session_id).await;
    tracing::debug!("[ws_subscribe_terminal] session_id={}, index={}", session_id, index);
    Ok(index)
}

/// 取消订阅终端
#[tauri::command]
pub async fn ws_unsubscribe_terminal(session_id: String) -> Result<()> {
    let terminal_mgr = get_terminal_manager();
    terminal_mgr.unsubscribe(&session_id).await;
    tracing::debug!("[ws_unsubscribe_terminal] session_id={}", session_id);
    Ok(())
}

/// 获取增量输出（自上次获取之后的新数据）
#[tauri::command]
pub async fn ws_get_terminal_incremental(
    session_id: String,
) -> Result<Option<crate::mobile::TerminalIncrementalOutput>> {
    let terminal_mgr = get_terminal_manager();
    let incremental = terminal_mgr.get_incremental(&session_id).await;
    if let Some(ref inc) = incremental {
        tracing::debug!(
            "[ws_get_terminal_incremental] session_id={}, new_events={}, current_index={}",
            session_id,
            inc.events.len(),
            inc.current_index
        );
    }
    Ok(incremental)
}

/// 更新订阅者的索引位置（在增量数据消费后调用）
#[tauri::command]
pub async fn ws_update_terminal_index(session_id: String, index: usize) -> Result<()> {
    let terminal_mgr = get_terminal_manager();
    terminal_mgr.update_subscriber_index(&session_id, index).await;
    tracing::debug!("[ws_update_terminal_index] session_id={}, index={}", session_id, index);
    Ok(())
}

/// 清空终端缓冲区
#[tauri::command]
pub async fn ws_clear_terminal_buffer(session_id: String) -> Result<()> {
    let terminal_mgr = get_terminal_manager();
    terminal_mgr.clear_buffer(&session_id).await;
    tracing::debug!("[ws_clear_terminal_buffer] session_id={}", session_id);
    Ok(())
}

/// 清除所有终端缓冲区（断开连接时调用）
#[tauri::command]
pub async fn ws_clear_all_terminal_buffers() -> Result<()> {
    let terminal_mgr = get_terminal_manager();
    terminal_mgr.clear_all().await;
    tracing::debug!("[ws_clear_all_terminal_buffers] All buffers cleared");
    Ok(())
}