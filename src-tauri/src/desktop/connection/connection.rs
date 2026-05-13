//! Connection Module
//!
//! 连接管理和认证模块

pub mod auth;
pub mod client;
pub mod types;

pub use auth::handle_auth;
pub use client::ClientInfo;
pub use types::*;