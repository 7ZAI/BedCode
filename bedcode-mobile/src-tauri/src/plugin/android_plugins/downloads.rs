//! 下载目录插件（DownloadsDirPlugin）
//!
//! 从 android_plugins.rs 拆分。

use std::sync::OnceLock;
use tauri::Manager;
use tauri::plugin::{Builder, PluginHandle};

/// 已注册的 DownloadsDirPlugin 句柄（仅 Android 平台使用）
static DOWNLOADS_DIR_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();


/// 注册 DownloadsDirPlugin（Android 外部私有下载目录路径获取）
///
/// gen/android 重建恢复清单：DownloadsDirPlugin.kt 须恢复
pub fn downloads_dir_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("downloads-dir")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin("com.bedcode.mobile", "DownloadsDirPlugin")?;
                let _ = DOWNLOADS_DIR_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))]
            let _ = api;
            Ok(())
        })
        .build()
}


/// 获取 Android 外部私有下载目录绝对路径
///
/// 通过 Kotlin DownloadsDirPlugin 调用 `getExternalFilesDir(DIRECTORY_DOWNLOADS)`。
/// 外部存储不可用或非 Android 平台返回 None。
#[cfg(target_os = "android")]
pub async fn get_external_downloads_dir() -> Option<String> {
    let handle = DOWNLOADS_DIR_HANDLE.get()?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("getDownloadsDir", serde_json::json!({}))
        .await
        .ok()?;
    let path = response.get("path").and_then(|v| v.as_str()).unwrap_or("");
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}


/// 非 Android 平台外部下载目录不可用
#[cfg(not(target_os = "android"))]
pub async fn get_external_downloads_dir() -> Option<String> {
    None
}


/// 解析 app 下载目录（免授权特殊条目基址，与 WASM host config 共用）
///
/// 策略（与 wasm_runtime.rs 的 resolve_downloads_dir 保持同一解析链）：
/// 1. Kotlin 桥 `getExternalFilesDir(DIRECTORY_DOWNLOADS)`（外部私有目录，免权限）
/// 2. 兜底 `app_data_dir()/Downloads`（内部存储；外部存储不可用时设备上
///    特殊条目仍以回退路径派生，浏览端（plugin_saf_list_dir）与配置端
///    （WASM host_config_get）必须一致，否则外部存储不可用的设备上
///    特殊条目派生成功但浏览被白名单拒绝）
/// 目录不存在时惰性创建。
pub async fn resolve_app_downloads_dir(app_handle: &tauri::AppHandle) -> Option<String> {
    // 首选：Kotlin 桥获取外部私有下载目录
    if let Some(ext) = get_external_downloads_dir().await {
        tracing::debug!(path = %ext, "resolve_app_downloads_dir: using external private downloads dir");
        return Some(ext);
    }
    // 兜底：app_data_dir()/Downloads（内部存储目录，文件管理器不可见）
    let data_dir = app_handle.path().app_data_dir().ok()?;
    let path = data_dir.join("Downloads");
    if !path.exists() {
        if let Err(e) = std::fs::create_dir_all(&path) {
            tracing::error!(error = %e, path = %path.display(), "resolve_app_downloads_dir: failed to create fallback dir");
            return None;
        }
        tracing::info!(path = %path.display(), "resolve_app_downloads_dir: created app_data/Downloads fallback");
    }
    Some(path.to_string_lossy().into_owned())
}


/// 判断路径是否位于 AppDownloadsDir（免授权特殊条目）之下
///
/// canonicalize 白名单，三处共用（命令层 saf 列表、WASM host media 落位、
/// save-to-document 落位）；基址解析失败返回 false（fail-closed）
pub async fn is_within_app_downloads_dir(app_handle: &tauri::AppHandle, path: &str) -> bool {
    let Some(base) = resolve_app_downloads_dir(app_handle).await else {
        return false;
    };
    let base_canon =
        std::fs::canonicalize(&base).unwrap_or_else(|_| std::path::PathBuf::from(&base));
    let target_canon =
        std::fs::canonicalize(path).unwrap_or_else(|_| std::path::PathBuf::from(path));
    target_canon.starts_with(&base_canon)
}

/// 打开系统文件（MediaStore 公共下载按名命中优先，未命中回退 FileProvider）
///
/// 经 Kotlin DownloadsDirPlugin.openFile；需 system:open 权限。
/// 非 Android 平台返回错误（桌面由宿主自行打开）。
#[cfg(target_os = "android")]
pub async fn open_download_file(path: &str, display_name: &str) -> crate::Result<()> {
    let handle = DOWNLOADS_DIR_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("DownloadsDirPlugin not registered".to_string())
    })?;
    let payload = serde_json::json!({ "path": path, "displayName": display_name });
    // 显式标注 Ok 类型：run_mobile_plugin_async 的 Ok 在无约束时会退化为
    // never type fallback（编译错误），与 android_plugins 其他调用点同模式
    let _response: serde_json::Value = handle
        .run_mobile_plugin_async("openFile", payload)
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke openFile: {}", e)))?;
    Ok(())
}

/// 非 Android 平台无法经 Kotlin 打开文件
#[cfg(not(target_os = "android"))]
pub async fn open_download_file(_path: &str, _display_name: &str) -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "openFile unavailable on this platform".to_string(),
    ))
}

/// 打开文件所在目录（历史记录「打开所在文件夹」）
///
/// 经 Kotlin DownloadsDirPlugin.openFileLocation（FileProvider 暴露父目录 +
/// ACTION_VIEW）。需 system:open 权限。非 Android 平台返回错误。
#[cfg(target_os = "android")]
pub async fn open_download_file_location(path: &str) -> crate::Result<()> {
    let handle = DOWNLOADS_DIR_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("DownloadsDirPlugin not registered".to_string())
    })?;
    let payload = serde_json::json!({ "path": path });
    // 显式标注 Ok 类型：run_mobile_plugin_async 的 Ok 在无约束时会退化为
    // never type fallback（编译错误），与 open_download_file 同模式
    let _response: serde_json::Value = handle
        .run_mobile_plugin_async("openFileLocation", payload)
        .await
        .map_err(|e| {
            crate::AppError::Plugin(format!("Failed to invoke openFileLocation: {}", e))
        })?;
    Ok(())
}

/// 非 Android 平台无法经 Kotlin 打开目录
#[cfg(not(target_os = "android"))]
pub async fn open_download_file_location(_path: &str) -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "openFileLocation unavailable on this platform".to_string(),
    ))
}

