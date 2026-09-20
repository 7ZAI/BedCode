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

impl Default for SessionType {
    fn default() -> Self {
        Self::Pty
    }
}

// 票 12 contract：`TaskStatus`（Plugin 会话任务执行状态）已随内核四个任务字段一并
// 摘除——内核对任务语义零认知（spec D5）。任务状值现由写入方插件落在会话注解槽，
// 经 `session::task_fields_from_slot` 以**不透明字符串**转发到对外形状
// （`taskStatus` 字段），内核不解析其取值含义。
