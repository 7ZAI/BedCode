//! System module
//!
//! 系统级功能模块

pub mod commands;
pub mod config;
pub mod error;
pub mod error_boundary;
pub mod process;

pub use config::AppConfig;
pub use error::{AppError, Result};
