//! SAF 目录/文件选择（SafPickerPlugin + SafTransferPlugin）
//!
//! 从 android_plugins.rs 拆分。

use std::sync::OnceLock;
use tauri::plugin::{Builder, PluginHandle};

/// 已注册的 SafPickerPlugin 句柄（仅 Android 平台使用）
static SAF_PICKER_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();


/// 已注册的 SafTransferPlugin 句柄（仅 Android 平台使用）
pub(super) static SAF_TRANSFER_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();


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
    let path = crate::plugin::saf_path::resolve_saf_path(authority, document_id, primary_dir).ok_or_else(|| {
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
