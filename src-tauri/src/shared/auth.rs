//! Authentication and Pairing
//!
//! 提供设备认证和配对功能
//!
//! 模块划分:
//! - pairing.rs: 配对服务
//! - qr_token.rs: QR 码 token 管理
//! - jwt.rs: JWT 认证服务
//! - storage.rs: 安全存储接口定义
//! - storage_desktop.rs: 桌面端存储实现（keyring）
//! - storage_mobile.rs: 移动端存储实现（内存）

mod pairing;
pub mod qr_token;
mod jwt;
mod storage;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod storage_desktop;

#[cfg(any(target_os = "android", target_os = "ios"))]
mod storage_mobile;

pub use pairing::*;
pub use qr_token::*;
pub use jwt::*;
pub use storage::*;