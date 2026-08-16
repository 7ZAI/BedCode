//! 文件删除插件（FileDeletePlugin）
//!
//! 从 android_plugins.rs 拆分。

use std::sync::OnceLock;
use tauri::plugin::{Builder, PluginHandle};

/// 已注册的 FileDeletePlugin 句柄（仅 Android 平台使用）
static FILE_DELETE_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();


/// 注册 FileDeletePlugin（删除文件，WASM HostFs::fs_delete 的 Android 实现）
pub fn file_delete_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("file-delete")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin("com.bedcode.mobile", "FileDeletePlugin")?;
                let _ = FILE_DELETE_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}


/// 删除文件（幂等：不存在也视为成功）
///
/// 经 Kotlin FileDeletePlugin 调用。仅 Android 平台可用；
/// 非 Android 平台（桌面 dev 由 loader 提供 WASM 运行时）由调用方直接用 std::fs。
#[cfg(target_os = "android")]
pub async fn delete_file(path: &str) -> crate::Result<()> {
    let handle = FILE_DELETE_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("FileDeletePlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("deleteFile", serde_json::json!({ "path": path }))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke deleteFile: {}", e)))?;
    if response.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(())
    } else {
        let err = response
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown delete error");
        Err(crate::AppError::Plugin(format!(
            "deleteFile failed for {}: {}",
            path, err
        )))
    }
}


/// 非 Android 平台无 Kotlin 删除能力（桌面 dev 场景由 WASM 宿主 std::fs 兜底）
#[cfg(not(target_os = "android"))]
pub async fn delete_file(_path: &str) -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "FileDeletePlugin unavailable on this platform".to_string(),
    ))
}
