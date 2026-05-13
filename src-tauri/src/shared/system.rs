//! System module
//!
//! 系统级功能模块

pub mod commands;
pub mod config;
pub mod error;

// 移动端设置存储模块 (仅移动端编译)
#[cfg(any(target_os = "android", target_os = "ios"))]
pub mod settings;

pub use config::AppConfig;
pub use error::{AppError, Result};