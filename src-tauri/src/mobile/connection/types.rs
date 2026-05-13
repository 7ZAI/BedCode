//! Connection Types - 连接相关类型定义
//!
//! 定义连接状态、设备信息、会话信息等共享类型

use tokio::sync::broadcast;

/// 连接状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// 未连接
    Disconnected,
    /// 正在连接
    Connecting,
    /// 已连接（WebSocket 连接已建立，等待认证）
    Connected,
    /// 配对中（等待用户输入配对码）
    Pairing,
    /// 已认证（配对成功）
    Authenticated,
    /// 连接错误
    Error(String),
}

/// 远程设备信息
#[derive(Debug, Clone)]
pub struct RemoteDevice {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub is_paired: bool,
}

/// 配对请求结果
#[derive(Debug, Clone)]
pub struct PairingRequestResult {
    pub code: String,
    pub expires_in: u64,
}

/// 会话信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct RemoteSession {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
}

/// 待发送的请求
pub struct PendingRequest {
    pub resolve: tokio::sync::oneshot::Sender<crate::desktop::websocket::message::Message>,
    pub timeout: tokio::task::JoinHandle<()>,
}

/// 输出事件
#[derive(Debug, Clone, serde::Serialize)]
pub struct OutputEvent {
    pub event_type: String,
    pub session_id: String,
    pub data: String,
    pub is_waiting: bool,
}
