//! Shared System Commands
//!
//! 桌面端和移动端共享的系统命令
//!
//! 桌面端专用命令在 desktop/commands.rs
//! 移动端专用命令在 mobile/commands/mobile_commands.rs

use crate::utils::auth::PairingCode;
use crate::db::Database;
use crate::Result;
use std::sync::Arc;
use tauri::{Manager, State};
use tokio::sync::Mutex;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::server::services::pairing_service::PairingService;
#[cfg(any(target_os = "android", target_os = "ios"))]
use crate::mobile::remote::PairingService;

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
    Ok(pairing_service.verify_and_consume_code(&code).await)
}

/// 清除当前配对码
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
) -> Result<Vec<crate::db::Pairing>> {
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

// ==================== Settings Commands ====================

/// 获取应用设置
#[tauri::command]
pub async fn get_app_settings(
    app_handle: tauri::AppHandle,
) -> crate::Result<crate::system::config::AppConfig> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map(|p| p.join("config.properties"))
        .map_err(|e: tauri::Error| crate::AppError::Config(e.to_string()))?;

    crate::system::config::AppConfig::load(&config_path)
        .map_err(|e| crate::AppError::Config(e.to_string()))
}

/// 保存应用设置
#[tauri::command]
pub async fn save_app_settings(
    app_handle: tauri::AppHandle,
    settings: crate::system::config::AppConfig,
) -> crate::Result<()> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map(|p| p.join("config.properties"))
        .map_err(|e: tauri::Error| crate::AppError::Config(e.to_string()))?;

    // 同步 PowerManager 开关状态
    crate::system::power::power_manager().set_enabled(settings.network.prevent_sleep);

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
#[tauri::command]
pub fn get_startup_time(start_time: State<'_, crate::AppStartTime>) -> u64 {
    start_time.0.elapsed().as_millis() as u64
}

/// 获取本地 IPv4 地址（排除回环和链路本地地址）
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
                        std::net::IpAddr::V6(_) => false,
                    }
                })
                .map(|(_, ip)| ip.to_string())
                .collect()
        })
        .unwrap_or_default()
}
