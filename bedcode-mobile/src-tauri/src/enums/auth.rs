//! Auth Types
//!
//! 认证相关类型定义

use serde::{Deserialize, Serialize};

/// 认证阶段
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStage {
    /// 请求配对
    RequestPairing,
    /// 配对码验证
    VerifyCode,
    /// 交换证书
    ExchangeCertificate,
    /// 生物认证请求（移动端发起，桌面端下发挑战值）
    BiometricRequest,
    /// 生物认证挑战（桌面端下发一次性 nonce）
    BiometricChallenge,
    /// 生物认证验证（移动端回传签名）
    BiometricVerify,
    /// 认证成功
    Authenticated,
    /// JWT 重新认证（移动端发送，携带 session_token）
    Reauthenticate,
    /// 认证失败
    Failed,
    /// QR 码连接
    QrConnect,
    /// QR 连接失败
    QrFailed,
}

/// 认证载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthPayload {
    /// 认证阶段
    pub stage: AuthStage,
    /// 设备 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// 设备名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// 设备指纹
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_fingerprint: Option<String>,
    /// 配对码
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pairing_code: Option<String>,
    /// 会话令牌
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    /// 错误消息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// QR 令牌
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_token: Option<String>,
    /// 生物凭证公钥（SPKI X.509 DER base64，绑定/解绑时使用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    /// 生物认证挑战值（hex）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenge_nonce: Option<String>,
    /// 生物认证签名（原始 r||s base64）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// 认证方式（pairing_code / qr / biometric / jwt）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,
}

impl Default for AuthPayload {
    fn default() -> Self {
        Self {
            stage: AuthStage::RequestPairing,
            device_id: None,
            device_name: None,
            device_fingerprint: None,
            pairing_code: None,
            session_token: None,
            error: None,
            qr_token: None,
            public_key: None,
            challenge_nonce: None,
            signature: None,
            auth_method: None,
        }
    }
}