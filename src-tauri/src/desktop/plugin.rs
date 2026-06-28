//! Plugin Module
//!
//! 插件系统模块入口 — 包含插件宿主、加载、权限、注册、存储和任务状态管理

pub mod manager;
pub mod permission;
pub mod setup;
pub mod types;

pub use self::manager::PluginManager;
pub use self::setup::{TokenSetupResult, ProjectHooksResult};
