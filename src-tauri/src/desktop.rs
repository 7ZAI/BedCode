//! Desktop-specific modules
//!
//! 桌面端专用模块

pub mod commands;
pub mod enums;
pub mod event_forwarder;
pub mod model;
pub mod plugin;
pub mod pty;
pub mod session;
pub mod server;
pub mod traits;
pub mod websocket_manager;

pub use event_forwarder::EventForwarder;

pub use websocket_manager::{BusinessHandler, ClientSummary, WebSocketManager};

pub use crate::shared::{AppError, Result};