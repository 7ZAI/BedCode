//! WS Session Registry
//!
//! 全局单例，维护所有 Actix WS actor 的地址映射
//! 提供 send_to_client / broadcast 等消息转发能力

use actix::dev::SendError;
use actix::Addr;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::RwLock;

use super::conn::{CloseConnection, SendBinaryMessage, SendTextMessage, WsConnBase};

/// WS 会话通道种类
///
/// 种类在注册时定死、不可变：路由创建 actor 时决定（`/ws/terminal/session/{id}`
/// → Terminal；`/ws/event` → Event；`/ws/plugin/{plugin_id}/{path}` → Plugin）。
/// 保持 `Copy` 的轻量枚举，属主与端点标识另挂条目字段（spec §3.2 A2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    /// 常驻事件通道：只接收同步/通知类消息（SyncData、对端公告），
    /// 「设备在线」的判定基准
    Event,
    /// 终端 I/O 通道：接收终端输出/输入/会话控制
    Terminal,
    /// 插件端点通道（阶段 B）：帧由插件通道处理器接管，不参与设备在线判定与事件广播
    Plugin,
}

/// 连接注册参数（注册表唯一入口）
pub struct WsRegistration {
    pub client_id: String,
    pub socket_addr: SocketAddr,
    pub actor_addr: Addr<WsConnBase>,
    /// 通道种类（广播过滤与在线判定的依据）
    pub channel_kind: ChannelKind,
    /// 属主插件 id（插件端点通道）；终端/事件通道为 `None`
    pub owner: Option<String>,
    /// 端点标识（插件端点通道）；终端/事件通道为 `None`
    pub endpoint_id: Option<String>,
}

/// WS 会话注册条目
struct WsSessionEntry {
    actor_addr: Addr<WsConnBase>,
    socket_addr: SocketAddr,
    /// JWT 主体（`claims.sub`；认证时设置，连接上下文的脱敏身份来源）
    subject: Option<String>,
    device_name: Option<String>,
    /// 设备指纹，认证时设置，用于与数据库 pairings 记录关联
    fingerprint: Option<String>,
    authenticated: bool,
    connected_at: i64,
    /// 通道种类（注册时定死，广播过滤与在线判定的依据）
    channel_kind: ChannelKind,
    /// 属主插件 id（插件端点通道；终端/事件通道为 `None`）
    ///
    /// 属主隔离的依据：按属主批量回收只命中本人条目，跨属主不可互操作
    owner: Option<String>,
    /// 端点标识（插件端点通道；终端/事件通道为 `None`）
    ///
    /// 端点域寻址（列表 / 单发 / 广播 / 批量断开）的依据
    endpoint_id: Option<String>,
    closing: bool,
}

/// WS 会话注册表 — 全局单例
///
/// 职责：
/// - 维护 client_id → Addr<WsConnBase> 映射
/// - 维护 SocketAddr → client_id 映射
/// - 提供 send / broadcast 等消息转发
pub struct WsSessionRegistry {
    /// client_id → 会话条目
    sessions: RwLock<HashMap<String, WsSessionEntry>>,
    /// SocketAddr → client_id（反向查找）
    addr_to_client_id: RwLock<HashMap<SocketAddr, String>>,
    pending_endpoint_reservations: RwLock<HashSet<(String, String)>>,
}

