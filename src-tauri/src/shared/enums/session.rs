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
}

/// 会话任务状态
///
/// Plugin 会话的 Claude Code 任务执行状态，由插件通过 HTTP API 推送
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 空闲 - 无任务运行
    Idle,
    /// 执行中 - 任务正在执行
    InProgress,
    /// 等待输入 - Claude 等待用户输入
    Asking,
    /// 已完成 - 任务完成
    Completed,
    /// 已中断 - 任务被中断或出错
    Interrupted,
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::Idle
    }
}

impl Default for SessionType {
    fn default() -> Self {
        Self::Pty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_default() {
        let status: TaskStatus = Default::default();
        assert_eq!(status, TaskStatus::Idle);
    }

    #[test]
    fn test_task_status_serde() {
        let cases = vec![
            (TaskStatus::Idle, "\"idle\""),
            (TaskStatus::InProgress, "\"in_progress\""),
            (TaskStatus::Asking, "\"asking\""),
            (TaskStatus::Completed, "\"completed\""),
            (TaskStatus::Interrupted, "\"interrupted\""),
        ];
        for (status, expected) in cases {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, expected);
            let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }
}