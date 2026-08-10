//! System Module
//!
//! 系统级功能 - 错误处理、配置管理、设置存储、共享命令

pub mod commands;
pub mod config;
pub mod constants;
pub mod error;
pub mod error_boundary;
pub mod info;
pub mod settings;

pub use config::AppConfig;
pub use error::{AppError, Result};
