//! Android 原生插件注册与调用
//!
//! 将 Kotlin 端的 PluginAssetExtractor / ForegroundServicePlugin 注册到 Tauri PluginManager，
//! 并提供 Rust → Kotlin 的调用入口。
//!
//! Tauri 2.0 的 Android 插件注册必须通过 Rust 端 `api.register_android_plugin()` 完成，
//! Kotlin 端的 `@TauriPlugin` 注解仅为标记，不触发自动注册。
//!
//! 注意：`register_android_plugin` 以 Builder 名称作为 Kotlin 端插件注册名（HashMap key），
//! 因此每个 Kotlin 插件必须使用独立的 Builder 名称，否则同名注册会互相覆盖，
//! 导致 `run_mobile_plugin_async` 路由到错误的插件。

use std::sync::OnceLock;
use tauri::Manager;
use tauri::plugin::{Builder, PluginHandle};

/// 已注册的 PluginAssetExtractor 句柄（仅 Android 平台使用）
///
/// 移动端应用运行时的 Runtime 固定为 Wry（桌面 dev 窗口与 Android 一致），
/// 因此可存储具体类型而非泛型。
static PLUGIN_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();

/// 注册 PluginAssetExtractor（内置插件资源解压）
pub fn asset_extractor_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("plugin-asset-extractor")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                // 保留 PluginAssetExtractor 句柄供 extract_bundled_plugins 调用
                let handle = api.register_android_plugin("com.bedcode.mobile", "PluginAssetExtractor")?;
                let _ = PLUGIN_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}

/// 注册 ForegroundServicePlugin（前台服务桥接）
pub fn foreground_service_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("foreground-service")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                api.register_android_plugin("com.bedcode.mobile", "ForegroundServicePlugin")?;
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}

/// 调用 Kotlin PluginAssetExtractor 解压内置插件到 app_data_dir/plugins
///
/// 返回解压的插件数量（已是最新版本的跳过）
#[cfg(target_os = "android")]
pub async fn extract_bundled_plugins(app_version: &str) -> crate::Result<u32> {
    let handle = PLUGIN_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("PluginAssetExtractor not registered".to_string())
    })?;
    let payload = serde_json::json!({ "appVersion": app_version });
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("extractBundledPlugins", payload)
        .await
        .map_err(|e| {
            crate::AppError::Plugin(format!("Failed to invoke extractBundledPlugins: {}", e))
        })?;
    let count = response.get("count").and_then(|c| c.as_u64()).unwrap_or(0) as u32;
    tracing::info!(count, "Extracted bundled plugin(s) from APK assets");
    Ok(count)
}

/// 非 Android 平台无内置插件解压（桌面 dev 由 loader 从源码资源目录复制）
#[cfg(not(target_os = "android"))]
pub async fn extract_bundled_plugins(_app_version: &str) -> crate::Result<u32> {
    Ok(0)
}

/// 已注册的 BiometricKeyPlugin 句柄（仅 Android 平台使用）
static BIOMETRIC_KEY_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();

/// 已注册的 DeviceIdPlugin 句柄（仅 Android 平台使用）
static DEVICE_ID_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();

/// 注册 DeviceIdPlugin（读取 Android 设备唯一 ID，卸载重装保持一致）
///
/// gen/android 重建恢复清单：DeviceIdPlugin.kt 须恢复
pub fn device_id_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("device-id")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin("com.bedcode.mobile", "DeviceIdPlugin")?;
                let _ = DEVICE_ID_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}

/// 获取 Android 设备唯一 ID（ANDROID_ID，卸载重装保持一致）
#[cfg(target_os = "android")]
pub async fn get_android_id() -> Option<String> {
    let handle = DEVICE_ID_HANDLE.get()?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("getAndroidId", serde_json::json!({}))
        .await
        .ok()?;
    if response.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        let id = response.get("androidId").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !id.is_empty() {
            return Some(id);
        }
    }
    None
}

/// 非 Android 平台无设备唯一 ID（插件不可用）
#[cfg(not(target_os = "android"))]
pub async fn get_android_id() -> Option<String> {
    None
}

/// 已注册的 DownloadsDirPlugin 句柄（仅 Android 平台使用）
static DOWNLOADS_DIR_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();

/// 已注册的 FileDeletePlugin 句柄（仅 Android 平台使用）
static FILE_DELETE_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();

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

/// 已注册的 SafPickerPlugin 句柄（仅 Android 平台使用）
static SAF_PICKER_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();

