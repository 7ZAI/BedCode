//! Shared System Commands
//!
//! 桌面端和移动端共享的系统命令
//!
//! 桌面端专用命令在 desktop/commands.rs
//! 移动端专用命令在 mobile/commands/mobile_commands.rs

use crate::utils::auth::PairingCode;
use crate::db::Database;
use crate::Result;
use serde::Serialize;
use std::sync::Arc;
use tauri::{Manager, State};
use tokio::sync::Mutex;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::server::services::pairing_service::PairingService;
#[cfg(any(target_os = "android", target_os = "ios"))]
use crate::mobile::remote::PairingService;

/// 运行中会话摘要信息，用于窗口关闭确认弹窗
#[derive(Debug, Clone, Serialize)]
pub struct RunningSessionInfo {
    /// 会话 ID
    pub id: String,
    /// 会话名称
    pub name: String,
    /// 会话状态（Running / Starting / WaitingInput）
    pub status: String,
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

/// 获取设备连接历史
#[tauri::command]
pub async fn list_connection_history(
    db: State<'_, Arc<Mutex<Database>>>,
    device_id: String,
) -> Result<Vec<crate::db::ConnectionHistory>> {
    let db = db.lock().await;
    db.get_connection_history(&device_id)
}

/// 删除设备连接历史
#[tauri::command]
pub async fn delete_connection_history(
    db: State<'_, Arc<Mutex<Database>>>,
    device_id: String,
) -> Result<()> {
    let db = db.lock().await;
    db.delete_connection_history(&device_id)
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

// ==================== Terminal Background Image ====================

use crate::system::constants::terminal::{TERMINAL_BG_EXTENSIONS, TERMINAL_BG_FILE_PREFIX, TERMINAL_BG_MAX_BYTES};

/// 设置终端背景图片
///
/// 传入源图片路径时，将图片复制到应用数据目录（统一命名为 `terminal_bg.<ext>`）并返回文件名；
/// 传入 `None` 时移除已有背景图片文件。选择复制而非直接引用源路径，
/// 避免用户移动/删除原图后背景失效。
#[tauri::command]
pub fn set_terminal_bg_image(
    app_handle: tauri::AppHandle,
    source_path: Option<String>,
) -> Result<Option<String>> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| crate::AppError::Config(e.to_string()))?;

    // 清理已有背景图片（每次选择扩展名可能不同，避免残留旧文件）
    if data_dir.exists() {
        let entries = std::fs::read_dir(&data_dir)
            .map_err(|e| crate::AppError::Config(format!("读取应用数据目录失败 {}: {e}", data_dir.display())))?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(ext) = name.strip_prefix(&format!("{TERMINAL_BG_FILE_PREFIX}.")) {
                if TERMINAL_BG_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) {
                    if let Err(e) = std::fs::remove_file(entry.path()) {
                        tracing::warn!("删除旧终端背景图片失败 {}: {e}", entry.path().display());
                    }
                }
            }
        }
    }

    let Some(source) = source_path.filter(|p| !p.is_empty()) else {
        // 仅移除背景图片
        return Ok(None);
    };

    // 校验扩展名，防止复制任意文件
    let src = std::path::Path::new(&source);
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .ok_or_else(|| crate::AppError::InvalidInput(format!("文件缺少扩展名，无法识别图片格式: {source}")))?;
    if !TERMINAL_BG_EXTENSIONS.contains(&ext.as_str()) {
        return Err(crate::AppError::InvalidInput(format!("不支持的图片格式: {ext}")));
    }

    // 限制文件大小，避免超大图片占用过多存储
    let metadata = std::fs::metadata(src)
        .map_err(|e| crate::AppError::Config(format!("读取图片文件信息失败 {source}: {e}")))?;
    if metadata.len() > TERMINAL_BG_MAX_BYTES {
        return Err(crate::AppError::InvalidInput(format!(
            "图片文件过大（{} 字节），上限 {} 字节",
            metadata.len(),
            TERMINAL_BG_MAX_BYTES
        )));
    }

    std::fs::create_dir_all(&data_dir)
        .map_err(|e| crate::AppError::Config(format!("创建应用数据目录失败 {}: {e}", data_dir.display())))?;

    let file_name = format!("{TERMINAL_BG_FILE_PREFIX}.{ext}");
    let dest = data_dir.join(&file_name);
    std::fs::copy(src, &dest)
        .map_err(|e| crate::AppError::Config(format!("复制背景图片 {source} 到 {} 失败: {e}", dest.display())))?;

    tracing::info!("终端背景图片已更新: {}", dest.display());
    Ok(Some(file_name))
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

/// 获取系统基本信息（OS / 设备名称 / IP 地址，启动时采集一次）
#[tauri::command]
pub fn get_system_info(app_handle: tauri::AppHandle) -> crate::system::SystemInfo {
    use tauri::Manager;
    (**app_handle.state::<Arc<crate::system::SystemInfo>>().inner()).clone()
}

// ==================== Window Close Commands ====================

/// 用户确认关闭窗口（前端确认弹窗后调用）
///
/// 使用 destroy() 直接销毁窗口，不再次触发 CloseRequested
#[tauri::command]
pub fn confirm_window_close(app_handle: tauri::AppHandle) -> Result<()> {
    if let Some(window) = app_handle.get_webview_window("main") {
        window.destroy().map_err(|e| crate::AppError::Internal(e.to_string()))?;
    }
    Ok(())
}
