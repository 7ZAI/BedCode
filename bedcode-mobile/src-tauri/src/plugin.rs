//! Android 原生插件桥接
//!
//! 通过 Rust 端 Tauri Plugin 的 `register_android_plugin()` API
//! 将 Kotlin 端的 ForegroundServicePlugin 注册到 Tauri PluginManager
//!
//! 任务状态通知已迁移到 @tauri-apps/plugin-notification (JS API)

pub mod android_plugins;

pub use android_plugins::init;
