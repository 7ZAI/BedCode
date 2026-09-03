//! Auth DTOs

use serde::{Deserialize, Serialize};

/// POST /api/auth/pairing request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingRequest {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
}

/// POST /api/auth/pairing response data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingResponseData {
    pub pairing_code: String,
    pub expires_in: u64,
}

/// POST /api/auth/verify request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyPairingRequest {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub pairing_code: String,
    pub address: String,
}

/// Auth token response (shared by verify, qr-connect, reauth, biometric-verify)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthTokenResponseData {
    pub token: String,
    pub expires_in: u64,
    /// 链路加密身份公钥（base64；移动端配对/重认证时建立或刷新 pin，issue 03）。
    /// 身份未初始化时省略（老客户端忽略未知字段）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kd_public_b64: Option<String>,
    /// 身份指纹（SHA-256 前 16 hex，供设置页人工核对）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kd_fingerprint: Option<String>,
}

/// POST /api/auth/qr-connect request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QrConnectRequest {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub qr_token: String,
    pub address: String,
}

/// POST /api/auth/reauth request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReauthRequest {
    pub device_id: String,
    pub fingerprint: String,
    pub session_token: String,
}

/// POST /api/auth/biometric-challenge request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiometricChallengeRequest {
    pub device_id: String,
    pub device_fingerprint: String,
}

/// POST /api/auth/biometric-challenge response data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiometricChallengeResponseData {
    pub challenge_nonce: String,
    pub expires_in: u64,
}

/// POST /api/auth/biometric-verify request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiometricVerifyRequest {
    pub device_id: String,
    pub device_fingerprint: String,
    pub challenge_nonce: String,
    pub signature: String,
}
