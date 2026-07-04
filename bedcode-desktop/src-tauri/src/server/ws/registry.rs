//! WS Session Registry
//!
//! 全局单例，维护所有 Actix WS actor 的地址映射
//! 提供 send_to_client / broadcast 等消息转发能力

use actix::Addr;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::sync::RwLock;

use super::terminal_ws::{SendTextMessage, TerminalWs};

/// WS 会话注册条目
struct WsSessionEntry {
    actor_addr: Addr<TerminalWs>,
    socket_addr: SocketAddr,
    device_name: Option<String>,
    /// 设备指纹，认证时设置，用于与数据库 pairings 记录关联
    fingerprint: Option<String>,
    authenticated: bool,
    connected_at: i64,
}

/// WS 会话注册表 — 全局单例
///
/// 职责：
/// - 维护 client_id → Addr<TerminalWs> 映射
/// - 维护 SocketAddr → client_id 映射
/// - 提供 send / broadcast 等消息转发
pub struct WsSessionRegistry {
    /// client_id → 会话条目
    sessions: RwLock<HashMap<String, WsSessionEntry>>,
    /// SocketAddr → client_id（反向查找）
    addr_to_client_id: RwLock<HashMap<SocketAddr, String>>,
}

impl WsSessionRegistry {
    /// 获取全局单例
    pub fn global() -> &'static Self {
        static INSTANCE: std::sync::LazyLock<WsSessionRegistry> =
            std::sync::LazyLock::new(|| WsSessionRegistry {
                sessions: RwLock::new(HashMap::new()),
                addr_to_client_id: RwLock::new(HashMap::new()),
            });
        &INSTANCE
    }

    /// 注册新的 WS 连接
    pub async fn register(
        &self,
        client_id: String,
        socket_addr: SocketAddr,
        actor_addr: Addr<TerminalWs>,
    ) {
        let connected_at = chrono::Utc::now().timestamp_millis();

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(client_id.clone(), WsSessionEntry {
                actor_addr,
                socket_addr,
                device_name: None,
                fingerprint: None,
                authenticated: false,
                connected_at,
            });
        }
        {
            let mut addr_map = self.addr_to_client_id.write().await;
            addr_map.insert(socket_addr, client_id.clone());
        }

        tracing::debug!("[WsSessionRegistry] Registered client {} from {}", client_id, socket_addr);
    }

    /// 注销 WS 连接
    pub async fn unregister(&self, client_id: &str) {
        if let Some(entry) = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(client_id)
        } {
            let mut addr_map = self.addr_to_client_id.write().await;
            addr_map.remove(&entry.socket_addr);
            tracing::debug!("[WsSessionRegistry] Unregistered client {} from {}", client_id, entry.socket_addr);
        }
    }

    /// 通过 SocketAddr 注销
    pub async fn unregister_by_addr(&self, addr: &SocketAddr) -> Option<String> {
        let client_id = {
            let mut addr_map = self.addr_to_client_id.write().await;
            addr_map.remove(addr)
        };

        if let Some(ref cid) = client_id {
            let mut sessions = self.sessions.write().await;
            sessions.remove(cid);
            tracing::debug!("[WsSessionRegistry] Unregistered client {} by addr {}", cid, addr);
        }

        client_id
    }

    /// 设置客户端认证状态
    pub async fn set_authenticated(&self, client_id: &str, device_name: Option<String>, fingerprint: Option<String>) {
        let mut sessions = self.sessions.write().await;
        if let Some(entry) = sessions.get_mut(client_id) {
            entry.authenticated = true;
            entry.device_name = device_name;
            entry.fingerprint = fingerprint;
        }
    }

    /// 设置设备名称
    pub async fn set_device_name(&self, client_id: &str, device_name: Option<String>) {
        let mut sessions = self.sessions.write().await;
        if let Some(entry) = sessions.get_mut(client_id) {
            entry.device_name = device_name;
        }
    }

    /// 向指定 client_id 发送文本
    pub async fn send_to_client(&self, client_id: &str, text: String) -> Result<(), String> {
        let sessions = self.sessions.read().await;
        if let Some(entry) = sessions.get(client_id) {
            entry.actor_addr
                .send(SendTextMessage { text })
                .await
                .map_err(|e| format!("Failed to send to client {}: {}", client_id, e))
        } else {
            Err(format!("Client {} not found", client_id))
        }
    }

    /// 通过 SocketAddr 发送文本
    pub async fn send_to_addr(&self, addr: &SocketAddr, text: String) -> Result<(), String> {
        let client_id = {
            let addr_map = self.addr_to_client_id.read().await;
            addr_map.get(addr).cloned()
        };

        match client_id {
            Some(cid) => self.send_to_client(&cid, text).await,
            None => Err(format!("No client at addr {}", addr)),
        }
    }

    /// 向所有已认证客户端广播文本
    ///
    /// exclude_device_name: 排除指定设备名称的客户端（用于同步事件排除操作者）
    pub async fn broadcast(&self, text: String, exclude_device_name: Option<&str>) {
        let sessions = self.sessions.read().await;
        let mut sent_count = 0usize;

        for (client_id, entry) in sessions.iter() {
            if !entry.authenticated {
                continue;
            }

            // 排除指定设备
            if let Some(exclude) = exclude_device_name {
                if let Some(ref name) = entry.device_name {
                    if name == exclude {
                        continue;
                    }
                }
            }

            if let Err(e) = entry.actor_addr.send(SendTextMessage { text: text.clone() }).await {
                tracing::warn!(client_id = %client_id, error = %e, "Failed to broadcast to client");
            } else {
                sent_count += 1;
            }
        }

        if sent_count > 0 {
            tracing::debug!("[WsSessionRegistry] Broadcast to {} clients", sent_count);
        }
    }

    /// 获取所有客户端 ID
    pub async fn all_client_ids(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }

    /// 获取已认证客户端数量
    pub async fn authenticated_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.values().filter(|e| e.authenticated).count()
    }

    /// 获取客户端总数
    pub async fn client_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }

    /// 获取客户端摘要信息列表
    pub async fn list_clients(&self) -> Vec<ClientSummary> {
        let sessions = self.sessions.read().await;
        sessions
            .iter()
            .map(|(client_id, entry)| ClientSummary {
                client_id: client_id.clone(),
                device_name: entry.device_name.clone(),
                fingerprint: entry.fingerprint.clone(),
                addr: entry.socket_addr.to_string(),
                authenticated: entry.authenticated,
                connected_at: entry.connected_at,
            })
            .collect()
    }

    /// 通过 client_id 获取客户端摘要
    pub async fn get_client(&self, client_id: &str) -> Option<ClientSummary> {
        let sessions = self.sessions.read().await;
        sessions.get(client_id).map(|entry| ClientSummary {
            client_id: client_id.to_string(),
            device_name: entry.device_name.clone(),
            fingerprint: entry.fingerprint.clone(),
            addr: entry.socket_addr.to_string(),
            authenticated: entry.authenticated,
            connected_at: entry.connected_at,
        })
    }

    /// 通过 SocketAddr 获取客户端摘要
    pub async fn get_client_by_addr(&self, addr: &SocketAddr) -> Option<ClientSummary> {
        let client_id = {
            let addr_map = self.addr_to_client_id.read().await;
            addr_map.get(addr).cloned()
        }?;

        self.get_client(&client_id).await
    }

    /// 客户端是否已认证
    pub async fn is_authenticated(&self, client_id: &str) -> bool {
        let sessions = self.sessions.read().await;
        sessions.get(client_id).map(|e| e.authenticated).unwrap_or(false)
    }

    /// 获取设备名称
    pub async fn get_device_name(&self, client_id: &str) -> Option<String> {
        let sessions = self.sessions.read().await;
        sessions.get(client_id).and_then(|e| e.device_name.clone())
    }

    /// 通过 SocketAddr 获取 device_name
    pub async fn get_device_name_by_addr(&self, addr: &SocketAddr) -> Option<String> {
        let client_id = {
            let addr_map = self.addr_to_client_id.read().await;
            addr_map.get(addr).cloned()
        }?;

        self.get_device_name(&client_id).await
    }

    /// 通过 device_name 获取 client_id
    pub async fn get_client_id_by_device_name(&self, device_name: &str) -> Option<String> {
        let sessions = self.sessions.read().await;
        for (client_id, entry) in sessions.iter() {
            if entry.device_name.as_deref() == Some(device_name) {
                return Some(client_id.clone());
            }
        }
        None
    }

    /// 清空所有注册信息（服务器停机时调用）
    pub async fn clear_all(&self) {
        let count = {
            let mut sessions = self.sessions.write().await;
            let count = sessions.len();
            sessions.clear();
            count
        };
        {
            let mut addr_map = self.addr_to_client_id.write().await;
            addr_map.clear();
        }
        if count > 0 {
            tracing::info!("[WsSessionRegistry] Cleared {} sessions", count);
        }
    }
}

/// 客户端摘要（与 websocket_manager 中的定义对齐）
#[derive(Debug, Clone)]
pub struct ClientSummary {
    pub client_id: String,
    pub device_name: Option<String>,
    /// 设备指纹，用于与数据库 pairings 记录关联
    pub fingerprint: Option<String>,
    pub addr: String,
    pub authenticated: bool,
    pub connected_at: i64,
}
