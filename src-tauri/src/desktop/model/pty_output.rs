//! PTY Output Event
//!
//! PTY 输出事件数据结构

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// PTY 输出事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyOutputEvent {
    pub session_id: String,
    /// Base64 编码的输出数据
    pub data: String,
    pub timestamp: DateTime<Utc>,
    /// 是否等待用户输入（用于插件会话）
    #[serde(default)]
    pub is_waiting: bool,
    /// 全局递增索引，用于去重（桌面端 + 移动端统一计数）
    #[serde(default)]
    pub index: usize,
}