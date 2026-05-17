//! Authentication and Pairing
//!
//! 提供设备认证和配对基础功能
//!
//! 模块划分:
//! - pairing.rs: 配对基础数据结构
//! - qr_token.rs: QR 码 token 基础方法
//! - jwt.rs: JWT 认证服务
//! - storage.rs: 安全存储接口定义

pub mod pairing;
pub mod qr_token;
mod jwt;
mod storage;

// Re-export basic structures
pub use pairing::*;
pub use qr_token::*;
pub use jwt::*;
pub use storage::*;

// Conditional re-exports for platform-specific implementations
// Desktop: use PairingService from desktop module
// Mobile: use PairingService from mobile module

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use crate::desktop::server::services::pairing_service::PairingService;

#[cfg(any(target_os = "android", target_os = "ios"))]
pub use crate::mobile::pairing_service::PairingService;

// QR Token: desktop uses QrTokenService, mobile uses QrTokenManager from shared
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use crate::desktop::server::services::qr_token_service::QrTokenService as QrTokenManager;

#[cfg(any(target_os = "android", target_os = "ios"))]
pub use crate::shared::auth::qr_token::QrTokenManager;