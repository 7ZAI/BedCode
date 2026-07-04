//! Plugin System
//!
//! 插件系统 - 加载、注册、权限管理和 API 桥接

pub mod api_bridge;
pub mod cdylib_loader;
pub mod host;
pub mod host_context;
pub mod loader;
pub mod manager;
pub mod permission;
pub mod registry;
pub mod setup;
pub mod storage;
pub mod types;

pub use manager::PluginManager;
pub use host::PluginHost;
pub use setup::{TokenSetupResult, ProjectHooksResult};
