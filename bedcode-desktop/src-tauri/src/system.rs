//! System Module
//!
//! 系统基础设施 - 应用上下文、配置、常量、错误类型、错误边界、电源管理和生命周期钩子

pub mod app_context;
pub mod config;
pub mod constants;
pub mod error;
pub mod error_boundary;
pub mod info;
pub mod lifecycle;
pub mod logging;
pub mod power;

pub use app_context::AppContext;
pub use config::AppConfig;
pub use error::{AppError, Result};
pub use error_boundary::spawn_with_error_boundary;
pub use info::SystemInfo;
pub use lifecycle::lifecycle_registry;
pub use power::power_manager;
