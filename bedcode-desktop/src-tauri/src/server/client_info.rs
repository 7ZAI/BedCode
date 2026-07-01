//! Client Info
//!
//! 客户端连接信息结构体

use std::net::SocketAddr;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub addr: SocketAddr,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub authenticated: bool,
    pub session_ids: Vec<String>,
    pub subscribed_sessions: Vec<String>,
    pub last_heartbeat: Instant,
    pub cols: u16,
    pub rows: u16,
}

impl ClientInfo {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            device_id: None,
            device_name: None,
            authenticated: false,
            session_ids: vec![],
            subscribed_sessions: vec![],
            last_heartbeat: Instant::now(),
            cols: 120,
            rows: 40,
        }
    }
}
