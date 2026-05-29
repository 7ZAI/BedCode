//! Mobile-specific modules
//!
//! 移动端业务模块 - 使用 shared WebSocket 基础设施

pub mod auth;
pub mod commands;
pub mod connection;
pub mod handler;
pub mod output_receiver;
pub mod pairing_service;
pub mod session;
pub mod storage;
pub mod terminal;

// Re-export public types
pub use self::auth::{AuthCredentials, AuthManager, AuthStatus};
pub use self::connection::{ConnectionManager, ConnectionStatus, TargetDevice, set_global_token, get_global_token, clear_global_token};
pub use self::handler::{MobileEvent, MobileHandler};
pub use self::output_receiver::{OutputEvent, OutputReceiver};
pub use self::session::{SessionInfo, SessionManager, SessionStatus};
pub use self::storage::TokenStorage;
pub use self::terminal::{TerminalHistory, TerminalOutputEvent, TerminalIncrementalOutput, TerminalManager, get_terminal_manager};

// Mobile uses crate-level re-exports
pub use crate::shared::system::error::{AppError, Result};