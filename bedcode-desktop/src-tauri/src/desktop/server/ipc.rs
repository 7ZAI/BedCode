//! IPC Protocol Types
//!
//! 定义主进程与服务器子进程之间的通信协议
//! 使用 stdin/stdout JSON 行协议，每行一条消息

use serde::{Deserialize, Serialize};

use super::metrics::ServerMetrics;

// ==================== 主进程 → 子进程 (stdin) ====================

/// 主进程发送给子进程的命令
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", content = "data")]
pub enum IpcCommand {
    /// 启动服务器（附带端口）
    #[serde(rename = "start")]
    Start { port: u16 },
    /// 优雅停机
    #[serde(rename = "stop")]
    Stop,
    /// 查询当前指标
    #[serde(rename = "get_metrics")]
    GetMetrics,
}

// ==================== 子进程 → 主进程 (stdout) ====================

/// 子进程发送给主进程的响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcResponse {
    /// 服务器已启动
    #[serde(rename = "started")]
    Started { port: u16 },
    /// 服务器已停止
    #[serde(rename = "stopped")]
    Stopped,
    /// 指标数据
    #[serde(rename = "metrics")]
    Metrics(Box<ServerMetrics>),
    /// 错误
    #[serde(rename = "error")]
    Error { message: String },
    /// 心跳 + 基础指标（每 5 秒）
    #[serde(rename = "heartbeat")]
    Heartbeat(Box<ServerMetrics>),
}

impl IpcCommand {
    /// 序列化为 JSON 行（带换行符）
    pub fn to_json_line(&self) -> serde_json::Result<String> {
        let mut json = serde_json::to_string(self)?;
        json.push('\n');
        Ok(json)
    }
}

impl IpcResponse {
    /// 序列化为 JSON 行（带换行符）
    pub fn to_json_line(&self) -> serde_json::Result<String> {
        let mut json = serde_json::to_string(self)?;
        json.push('\n');
        Ok(json)
    }

    /// 从 JSON 行解析
    pub fn from_json_line(line: &str) -> serde_json::Result<Self> {
        serde_json::from_str(line.trim())
    }
}
