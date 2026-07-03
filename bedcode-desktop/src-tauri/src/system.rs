//! System Module
//!
//! 系统基础设施 - 应用上下文、配置、错误类型、错误边界和电源管理

pub mod app_context;
pub mod config;
pub mod error;
pub mod error_boundary;
pub mod power;

pub use app_context::AppContext;
pub use config::AppConfig;
pub use error::{AppError, Result};
pub use error_boundary::spawn_with_error_boundary;
pub use power::power_manager;
