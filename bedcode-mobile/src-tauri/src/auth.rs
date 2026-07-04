//! Authentication Module
//!
//! 认证和配对 - 包含认证管理器、配对数据结构和认证状态

pub mod manager;
pub mod pairing;

use serde::{Deserialize, Serialize};

// Re-export public types
pub use manager::AuthManager;
pub use pairing::{PairingCode, PendingDevice};

/// 认证凭据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthCredentials {
    /// 设备配对 ID
    pub pairing_id: String,
    /// 设备指纹
    pub fingerprint: String,
    /// 会话令牌
    pub session_token: String,
}

/// 认证状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStatus {
    /// 未认证
    Unauthenticated,
    /// 正在认证
    Authenticating,
    /// 等待配对码输入
    WaitingPairingCode,
    /// 已认证
    Authenticated,
    /// 认证失败
    Failed(String),
}