/// 已注册的 SafTransferPlugin 句柄（仅 Android 平台使用）
static SAF_TRANSFER_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();

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

/// 注册 SafPickerPlugin（SAF 文件/目录选择器，免存储权限）
///
/// gen/android 重建恢复清单：SafPickerPlugin.kt 须恢复
pub fn saf_picker_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("saf-picker")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin("com.bedcode.mobile", "SafPickerPlugin")?;
                let _ = SAF_PICKER_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}

/// 注册 SafTransferPlugin（SAF 存储传输后端：目录树遍历 / 中转复制 / 授权检测）
///
/// 对应宿主 Rust SafIo trait（saf_io.rs）的 Kotlin 实现，转发经本文件
/// saf_* 函数（run_mobile_plugin_async 模式）。
/// gen/android 重建恢复清单：SafTransferPlugin.kt 须恢复
pub fn saf_transfer_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("saf-transfer")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin("com.bedcode.mobile", "SafTransferPlugin")?;
                let _ = SAF_TRANSFER_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}

/// 弹系统目录树选择器，返回真实路径（用户取消返回 None）
///
/// Kotlin 侧只提取 SAF Uri 元数据，真实路径由 saf_path::resolve_saf_path 解析
/// （主存储/SD 卡/downloads raw: 支持；解析失败返回明确错误供插件降级手动输入）。
#[cfg(target_os = "android")]
pub async fn pick_directory_android() -> crate::Result<Option<String>> {
    let handle = SAF_PICKER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafPickerPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("pickDirectory", serde_json::json!({}))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke pickDirectory: {}", e)))?;
    saf_response_to_path(&response, "directory")
}

/// 弹系统文件选择器，返回真实路径（用户取消返回 None）
///
/// 优先用 Kotlin 侧 `_data` 列直读路径（Downloads/Media provider），
/// 否则回退 saf_path 解析（externalstorage/downloads raw:）。
#[cfg(target_os = "android")]
pub async fn pick_file_android() -> crate::Result<Option<String>> {
    let handle = SAF_PICKER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafPickerPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("pickFile", serde_json::json!({}))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke pickFile: {}", e)))?;
    // _data 直读路径优先（非空即用）
    if let Some(p) = response.get("dataPath").and_then(|v| v.as_str()) {
        if !p.is_empty() {
            return Ok(Some(p.to_string()));
        }
    }
    saf_response_to_path(&response, "file")
}

/// 非 Android 平台 SAF 选择器不可用（iOS 走系统文档选择器，另行实现）
#[cfg(not(target_os = "android"))]
pub async fn pick_directory_android() -> crate::Result<Option<String>> {
    Err(crate::AppError::Plugin(
        "SAF picker unavailable on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
pub async fn pick_file_android() -> crate::Result<Option<String>> {
    Err(crate::AppError::Plugin(
        "SAF picker unavailable on this platform".to_string(),
    ))
}

/// 把 Kotlin SAF 选择结果（authority/documentId/primaryDir）解析为真实路径
///
/// 用户取消（cancelled=true）返回 Ok(None)；不支持的 provider 返回明确错误。
fn saf_response_to_path(
    response: &serde_json::Value,
    kind: &str,
) -> crate::Result<Option<String>> {
    if response.get("cancelled").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Ok(None);
    }
    let authority = response.get("authority").and_then(|v| v.as_str()).unwrap_or("");
    let document_id = response.get("documentId").and_then(|v| v.as_str()).unwrap_or("");
    let primary_dir = response
        .get("primaryDir")
        .and_then(|v| v.as_str())
        .unwrap_or("/storage/emulated/0");
    let path = super::saf_path::resolve_saf_path(authority, document_id, primary_dir).ok_or_else(|| {
        crate::AppError::Plugin(format!(
            "SAF {} not resolvable to a real path (authority={}, documentId={}); fall back to manual path input",
            kind, authority, document_id
        ))
    })?;
    Ok(Some(path))
}

/// 弹系统目录树选择器，返回 SAF 树元数据（共享目录条目用，不做真实路径解析）
///
/// 共享目录条目存储 content://tree URI + documentId + 展示名（spec：
/// 共享目录 = SAF URI 存储）；真实路径解析仅旧选路需要，SAF 化后废除。
/// 用户取消返回 Ok(None)；持久化授权由 Kotlin 侧 takePersistableUriPermission
/// 完成（重启仍有效）。
#[cfg(target_os = "android")]
pub async fn pick_shared_directory_android(
) -> crate::Result<Option<(String, String, String)>> {
    let handle = SAF_PICKER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafPickerPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("pickDirectory", serde_json::json!({}))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke pickDirectory: {}", e)))?;
    if response.get("cancelled").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Ok(None);
    }
    let uri = response.get("uri").and_then(|v| v.as_str()).unwrap_or("");
    let document_id = response.get("documentId").and_then(|v| v.as_str()).unwrap_or("");
    let display_name = response.get("displayName").and_then(|v| v.as_str()).unwrap_or("");
    if uri.is_empty() || document_id.is_empty() {
        return Err(crate::AppError::Plugin(format!(
            "pickDirectory returned incomplete SAF metadata (uri={}, documentId={})",
            uri, document_id
        )));
    }
    Ok(Some((uri.to_string(), document_id.to_string(), display_name.to_string())))
}

/// 非 Android 平台无 SAF 目录树选择器（共享目录功能仅 Android 可用）
#[cfg(not(target_os = "android"))]
pub async fn pick_shared_directory_android() -> crate::Result<Option<(String, String, String)>> {
    Err(crate::AppError::Plugin(
        "SAF directory picker unavailable on this platform".to_string(),
    ))
}

// ==================== SafIo 桥（SafTransferPlugin 转发） ====================
//
// 对应 saf_io.rs 的 KotlinSafIo：把 trait 方法经 run_mobile_plugin_async
// 转发到 Kotlin SafTransferPlugin，并把响应解析为 Rust 类型（解析函数在
// saf_io.rs，可单测）。

/// 列出目录树子条目
#[cfg(target_os = "android")]
pub async fn saf_list_tree(tree_uri: &str, document_id: &str) -> crate::Result<Vec<crate::plugin::saf_io::SafEntry>> {
    let handle = SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "listTreeChildren",
            serde_json::json!({ "treeUri": tree_uri, "documentId": document_id }),
        )
        .await
        .map_err(|e| {
            crate::AppError::Plugin(format!("Failed to invoke listTreeChildren: {}", e))
        })?;
    crate::plugin::saf_io::parse_saf_entries(&response)
}

