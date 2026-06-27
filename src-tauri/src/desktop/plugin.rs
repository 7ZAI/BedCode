//! Plugin Module
//!
//! 插件任务状态管理模块入口

pub mod manager;
pub mod setup;

pub use self::manager::PluginManager;
pub use setup::{TokenSetupResult, ProjectHooksResult};
