//! Services Module
//!
//! 业务服务层，按职责划分

pub mod auth_service;
pub mod terminal_service;
pub mod pairing_service;
pub mod session_control;
pub mod session_sub;
pub mod session_config;

pub use auth_service::handle_auth;
pub use auth_service::handle_jwt_auth;
pub use terminal_service::handle_input;
pub use pairing_service::PairingService;
pub use session_control::handle_control;
pub use session_control::handle_control_message;
pub use session_sub::{subscribe_session, unsubscribe_session};
pub use session_config::{list_session_configs, list_quick_actions};