/// 启动中转复制（SAF 源 → app 私有 cache）
#[cfg(target_os = "android")]
pub async fn saf_copy_start(
    uri: &str,
    dest_name: &str,
) -> crate::Result<crate::plugin::saf_io::SafCopyHandle> {
    let handle = SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "safToCache",
            serde_json::json!({ "uri": uri, "destName": dest_name }),
        )
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke safToCache: {}", e)))?;
    crate::plugin::saf_io::parse_saf_copy_handle(&response)
}

/// 轮询中转复制进度
#[cfg(target_os = "android")]
pub async fn saf_copy_status(
    copy_id: &str,
) -> crate::Result<crate::plugin::saf_io::SafCopyStatus> {
    let handle = SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("copyProgress", serde_json::json!({ "copyId": copy_id }))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke copyProgress: {}", e)))?;
    crate::plugin::saf_io::parse_saf_copy_status(&response)
}

/// 取消中转复制
#[cfg(target_os = "android")]
pub async fn saf_copy_cancel(copy_id: &str) -> crate::Result<()> {
    let handle = SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("cancelCopy", serde_json::json!({ "copyId": copy_id }))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke cancelCopy: {}", e)))?;
    if response.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(())
    } else {
        Err(crate::AppError::Plugin(format!(
            "cancelCopy rejected for copyId {}",
            copy_id
        )))
    }
}

/// 清扫中转复制残留（file-transfer 插件激活时调用，删除 staging 目录全部文件）
#[cfg(target_os = "android")]
pub async fn saf_cleanup_stale_copies() -> crate::Result<()> {
    let handle = SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    handle
        .run_mobile_plugin_async::<()>("cleanupStaleCopies", serde_json::json!({}))
        .await
        .map_err(|e| {
            crate::AppError::Plugin(format!("Failed to invoke cleanupStaleCopies: {}", e))
        })
}

/// 检测树授权是否仍有效
#[cfg(target_os = "android")]
pub async fn saf_check_authorized(tree_uri: &str) -> crate::Result<bool> {
    let handle = SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "checkAuthorized",
            serde_json::json!({ "treeUri": tree_uri }),
        )
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke checkAuthorized: {}", e)))?;
    Ok(response.get("authorized").and_then(|v| v.as_bool()).unwrap_or(false))
}

