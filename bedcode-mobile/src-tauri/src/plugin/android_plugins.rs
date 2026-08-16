//! Android 原生插件注册与调用（模块入口）
//!
//! 将 Kotlin 端插件注册到 Tauri PluginManager，并提供 Rust → Kotlin 调用入口。
//! 按插件域拆分子模块（assets / notifications / biometric / saf / ...），
//! 本文件仅保留模块声明与 re-export，外部路径 `android_plugins::xxx` 不变。
//!
//! Tauri 2.0 的 Android 插件注册必须通过 Rust 端 `api.register_android_plugin()` 完成，
//! Kotlin 端的 `@TauriPlugin` 注解仅为标记，不触发自动注册。
//!
//! 注意：`register_android_plugin` 以 Builder 名称作为 Kotlin 端插件注册名（HashMap key），
//! 因此每个 Kotlin 插件必须使用独立的 Builder 名称，否则同名注册会互相覆盖，
//! 导致 `run_mobile_plugin_async` 路由到错误的插件。

mod all_files;
mod assets;
mod biometric;
mod device;
mod device_info;
mod downloads;
mod file_delete;
mod foreground;
mod notifications;
mod picker;
mod saf;

// 保持外部路径 crate::plugin::android_plugins::xxx 不变
pub use all_files::*;
pub use assets::*;
pub use biometric::*;
pub use device::*;
pub use device_info::*;
pub use downloads::*;
pub use file_delete::*;
pub use foreground::*;
pub use notifications::*;
pub use picker::*;
pub use saf::*;
