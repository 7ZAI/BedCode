//! Data Models
//!
//! 移动端数据模型定义

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 会话配置
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

/// 应用设置项
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
