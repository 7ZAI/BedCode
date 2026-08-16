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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_initializes_defaults() {
        let addr: SocketAddr = "127.0.0.1:8765".parse().unwrap();
        let info = ClientInfo::new(addr);

        assert_eq!(info.addr, addr);
        assert!(info.device_id.is_none());
        assert!(info.device_name.is_none());
        assert!(!info.authenticated);
        assert!(info.session_ids.is_empty());
        assert!(info.subscribed_sessions.is_empty());
        // 默认终端尺寸（客户端 resize 前使用的初值）
        assert_eq!(info.cols, 120);
        assert_eq!(info.rows, 40);
    }
}
