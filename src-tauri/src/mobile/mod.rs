//! Mobile-specific modules
//!
//! 移动端专用模块

pub mod commands;

// Mobile uses crate-level re-exports
pub use crate::shared::error::{AppError, Result};