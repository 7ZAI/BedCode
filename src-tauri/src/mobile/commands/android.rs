//! Mobile Android Commands
//!
//! Android 平台专用命令

use crate::Result;

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

// ==================== Foreground Service Commands ====================

/// 启动前台服务
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn start_foreground_service(
    title: String,
    content: String,
) -> Result<()> {
    tracing::info!("Starting foreground service: {} - {}", title, content);
    // 实际调用通过 Tauri 插件实现
    Ok(())
}

/// 停止前台服务
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn stop_foreground_service() -> Result<()> {
    tracing::info!("Stopping foreground service");
    Ok(())
}

/// 更新前台服务通知
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn update_foreground_notification(
    title: String,
    content: String,
) -> Result<()> {
    tracing::info!("Updating foreground notification: {} - {}", title, content);
    Ok(())
}

/// 非 Android 平台的空实现
#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn start_foreground_service(_title: String, _content: String) -> Result<()> {
    Ok(())
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn stop_foreground_service() -> Result<()> {
    Ok(())
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn update_foreground_notification(_title: String, _content: String) -> Result<()> {
    Ok(())
}