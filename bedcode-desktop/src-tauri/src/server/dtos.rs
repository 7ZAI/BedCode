//! Server DTOs
//!
//! 请求/响应数据传输对象

pub mod auth_dto;
pub mod common_dto;
pub mod config_dto;
pub mod file_dto;
pub mod git_dto;
pub mod plugin_dto;
pub mod session_dto;

pub use common_dto::{ApiResponse, CODE_INVALID_REQUEST, CODE_PLUGIN_AUTH_FAILED};
