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
#[cfg(target_os = "android")]
pub async fn biometric_device_supported() -> crate::Result<bool> {
    let handle = BIOMETRIC_KEY_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("BiometricKeyPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("isDeviceSupported", serde_json::json!({}))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to check biometric support: {}", e)))?;
    Ok(response.get("supported").and_then(|v| v.as_bool()).unwrap_or(false))
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
pub async fn biometric_device_supported() -> crate::Result<bool> {
    Ok(false)
}
