//! Shared Tauri Commands
//!
//! 跨平台命令 - 桌面端和移动端都可用

use crate::shared::auth::{PairingCode, PairingService};
use crate::shared::db::{Database, QuickAction, SessionConfig};
use crate::Result;
use serde::Serialize;
use std::sync::Arc;
use tauri::{Manager, State};
use tokio::sync::Mutex;

// ==================== Session Config Commands ====================

/// 创建会话配置
#[tauri::command]
pub async fn create_session_config(
    db: State<'_, Arc<Mutex<Database>>>,
    name: String,
    environment: String,
    working_dir: String,
    command: String,
    wsl_distro: Option<String>,
    tmux_session: Option<String>,
) -> Result<SessionConfig> {
    let config = SessionConfig::new(name, environment, working_dir, command);
    let mut config = config;
    config.wsl_distro = wsl_distro;
    config.tmux_session = tmux_session;

    let db = db.lock().await;
    db.create_session_config(&config)?;
    Ok(config)
}

/// 获取所有会话配置
#[tauri::command]
pub async fn list_session_configs(
    db: State<'_, Arc<Mutex<Database>>>,
) -> Result<Vec<SessionConfig>> {
    let db = db.lock().await;
    db.get_session_configs()
}

/// 获取单个会话配置
#[tauri::command]
pub async fn get_session_config(
    db: State<'_, Arc<Mutex<Database>>>,
    id: String,
) -> Result<Option<SessionConfig>> {
    let db = db.lock().await;
    db.get_session_config(&id)
}

/// 删除会话配置
#[tauri::command]
pub async fn delete_session_config(
    db: State<'_, Arc<Mutex<Database>>>,
    id: String,
) -> Result<()> {
    let db = db.lock().await;
    db.delete_session_config(&id)
}

/// 更新会话配置
#[tauri::command]
pub async fn update_session_config(
    db: State<'_, Arc<Mutex<Database>>>,
    id: String,
    name: String,
    environment: String,
    working_dir: String,
    command: String,
    wsl_distro: Option<String>,
    tmux_session: Option<String>,
    auto_start: Option<bool>,
) -> Result<SessionConfig> {
    let db = db.lock().await;
    let mut config = db
        .get_session_config(&id)?
        .ok_or_else(|| crate::AppError::NotFound(format!("Config not found: {}", id)))?;

    config.name = name;
    config.environment = environment;
    config.wsl_distro = wsl_distro;
    config.working_dir = working_dir;
    config.command = command;
    config.tmux_session = tmux_session;
    config.auto_start = auto_start.unwrap_or(false);

    db.update_session_config(&config)?;
    Ok(config)
}

// ==================== Pairing Commands ====================

/// 生成配对码
#[tauri::command]
pub async fn generate_pairing_code(
    pairing_service: State<'_, Arc<PairingService>>,
) -> Result<PairingCode> {
    Ok(pairing_service.generate_code().await)
}

/// 获取当前配对码
#[tauri::command]
pub async fn get_current_pairing_code(
    pairing_service: State<'_, Arc<PairingService>>,
) -> Result<Option<PairingCode>> {
    Ok(pairing_service.get_current_code().await)
}

/// 验证配对码
#[tauri::command]
pub async fn verify_pairing_code(
    pairing_service: State<'_, Arc<PairingService>>,
    code: String,
) -> Result<bool> {
    Ok(pairing_service.verify_code(&code).await)
}

/// 清除当前配对码（用于取消配对或配对码过期）
#[tauri::command]
pub async fn clear_pairing_code(
    pairing_service: State<'_, Arc<PairingService>>,
) -> Result<()> {
    pairing_service.clear_code().await;
    Ok(())
}

/// 获取已配对设备
#[tauri::command]
pub async fn list_paired_devices(
    db: State<'_, Arc<Mutex<Database>>>,
) -> Result<Vec<crate::shared::db::Pairing>> {
    let db = db.lock().await;
    db.get_pairings()
}

/// 移除配对设备
#[tauri::command]
pub async fn remove_paired_device(
    db: State<'_, Arc<Mutex<Database>>>,
    id: String,
) -> Result<()> {
    let db = db.lock().await;
    db.remove_pairing(&id)
}

// ==================== QR Token Commands ====================

#[derive(Debug, Clone, Serialize)]
pub struct QrConnectionInfo {
    pub token: String,
    pub host: String,
    pub port: u16,
}

