//! 任务/连接通知桥接（TaskNotificationPlugin）
//!
//! 从 android_plugins.rs 拆分；task_notification_plugin / notification_plugin_handle
//! 为工作区未提交代码重建（原内容被误覆盖）。

use std::sync::OnceLock;
use tauri::plugin::{Builder, PluginHandle};

/// 已注册的 TaskNotificationPlugin 句柄（仅 Android 平台使用）
static NOTIFICATION_PLUGIN_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();

/// 注册 TaskNotificationPlugin（任务/连接通知桥接，支持震动/声音分开控制）
pub fn task_notification_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("task-notification")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                // 保留句柄供 wasm_runtime host_notify 调用
                let handle = api.register_android_plugin("com.bedcode.mobile", "TaskNotificationPlugin")?;
                let _ = NOTIFICATION_PLUGIN_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}

/// 获取 TaskNotificationPlugin 句柄（host_notify 使用）
pub fn notification_plugin_handle() -> Option<&'static PluginHandle<tauri::Wry>> {
    NOTIFICATION_PLUGIN_HANDLE.get()
}