/// 写入 MediaStore.Downloads 公共下载目录（接收方向统一落点，M2）
///
/// src_path 为 app 私有下载目录中的最终文件；mime_type 为空串时由 Kotlin
/// 按扩展名推断。失败（含 API<29 设备不支持）返回错误，调用方回退私有目录。
#[cfg(target_os = "android")]
pub async fn saf_write_media_downloads(
    src_path: &str,
    display_name: &str,
    mime_type: &str,
) -> crate::Result<()> {
    let handle = SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "writeMediaDownloads",
            serde_json::json!({
                "srcPath": src_path,
                "displayName": display_name,
                "mimeType": mime_type,
            }),
        )
        .await
        .map_err(|e| {
            crate::AppError::Plugin(format!("Failed to invoke writeMediaDownloads: {}", e))
        })?;
    crate::plugin::saf_io::parse_media_write_response(&response)
}

/// 打开 SAF 源为可流读句柄（M3 上传流直传；offset 语义见 SafIo::open_stream）
#[cfg(target_os = "android")]
pub async fn saf_stream_open(
    uri: &str,
    offset: u64,
) -> crate::Result<crate::plugin::saf_io::SafStreamHandle> {
    let handle = SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "safOpen",
            serde_json::json!({ "uri": uri, "mode": "r", "offset": offset }),
        )
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke safOpen: {}", e)))?;
    crate::plugin::saf_io::parse_saf_stream_handle(&response)
}

/// 从流句柄读取至多 len 字节（EOF 返回空；base64 跨桥传输）
#[cfg(target_os = "android")]
pub async fn saf_stream_read(handle_id: &str, len: usize) -> crate::Result<Vec<u8>> {
    let handle = SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "safRead",
            serde_json::json!({ "handleId": handle_id, "len": len }),
        )
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke safRead: {}", e)))?;
    crate::plugin::saf_io::parse_saf_read_response(&response)
}

/// 移动流句柄到指定偏移（仅可 seek 句柄）
#[cfg(target_os = "android")]
pub async fn saf_stream_seek(handle_id: &str, offset: u64) -> crate::Result<()> {
    let handle = SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "safSeek",
            serde_json::json!({ "handleId": handle_id, "offset": offset }),
        )
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke safSeek: {}", e)))?;
    if response.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(())
    } else {
        Err(crate::AppError::Plugin(format!(
            "safSeek rejected for handle {}",
            handle_id
        )))
    }
}

/// 关闭流句柄（任务终态后调用；任务内恢复不调用，fd 保留续读）
#[cfg(target_os = "android")]
pub async fn saf_stream_close(handle_id: &str) -> crate::Result<()> {
    let handle = SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("safClose", serde_json::json!({ "handleId": handle_id }))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke safClose: {}", e)))?;
    if response.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(())
    } else {
        Err(crate::AppError::Plugin(format!(
            "safClose rejected for handle {}",
            handle_id
        )))
    }
}

/// 探测 SAF 源是否可 seek（getStatSize()==-1 为 pipe 流）
#[cfg(target_os = "android")]
pub async fn saf_stream_seekable(uri: &str) -> crate::Result<bool> {
    let handle = SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("safSeekable", serde_json::json!({ "uri": uri }))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke safSeekable: {}", e)))?;
    crate::plugin::saf_io::parse_saf_seekable_response(&response)
}

/// 「保存到…」（M3）：弹 ACTION_CREATE_DOCUMENT 对话框并流拷贝到用户选择的位置
///
/// 用户取消视为失败（保留私有副本回退）。suggested_name 为对话框默认文件名，
/// mime_type 为空串时按扩展名推断。
#[cfg(target_os = "android")]
pub async fn saf_save_to_document(
    src_path: &str,
    suggested_name: &str,
    mime_type: &str,
) -> crate::Result<()> {
    let handle = SAF_TRANSFER_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("SafTransferPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async(
            "saveToDocument",
            serde_json::json!({
                "srcPath": src_path,
                "suggestedName": suggested_name,
                "mimeType": mime_type,
            }),
        )
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to invoke saveToDocument: {}", e)))?;
    crate::plugin::saf_io::parse_save_to_document_response(&response)
}