#[tauri::command]
pub async fn generate_qr_code(
    qr_manager: tauri::State<'_, Arc<crate::shared::auth::QrTokenManager>>,
    db: tauri::State<'_, Arc<Mutex<Database>>>,
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

#[tauri::command]
pub async fn clear_qr_code(
    qr_manager: tauri::State<'_, Arc<crate::shared::auth::QrTokenManager>>,
) -> Result<()> {
    qr_manager.clear().await;
    tracing::info!("QR code cleared");
    Ok(())
}

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

            let config = crate::AppConfig::load(
                &app_handle.path().app_data_dir()
                    .unwrap_or_default()
                    .join("config.json")
            ).unwrap_or_default();
            let port = config.network.port;

            Ok(Some(QrConnectionInfo { token, host, port }))
        }
    }
}

#[tauri::command]
pub async fn get_qr_token_ttl(
    db: tauri::State<'_, Arc<Mutex<Database>>>,
) -> Result<u64> {
    let db = db.lock().await;
    match db.get_setting("qr_token_ttl") {
        Ok(Some(value)) => value.parse::<u64>().map_err(|e| crate::AppError::Config(e.to_string())),
        _ => Ok(300),
    }
}

#[tauri::command]
pub async fn set_qr_token_ttl(
    db: tauri::State<'_, Arc<Mutex<Database>>>,
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
    db: State<'_, Arc<Mutex<Database>>>,
) -> Result<Vec<QuickAction>> {
    let db = db.lock().await;
    db.get_quick_actions()
}

/// 创建快捷指令
#[tauri::command]
pub async fn create_quick_action(
    db: State<'_, Arc<Mutex<Database>>>,
    name: String,
    content: String,
    icon: Option<String>,
    color: Option<String>,
) -> Result<QuickAction> {
    let mut action = QuickAction::new(name, content);
    action.icon = icon;
    action.color = color;

    let db = db.lock().await;
    db.create_quick_action(&action)?;
    Ok(action)
}

/// 更新快捷指令
#[tauri::command]
pub async fn update_quick_action(
    db: State<'_, Arc<Mutex<Database>>>,
    id: String,
    name: String,
    content: String,
    icon: Option<String>,
    color: Option<String>,
) -> Result<QuickAction> {
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
    db: State<'_, Arc<Mutex<Database>>>,
    id: String,
) -> Result<()> {
    let db = db.lock().await;
    db.delete_quick_action(&id)
}

/// 获取所有数据库设置
#[tauri::command]
pub async fn get_all_db_settings(
    db: State<'_, Arc<Mutex<Database>>>,
) -> Result<Vec<crate::shared::db::Setting>> {
    let db = db.lock().await;
    db.get_all_settings()
}

/// 设置数据库配置项
#[tauri::command]
pub async fn set_db_setting(
    db: State<'_, Arc<Mutex<Database>>>,
    key: String,
    value: String,
) -> Result<()> {
    let db = db.lock().await;
    db.set_setting(&key, &value)
}

// ==================== Settings Commands ====================

/// 获取应用设置
#[tauri::command]
pub async fn get_app_settings(
    app_handle: tauri::AppHandle,
) -> crate::Result<crate::shared::system::config::AppConfig> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map(|p| p.join("config.json"))
        .map_err(|e: tauri::Error| crate::AppError::Config(e.to_string()))?;

    crate::shared::system::config::AppConfig::load(&config_path)
        .map_err(|e| crate::AppError::Config(e.to_string()))
}

/// 保存应用设置
#[tauri::command]
pub async fn save_app_settings(
    app_handle: tauri::AppHandle,
    settings: crate::shared::system::config::AppConfig,
) -> crate::Result<()> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map(|p| p.join("config.json"))
        .map_err(|e: tauri::Error| crate::AppError::Config(e.to_string()))?;

    settings.save(&config_path)?;

    tracing::info!("App settings saved to {:?}", config_path);
    Ok(())
}

// ==================== Utility Commands ====================

/// 测试命令
#[tauri::command]
pub fn ping() -> String {
    "pong".to_string()
}

/// 获取应用版本
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 获取自应用启动以来的耗时（毫秒）
/// 用于前端计算从进程启动到页面渲染完成的总耗时
#[tauri::command]
pub fn get_startup_time(start_time: State<'_, crate::AppStartTime>) -> u64 {
    start_time.0.elapsed().as_millis() as u64
}

/// 获取本地 IP 地址
#[tauri::command]
pub fn get_local_ip_addresses() -> Vec<String> {
    local_ip_address::list_afinet_netifas()
        .map(|interfaces| {
            interfaces
                .into_iter()
                .filter(|(_, ip)| {
                    match ip {
                        std::net::IpAddr::V4(ipv4) => {
                            !ipv4.is_loopback() && !ipv4.is_link_local()
                        }
                        std::net::IpAddr::V6(ipv6) => {
                            !ipv6.is_loopback()
                        }
                    }
                })
                .map(|(_, ip)| ip.to_string())
                .collect()
        })
        .unwrap_or_default()
}