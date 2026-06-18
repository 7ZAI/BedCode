//! Desktop Authentication
//!
//! 桌面端专用认证模块 - JWT 服务和 QR Token 管理

pub mod jwt;
pub mod qr_token;

pub use jwt::*;
pub use qr_token::*;
