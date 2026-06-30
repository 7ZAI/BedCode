//! Desktop-specific modules
//!
//! 桌面端专用模块

pub mod app_context;
pub mod auth;
pub mod commands;
pub mod enums;
pub mod event_forwarder;
pub mod events;
pub mod model;
pub mod parser;
pub mod plugin;
pub mod pty;
pub mod session;
pub mod server;
pub mod traits;
pub mod websocket_manager;

pub use event_forwarder::EventForwarder;
pub use app_context::AppContext;

pub use websocket_manager::{ClientSummary, WebSocketManager};

pub use crate::shared::{AppError, Result};