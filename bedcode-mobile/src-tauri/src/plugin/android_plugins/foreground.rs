//! 前台服务保活（ForegroundServicePlugin）
//!
//! 从 android_plugins.rs 拆分。

use tauri::plugin::Builder;

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
