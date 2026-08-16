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
    /// 交换证书（生物凭证绑定：移动端上报公钥，空公钥表示解绑）
    ExchangeCertificate,
    /// 生物认证请求（移动端 → 桌面端，请求挑战值）
    BiometricRequest,
    /// 生物认证挑战值下发（桌面端 → 移动端，携带一次性随机数）
    BiometricChallenge,
    /// 生物认证应答（移动端 → 桌面端，携带挑战值签名）
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
    /// 生物凭证公钥（SPKI base64，绑定/解绑时携带）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    /// 生物认证挑战值（一次性随机数）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenge_nonce: Option<String>,
    /// 生物认证挑战值签名（base64，r||s 原始格式）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// 实际使用的认证方式（pairing_code / qr / biometric / jwt）
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