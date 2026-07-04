//! Mobile Android Commands
//!
//! Android 平台专用命令

use crate::Result;
use tauri_plugin_shell::ShellExt;

/// 使用系统浏览器打开 URL
#[tauri::command]
pub async fn open_url_in_browser(
    app_handle: tauri::AppHandle,
    url: String,
) -> Result<()> {
    tracing::info!("Opening URL in browser: {}", url);
    app_handle
        .shell()
        .open(&url, None)
        .map_err(|e| crate::system::error::AppError::Internal(e.to_string()))?;
    Ok(())
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