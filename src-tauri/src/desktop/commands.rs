//! Desktop-only Tauri Commands
//!
//! 桌面端专用命令 - 移动端不可用

use crate::desktop::session::SessionConfigManager;
use crate::Result;
use std::sync::Arc;
use tauri::{Manager, State};

// ==================== Session Config Commands ====================

/// 创建会话配置
#[tauri::command]
pub async fn create_session_config(
    config_manager: State<'_, Arc<SessionConfigManager>>,
    name: String,
    environment: String,
    working_dir: String,
    command: String,
    wsl_distro: Option<String>,
    tmux_session: Option<String>,
) -> Result<crate::shared::db::SessionConfig> {
    config_manager
        .create_config_full(
            name,
            environment,
            wsl_distro,
            working_dir,
            command,
            tmux_session,
            false,
        )
        .await
}

/// 获取所有会话配置
#[tauri::command]
pub async fn list_session_configs(
    config_manager: State<'_, Arc<SessionConfigManager>>,
) -> Result<Vec<crate::shared::db::SessionConfig>> {
    config_manager.list_configs().await
}

/// 获取单个会话配置
#[tauri::command]
pub async fn get_session_config(
    config_manager: State<'_, Arc<SessionConfigManager>>,
    id: String,
) -> Result<Option<crate::shared::db::SessionConfig>> {
    config_manager.get_config(&id).await
}

/// 删除会话配置
#[tauri::command]
pub async fn delete_session_config(
    config_manager: State<'_, Arc<SessionConfigManager>>,
    id: String,
) -> Result<()> {
    config_manager.delete_config(&id).await
}

/// 更新会话配置
#[tauri::command]
pub async fn update_session_config(
    config_manager: State<'_, Arc<SessionConfigManager>>,
    id: String,
    name: String,
    environment: String,
    working_dir: String,
    command: String,
    wsl_distro: Option<String>,
    tmux_session: Option<String>,
    auto_start: Option<bool>,
) -> Result<crate::shared::db::SessionConfig> {
    config_manager
        .update_config(
            &id,
            Some(name),
            Some(environment),
            wsl_distro,
            Some(working_dir),
            Some(command),
            tmux_session,
            auto_start,
        )
        .await
}

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
pub async fn get_connected_devices() -> Result<Vec<crate::desktop::server::DeviceConnectionInfo>> {
    let manager = crate::desktop::websocket_manager::WebSocketManager::global();
    let clients = manager.list_clients().await;
    let devices = clients
        .into_iter()
        .map(|c| crate::desktop::server::DeviceConnectionInfo {
            addr: c.addr,
            device_id: c.client_id,
            session_count: 0,
        })
        .collect();
    Ok(devices)
}

// ==================== QR Token Commands ====================

#[derive(Debug, Clone, serde::Serialize)]
pub struct QrConnectionInfo {
    pub token: String,
    pub host: String,
    pub port: u16,
}

/// 生成二维码
#[tauri::command]
pub async fn generate_qr_code(
    qr_manager: tauri::State<'_, Arc<crate::shared::auth::QrTokenManager>>,
    db: tauri::State<'_, Arc<tokio::sync::Mutex<crate::shared::db::Database>>>,
) -> Result<String> {
    let ttl = {
        let db = db.lock().await;
        db.get_setting("qr_token_ttl")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(300)
    };

    let token = qr_manager.generate(ttl).await;
    tracing::info!("QR code generated, TTL: {}s", ttl);
    Ok(token)
}

/// 清除二维码
#[tauri::command]
pub async fn clear_qr_code(
    qr_manager: tauri::State<'_, Arc<crate::shared::auth::QrTokenManager>>,
) -> Result<()> {
    qr_manager.clear().await;
    tracing::info!("QR code cleared");
    Ok(())
}