/// 非 Android 平台 SafIo 不可用（dev 窗口无 SAF 概念；调用方展示明确提示）
#[cfg(not(target_os = "android"))]
pub async fn saf_cleanup_stale_copies() -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
pub async fn saf_list_tree(
    _tree_uri: &str,
    _document_id: &str,
) -> crate::Result<Vec<crate::plugin::saf_io::SafEntry>> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
pub async fn saf_copy_start(
    _uri: &str,
    _dest_name: &str,
) -> crate::Result<crate::plugin::saf_io::SafCopyHandle> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
pub async fn saf_copy_status(
    _copy_id: &str,
) -> crate::Result<crate::plugin::saf_io::SafCopyStatus> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
pub async fn saf_copy_cancel(_copy_id: &str) -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
pub async fn saf_check_authorized(_tree_uri: &str) -> crate::Result<bool> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
pub async fn saf_write_media_downloads(
    _src_path: &str,
    _display_name: &str,
    _mime_type: &str,
) -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
pub async fn saf_stream_open(
    _uri: &str,
    _offset: u64,
) -> crate::Result<crate::plugin::saf_io::SafStreamHandle> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
pub async fn saf_stream_read(_handle_id: &str, _len: usize) -> crate::Result<Vec<u8>> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
pub async fn saf_stream_seek(_handle_id: &str, _offset: u64) -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
pub async fn saf_stream_close(_handle_id: &str) -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
pub async fn saf_stream_seekable(_uri: &str) -> crate::Result<bool> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
pub async fn saf_save_to_document(
    _src_path: &str,
    _suggested_name: &str,
    _mime_type: &str,
) -> crate::Result<()> {
    Err(crate::AppError::Plugin(
        "SAF storage is not available on this platform".to_string(),
    ))
}

/// 注册 BiometricKeyPlugin（生物认证密钥：Android Keystore 生成/签名/删除）
pub fn biometric_key_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("biometric-key")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin("com.bedcode.mobile", "BiometricKeyPlugin")?;
                let _ = BIOMETRIC_KEY_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}

/// 生成生物认证密钥对（P-256，私钥存 Keystore 且需生物认证解锁）
///
/// 返回公钥（SPKI X.509 DER，base64）
#[cfg(target_os = "android")]
pub async fn biometric_generate_keypair(fingerprint: &str) -> crate::Result<String> {
    let handle = BIOMETRIC_KEY_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("BiometricKeyPlugin not registered".to_string())
    })?;
    let payload = serde_json::json!({ "alias": biometric_alias(fingerprint) });
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("generateKeyPair", payload)
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to generate biometric key: {}", e)))?;
    // Kotlin 端失败时透传具体原因（如 Keystore 异常）
    if response.get("success").and_then(|v| v.as_bool()).unwrap_or(true) == false {
        let reason = response
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown key generation error");
        return Err(crate::AppError::Plugin(format!("Biometric key generation failed: {}", reason)));
    }
    response.get("publicKey").and_then(|v| v.as_str()).map(String::from)
        .ok_or_else(|| crate::AppError::Plugin("Missing publicKey in biometric response".to_string()))
}

/// 生物认证签名：弹系统生物识别，认证通过后对消息签名
///
/// 返回原始 r||s 格式签名（base64）
#[cfg(target_os = "android")]
pub async fn biometric_sign(fingerprint: &str, message_hex: &str) -> crate::Result<String> {
    let handle = BIOMETRIC_KEY_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("BiometricKeyPlugin not registered".to_string())
    })?;
    let payload = serde_json::json!({ "alias": biometric_alias(fingerprint), "message": message_hex });
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("sign", payload)
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to sign with biometric key: {}", e)))?;
    // Kotlin 端失败（用户取消 / 认证失败 / 异常）时透传具体原因（系统文案，已是用户语言），
    // 不再加英文包装前缀，避免 toast 出现 "Plugin error: Biometric sign failed: ..." 中英混杂
    if response.get("success").and_then(|v| v.as_bool()).unwrap_or(true) == false {
        let reason = response
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown biometric error");
        return Err(crate::AppError::Plugin(reason.to_string()));
    }
    response.get("signature").and_then(|v| v.as_str()).map(String::from)
        .ok_or_else(|| crate::AppError::Plugin("Missing signature in biometric response".to_string()))
}

/// 删除生物认证密钥（解绑时调用）
#[cfg(target_os = "android")]
pub async fn biometric_delete_key(fingerprint: &str) -> crate::Result<()> {
    let handle = BIOMETRIC_KEY_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("BiometricKeyPlugin not registered".to_string())
    })?;
    let payload = serde_json::json!({ "alias": biometric_alias(fingerprint) });
    handle
        .run_mobile_plugin_async::<()>("deleteKey", payload)
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to delete biometric key: {}", e)))?;
    Ok(())
}

