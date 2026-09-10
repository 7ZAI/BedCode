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
    /// 设备唯一 ID 哈希（跨重装稳定，用于把拆分后的配对合并回原记录；老客户端不携带）
    pub uid_hash: Option<String>,
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
    /// 设备唯一 ID 哈希（跨重装稳定，用于把拆分后的配对合并回原记录；老客户端不携带）
    pub uid_hash: Option<String>,
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

/// POST /api/auth/biometric-bind request
///
/// 绑定/解绑生物凭证公钥（绑定传 SPKI base64，解绑传空串）。
/// 绑定/解绑须已认证：携带已有 JWT，桌面端校验其指纹与请求一致。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiometricBindRequest {
    pub device_id: String,
    pub device_fingerprint: String,
    pub public_key: String,
    pub session_token: String,
}

/// POST /api/auth/biometric-bind response data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiometricBindResponseData {
    pub bound: bool,
}
