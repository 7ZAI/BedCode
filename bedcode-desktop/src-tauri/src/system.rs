//! System Module
//!
//! 系统基础设施 - 应用上下文、配置、错误类型和错误边界

pub mod app_context;
pub mod config;
pub mod error;
pub mod error_boundary;

pub use app_context::AppContext;
pub use config::AppConfig;
pub use error::{AppError, Result};
pub use error_boundary::spawn_with_error_boundary;
