//! Session Subscription Service
//!
//! 管理客户端与会话的订阅关系

use crate::server::client_info::ClientInfo;
use crate::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 订阅会话
pub async fn subscribe_session(
    clients: &Arc<RwLock<HashMap<SocketAddr, ClientInfo>>>,
    addr: &SocketAddr,
    session_id: &str,
) -> Result<()> {
    let mut clients = clients.write().await;
    if let Some(client) = clients.get_mut(addr) {
        if !client.subscribed_sessions.contains(&session_id.to_string()) {
            client.subscribed_sessions.push(session_id.to_string());
        }
    }
    Ok(())
}

/// 取消订阅会话
pub async fn unsubscribe_session(
    clients: &Arc<RwLock<HashMap<SocketAddr, ClientInfo>>>,
    addr: &SocketAddr,
    session_id: &str,
) -> Result<()> {
    let mut clients = clients.write().await;
    if let Some(client) = clients.get_mut(addr) {
        client.subscribed_sessions.retain(|s| s != session_id);
    }
    Ok(())
}