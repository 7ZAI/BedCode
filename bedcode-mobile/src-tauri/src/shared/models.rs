//! Mobile Data Models
//!
//! 移动端使用的数据模型定义
//! 从 shared/db/models.rs 迁移，去除数据库依赖

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 快捷指令
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
}

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