/// 获取二维码连接信息
#[tauri::command]
pub async fn get_qr_connection_info(
    qr_manager: tauri::State<'_, Arc<crate::shared::auth::QrTokenManager>>,
    app_handle: tauri::AppHandle,
    host: Option<String>,
) -> Result<Option<QrConnectionInfo>> {
    let active = qr_manager.get_active().await;
    match active {
        None => Ok(None),
        Some((token, _ttl, _remaining)) => {
            let host = host.or_else(|| {
                crate::shared::system::commands::get_local_ip_addresses()
                    .into_iter()
                    .find(|ip| !ip.starts_with("127.") && !ip.starts_with("169.254."))
            }).unwrap_or_else(|| "127.0.0.1".to_string());

            let config = crate::shared::system::config::AppConfig::load(
                &app_handle.path().app_data_dir()
                    .unwrap_or_default()
                    .join("config.json")
            ).unwrap_or_default();
            let port = config.network.port;

            Ok(Some(QrConnectionInfo { token, host, port }))
        }
    }
}

/// 获取 QR token TTL
#[tauri::command]
pub async fn get_qr_token_ttl(
    db: tauri::State<'_, Arc<tokio::sync::Mutex<crate::shared::db::Database>>>,
) -> Result<u64> {
    let db = db.lock().await;
    match db.get_setting("qr_token_ttl") {
        Ok(Some(value)) => value.parse::<u64>().map_err(|e| crate::AppError::Config(e.to_string())),
        _ => Ok(300),
    }
}

/// 设置 QR token TTL
#[tauri::command]
pub async fn set_qr_token_ttl(
    db: tauri::State<'_, Arc<tokio::sync::Mutex<crate::shared::db::Database>>>,
    seconds: u64,
) -> Result<()> {
    let db = db.lock().await;
    db.set_setting("qr_token_ttl", &seconds.to_string())
        .map_err(|e| crate::AppError::Config(e.to_string()))
}

// ==================== Quick Actions Commands ====================

/// 获取快捷指令
#[tauri::command]
pub async fn list_quick_actions(
    db: tauri::State<'_, Arc<tokio::sync::Mutex<crate::shared::db::Database>>>,
) -> Result<Vec<crate::shared::db::QuickAction>> {
    let db = db.lock().await;
    db.get_quick_actions()
}

/// 创建快捷指令
#[tauri::command]
pub async fn create_quick_action(
    db: tauri::State<'_, Arc<tokio::sync::Mutex<crate::shared::db::Database>>>,
    name: String,
    content: String,
    icon: Option<String>,
    color: Option<String>,
) -> Result<crate::shared::db::QuickAction> {
    let mut action = crate::shared::db::QuickAction::new(name, content);
    action.icon = icon;
    action.color = color;

    let db = db.lock().await;
    db.create_quick_action(&action)?;
    Ok(action)
}

/// 更新快捷指令
#[tauri::command]
pub async fn update_quick_action(
    db: tauri::State<'_, Arc<tokio::sync::Mutex<crate::shared::db::Database>>>,
    id: String,
    name: String,
    content: String,
    icon: Option<String>,
    color: Option<String>,
) -> Result<crate::shared::db::QuickAction> {
    let db = db.lock().await;
    let mut action = db.get_quick_actions()?
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| crate::AppError::NotFound(format!("Quick action not found: {}", id)))?;

    action.name = name;
    action.content = content;
    action.icon = icon;
    action.color = color;

    db.update_quick_action(&action)?;
    Ok(action)
}

/// 删除快捷指令
#[tauri::command]
pub async fn delete_quick_action(
    db: tauri::State<'_, Arc<tokio::sync::Mutex<crate::shared::db::Database>>>,
    id: String,
) -> Result<()> {
    let db = db.lock().await;
    db.delete_quick_action(&id)
}

// ==================== Database Settings Commands ====================

/// 获取所有数据库设置
#[tauri::command]
pub async fn get_all_db_settings(
    db: tauri::State<'_, Arc<tokio::sync::Mutex<crate::shared::db::Database>>>,
) -> Result<Vec<crate::shared::db::Setting>> {
    let db = db.lock().await;
    db.get_all_settings()
}

/// 设置数据库配置项
#[tauri::command]
pub async fn set_db_setting(
    db: tauri::State<'_, Arc<tokio::sync::Mutex<crate::shared::db::Database>>>,
    key: String,
    value: String,
) -> Result<()> {
    let db = db.lock().await;
    db.set_setting(&key, &value)
}