impl WsSessionRegistry {
    /// 获取全局单例
    pub fn global() -> &'static Self {
        static INSTANCE: std::sync::LazyLock<WsSessionRegistry> = std::sync::LazyLock::new(|| WsSessionRegistry {
            sessions: RwLock::new(HashMap::new()),
            addr_to_client_id: RwLock::new(HashMap::new()),
            pending_endpoint_reservations: RwLock::new(HashSet::new()),
        });
        &INSTANCE
    }

    pub fn reserve_endpoint_client(
        &self,
        endpoint_id: &str,
        client_id: &str,
        max_clients: usize,
    ) -> bool {
        let mut reservations = self
            .pending_endpoint_reservations
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        let active = sessions
            .values()
            .filter(|entry| entry.endpoint_id.as_deref() == Some(endpoint_id) && !entry.closing)
            .count();
        let pending = reservations
            .iter()
            .filter(|(endpoint, _)| endpoint == endpoint_id)
            .count();
        if active + pending >= max_clients {
            return false;
        }
        reservations.insert((endpoint_id.to_string(), client_id.to_string()));
        true
    }

    pub fn release_endpoint_reservation(&self, endpoint_id: &str, client_id: &str) {
        self.pending_endpoint_reservations
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&(endpoint_id.to_string(), client_id.to_string()));
    }

    /// 注册新的 WS 连接
    pub async fn register(&self, reg: WsRegistration) {
        if let Err(error) = self.register_now(reg) {
            tracing::error!(error = %error, "WS connection registration failed");
        }
    }

    pub fn register_now(&self, reg: WsRegistration) -> Result<(), String> {
        let WsRegistration {
            client_id,
            socket_addr,
            actor_addr,
            channel_kind,
            owner,
            endpoint_id,
        } = reg;
        let reservation_key = endpoint_id
            .as_ref()
            .map(|endpoint| (endpoint.clone(), client_id.clone()));
        let reserved = reservation_key
            .as_ref()
            .is_some_and(|key| {
                self.pending_endpoint_reservations
                    .write()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(key)
            });
        if reserved
            && endpoint_id
                .as_ref()
                .is_some_and(|endpoint| crate::server::websocket::endpoint::get(endpoint).is_none())
        {
            return Err(format!("WS endpoint not registered: {}", endpoint_id.as_deref().unwrap_or("")));
        }
        let connected_at = chrono::Utc::now().timestamp_millis();
        let mut sessions = self.sessions.write().unwrap_or_else(|e| e.into_inner());
        if sessions.contains_key(&client_id) {
            return Err(format!("WS client already registered: {client_id}"));
        }
        let mut addr_map = self.addr_to_client_id.write().unwrap_or_else(|e| e.into_inner());
        if addr_map.contains_key(&socket_addr) {
            return Err(format!("WS address already registered: {socket_addr}"));
        }
        sessions.insert(
            client_id.clone(),
            WsSessionEntry {
                actor_addr,
                socket_addr,
                subject: None,
                device_name: None,
                fingerprint: None,
                authenticated: false,
                connected_at,
                channel_kind,
                owner,
                endpoint_id,
                closing: false,
            },
        );
        addr_map.insert(socket_addr, client_id.clone());
        tracing::debug!(
            "[WsSessionRegistry] Registered client {} from {}",
            client_id,
            socket_addr
        );
        Ok(())
    }

    /// 注销 WS 连接
    pub async fn unregister(&self, client_id: &str) {
        if let Some(entry) = {
            let mut sessions = self.sessions.write().unwrap_or_else(|e| e.into_inner());
            sessions.remove(client_id)
        } {
            let mut addr_map = self.addr_to_client_id.write().unwrap_or_else(|e| e.into_inner());
            addr_map.remove(&entry.socket_addr);
            tracing::debug!(
                "[WsSessionRegistry] Unregistered client {} from {}",
                client_id,
                entry.socket_addr
            );
        }
    }

    /// 通过 SocketAddr 注销
    pub async fn unregister_by_addr(&self, addr: &SocketAddr) -> Option<String> {
        let client_id = {
            let _sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
            let addr_map = self.addr_to_client_id.read().unwrap_or_else(|e| e.into_inner());
            addr_map.get(addr).cloned()
        }?;

        let removed = {
            let mut sessions = self.sessions.write().unwrap_or_else(|e| e.into_inner());
            sessions.remove(&client_id)
        };
        if removed.is_some() {
            let mut addr_map = self.addr_to_client_id.write().unwrap_or_else(|e| e.into_inner());
            addr_map.remove(addr);
            tracing::debug!(client_id = %client_id, peer = %addr, "[WsSessionRegistry] Unregistered client");
        }
        Some(client_id)
    }

    pub fn set_authenticated_now(
        &self,
        client_id: &str,
        subject: Option<String>,
        device_name: Option<String>,
        fingerprint: Option<String>,
    ) -> bool {
        let mut sessions = self.sessions.write().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = sessions.get_mut(client_id) else {
            return false;
        };
        if entry.closing {
            return false;
        }
        entry.authenticated = true;
        entry.subject = subject;
        entry.device_name = device_name;
        entry.fingerprint = fingerprint;
        true
    }

    pub async fn set_authenticated(
        &self,
        client_id: &str,
        subject: Option<String>,
        device_name: Option<String>,
        fingerprint: Option<String>,
    ) {
        self.set_authenticated_now(client_id, subject, device_name, fingerprint);
    }

    /// 设置设备名称
    pub async fn set_device_name(&self, client_id: &str, device_name: Option<String>) {
        let mut sessions = self.sessions.write().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = sessions.get_mut(client_id) {
            entry.device_name = device_name;
        }
    }

    pub async fn send_to_client(&self, client_id: &str, text: String) -> Result<(), String> {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = sessions.get(client_id) else {
            return Err(format!("Client {} not found", client_id));
        };
        if entry.closing {
            return Err(format!("Client {} is closing", client_id));
        }
        match entry.actor_addr.try_send(SendTextMessage { text }) {
            Ok(()) => Ok(()),
            Err(SendError::Full(_)) => Err("ws send queue full".to_string()),
            Err(SendError::Closed(_)) => Err(format!("Client {} is closed", client_id)),
        }
    }

    pub async fn send_binary_to_client(&self, client_id: &str, data: Vec<u8>) -> Result<(), String> {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = sessions.get(client_id) else {
            return Err(format!("Client {} not found", client_id));
        };
        if entry.closing {
            return Err(format!("Client {} is closing", client_id));
        }
        match entry.actor_addr.try_send(SendBinaryMessage { data }) {
            Ok(()) => Ok(()),
            Err(SendError::Full(_)) => Err("ws send queue full".to_string()),
            Err(SendError::Closed(_)) => Err(format!("Client {} is closed", client_id)),
        }
    }

    /// 通过 SocketAddr 发送文本
    pub async fn send_to_addr(&self, addr: &SocketAddr, text: String) -> Result<(), String> {
        let client_id = {
            let addr_map = self.addr_to_client_id.read().unwrap_or_else(|e| e.into_inner());
            addr_map.get(addr).cloned()
        };

        match client_id {
            Some(cid) => self.send_to_client(&cid, text).await,
            None => Err(format!("No client at addr {}", addr)),
        }
    }

    /// 向所有已认证 Event 通道客户端广播文本
    ///
    /// 广播承载同步/通知语义（SyncData、对端公告），只投递到事件通道，
    /// 终端通道不接收（产品决策：常驻事件 WS = 在线语义，见 ChannelKind）
    ///
    /// exclude_device_name: 排除指定设备名称的客户端（用于同步事件排除操作者）
    pub async fn broadcast(&self, text: String, exclude_device_name: Option<&str>) {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        let targets = broadcast_targets(&sessions, exclude_device_name);
        let mut sent_count = 0usize;

        for client_id in targets {
            let Some(entry) = sessions.get(&client_id) else {
                continue;
            };

            if entry.closing {
                continue;
            }
            match entry.actor_addr.try_send(SendTextMessage { text: text.clone() }) {
                Ok(()) => sent_count += 1,
                Err(SendError::Full(_)) => {
                    tracing::warn!(client_id = %client_id, "WS broadcast queue full");
                }
                Err(SendError::Closed(_)) => {
                    tracing::debug!(client_id = %client_id, "WS broadcast target closed");
                }
            }
        }

        if sent_count > 0 {
            tracing::debug!("[WsSessionRegistry] Broadcast to {} clients", sent_count);
        }
    }

    // ==================== 插件端点域（按端点寻址 / 属主回收） ====================

    /// 按端点标识寻址：该端点在线的客户端摘要列表
    pub async fn list_by_endpoint(&self, endpoint_id: &str) -> Vec<ClientSummary> {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        sessions
            .iter()
            .filter(|(_, entry)| entry.endpoint_id.as_deref() == Some(endpoint_id))
            .map(|(client_id, entry)| ClientSummary {
                client_id: client_id.clone(),
                subject: entry.subject.clone(),
                device_name: entry.device_name.clone(),
                fingerprint: entry.fingerprint.clone(),
                addr: entry.socket_addr.to_string(),
                authenticated: entry.authenticated,
                connected_at: entry.connected_at,
            })
            .collect()
    }

    /// 该端点在线的客户端数量
    pub async fn endpoint_client_count(&self, endpoint_id: &str) -> usize {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        sessions
            .values()
            .filter(|entry| entry.endpoint_id.as_deref() == Some(endpoint_id))
            .count()
    }

    /// 该 client_id 是否登记在指定端点名下
    ///
    /// 插件单发前的端点域校验：客户端不在该端点名下 → 调用方返回错误，
    /// 避免「A 端点句柄 + B 端点客户端」的错配寻址
    pub async fn is_endpoint_client(&self, endpoint_id: &str, client_id: &str) -> bool {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        sessions
            .get(client_id)
            .is_some_and(|entry| entry.endpoint_id.as_deref() == Some(endpoint_id))
    }

    /// 向端点的指定客户端发文本；客户端不在该端点名下 → `Err`
    pub async fn send_to_endpoint_client(
        &self,
        endpoint_id: &str,
        client_id: &str,
        text: String,
    ) -> Result<(), String> {
        if !self.is_endpoint_client(endpoint_id, client_id).await {
            return Err(format!("client {client_id} not found in endpoint {endpoint_id}"));
        }
        self.send_to_client(client_id, text).await
    }

    /// 向端点全部客户端广播文本 → 成功入队客户端数
    ///
    /// 部分失败不回滚（fail-visible：失败明细记 debug，成功数供调用方判定）
    pub async fn broadcast_to_endpoint(&self, endpoint_id: &str, text: String) -> usize {
        let targets = self.list_by_endpoint(endpoint_id).await;
        let mut sent_count = 0usize;
        for summary in targets {
            match self.send_to_client(&summary.client_id, text.clone()).await {
                Ok(()) => sent_count += 1,
                Err(e) => {
                    tracing::debug!(
                        endpoint_id = %endpoint_id,
                        client_id = %summary.client_id,
                        error = %e,
                        "endpoint broadcast target unreachable"
                    );
                }
            }
        }
        sent_count
    }

    /// 向端点指定客户端发二进制帧；客户端不在该端点名下 → `Err`
    pub async fn send_binary_to_endpoint_client(
        &self,
        endpoint_id: &str,
        client_id: &str,
        data: Vec<u8>,
    ) -> Result<(), String> {
        if !self.is_endpoint_client(endpoint_id, client_id).await {
            return Err(format!("client {client_id} not found in endpoint {endpoint_id}"));
        }
        self.send_binary_to_client(client_id, data).await
    }

    /// 向端点全部客户端广播二进制帧 → 成功入队客户端数
    pub async fn broadcast_binary_to_endpoint(&self, endpoint_id: &str, data: Vec<u8>) -> usize {
        let targets = self.list_by_endpoint(endpoint_id).await;
        let mut sent_count = 0usize;
        for summary in targets {
            match self.send_binary_to_client(&summary.client_id, data.clone()).await {
                Ok(()) => sent_count += 1,
                Err(e) => {
                    tracing::debug!(
                        endpoint_id = %endpoint_id,
                        client_id = %summary.client_id,
                        error = %e,
                        "endpoint binary broadcast target unreachable"
                    );
                }
            }
        }
        sent_count
    }

    /// 按端点 + 客户端单点断开（踢出）
    pub async fn disconnect_endpoint_client(
        &self,
        endpoint_id: &str,
        client_id: &str,
        close_code: u16,
        reason: &str,
    ) -> bool {
        let closing = self
            .mark_closing(|cid, entry| cid == client_id && entry.endpoint_id.as_deref() == Some(endpoint_id))
            .await;
        let hit = !closing.is_empty();
        self.close_marked(closing, close_code, reason);
        if hit {
            tracing::info!(
                endpoint_id = %endpoint_id,
                client_id = %client_id,
                close_code,
                "[WsSessionRegistry] Endpoint client disconnected"
            );
        }
        hit
    }

    /// 服务器停机：向全部插件端点客户端下发 Close
    pub async fn disconnect_all_endpoint_clients(&self, close_code: u16, reason: &str) -> usize {
        let closing = self
            .mark_closing(|_, entry| entry.channel_kind == ChannelKind::Plugin)
            .await;
        let count = closing.len();
        self.close_marked(closing, close_code, reason);
        if count > 0 {
            tracing::info!(
                close_code,
                clients = count,
                "[WsSessionRegistry] Plugin endpoint clients disconnected (server shutdown)"
            );
        }
        count
    }

    /// 按端点批量断开
    pub async fn disconnect_by_endpoint(&self, endpoint_id: &str, close_code: u16, reason: &str) -> usize {
        let closing = self
            .mark_closing(|_, entry| entry.endpoint_id.as_deref() == Some(endpoint_id))
            .await;
        let count = closing.len();
        self.close_marked(closing, close_code, reason);
        if count > 0 {
            tracing::info!(
                endpoint_id = %endpoint_id,
                close_code,
                clients = count,
                "[WsSessionRegistry] Endpoint clients disconnected"
            );
        }
        count
    }

    /// 按属主批量回收
    pub async fn purge_for_plugin(&self, owner: &str, close_code: u16, reason: &str) -> Vec<String> {
        let closing = self
            .mark_closing(|_, entry| entry.owner.as_deref() == Some(owner))
            .await;
        let client_ids: Vec<String> = closing.iter().map(|(client_id, _, _)| client_id.clone()).collect();
        self.close_marked(closing, close_code, reason);
        if !client_ids.is_empty() {
            tracing::info!(
                plugin_id = %owner,
                close_code,
                connections = client_ids.len(),
                "[WsSessionRegistry] Plugin connections purged"
            );
        }
        client_ids
    }

    async fn mark_closing(
        &self,
        matches: impl Fn(&str, &WsSessionEntry) -> bool,
    ) -> Vec<(String, SocketAddr, Addr<WsConnBase>)> {
        let mut sessions = self.sessions.write().unwrap_or_else(|e| e.into_inner());
        let mut closing = Vec::new();
        for (client_id, entry) in sessions.iter_mut() {
            if !entry.closing && matches(client_id, entry) {
                entry.closing = true;
                closing.push((client_id.clone(), entry.socket_addr, entry.actor_addr.clone()));
            }
        }
        closing
    }

    fn close_marked(
        &self,
        closing: Vec<(String, SocketAddr, Addr<WsConnBase>)>,
        close_code: u16,
        reason: &str,
    ) {
        for (client_id, socket_addr, actor_addr) in closing {
            let close = CloseConnection {
                code: close_code,
                reason: reason.to_string(),
            };
            match actor_addr.try_send(close) {
                Ok(()) => {}
                Err(SendError::Full(_)) => {
                    tracing::warn!(client_id = %client_id, peer = %socket_addr, "WS close queue full");
                }
                Err(SendError::Closed(_)) => {
                    tracing::warn!(client_id = %client_id, peer = %socket_addr, "WS connection already closed");
                }
            }
        }
    }

    /// 获取所有客户端 ID
    pub async fn all_client_ids(&self) -> Vec<String> {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        sessions.keys().cloned().collect()
    }

    /// 获取已认证客户端数量
    pub async fn authenticated_count(&self) -> usize {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        sessions.values().filter(|e| e.authenticated).count()
    }

    /// 获取客户端总数
    pub async fn client_count(&self) -> usize {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        sessions.len()
    }

    /// 获取客户端摘要信息列表
    pub async fn list_clients(&self) -> Vec<ClientSummary> {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        sessions
            .iter()
            .map(|(client_id, entry)| ClientSummary {
                client_id: client_id.clone(),
                subject: entry.subject.clone(),
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
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        sessions.get(client_id).map(|entry| ClientSummary {
            client_id: client_id.to_string(),
            subject: entry.subject.clone(),
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
            let addr_map = self.addr_to_client_id.read().unwrap_or_else(|e| e.into_inner());
            addr_map.get(addr).cloned()
        }?;

        self.get_client(&client_id).await
    }

    /// 客户端是否已认证
    pub async fn is_authenticated(&self, client_id: &str) -> bool {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        sessions.get(client_id).map(|e| e.authenticated).unwrap_or(false)
    }

    /// 获取设备名称
    pub async fn get_device_name(&self, client_id: &str) -> Option<String> {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        sessions.get(client_id).and_then(|e| e.device_name.clone())
    }

    /// 通过 SocketAddr 获取 device_name
    /// 通过 SocketAddr 获取 device_name
    pub async fn get_device_name_by_addr(&self, addr: &SocketAddr) -> Option<String> {
        let client_id = {
            let addr_map = self.addr_to_client_id.read().unwrap_or_else(|e| e.into_inner());
            addr_map.get(addr).cloned()
        }?;

        self.get_device_name(&client_id).await
    }

    /// 清空所有注册信息（服务器停机时调用）
    pub async fn clear_all(&self) {
        self.pending_endpoint_reservations
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        let count = {
            let mut sessions = self.sessions.write().unwrap_or_else(|e| e.into_inner());
            let count = sessions.len();
            sessions.clear();
            count
        };
        {
            let mut addr_map = self.addr_to_client_id.write().unwrap_or_else(|e| e.into_inner());
            addr_map.clear();
        }
        if count > 0 {
            tracing::info!("[WsSessionRegistry] Cleared {} sessions", count);
        }
    }
}

/// 计算广播目标 client_id 列表（纯函数，供单测矩阵直接验证）
///
/// 过滤条件：
/// - 已认证且通道类型为 Event（广播是同步/通知语义，只达事件通道）
/// - exclude_device_name 命中的设备跳过（保留既有排除语义）
/// - 同 fingerprint 的多条 Event 连接只保留一条（防重复通知）；fingerprint
///   为 None 的匿名条目不去重、逐条保留（避免丢失匿名连接）
fn broadcast_targets(entries: &HashMap<String, WsSessionEntry>, exclude_device_name: Option<&str>) -> Vec<String> {
    let mut seen_fingerprints = std::collections::HashSet::new();
    let mut targets = Vec::new();

    for (client_id, entry) in entries {
        if entry.closing || !entry.authenticated || entry.channel_kind != ChannelKind::Event {
            continue;
        }

        // 排除指定设备
        if let Some(exclude) = exclude_device_name {
            if entry.device_name.as_deref() == Some(exclude) {
                continue;
            }
        }

        // 同设备（fingerprint）多条事件连接只发第一条
        if let Some(fp) = entry.fingerprint.as_deref() {
            if !seen_fingerprints.insert(fp) {
                continue;
            }
        }

        targets.push(client_id.clone());
    }

    targets
}

/// 客户端摘要（与 websocket_manager 中的定义对齐）
#[derive(Debug, Clone)]
pub struct ClientSummary {
    pub client_id: String,
    /// JWT 主体（`claims.sub`；认证前为 None）——连接上下文的脱敏身份来源
    pub subject: Option<String>,
    pub device_name: Option<String>,
    /// 设备指纹，用于与数据库 pairings 记录关联
    pub fingerprint: Option<String>,
    pub addr: String,
    pub authenticated: bool,
    pub connected_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一条注册条目：经伪 WS 握手取得真实 `Addr<WsConnBase>`（无需网络连接）。
    ///
    /// WebsocketContext 无法用 `Actor::start()` 启动（类型要求普通 Context），
    /// 走 `WsResponseBuilder::start_with_addr` 升级路径：伪造持有 Upgrade 头的
    /// 请求 + 空 Payload，握手成功即返回有效 Addr；actor future 不驱动，仅作
    /// 注册表条目占位（测试不向其发消息）。请求地址由 client_id 哈希派生，
    /// 避免同长度前缀的 client_id 撞同一端口
    fn entry(
        client_id: &str,
        channel_kind: ChannelKind,
        authenticated: bool,
        device_name: Option<&str>,
        fingerprint: Option<&str>,
    ) -> (String, WsSessionEntry) {
        entry_with(
            client_id,
            channel_kind,
            None,
            None,
            authenticated,
            device_name,
            fingerprint,
        )
    }

    /// 构造带属主 / 端点标识的注册条目（插件端点通道域测试用）
    fn entry_owned(client_id: &str, owner: &str, endpoint_id: &str, authenticated: bool) -> (String, WsSessionEntry) {
        entry_with(
            client_id,
            ChannelKind::Plugin,
            Some(owner),
            Some(endpoint_id),
            authenticated,
            None,
            None,
        )
    }

    /// 构造条目共用体：经伪 WS 握手取得真实 Addr，按通道种类装配骨架与处理器
    #[allow(clippy::too_many_arguments)]
    fn entry_with(
        client_id: &str,
        channel_kind: ChannelKind,
        owner: Option<&str>,
        endpoint_id: Option<&str>,
        authenticated: bool,
        device_name: Option<&str>,
        fingerprint: Option<&str>,
    ) -> (String, WsSessionEntry) {
        let port = 20000u16 + (client_id.bytes().fold(0usize, |acc, b| acc.wrapping_add(b as usize)) % 10000) as u16;
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();

        let req = actix_web::test::TestRequest::default()
            .insert_header(("Connection", "Upgrade"))
            .insert_header(("Upgrade", "websocket"))
            .insert_header(("Sec-WebSocket-Version", "13"))
            .insert_header(("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ=="))
            .to_http_request();
        // 空 payload 流：握手需要流参数，测试不驱动 actor future，空流即可
        let payload: actix_web::dev::Payload = actix_web::dev::Payload::None;
        // 按条目通道种类构造骨架 + 对应通道处理器（handler 不被驱动，仅作占位）
        let mut actor = match channel_kind {
            ChannelKind::Terminal => WsConnBase::new_for_session(addr, "test-session".to_string()),
            ChannelKind::Event => WsConnBase::new_event(addr),
            ChannelKind::Plugin => WsConnBase::new(
                crate::server::websocket::conn::ConnSpec {
                    owner: owner.map(|s| s.to_string()),
                    endpoint_id: endpoint_id.map(|s| s.to_string()),
                    ..crate::server::websocket::conn::ConnSpec::new(addr, ChannelKind::Plugin)
                },
                Box::new(StubChannel),
            ),
        };
        actor.disable_registry_for_test();
        let (actor_addr, _resp) = actix_web_actors::ws::WsResponseBuilder::new(actor, &req, payload)
            .start_with_addr()
            .expect("fake ws handshake must succeed");

        (
            client_id.to_string(),
            WsSessionEntry {
                actor_addr,
                socket_addr: addr,
                subject: None,
                device_name: device_name.map(|s| s.to_string()),
                fingerprint: fingerprint.map(|s| s.to_string()),
                authenticated,
                connected_at: 0,
                channel_kind,
                owner: owner.map(|s| s.to_string()),
                endpoint_id: endpoint_id.map(|s| s.to_string()),
                closing: false,
            },
        )
    }

    /// 测试用空通道处理器（不被驱动，仅满足骨架构造约束）
    struct StubChannel;

    impl crate::server::websocket::conn::ChannelHandler for StubChannel {
        fn auth_mode(&self) -> crate::server::websocket::conn::AuthMode {
            crate::server::websocket::conn::AuthMode::None
        }

        fn on_text(&mut self, _conn: &mut WsConnBase, _text: String, _ctx: &mut crate::server::websocket::conn::ConnCtx) {}

        fn on_binary(&mut self, _conn: &mut WsConnBase, _data: Vec<u8>, _ctx: &mut crate::server::websocket::conn::ConnCtx) {}
    }

    /// 把条目列表转成测试用 HashMap
    fn entries_map(entries: Vec<(String, WsSessionEntry)>) -> HashMap<String, WsSessionEntry> {
        entries.into_iter().collect()
    }

    /// 构造独立注册表实例
    ///
    /// 端点域 / 属主回收用例用本地实例，避免全局单例被并行用例相互清空
    fn local_registry() -> WsSessionRegistry {
        WsSessionRegistry {
            sessions: RwLock::new(HashMap::new()),
            addr_to_client_id: RwLock::new(HashMap::new()),
            pending_endpoint_reservations: RwLock::new(HashSet::new()),
        }
    }

    /// 批量写入条目（直插 sessions：伪造 Addr 不走 register 的真实地址路径）
    async fn seed(registry: &WsSessionRegistry, entries: Vec<(String, WsSessionEntry)>) {
        let mut sessions = registry.sessions.write().unwrap_or_else(|e| e.into_inner());
        for (client_id, entry) in entries {
            sessions.insert(client_id, entry);
        }
    }

    /// 取注册表当前全部 client_id（升序，便于断言）
    async fn client_ids(registry: &WsSessionRegistry) -> Vec<String> {
        let mut ids: Vec<String> = registry.sessions.read().unwrap_or_else(|e| e.into_inner()).keys().cloned().collect();
        ids.sort();
        ids
    }

    // ==================== broadcast_targets 矩阵 ====================
    //
    // 用 #[actix_rt::test]：入口 helper 持有 WsResponseBuilder 的响应流（内含
    // actor future），drop 时触发 started()（心跳 IntervalFunc 需要 Tokio reactor、
    // actix::spawn 需要 LocalSet），必须运行在 actix System 上下文

    #[actix_rt::test]
    async fn broadcast_targets_only_event_authenticated() {
        let map = entries_map(vec![
            entry("ev-auth", ChannelKind::Event, true, Some("Phone"), Some("fp-1")),
            entry("term-auth", ChannelKind::Terminal, true, Some("Phone"), Some("fp-1")),
            entry("ev-anon", ChannelKind::Event, false, None, None),
        ]);
        let targets = broadcast_targets(&map, None);
        assert_eq!(targets, vec!["ev-auth"], "仅已认证 Event 通道是广播目标");
    }

    #[actix_rt::test]
    async fn broadcast_targets_dedup_by_fingerprint() {
        // 同一设备两条 Event 通道 → 只发一条（去重）；匿名（fingerprint=None）不去重
        let map = entries_map(vec![
            entry("ev-1", ChannelKind::Event, true, Some("Phone"), Some("fp-same")),
            entry("ev-2", ChannelKind::Event, true, Some("Phone"), Some("fp-same")),
            entry("ev-3", ChannelKind::Event, true, Some("Phone 2"), None),
            entry("ev-4", ChannelKind::Event, true, Some("Phone 3"), None),
        ]);
        let targets = broadcast_targets(&map, None);
        assert_eq!(targets.len(), 3, "同指纹去重到 1 条，匿名 2 条全保留");
        assert!(
            targets.iter().any(|t| t == "ev-1") ^ targets.iter().any(|t| t == "ev-2"),
            "两条同指纹事件连接只保留其中一条"
        );
    }

    #[actix_rt::test]
    async fn broadcast_targets_excludes_device_name() {
        let map = entries_map(vec![
            entry("ev-1", ChannelKind::Event, true, Some("operator"), Some("fp-op")),
            entry("ev-2", ChannelKind::Event, true, Some("peer"), Some("fp-peer")),
        ]);
        let targets = broadcast_targets(&map, Some("operator"));
        assert_eq!(targets, vec!["ev-2"], "exclude_device_name 命中设备必须被排除");
    }

    // ==================== 端点域寻址 / 属主回收 / 按端点断开 ====================
    //
    // 一律用 `local_registry()`：这些用例只验证注册表自身语义，写全局单例会在
    // 并行执行时干扰同居用例

    #[actix_rt::test]
    async fn broadcast_targets_exclude_plugin_channel() {
        // 插件端点通道即便已认证且指纹相同，也不得被卷入同步广播
        let map = entries_map(vec![
            entry_owned("plug-1", "plugin-a", "wse-a", true),
            entry("ev-1", ChannelKind::Event, true, Some("Phone"), Some("fp-1")),
        ]);
        let targets = broadcast_targets(&map, None);
        assert_eq!(targets, vec!["ev-1"], "插件端点通道不得卷入同步广播");
    }

    #[actix_rt::test]
    async fn purge_for_plugin_only_hits_owner() {
        let registry = local_registry();

        seed(
            &registry,
            vec![
                entry_owned("p-a1", "plugin-a", "wse-a", true),
                entry_owned("p-a2", "plugin-a", "wse-a", true),
                entry_owned("p-b1", "plugin-b", "wse-b", true),
                entry("ev-host", ChannelKind::Event, true, Some("Phone"), Some("fp-host")),
                entry("term-host", ChannelKind::Terminal, true, Some("Phone"), Some("fp-host")),
            ],
        )
        .await;

        let mut purged = registry.purge_for_plugin("plugin-a", 4005, "plugin deactivated").await;
        purged.sort();
        assert_eq!(purged, vec!["p-a1", "p-a2"], "只回收本人属主条目");
        assert_eq!(
            client_ids(&registry).await,
            vec!["ev-host", "p-a1", "p-a2", "p-b1", "term-host"],
            "关闭标记期间条目仍由 actor 持有，最终摘除由 stopping 完成"
        );
        for client_id in ["p-a1", "p-a2"] {
            registry.unregister(client_id).await;
        }

        // 未知属主：命中为空、幂等不 panic
        assert!(
            registry.purge_for_plugin("plugin-none", 4005, "gone").await.is_empty(),
            "未知属主无命中"
        );
        assert_eq!(client_ids(&registry).await.len(), 3, "未知属主回收不得误伤条目");
    }

    #[actix_rt::test]
    async fn disconnect_by_endpoint_hits_only_that_endpoint() {
        let registry = local_registry();

        seed(
            &registry,
            vec![
                entry_owned("p-a1", "plugin-a", "wse-a", true),
                entry_owned("p-a2", "plugin-a", "wse-a", true),
                entry_owned("p-b1", "plugin-b", "wse-b", true),
            ],
        )
        .await;

        assert_eq!(registry.endpoint_client_count("wse-a").await, 2);
        assert_eq!(registry.list_by_endpoint("wse-a").await.len(), 2);
        assert!(registry.is_endpoint_client("wse-a", "p-a1").await);
        assert!(
            !registry.is_endpoint_client("wse-a", "p-b1").await,
            "端点域寻址不得跨端点命中"
        );

        let closed = registry.disconnect_by_endpoint("wse-a", 4004, "kicked by plugin").await;
        assert_eq!(closed, 2, "按端点断开命中该端点全部客户端");
        assert_eq!(registry.list_by_endpoint("wse-a").await.len(), 2, "关闭事件前条目仍在");
        assert_eq!(registry.list_by_endpoint("wse-b").await.len(), 1, "其他端点不受影响");
        for client_id in ["p-a1", "p-a2"] {
            registry.unregister(client_id).await;
        }

        assert_eq!(registry.disconnect_by_endpoint("wse-a", 4004, "kicked").await, 0);
    }

    #[test]
    fn endpoint_reservation_enforces_limit_before_upgrade() {
        let registry = local_registry();
        assert!(registry.reserve_endpoint_client("wse-a", "client-a", 1));
        assert!(!registry.reserve_endpoint_client("wse-a", "client-b", 1));
        registry.release_endpoint_reservation("wse-a", "client-a");
        assert!(registry.reserve_endpoint_client("wse-a", "client-b", 1));
    }

    #[actix_rt::test]
    async fn endpoint_queries_unknown_id_are_idempotent() {
        let registry = local_registry();

        // 未登记条目的端点 / 属主标识：一律返回空命中，不 panic
        assert!(registry.list_by_endpoint("wse-none").await.is_empty());
        assert_eq!(registry.endpoint_client_count("wse-none").await, 0);
        assert!(!registry.is_endpoint_client("wse-none", "c-none").await);
        assert_eq!(registry.broadcast_to_endpoint("wse-none", "ping".to_string()).await, 0);
        assert_eq!(registry.disconnect_by_endpoint("wse-none", 4005, "gone").await, 0);
        assert!(registry.purge_for_plugin("plugin-none", 4005, "gone").await.is_empty());
    }

    #[actix_rt::test]
    async fn endpoint_targeted_send_and_broadcast() {
        let registry = local_registry();

        seed(
            &registry,
            vec![
                entry_owned("p-a1", "plugin-a", "wse-a", true),
                entry_owned("p-a2", "plugin-a", "wse-a", true),
            ],
        )
        .await;

        // 错配寻址：端点句柄与客户端不属于同一端点 → Err（不消费句柄）
        let mismatch = registry
            .send_to_endpoint_client("wse-b", "p-a1", "hello".to_string())
            .await;
        assert!(mismatch.is_err(), "跨端点寻址必须被拒");
        assert!(
            registry.is_endpoint_client("wse-a", "p-a1").await,
            "拒绝不得消费/摘除句柄"
        );

        // 单发命中：客户端已摘除时返回 Err（fail-visible，不静默丢弃）
        let missing = registry
            .send_to_endpoint_client("wse-b", "p-none", "hello".to_string())
            .await;
        assert!(missing.is_err(), "未知客户端必须返回错误");

        // 广播成功入队数不超过端点在线客户端数（actor 未被驱动时投递失败 → 计数 0，
        // 故只断言上界与不 panic）
        let sent = registry.broadcast_to_endpoint("wse-a", "ping".to_string()).await;
        assert!(sent <= 2, "广播成功数不得超过在线客户端数，实际 {sent}");
    }

    // ==================== 停机下线（票据 05 四条断开路径之一：服务器停机 1001） ====================

    #[actix_rt::test]
    async fn shutdown_closes_only_plugin_endpoint_clients() {
        let registry = local_registry();
        seed(
            &registry,
            vec![
                entry_owned("p-a1", "plugin-a", "wse-a", true),
                entry_owned("p-a2", "plugin-b", "wse-b", true),
                entry("ev-host", ChannelKind::Event, true, Some("Phone"), Some("fp-host")),
                entry("term-host", ChannelKind::Terminal, true, Some("Phone"), Some("fp-host")),
            ],
        )
        .await;

        // 停机只下线插件端点客户端（宿主通道由 Actix 优雅停机收敛）
        assert_eq!(
            registry
                .disconnect_all_endpoint_clients(1001, "server shutting down")
                .await,
            2
        );
        assert_eq!(
            client_ids(&registry).await,
            vec!["ev-host", "p-a1", "p-a2", "term-host"],
            "插件端点先标记关闭，最终摘除由 actor stopping 完成"
        );
        for client_id in ["p-a1", "p-a2"] {
            registry.unregister(client_id).await;
        }
        assert_eq!(
            registry
                .disconnect_all_endpoint_clients(1001, "server shutting down")
                .await,
            0
        );
    }
}
