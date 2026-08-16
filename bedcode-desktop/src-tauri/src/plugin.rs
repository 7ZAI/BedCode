//! Plugin System
//!
//! 插件系统 - 加载、注册、权限管理和 API 桥接

pub mod api_bridge;
pub mod api_registry;
pub mod approval;
pub mod file_service;
pub mod fs_auth;
pub mod host;
pub mod loader;
pub mod message_bus;
pub mod permission;
pub mod registry;
pub mod storage;
pub mod types;
pub mod validation;
pub mod wasm_runtime;
#[cfg(debug_assertions)]
pub mod watcher;

pub use host::PluginHost;
pub use fs_auth::FsAuthChecker;
