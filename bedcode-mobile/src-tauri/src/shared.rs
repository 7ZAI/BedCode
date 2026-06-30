//! Shared modules
//!
//! 桌面端和移动端共享模块

pub mod auth;
pub mod enums;
pub mod model;
pub mod models;
pub mod system;

pub use system::error::{AppError, Result};
pub use system::config;