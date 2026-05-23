//! Connection Manager - WebSocket 连接管理器
//!
//! 职责：维护当前所有活跃的 WebSocket 连接，提供注册、注销、查找和广播能力。
//! 只关心谁在线和如何向他们发消息，不关心消息内容。

use crate::shared::websocket::server::server_config::{IpFilter, WsServerConfig};
use crate::shared::websocket::traits::ClientInfoTrait;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tracing::{debug, warn};

/// 连接唯一ID
pub type ConnectionId = u64;

/// 连接状态
#[derive(Debug, Clone)]
pub struct Connection {
    /// 连接唯一ID
    pub id: ConnectionId,
    /// 客户端地址
    pub addr: SocketAddr,
    /// 客户端标识（认证后设置）
    pub client_id: Option<String>,
    /// 标签（用于分组）
    pub tags: Vec<String>,
    /// 是否已认证
    pub authenticated: bool,
    /// 最后心跳时间
    pub last_heartbeat: std::time::Instant,
}

impl Connection {
    pub fn new(id: ConnectionId, addr: SocketAddr) -> Self {
        Self {
            id,
            addr,
            client_id: None,
            tags: Vec::new(),
            authenticated: false,
            last_heartbeat: std::time::Instant::now(),
        }
    }

    /// 添加标签
    pub fn add_tag(&mut self, tag: impl Into<String>) {
        let tag = tag.into();
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
    }

    /// 移除标签
    pub fn remove_tag(&mut self, tag: &str) {
        self.tags.retain(|t| t != tag);
    }

    /// 检查是否有指定标签
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(&tag.to_string())
    }
}

/// 实现 ClientInfoTrait，使 Connection 可以替代 DefaultClientInfo
impl ClientInfoTrait for Connection {
    fn addr(&self) -> SocketAddr {
        self.addr
    }

    fn client_id(&self) -> Option<&str> {
        self.client_id.as_deref()
    }

    fn set_client_id(&mut self, id: Option<String>) {
        self.client_id = id;
    }

    fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    fn set_authenticated(&mut self, auth: bool) {
        self.authenticated = auth;
    }

    fn last_heartbeat(&self) -> Instant {
        self.last_heartbeat
    }

    fn set_last_heartbeat(&mut self, time: Instant) {
        self.last_heartbeat = time;
    }
}

/// 连接管理器
pub struct ConnectionManager {
    /// 连接池：ConnectionId -> (SocketAddr, Sender, Connection元数据)
    connections: Arc<RwLock<HashMap<ConnectionId, (SocketAddr, mpsc::Sender<WsMsg>, Connection)>>>,
    /// SocketAddr -> ConnectionId 映射（用于快速查找）
    addr_to_id: Arc<RwLock<HashMap<SocketAddr, ConnectionId>>>,
    /// 标签分组：tag -> ConnectionId 集合
    tag_groups: Arc<RwLock<HashMap<String, std::collections::HashSet<ConnectionId>>>>,
    /// 下一个可用的连接ID
    next_id: Arc<RwLock<ConnectionId>>,
    /// IP 过滤规则
    ip_filter: IpFilter,
    /// 最大连接数（0 表示不限制）
    max_connections: usize,
    /// 当前连接数
    connection_count: Arc<RwLock<usize>>,
    /// 连接事件广播
    event_tx: broadcast::Sender<ConnectionEvent>,
}

