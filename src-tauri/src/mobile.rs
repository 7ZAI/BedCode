//! Mobile-specific modules
//!
//! 移动端业务模块 - 使用 shared WebSocket 基础设施

pub mod auth;
pub mod commands;
pub mod connection;
pub mod handler;
pub mod pairing_service;
pub mod session;
pub mod terminal;

// Re-export public types
pub use self::auth::{AuthCredentials, AuthManager, AuthStatus};
pub use self::connection::{ConnectionManager, ConnectionStatus, TargetDevice};
pub use self::handler::{MobileEvent, MobileHandler, MobileMessage};
pub use self::session::{SessionInfo, SessionManager, SessionStatus};
pub use self::terminal::{TerminalHistory, TerminalOutputEvent, TerminalIncrementalOutput, TerminalManager, get_terminal_manager};

// Mobile uses crate-level re-exports
pub use crate::shared::system::error::{AppError, Result};