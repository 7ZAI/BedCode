//! WebSocket Session State
//!
//! 管理 WS 连接的认证状态和订阅信息

use std::collections::HashSet;
use std::net::SocketAddr;

/// WebSocket 会话状态
pub struct WsSession {
    /// 客户端地址
    pub addr: SocketAddr,
    /// 设备 ID（认证后设置）
    pub device_id: Option<String>,
    /// 设备名称
    pub device_name: Option<String>,
    /// 设备指纹，用于与数据库 pairings 记录关联
    pub fingerprint: Option<String>,
    /// 是否已认证
    pub authenticated: bool,
    /// 订阅的会话列表
    pub subscribed_sessions: HashSet<String>,
}

impl WsSession {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            device_id: None,
            device_name: None,
            fingerprint: None,
            authenticated: false,
            subscribed_sessions: HashSet::new(),
        }
    }
}
