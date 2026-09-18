//! WS Session Registry
//!
//! 全局单例，维护所有 Actix WS actor 的地址映射
//! 提供 send_to_client / broadcast 等消息转发能力

use actix::Addr;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::sync::RwLock;

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
}

impl WsSessionRegistry {
    /// 获取全局单例
    pub fn global() -> &'static Self {
        static INSTANCE: std::sync::LazyLock<WsSessionRegistry> = std::sync::LazyLock::new(|| WsSessionRegistry {
            sessions: RwLock::new(HashMap::new()),
            addr_to_client_id: RwLock::new(HashMap::new()),
        });
        &INSTANCE
    }

    /// 注册新的 WS 连接
    ///
    /// `channel_kind` / `owner` / `endpoint_id` 由创建路由决定：
    /// 终端路由 → Terminal；事件通道（/ws/event）→ Event；
    /// 插件端点（阶段 B）→ Plugin + 属主 + 端点标识
    pub async fn register(&self, reg: WsRegistration) {
        let WsRegistration {
            client_id,
            socket_addr,
            actor_addr,
            channel_kind,
            owner,
            endpoint_id,
        } = reg;
        let connected_at = chrono::Utc::now().timestamp_millis();

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(
                client_id.clone(),
                WsSessionEntry {
                    actor_addr,
                    socket_addr,
                    device_name: None,
                    fingerprint: None,
                    authenticated: false,
                    connected_at,
                    channel_kind,
                    owner,
                    endpoint_id,
                },
            );
        }
        {
            let mut addr_map = self.addr_to_client_id.write().await;
            addr_map.insert(socket_addr, client_id.clone());
        }

        tracing::debug!(
            "[WsSessionRegistry] Registered client {} from {}",
            client_id,
            socket_addr
        );
    }

    /// 注销 WS 连接
    pub async fn unregister(&self, client_id: &str) {
        if let Some(entry) = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(client_id)
        } {
            let mut addr_map = self.addr_to_client_id.write().await;
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
            let mut addr_map = self.addr_to_client_id.write().await;
            addr_map.remove(addr)
        };

        if let Some(ref cid) = client_id {
            let mut sessions = self.sessions.write().await;
            sessions.remove(cid);
            tracing::debug!(client_id = %cid, peer = %addr, "[WsSessionRegistry] Unregistered client");
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
            entry
                .actor_addr
                .send(SendTextMessage { text })
                .await
                .map_err(|e| format!("Failed to send to client {}: {}", client_id, e))
        } else {
            Err(format!("Client {} not found", client_id))
        }
    }

    /// 向指定 client_id 发送二进制帧
    pub async fn send_binary_to_client(&self, client_id: &str, data: Vec<u8>) -> Result<(), String> {
        let sessions = self.sessions.read().await;
        if let Some(entry) = sessions.get(client_id) {
            entry
                .actor_addr
                .send(SendBinaryMessage { data })
                .await
                .map_err(|e| format!("Failed to send binary to client {}: {}", client_id, e))
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

    /// 向所有已认证 Event 通道客户端广播文本
    ///
    /// 广播承载同步/通知语义（SyncData、对端公告），只投递到事件通道，
    /// 终端通道不接收（产品决策：常驻事件 WS = 在线语义，见 ChannelKind）
    ///
    /// exclude_device_name: 排除指定设备名称的客户端（用于同步事件排除操作者）
    pub async fn broadcast(&self, text: String, exclude_device_name: Option<&str>) {
        let sessions = self.sessions.read().await;
        let targets = broadcast_targets(&sessions, exclude_device_name);
        let mut sent_count = 0usize;

        for client_id in targets {
            let Some(entry) = sessions.get(&client_id) else {
                continue;
            };

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

    // ==================== 插件端点域（按端点寻址 / 属主回收） ====================

    /// 按端点标识寻址：该端点在线的客户端摘要列表
    pub async fn list_by_endpoint(&self, endpoint_id: &str) -> Vec<ClientSummary> {
        let sessions = self.sessions.read().await;
        sessions
            .iter()
            .filter(|(_, entry)| entry.endpoint_id.as_deref() == Some(endpoint_id))
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

    /// 该端点在线的客户端数量
    pub async fn endpoint_client_count(&self, endpoint_id: &str) -> usize {
        let sessions = self.sessions.read().await;
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
        let sessions = self.sessions.read().await;
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

    /// 按端点 + 客户端单点断开（踢出）：向该客户端下发 Close 并摘除条目
    ///
    /// 返回是否命中（端点域寻址错配 / 未知客户端 → `false`，幂等不 panic）。
    /// 摘除条目后该连接的 `stopping` 走既有注销路径（重复注销为 no-op），
    /// 断开事件由通道层上报
    pub async fn disconnect_endpoint_client(
        &self,
        endpoint_id: &str,
        client_id: &str,
        close_code: u16,
        reason: &str,
    ) -> bool {
        let removed = self
            .take_matching(|cid, entry| cid == client_id && entry.endpoint_id.as_deref() == Some(endpoint_id))
            .await;
        let hit = !removed.is_empty();
        self.close_removed(removed, close_code, reason);
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

    /// 服务器停机：向全部插件端点客户端下发 Close（spec §4.5：停机 1001）
    ///
    /// 返回命中的客户端数。宿主（终端 / 事件通道）连接不在此列——停机对它们的
    /// 收敛由 Actix 优雅停机承担，本方法只补齐插件端点通道的关闭码语义
    pub async fn disconnect_all_endpoint_clients(&self, close_code: u16, reason: &str) -> usize {
        let removed = self
            .take_matching(|_, entry| entry.channel_kind == ChannelKind::Plugin)
            .await;
        let count = removed.len();
        self.close_removed(removed, close_code, reason);
        if count > 0 {
            tracing::info!(
                close_code,
                clients = count,
                "[WsSessionRegistry] Plugin endpoint clients disconnected (server shutdown)"
            );
        }
        count
    }

    /// 按端点批量断开：向该端点全部在线客户端下发 Close 并摘除条目
    ///
    /// 返回命中的客户端数（无命中 = 0，幂等不 panic）。摘除条目后各连接的
    /// `stopping` 走既有注销路径（重复注销为 no-op），断连事件由通道层上报
    pub async fn disconnect_by_endpoint(&self, endpoint_id: &str, close_code: u16, reason: &str) -> usize {
        let removed = self
            .take_matching(|_, entry| entry.endpoint_id.as_deref() == Some(endpoint_id))
            .await;
        let count = removed.len();
        self.close_removed(removed, close_code, reason);
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

    /// 按属主批量回收：关闭并摘除该属主的全部连接条目（只碰本人）
    ///
    /// 返回被回收的 client_id 列表。属主隔离依据 `owner` 字段：终端 / 事件通道
    /// （`owner = None`）与他人条目一律不受影响
    pub async fn purge_for_plugin(&self, owner: &str, close_code: u16, reason: &str) -> Vec<String> {
        let removed = self.take_matching(|_, entry| entry.owner.as_deref() == Some(owner)).await;
        let client_ids: Vec<String> = removed.iter().map(|(client_id, _, _)| client_id.clone()).collect();
        self.close_removed(removed, close_code, reason);
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

    /// 摘除满足条件的条目（含反向地址映射），返回 `(client_id, socket_addr, actor_addr)`
    ///
    /// 断言函数同时收到 `client_id`（会话键）与条目：单点踢出按会话键精确定位，
    /// 端点域 / 属主域回收只看条目字段
    ///
    /// 锁序与 `register` / `unregister` 一致（sessions → addr_to_client_id）
    async fn take_matching(
        &self,
        matches: impl Fn(&str, &WsSessionEntry) -> bool,
    ) -> Vec<(String, SocketAddr, Addr<WsConnBase>)> {
        let mut removed = Vec::new();
        let mut sessions = self.sessions.write().await;
        let targets: Vec<String> = sessions
            .iter()
            .filter(|(client_id, entry)| matches(client_id, entry))
            .map(|(client_id, _)| client_id.clone())
            .collect();
        let mut addr_map = self.addr_to_client_id.write().await;
        for client_id in targets {
            if let Some(entry) = sessions.remove(&client_id) {
                addr_map.remove(&entry.socket_addr);
                removed.push((client_id, entry.socket_addr, entry.actor_addr));
            }
        }
        removed
    }

    /// 向被摘除的连接下发 Close
    ///
    /// 用 `do_send` 而非 `send().await`：条目已摘除，关闭命令无需送达确认，
    /// 且避免在 actor 未被驱动的场景（测试 / 竞态窗口）阻塞调用方；
    /// actor 已退出属正常竞态，仅记 debug
    fn close_removed(&self, removed: Vec<(String, SocketAddr, Addr<WsConnBase>)>, close_code: u16, reason: &str) {
        for (client_id, socket_addr, actor_addr) in removed {
            let close = CloseConnection {
                code: close_code,
                reason: reason.to_string(),
            };
            if let Err(e) = actor_addr.try_send(close) {
                tracing::debug!(
                    client_id = %client_id,
                    peer = %socket_addr,
                    error = %e,
                    "close command not delivered (connection already gone)"
                );
            }
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

    /// 设备是否在线（存在已认证的事件通道连接）
    ///
    /// 「常驻事件 WS = 在线」语义（ticket 02）：状态查询只认事件通道，
    /// 终端通道的短暂连接不视为设备在线
    pub async fn is_device_online(&self, fingerprint: &str) -> bool {
        let sessions = self.sessions.read().await;
        sessions.values().any(|e| {
            e.authenticated && e.channel_kind == ChannelKind::Event && e.fingerprint.as_deref() == Some(fingerprint)
        })
    }

    /// 设备的在线事件连接数（已认证 Event 通道数量）
    ///
    /// ticket 02 用于判定「最后一条事件 WS 断开」：计数从 1 归零才触发
    /// DEVICE_DISCONNECTED（多事件连接仅最后一条断开时发下线）
    pub async fn event_connection_count(&self, fingerprint: &str) -> usize {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|e| {
                e.authenticated && e.channel_kind == ChannelKind::Event && e.fingerprint.as_deref() == Some(fingerprint)
            })
            .count()
    }

    /// 设备的在线终端连接数（已认证 Terminal 通道数量）
    ///
    /// 旧 v2.0.0 客户端没有事件通道，仅靠 /ws/terminal 单通道维持会话：
    /// 其离线判定回退为「既无事件连接、终端连接也归零」才触发（决策 R1，
    /// 见 stopping() 注释），本方法供该判定取终端计数
    pub async fn terminal_connection_count(&self, fingerprint: &str) -> usize {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|e| {
                e.authenticated
                    && e.channel_kind == ChannelKind::Terminal
                    && e.fingerprint.as_deref() == Some(fingerprint)
            })
            .count()
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
        if !entry.authenticated || entry.channel_kind != ChannelKind::Event {
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
        let actor = match channel_kind {
            ChannelKind::Terminal => WsConnBase::new_for_session(addr, "test-session".to_string()),
            ChannelKind::Event => WsConnBase::new_event(addr),
            ChannelKind::Plugin => WsConnBase::new(
                crate::server::ws::conn::ConnSpec {
                    owner: owner.map(|s| s.to_string()),
                    endpoint_id: endpoint_id.map(|s| s.to_string()),
                    ..crate::server::ws::conn::ConnSpec::new(addr, ChannelKind::Plugin)
                },
                Box::new(StubChannel),
            ),
        };
        let (actor_addr, _resp) = actix_web_actors::ws::WsResponseBuilder::new(actor, &req, payload)
            .start_with_addr()
            .expect("fake ws handshake must succeed");

        (
            client_id.to_string(),
            WsSessionEntry {
                actor_addr,
                socket_addr: addr,
                device_name: device_name.map(|s| s.to_string()),
                fingerprint: fingerprint.map(|s| s.to_string()),
                authenticated,
                connected_at: 0,
                channel_kind,
                owner: owner.map(|s| s.to_string()),
                endpoint_id: endpoint_id.map(|s| s.to_string()),
            },
        )
    }

    /// 测试用空通道处理器（不被驱动，仅满足骨架构造约束）
    struct StubChannel;

    impl crate::server::ws::conn::ChannelHandler for StubChannel {
        fn auth_mode(&self) -> crate::server::ws::conn::AuthMode {
            crate::server::ws::conn::AuthMode::None
        }

        fn on_text(&mut self, _conn: &mut WsConnBase, _text: String, _ctx: &mut crate::server::ws::conn::ConnCtx) {}

        fn on_binary(&mut self, _conn: &mut WsConnBase, _data: Vec<u8>, _ctx: &mut crate::server::ws::conn::ConnCtx) {}
    }

    /// 把条目列表转成测试用 HashMap
    fn entries_map(entries: Vec<(String, WsSessionEntry)>) -> HashMap<String, WsSessionEntry> {
        entries.into_iter().collect()
    }

    /// 构造独立注册表实例
    ///
    /// 端点域 / 属主回收用例必须与使用全局单例的 `device_online_queries` 隔离：
    /// `#[actix_rt::test]` 并行执行，共享全局单例会互相清空刚插入的条目
    fn local_registry() -> WsSessionRegistry {
        WsSessionRegistry {
            sessions: RwLock::new(HashMap::new()),
            addr_to_client_id: RwLock::new(HashMap::new()),
        }
    }

    /// 批量写入条目（直插 sessions：伪造 Addr 不走 register 的真实地址路径）
    async fn seed(registry: &WsSessionRegistry, entries: Vec<(String, WsSessionEntry)>) {
        let mut sessions = registry.sessions.write().await;
        for (client_id, entry) in entries {
            sessions.insert(client_id, entry);
        }
    }

    /// 取注册表当前全部 client_id（升序，便于断言）
    async fn client_ids(registry: &WsSessionRegistry) -> Vec<String> {
        let mut ids: Vec<String> = registry.sessions.read().await.keys().cloned().collect();
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

    // ==================== is_device_online / event_connection_count ====================
    //
    // **用本地实例**（`local_registry()`）：这三个场景只验证计数/在线判定语义，
    // 与「哪个实例」无关；写全局单例 + `clear_all()` 会清掉同进程并行用例刚登记的
    // 连接（实证：插件端点 e2e 的入站客户端被清空 → 回显拿不到客户端而假失败）

    #[actix_rt::test]
    async fn device_online_queries() {
        let registry = local_registry();

        // 场景 1：Event 已认证（在线）/ Terminal 已认证（不算在线）/ Event 未认证（不算）
        for (cid, e) in [
            entry("ev-online", ChannelKind::Event, true, Some("Phone"), Some("fp-dev-a")),
            entry(
                "term-only",
                ChannelKind::Terminal,
                true,
                Some("Phone"),
                Some("fp-dev-a"),
            ),
            entry("ev-unauthed", ChannelKind::Event, false, None, Some("fp-dev-a")),
        ] {
            registry.sessions.write().await.insert(cid, e);
        }

        assert!(registry.is_device_online("fp-dev-a").await, "Event 已认证连接 → 在线");
        assert_eq!(registry.event_connection_count("fp-dev-a").await, 1);
        // Terminal 通道 + 未认证 Event 都不贡献在线计数
        assert!(!registry.is_device_online("fp-unknown").await, "未注册指纹 → 离线");
        assert_eq!(registry.event_connection_count("fp-unknown").await, 0);

        registry.clear_all().await;

        // 场景 2：多事件连接计数（供 ticket 02：断开一条后 count=1 仍在线，
        // 归零才触发 DISCONNECTED）
        for (cid, e) in [
            entry("ev-a1", ChannelKind::Event, true, Some("Phone"), Some("fp-multi")),
            entry("ev-a2", ChannelKind::Event, true, Some("Phone"), Some("fp-multi")),
            entry("term-a", ChannelKind::Terminal, true, Some("Phone"), Some("fp-multi")),
        ] {
            registry.sessions.write().await.insert(cid, e);
        }

        assert_eq!(registry.event_connection_count("fp-multi").await, 2);
        assert!(registry.is_device_online("fp-multi").await);
        // 终端计数：1 条已认证 Terminal 连接），场景 3 的 R1 回退判定依赖
        assert_eq!(registry.terminal_connection_count("fp-multi").await, 1);

        registry.clear_all().await;

        // 场景 3：纯终端设备（旧 v2.0.0 客户端形态，无 Event 通道）——
        // 终端计数存在但事件计数为 0，is_device_online 仍为 false（在线判定
        // 只认事件通道）；stopping() 的 Terminal 回退分支用两个计数联合判定
        for (cid, e) in [entry(
            "term-legacy",
            ChannelKind::Terminal,
            true,
            Some("Phone"),
            Some("fp-legacy"),
        )] {
            registry.sessions.write().await.insert(cid, e);
        }

        assert_eq!(registry.terminal_connection_count("fp-legacy").await, 1);
        assert_eq!(registry.event_connection_count("fp-legacy").await, 0);
        assert!(!registry.is_device_online("fp-legacy").await, "纯终端通道不算在线");

        registry.clear_all().await;
    }

    // ==================== 端点域寻址 / 属主回收 / 按端点断开 ====================
    //
    // 一律用 `local_registry()`：这些用例只验证注册表自身语义，写全局单例会在
    // 并行执行时干扰同居用例（见上方 device_online_queries 的说明）

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
            vec!["ev-host", "p-b1", "term-host"],
            "他人插件条目与宿主（owner=None）条目不受影响"
        );

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
        assert!(registry.list_by_endpoint("wse-a").await.is_empty(), "条目已摘除");
        assert_eq!(registry.list_by_endpoint("wse-b").await.len(), 1, "其他端点不受影响");

        // 幂等：同端点再断开命中 0（标识已失效），不 panic
        assert_eq!(registry.disconnect_by_endpoint("wse-a", 4004, "kicked").await, 0);
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
            vec!["ev-host", "term-host"],
            "终端 / 事件通道条目不得被停机下线触碰"
        );
        // 幂等：再次调用命中 0（条目已摘除）
        assert_eq!(
            registry.disconnect_all_endpoint_clients(1001, "server shutting down").await,
            0
        );
    }
}
