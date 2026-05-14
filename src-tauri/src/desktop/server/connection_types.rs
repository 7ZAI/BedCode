//! Connection Types
//!
//! 连接管理和认证相关的类型定义

use serde::{Deserialize, Serialize};

/// 配对码生成事件 payload
#[derive(Debug, Clone, Serialize)]
pub struct PairingCodeGeneratedEvent {
    pub code: String,
    pub expires_in: u64,
    pub device_name: Option<String>,
}

/// 设备连接/断开事件（发给前端）
#[derive(Debug, Clone, Serialize)]
pub struct DeviceConnectionEvent {
    pub addr: String,
    pub device_id: String,
    pub device_name: Option<String>,
    pub event: String, // "connected", "disconnected", "authenticated"
}

/// 设备连接信息（前端展示用）
#[derive(Debug, Clone, Serialize)]
pub struct DeviceConnectionInfo {
    pub addr: String,
    pub device_id: String,
    pub session_count: usize,
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
}

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
    /// 认证成功
    Authenticated,
    /// 认证失败
    Failed,
    /// QR 码连接
    QrConnect,
    /// QR 连接失败
    QrFailed,
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
        }
    }
}