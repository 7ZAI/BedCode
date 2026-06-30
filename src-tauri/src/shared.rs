//! Shared modules
//!
//! 桌面端和移动端共享模块

pub mod auth;
pub mod db;
pub mod enums;
pub mod event;
pub mod model;
pub mod system;
pub mod utils;

pub use system::error::{AppError, Result};
pub use system::config;