//! Data Models
//!
//! 移动端数据模型定义。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
