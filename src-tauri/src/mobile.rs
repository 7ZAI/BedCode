//! Mobile-specific modules
//!
//! 移动端业务模块 - 使用 shared WebSocket 基础设施

pub mod auth;
pub mod commands;
pub mod global;
pub mod handler;
pub mod managers;
pub mod remote;
pub mod router;
pub mod session;
pub mod system;
pub mod websocket_client;

// Re-export public types
pub use self::auth::{AuthCredentials, AuthManager, AuthStatus};
pub use self::global::{set_global_token, get_global_token, clear_global_token};
pub use self::managers::{get_connection_manager, get_auth_manager, get_session_manager};
pub use self::remote::{
    ConnectionManager, ConnectionStatus, TargetDevice,
};
pub use self::router::{ClientRouteContext, ClientBusinessRouter, ClientRouteRegistry, ClientRouteHandler};
pub use self::router::event::MobileEvent;
pub use self::router::{TerminalHandler, AuthHandler, SyncHandler, SystemHandler};
pub use self::session::{SessionInfo, SessionManager, SessionStatus};

// Re-export commands module public items
pub use self::commands::{
    // All Tauri commands
    ws_set_token, ws_get_token, ws_clear_token,
    ws_connect, ws_disconnect, ws_get_status, ws_is_connected, ws_reconnect,
    ws_get_auth_status, ws_authenticate, ws_request_pairing, ws_verify_pairing_code, ws_authenticate_with_qr,
    ws_load_sessions, ws_join_session, ws_leave_session, ws_subscribe_session,
    ws_start_session, ws_stop_session, ws_remove_session, ws_load_session_configs,
    ws_send_input_async, ws_send_message, ws_send_and_wait, ws_resize_terminal,
    set_screen_orientation, keep_screen_awake,
};

// Mobile uses crate-level re-exports
pub use crate::shared::system::error::{AppError, Result};