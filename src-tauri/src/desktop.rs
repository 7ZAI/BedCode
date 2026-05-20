//! Desktop-specific modules
//!
//! 桌面端专用模块

pub mod commands;
pub mod enums;
pub mod model;
pub mod plugin;
pub mod pty;
pub mod session;
pub mod server;
pub mod websocket_manager;

pub use websocket_manager::{BusinessHandler, ClientSummary, WebSocketManager};

pub use crate::shared::{AppError, Result};