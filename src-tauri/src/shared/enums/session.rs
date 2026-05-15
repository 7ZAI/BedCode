//! Session Types
//!
//! 会话相关类型定义

use serde::{Deserialize, Serialize};

/// 会话状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus {
    /// 空闲（移动端使用）
    Idle,
    /// 正在启动
    Starting,
    /// 运行中
    Running,
    /// 等待输入
    WaitingInput,
    /// 正在停止
    Stopping,
    /// 已停止
    Stopped,
    /// 出错（可选错误信息）
    Error(Option<String>),
}

impl Default for SessionStatus {
    fn default() -> Self {
        Self::Starting
    }
}

/// 会话类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionType {
    Pty,
    Plugin,
}

impl Default for SessionType {
    fn default() -> Self {
        Self::Pty
    }
}