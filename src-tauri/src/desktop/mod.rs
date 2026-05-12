//! Desktop-specific modules
//!
//! 桌面端专用模块

pub mod commands;
pub mod error;
pub mod plugin;
pub mod pty;
pub mod session;
pub mod websocket;

pub use crate::shared::{AppError, Result};