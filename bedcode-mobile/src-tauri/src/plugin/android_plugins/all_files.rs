//! 分区存储全文件访问授权引导（AllFilesAccessPlugin）
//!
//! 从 android_plugins.rs 拆分。

use std::sync::OnceLock;
use tauri::plugin::{Builder, PluginHandle};

/// 已注册的 AllFilesAccessPlugin 句柄（仅 Android 平台使用）
static ALL_FILES_ACCESS_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();


/// 注册 AllFilesAccessPlugin（「所有文件访问权限」一键引导跳转）
///
/// Android 11+ 分区存储下该权限无运行时弹窗，只能经系统设置页手动开启；
/// Kotlin 侧负责查询 isExternalStorageManager 并跳转授权页。
pub fn all_files_access_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("all-files-access")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin("com.bedcode.mobile", "AllFilesAccessPlugin")?;
                let _ = ALL_FILES_ACCESS_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}


/// 查询「所有文件访问权限」状态；未授权时跳转系统授权页，返回跳转前是否已授权
#[cfg(target_os = "android")]
pub async fn open_all_files_settings_android() -> crate::Result<bool> {
    let handle = ALL_FILES_ACCESS_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("AllFilesAccessPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("openAllFilesAccessSettings", serde_json::json!({}))
        .await
        .map_err(|e| {
            crate::AppError::Plugin(format!("Failed to invoke openAllFilesAccessSettings: {}", e))
        })?;
    Ok(response.get("granted").and_then(|v| v.as_bool()).unwrap_or(false))
}


/// 非 Android 平台（桌面 dev 窗口 / iOS）：无「所有文件访问权限」概念
#[cfg(not(target_os = "android"))]
pub async fn open_all_files_settings_android() -> crate::Result<bool> {
    Err(crate::AppError::Plugin(
        "All files access is only available on Android".to_string(),
    ))
}
