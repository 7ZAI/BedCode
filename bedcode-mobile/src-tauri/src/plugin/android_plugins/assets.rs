//! 内置插件资源解压（PluginAssetExtractor）
//!
//! 从 android_plugins.rs 拆分。

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
