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

/// Auth token response (shared by verify, qr-connect, reauth)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthTokenResponseData {
    pub token: String,
    pub expires_in: u64,
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