impl ConnectionManager {
    /// 创建新的连接管理器
    pub fn new(config: &WsServerConfig) -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            addr_to_id: Arc::new(RwLock::new(HashMap::new())),
            tag_groups: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
            ip_filter: config.ip_filter.clone(),
            max_connections: config.max_connections,
            connection_count: Arc::new(RwLock::new(0)),
            event_tx,
        }
    }

    /// 检查 IP 是否允许连接
    pub fn is_ip_allowed(&self, ip: &std::net::IpAddr) -> bool {
        self.ip_filter.is_allowed(ip)
    }

    /// 检查是否达到连接上限
    pub async fn is_full(&self) -> bool {
        if self.max_connections == 0 {
            return false;
        }
        *self.connection_count.read().await >= self.max_connections
    }

    /// 注册新连接
    pub async fn register(
        &self,
        addr: SocketAddr,
        sender: mpsc::Sender<WsMsg>,
    ) -> Option<ConnectionId> {
        // IP 检查
        if !self.is_ip_allowed(&addr.ip()) {
            warn!("Connection from {} denied by IP filter", addr);
            return None;
        }

        // 连接数检查
        if self.is_full().await {
            warn!("Connection from {} denied: max connections reached", addr);
            return None;
        }

        // 生成连接ID
        let id = {
            let mut next = self.next_id.write().await;
            let id = *next;
            *next += 1;
            id
        };

        let connection = Connection::new(id, addr);

        // 存储连接
        {
            let mut connections = self.connections.write().await;
            connections.insert(id, (addr, sender, connection));
        }

        // 记录 addr -> id 映射
        {
            let mut addr_to_id = self.addr_to_id.write().await;
            addr_to_id.insert(addr, id);
        }

        // 更新连连接总数
        {
            let mut count = self.connection_count.write().await;
            *count += 1;
        }

        // 发送连接事件
        let _ = self.event_tx.send(ConnectionEvent::Connected { id, addr });

        debug!("Connection registered: {} -> {}", id, addr);
        Some(id)
    }

    /// 注销连接
    pub async fn unregister(&self, id: ConnectionId) -> Option<SocketAddr> {
        let (addr, _, connection) = {
            let mut connections = self.connections.write().await;
            match connections.remove(&id) {
                Some((addr, sender, connection)) => (addr, sender, connection),
                None => return None,
            }
        };

        // 移除 addr -> id 映射
        {
            let mut addr_to_id = self.addr_to_id.write().await;
            addr_to_id.remove(&addr);
        }

        // 从所有标签组中移除
        {
            let mut tag_groups = self.tag_groups.write().await;
            for tag in &connection.tags {
                if let Some(ids) = tag_groups.get_mut(tag) {
                    ids.remove(&id);
                }
            }
        }

        // 更新连接数
        {
            let mut count = self.connection_count.write().await;
            *count = count.saturating_sub(1);
        }

        // 发送断开事件
        let _ = self.event_tx.send(ConnectionEvent::Disconnected {
            id,
            addr,
            client_id: connection.client_id,
        });

        debug!("Connection unregistered: {} -> {}", id, addr);
        Some(addr)
    }

    /// 通过 SocketAddr 注销连接
    pub async fn unregister_by_addr(&self, addr: &SocketAddr) -> Option<ConnectionId> {
        let id = {
            let addr_to_id = self.addr_to_id.read().await;
            addr_to_id.get(addr).copied()
        };

        if let Some(id) = id {
            self.unregister(id).await?;
            Some(id)
        } else {
            None
        }
    }

    /// 获取连接
    pub async fn get(&self, id: ConnectionId) -> Option<Connection> {
        let connections = self.connections.read().await;
        connections.get(&id).map(|(_, _, c)| c.clone())
    }

    /// 通过地址获取连接ID
    pub async fn get_id_by_addr(&self, addr: &SocketAddr) -> Option<ConnectionId> {
        let addr_to_id = self.addr_to_id.read().await;
        addr_to_id.get(addr).copied()
    }

    /// 更新客户端标识
    pub async fn set_client_id(&self, id: ConnectionId, client_id: Option<String>) {
        let mut connections = self.connections.write().await;
        if let Some((_, _, conn)) = connections.get_mut(&id) {
            let was_authenticated = conn.authenticated;
            conn.client_id = client_id.clone();
            conn.authenticated = client_id.is_some();

            // 发布认证事件
            if let Some(ref new_client_id) = client_id {
                if !was_authenticated {
                    // 首次认证成功
                    let _ = self.event_tx.send(ConnectionEvent::Authenticated {
                        id,
                        client_id: new_client_id.clone(),
                    });
                }
            }
        }
    }

    /// 更新心跳时间
    pub async fn update_heartbeat(&self, id: ConnectionId) {
        let mut connections = self.connections.write().await;
        if let Some((_, _, conn)) = connections.get_mut(&id) {
            conn.last_heartbeat = std::time::Instant::now();
        }
    }

    /// 添加标签
    pub async fn add_tag(&self, id: ConnectionId, tag: impl Into<String>) {
        let tag = tag.into();
        let mut connections = self.connections.write().await;
        if let Some((_, _, conn)) = connections.get_mut(&id) {
            conn.add_tag(&tag);

            // 加入标签组
            drop(connections);
            let mut tag_groups = self.tag_groups.write().await;
            tag_groups.entry(tag).or_default().insert(id);
        }
    }

    /// 移除标签
    pub async fn remove_tag(&self, id: ConnectionId, tag: &str) {
        let mut connections = self.connections.write().await;
        if let Some((_, _, conn)) = connections.get_mut(&id) {
            conn.remove_tag(tag);

            // 从标签组移除
            drop(connections);
            let mut tag_groups = self.tag_groups.write().await;
            if let Some(ids) = tag_groups.get_mut(tag) {
                ids.remove(&id);
            }
        }
    }

    /// 获取所有连接ID
    pub async fn all_ids(&self) -> Vec<ConnectionId> {
        let connections = self.connections.read().await;
        connections.keys().copied().collect()
    }

    /// 获取所有已认证的连接ID
    pub async fn authenticated_ids(&self) -> Vec<ConnectionId> {
        let connections = self.connections.read().await;
        connections
            .iter()
            .filter(|(_, (_, _, c))| c.authenticated)
            .map(|(id, _)| *id)
            .collect()
    }

    /// 获取指定标签的所有连接ID
    pub async fn ids_by_tag(&self, tag: &str) -> Vec<ConnectionId> {
        let tag_groups = self.tag_groups.read().await;
        tag_groups
            .get(tag)
            .map(|ids| ids.iter().copied().collect())
            .unwrap_or_default()
    }

    /// 获取当前连接数
    pub async fn count(&self) -> usize {
        *self.connection_count.read().await
    }

    /// 获取已认证连接数
    pub async fn authenticated_count(&self) -> usize {
        let connections = self.connections.read().await;
        connections.values().filter(|(_, _, c)| c.authenticated).count()
    }

    /// 获取指定连接的发送通道
    pub async fn get_sender(&self, id: ConnectionId) -> Option<mpsc::Sender<WsMsg>> {
        let connections = self.connections.read().await;
        connections.get(&id).map(|(_, tx, _)| tx.clone())
    }

    /// 获取指定地址的发送通道
    pub async fn get_sender_by_addr(&self, addr: &SocketAddr) -> Option<mpsc::Sender<WsMsg>> {
        let id = self.get_id_by_addr(addr).await?;
        self.get_sender(id).await
    }

    /// 获取所有连接的发送通道
    pub async fn get_all_senders(&self) -> Vec<mpsc::Sender<WsMsg>> {
        let connections = self.connections.read().await;
        connections.values().map(|(_, tx, _)| tx.clone()).collect()
    }

    /// 获取多个连接的发送通道
    pub async fn get_senders(&self, ids: &[ConnectionId]) -> Vec<mpsc::Sender<WsMsg>> {
        let connections = self.connections.read().await;
        ids.iter()
            .filter_map(|id| connections.get(id).map(|(_, tx, _)| tx.clone()))
            .collect()
    }

    /// 获取除指定连接外的所有发送通道
    pub async fn get_other_senders(&self, exclude_id: ConnectionId) -> Vec<mpsc::Sender<WsMsg>> {
        let connections = self.connections.read().await;
        connections
            .iter()
            .filter(|(id, _)| **id != exclude_id)
            .map(|(_, (_, tx, _))| tx.clone())
            .collect()
    }

    /// 获取指定标签的所有连接发送通道
    pub async fn get_senders_by_tag(&self, tag: &str) -> Vec<mpsc::Sender<WsMsg>> {
        let ids = self.ids_by_tag(tag).await;
        self.get_senders(&ids).await
    }

    /// 获取事件订阅
    pub fn subscribe(&self) -> broadcast::Receiver<ConnectionEvent> {
        self.event_tx.subscribe()
    }
}

/// 连接事件
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    /// 新连接
    Connected {
        id: ConnectionId,
        addr: SocketAddr,
    },
    /// 断开连接
    Disconnected {
        id: ConnectionId,
        addr: SocketAddr,
        client_id: Option<String>,
    },
    /// 认证成功
    Authenticated {
        id: ConnectionId,
        client_id: String,
    },
    /// 心跳
    Heartbeat {
        id: ConnectionId,
    },
}