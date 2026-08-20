//! Shared System Commands
//!
//! 桌面端和移动端共享的系统命令
//!
//! 桌面端专用命令在 desktop/commands.rs
//! 移动端专用命令在 mobile/commands/mobile_commands.rs

use crate::db::Database;
use crate::utils::auth::PairingCode;
use crate::Result;
use serde::Serialize;
use std::sync::Arc;
use tauri::{Manager, State};
use tokio::sync::Mutex;

#[cfg(any(target_os = "android", target_os = "ios"))]
use crate::mobile::remote::PairingService;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::server::services::pairing_service::PairingService;

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

/// 生成配对码（有效期取数据库中的 pairing_code_ttl，缺省回退常量）
#[tauri::command]
pub async fn generate_pairing_code(
    pairing_service: State<'_, Arc<PairingService>>,
    db: State<'_, Arc<Mutex<Database>>>,
) -> Result<PairingCode> {
    let ttl = {
        let db = db.lock().await;
        db.get_setting("pairing_code_ttl")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(crate::system::constants::auth::PAIRING_CODE_TTL_SECS)
    };
    Ok(pairing_service.generate_code_with_ttl(ttl).await)
}

/// 获取配对码有效期（秒）
#[tauri::command]
pub async fn get_pairing_code_ttl(db: State<'_, Arc<Mutex<Database>>>) -> Result<u64> {
    let db = db.lock().await;
    match db.get_setting("pairing_code_ttl") {
        Ok(Some(value)) => value.parse::<u64>().map_err(|e| crate::AppError::Config(e.to_string())),
        _ => Ok(crate::system::constants::auth::PAIRING_CODE_TTL_SECS),
    }
}

/// 设置配对码有效期（秒）
#[tauri::command]
pub async fn set_pairing_code_ttl(db: State<'_, Arc<Mutex<Database>>>, ttl: u64) -> Result<()> {
    let db = db.lock().await;
    db.set_setting("pairing_code_ttl", &ttl.to_string())
        .map_err(|e| crate::AppError::Config(e.to_string()))
}

/// 获取当前配对码
#[tauri::command]
pub async fn get_current_pairing_code(pairing_service: State<'_, Arc<PairingService>>) -> Result<Option<PairingCode>> {
    Ok(pairing_service.get_current_code().await)
}

/// 验证配对码
#[tauri::command]
pub async fn verify_pairing_code(pairing_service: State<'_, Arc<PairingService>>, code: String) -> Result<bool> {
    Ok(pairing_service.verify_and_consume_code(&code).await)
}

/// 清除当前配对码
#[tauri::command]
pub async fn clear_pairing_code(pairing_service: State<'_, Arc<PairingService>>) -> Result<()> {
    pairing_service.clear_code().await;
    Ok(())
}

/// 获取已配对设备
#[tauri::command]
pub async fn list_paired_devices(db: State<'_, Arc<Mutex<Database>>>) -> Result<Vec<crate::db::Pairing>> {
    let db = db.lock().await;
    db.get_pairings()
}

/// 移除配对设备
#[tauri::command]
pub async fn remove_paired_device(db: State<'_, Arc<Mutex<Database>>>, id: String) -> Result<()> {
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
pub async fn delete_connection_history(db: State<'_, Arc<Mutex<Database>>>, device_id: String) -> Result<()> {
    let db = db.lock().await;
    db.delete_connection_history(&device_id)
}

// ==================== Settings Commands ====================

/// 获取应用设置
#[tauri::command]
pub async fn get_app_settings(app_handle: tauri::AppHandle) -> crate::Result<crate::system::config::AppConfig> {
    let config_path = app_handle
        .path()
        .app_data_dir()
        .map(|p| p.join("config.properties"))
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
pub fn set_terminal_bg_image(app_handle: tauri::AppHandle, source_path: Option<String>) -> Result<Option<String>> {
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
    let metadata =
        std::fs::metadata(src).map_err(|e| crate::AppError::Config(format!("读取图片文件信息失败 {source}: {e}")))?;
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
                .filter(|(_, ip)| match ip {
                    std::net::IpAddr::V4(ipv4) => !ipv4.is_loopback() && !ipv4.is_link_local(),
                    std::net::IpAddr::V6(_) => false,
                })
                .map(|(_, ip)| ip.to_string())
                .collect()
        })
        .unwrap_or_default()
}

// ==================== 临时调试命令（已注释禁用，恢复排查时取消注释） ====================

// 2026-08: 终端侧输出字节 dump 命令，配合 PtyReader 源头 dump 逐字节对比。
// [已注释禁用] 恢复时取消下方 /* */ 注释，并在 lib.rs 中重新注册该命令。
/*
/// 追加终端侧输出字节到 dump 文件（临时调试用，仅 dev 构建生效）
///
/// 配合后端 PTY 源头 dump：前端在每次 terminal.write 前把待写字节追加到
/// terminal_output_dump.bin（追加模式），与源头 pty_output_dump.bin 同目录
/// （= 日志目录，见 system::logging::dump_dir）逐字节对比，排查「源头输出 vs
/// 终端显示」不一致问题。`reset=true` 表示新会话开始：覆盖旧文件，
/// 与源头 dump 每次会话 truncate 对齐。排查完成后删除本命令及前端调用。
#[tauri::command]
pub fn append_terminal_output_dump(data: Vec<u8>, reset: bool) {
    use std::io::Write;
    // 仅 dev 构建生效（release 前端不会调用，此处双保险）
    if !cfg!(debug_assertions) || data.is_empty() {
        return;
    }
    let Some(log_dir) = crate::system::logging::dump_dir() else {
        tracing::warn!("[debug-dump] dump 目录未初始化，跳过终端 dump");
        return;
    };
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    if reset {
        // 新会话覆盖：打开即截断旧内容（与源头 pty dump 对齐）
        opts.truncate(true);
    }
    let Ok(mut file) = opts.open(log_dir.join("terminal_output_dump.bin")) else {
        tracing::warn!(
            "[debug-dump] 打开 terminal dump 文件失败: {}",
            log_dir.display()
        );
        return;
    };
    if let Err(e) = file.write_all(&data).and_then(|_| file.flush()) {
        tracing::warn!("[debug-dump] 写入 terminal dump 失败: {e}");
    }
}
*/

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
