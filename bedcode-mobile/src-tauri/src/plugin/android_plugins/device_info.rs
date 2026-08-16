//! 设备信息插件（DeviceInfoPlugin）
//!
//! 从 android_plugins.rs 拆分。

use std::sync::OnceLock;
use tauri::plugin::{Builder, PluginHandle};

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
