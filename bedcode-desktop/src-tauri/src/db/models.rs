//! Database models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Paired device
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pairing {
    pub id: String,
    pub device_name: String,
    pub device_fingerprint: String,
    pub public_key: String,
    pub address: Option<String>,
    pub session_token: Option<String>,
    pub paired_at: DateTime<Utc>,
    pub last_seen: Option<DateTime<Utc>>,
    pub connect_count: i32,
    pub is_active: bool,
}

/// 设备连接历史事件
///
/// 每次认证成功/失败记录一条；认证方式取值见 `auth_method` 常量
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionHistory {
    pub id: i64,
    /// 关联 pairings.id（设备 ID）
    pub device_id: String,
    /// 认证方式：pairing_code / qr / biometric / jwt
    pub auth_method: String,
    /// 结果：success / failed
    pub result: String,
    pub address: Option<String>,
    pub connected_at: DateTime<Utc>,
    pub disconnected_at: Option<DateTime<Utc>>,
}

/// 连接历史的认证方式取值
pub mod connection_method {
    /// 配对码
    pub const PAIRING_CODE: &str = "pairing_code";
    /// QR 令牌
    pub const QR: &str = "qr";
    /// 生物认证
    pub const BIOMETRIC: &str = "biometric";
    /// JWT 会话令牌（重连）
    pub const JWT: &str = "jwt";
}

/// 连接历史的结果取值
pub mod connection_result {
    /// 认证成功
    pub const SUCCESS: &str = "success";
    /// 认证失败
    pub const FAILED: &str = "failed";
}

/// 每设备保留的最大历史条数
pub const CONNECTION_HISTORY_MAX_PER_DEVICE: i64 = 100;

/// Session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfig {
    pub id: String,
    pub name: String,
    pub environment: String,
    pub wsl_distro: Option<String>,
    pub working_dir: String,
    pub command: String,
    pub auto_start: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SessionConfig {
    pub fn new(name: String, environment: String, working_dir: String, command: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            environment,
            wsl_distro: None,
            working_dir,
            command,
            auto_start: false,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Quick action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickAction {
    pub id: String,
    pub name: String,
    pub content: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub category: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

impl QuickAction {
    pub fn new(name: String, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            content,
            icon: None,
            color: None,
            category: None,
            sort_order: 0,
            created_at: Utc::now(),
        }
    }

    pub fn with_icon(mut self, icon: String) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn with_color(mut self, color: String) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_category(mut self, category: String) -> Self {
        self.category = Some(category);
        self
    }
}

/// App setting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub updated_at: DateTime<Utc>,
}

impl Setting {
    pub fn new(key: String, value: String) -> Self {
        Self {
            key,
            value,
            updated_at: Utc::now(),
        }
    }
}