/// 检查生物认证密钥是否已存在
#[cfg(target_os = "android")]
pub async fn biometric_has_key(fingerprint: &str) -> crate::Result<bool> {
    let handle = BIOMETRIC_KEY_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("BiometricKeyPlugin not registered".to_string())
    })?;
    let payload = serde_json::json!({ "alias": biometric_alias(fingerprint) });
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("hasKey", payload)
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to check biometric key: {}", e)))?;
    Ok(response.get("hasKey").and_then(|v| v.as_bool()).unwrap_or(false))
}

/// 检查设备是否支持生物认证密钥（硬件 + 已录入生物特征）
///
/// 返回 (是否支持, BiometricManager 结果码)：原因码供 UI 展示具体不支持原因。
#[cfg(target_os = "android")]
pub async fn biometric_device_supported() -> crate::Result<(bool, i32)> {
    let handle = BIOMETRIC_KEY_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("BiometricKeyPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("isDeviceSupported", serde_json::json!({}))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to check biometric support: {}", e)))?;
    let supported = response
        .get("supported")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let reason = response
        .get("reason")
        .and_then(|v| v.as_i64())
        .unwrap_or(-1) as i32;
    Ok((supported, reason))
}

/// 生成 Keystore 别名（指纹哈希，避免非法字符并保证长度稳定）
fn biometric_alias(fingerprint: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(fingerprint.as_bytes());
    format!("bedcode_biometric_{}", hex::encode(hash))
}

/// 非 Android 平台生物认证不可用（桌面 dev 环境）
#[cfg(not(target_os = "android"))]
pub async fn biometric_generate_keypair(_fingerprint: &str) -> crate::Result<String> {
    Err(crate::AppError::Plugin("Biometric key unavailable on this platform".to_string()))
}

#[cfg(not(target_os = "android"))]
pub async fn biometric_sign(_fingerprint: &str, _message_hex: &str) -> crate::Result<String> {
    Err(crate::AppError::Plugin("Biometric key unavailable on this platform".to_string()))
}

#[cfg(not(target_os = "android"))]
pub async fn biometric_delete_key(_fingerprint: &str) -> crate::Result<()> {
    Err(crate::AppError::Plugin("Biometric key unavailable on this platform".to_string()))
}

#[cfg(not(target_os = "android"))]
pub async fn biometric_has_key(_fingerprint: &str) -> crate::Result<bool> {
    Ok(false)
}

#[cfg(not(target_os = "android"))]
pub async fn biometric_device_supported() -> crate::Result<(bool, i32)> {
    Ok((false, -1))
}

/// 已注册的 DeviceInfoPlugin 句柄（仅 Android 平台使用）
static DEVICE_INFO_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();

/// 注册 DeviceInfoPlugin（读取系统设备信息：用户设备名 / 机型 / OS 版本）
///
/// gen/android 重建恢复清单：DeviceInfoPlugin.kt 须恢复
pub fn device_info_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("device-info")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin("com.bedcode.mobile", "DeviceInfoPlugin")?;
                let _ = DEVICE_INFO_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}

/// Android 设备信息（Kotlin DeviceInfoPlugin 返回值）
#[derive(Debug, Clone)]
pub struct AndroidDeviceInfo {
    /// 用户设置的设备名称（Settings.Global device_name，回退 Build.MODEL）
    pub device_name: String,
    /// 机型（Build.MODEL）
    pub model: String,
    /// 厂商（Build.MANUFACTURER）
    pub manufacturer: String,
    /// OS 版本（Build.VERSION.RELEASE，如 "13"）
    pub os_version: String,
    /// API 级别（Build.VERSION.SDK_INT）
    pub sdk_int: i32,
}

/// 获取 Android 系统设备信息
///
/// 经 Kotlin DeviceInfoPlugin 调用。仅 Android 平台可用；非 Android 返回 None。
#[cfg(target_os = "android")]
pub async fn get_android_device_info() -> Option<AndroidDeviceInfo> {
    let handle = DEVICE_INFO_HANDLE.get()?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("getDeviceInfo", serde_json::json!({}))
        .await
        .ok()?;
    if !response.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        return None;
    }
    Some(AndroidDeviceInfo {
        device_name: response.get("deviceName").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        model: response.get("model").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        manufacturer: response.get("manufacturer").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        os_version: response.get("osVersion").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        sdk_int: response.get("sdkInt").and_then(|v| v.as_i64()).unwrap_or_default() as i32,
    })
}

/// 非 Android 平台无系统设备信息
#[cfg(not(target_os = "android"))]
pub async fn get_android_device_info() -> Option<AndroidDeviceInfo> {
    None
}
