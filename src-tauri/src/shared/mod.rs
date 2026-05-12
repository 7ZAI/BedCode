//! Shared modules
//!
//! 桌面端和移动端共享模块

pub mod auth;
pub mod commands;
pub mod config;
pub mod db;
pub mod error;
pub mod notify;
pub mod parser;

pub use error::{AppError, Result};