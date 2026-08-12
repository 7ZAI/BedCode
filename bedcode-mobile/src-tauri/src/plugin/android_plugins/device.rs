//! 设备 ID 插件（DeviceIdPlugin）
//!
//! 从 android_plugins.rs 拆分。

use std::sync::OnceLock;
use tauri::plugin::{Builder, PluginHandle};

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
