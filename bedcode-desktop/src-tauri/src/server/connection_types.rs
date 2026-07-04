//! Connection Types
//!
//! 连接管理和认证相关的类型定义

use serde::Serialize;

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
    /// 设备指纹，用于与数据库 pairings 记录关联匹配
    pub fingerprint: Option<String>,
    pub event: String, // "connected", "disconnected", "authenticated"
}

/// 设备连接信息（前端展示用）
#[derive(Debug, Clone, Serialize)]
pub struct DeviceConnectionInfo {
    pub addr: String,
    pub device_id: String,
    /// 设备指纹，用于与数据库 pairings 记录关联匹配
    pub fingerprint: Option<String>,
    pub session_count: usize,
}

// Re-export from shared module for backward compatibility
pub use crate::enums::{AuthPayload, AuthStage};