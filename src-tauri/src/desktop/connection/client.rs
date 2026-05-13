//! Client Info
//!
//! 客户端连接信息结构体

use std::net::SocketAddr;
use std::time::Instant;

/// 客户端连接信息
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub addr: SocketAddr,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub authenticated: bool,
    pub session_ids: Vec<String>,
    /// 订阅的会话列表（用于输出转发）
    pub subscribed_sessions: Vec<String>,
    /// 最后收到心跳的时间
    pub last_heartbeat: Instant,
    /// 客户端的终端列数（每个客户端独立）
    pub cols: u16,
    /// 客户端的终端行数（每个客户端独立）
    pub rows: u16,
}
