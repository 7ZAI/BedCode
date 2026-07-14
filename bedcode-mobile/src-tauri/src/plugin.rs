//! Android 原生插件桥接
//!
//! 通过 Rust 端 Tauri Plugin 的 `register_android_plugin()` API
//! 将 Kotlin 端的 ForegroundServicePlugin 注册到 Tauri PluginManager
//!
//! 任务状态通知已迁移到 @tauri-apps/plugin-notification (JS API)

pub mod android_plugins;
pub mod commands;
pub mod host;
pub mod manager;
pub mod registry;
pub mod storage;
pub mod types;

pub use android_plugins::init;
pub use registry::{MobilePlugin, PluginHostContext, builtin_manifests};
