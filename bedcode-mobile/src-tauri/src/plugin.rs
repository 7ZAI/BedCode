//! Mobile Plugin System
//!
//! 插件系统入口 — 管理内置插件的加载、激活、停用和状态持久化

pub mod android_plugins;
pub mod commands;
pub mod host;
pub mod manager;
pub mod registry;
pub mod storage;
pub mod types;

pub use android_plugins::init;
pub use registry::{MobilePlugin, PluginHostContext, builtin_manifests};
