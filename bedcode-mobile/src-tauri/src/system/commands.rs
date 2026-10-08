//! Shared System Commands
//!
//! 桌面端和移动端共享的系统命令
//!
//! 本地配对码命令（生成 / 读取 / 校验 / 清除）已于票 14 随
//! `connection::pairing_service` 一并退役——移动端不再是配对码颁发方，配对码由
//! 桌面端生成、移动端只做提交（`commands::auth::ws_verify_pairing_code`）。

use tauri::{Manager, State};

// ==================== Settings Commands ====================

/// 获取应用设置
#[tauri::command]
pub async fn get_app_settings(app_handle: tauri::AppHandle) -> crate::Result<crate::system::config::AppConfig> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map(|p| p.join("config.json"))
        .map_err(|e: tauri::Error| crate::AppError::Config(e.to_string()))?;

    crate::system::config::AppConfig::load(&config_path).map_err(|e| crate::AppError::Config(e.to_string()))
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

/// 获取系统基本信息（OS / 设备名称 / IP 地址，启动时采集一次）
///
/// 采集在 setup 的异步 spawn 中完成，前端过早 invoke 时返回兜底实例而非 panic
#[tauri::command]
pub fn get_system_info() -> crate::system::info::SystemInfo {
    crate::state::try_get_system_info()
        .map(|i| i.as_ref().clone())
        .unwrap_or_else(crate::system::info::SystemInfo::fallback)
}

/// 获取自应用启动以来的耗时（毫秒）
#[tauri::command]
pub fn get_startup_time(start_time: State<'_, crate::AppStartTime>) -> u64 {
    start_time.0.elapsed().as_millis() as u64
}

/// 获取本地 IP 地址（移动端不适用，返回空列表）
#[tauri::command]
pub fn get_local_ip_addresses() -> Vec<String> {
    vec![]
}
