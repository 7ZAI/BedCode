//! Android 原生插件注册
//!
//! 将 Kotlin 端的 ForegroundServicePlugin 注册到 Tauri PluginManager。
//!
//! Tauri 2.0 的 Android 插件注册必须通过 Rust 端 `api.register_android_plugin()` 完成，
//! Kotlin 端的 `@TauriPlugin` 注解仅为标记，不触发自动注册。
//!
//! 任务状态通知已迁移到 @tauri-apps/plugin-notification (JS API)，
//! 不再需要 Kotlin TaskNotificationPlugin。

use tauri::plugin::Builder;
use tauri::Runtime;

/// 初始化 Android 原生插件桥接
///
/// 仅在 Android 平台注册，其他平台为空操作
pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    Builder::new("android-plugins")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            api.register_android_plugin("com.bedcode.mobile", "ForegroundServicePlugin")?;
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}
