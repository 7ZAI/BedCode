//! WS Session Registry
//!
//! 全局单例，维护所有 Actix WS actor 的地址映射
//! 提供 send_to_client / broadcast 等消息转发能力

use actix::Addr;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::sync::RwLock;

use super::terminal_ws::{SendTextMessage, TerminalWs};

/// WS 会话通道类型
///
/// 通道在注册时定死、不可变：路由创建 actor 时决定（现阶段仅 /ws/terminal
/// 与本地环回 → Terminal；/ws/event 事件通道由 ticket 02 引入）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    /// 常驻事件通道：只接收同步/通知类消息（SyncData、对端公告），
    /// 「设备在线」的判定基准
    Event,
    /// 终端 I/O 通道：接收终端输出/输入/会话控制
    Terminal,
}

/// WS 会话注册条目
struct WsSessionEntry {
    actor_addr: Addr<TerminalWs>,
    socket_addr: SocketAddr,
    device_name: Option<String>,
    /// 设备指纹，认证时设置，用于与数据库 pairings 记录关联
    fingerprint: Option<String>,
    authenticated: bool,
    connected_at: i64,
    /// 通道类型（注册时定死，广播过滤与在线判定的依据）
    channel_type: ChannelType,
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
    ///
    /// `channel_type` 由创建路由决定：/ws/terminal 与本地环回 → Terminal；
    /// 事件通道（ticket 02 的 /ws/event）→ Event
    pub async fn register(
        &self,
        client_id: String,
        socket_addr: SocketAddr,
        actor_addr: Addr<TerminalWs>,
        channel_type: ChannelType,
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
                channel_type,
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

    /// 向所有已认证 Event 通道客户端广播文本
    ///
    /// 广播承载同步/通知语义（SyncData、对端公告），只投递到事件通道，
    /// 终端通道不接收（产品决策：常驻事件 WS = 在线语义，见 ChannelType）
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
            e.authenticated
                && e.channel_type == ChannelType::Event
                && e.fingerprint.as_deref() == Some(fingerprint)
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
                e.authenticated
                    && e.channel_type == ChannelType::Event
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
fn broadcast_targets(
    entries: &HashMap<String, WsSessionEntry>,
    exclude_device_name: Option<&str>,
) -> Vec<String> {
    let mut seen_fingerprints = std::collections::HashSet::new();
    let mut targets = Vec::new();

    for (client_id, entry) in entries {
        if !entry.authenticated || entry.channel_type != ChannelType::Event {
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

    /// 构造一条注册条目：经伪 WS 握手取得真实 `Addr<TerminalWs>`（无需网络连接）。
    ///
    /// WebsocketContext 无法用 `Actor::start()` 启动（类型要求普通 Context），
    /// 走 `WsResponseBuilder::start_with_addr` 升级路径：伪造持有 Upgrade 头的
    /// 请求 + 空 Payload，握手成功即返回有效 Addr；actor future 不驱动，仅作
    /// 注册表条目占位（测试不向其发消息）。请求地址由 client_id 哈希派生，
    /// 避免同长度前缀的 client_id 撞同一端口
    fn entry(
        client_id: &str,
        channel_type: ChannelType,
        authenticated: bool,
        device_name: Option<&str>,
        fingerprint: Option<&str>,
    ) -> (String, WsSessionEntry) {
        let port = 20000u16
            + (client_id.bytes().fold(0usize, |acc, b| acc.wrapping_add(b as usize)) % 10000)
                as u16;
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();

        let req = actix_web::test::TestRequest::default()
            .insert_header(("Connection", "Upgrade"))
            .insert_header(("Upgrade", "websocket"))
            .insert_header(("Sec-WebSocket-Version", "13"))
            .insert_header(("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ=="))
            .to_http_request();
        // 空 payload 流：握手需要流参数，测试不驱动 actor future，空流即可
        let payload: actix_web::dev::Payload = actix_web::dev::Payload::None;
        let (actor_addr, _resp) = actix_web_actors::ws::WsResponseBuilder::new(
            TerminalWs::new(addr),
            &req,
            payload,
        )
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
                channel_type,
            },
        )
    }

    /// 把条目列表转成测试用 HashMap
    fn entries_map(entries: Vec<(String, WsSessionEntry)>) -> HashMap<String, WsSessionEntry> {
        entries.into_iter().collect()
    }

    // ==================== broadcast_targets 矩阵 ====================
    //
    // 用 #[actix_rt::test]：入口 helper 持有 WsResponseBuilder 的响应流（内含
    // actor future），drop 时触发 started()（心跳 IntervalFunc 需要 Tokio reactor、
    // actix::spawn 需要 LocalSet），必须运行在 actix System 上下文

    #[actix_rt::test]
    async fn broadcast_targets_only_event_authenticated() {
        let map = entries_map(vec![
            entry("ev-auth", ChannelType::Event, true, Some("Phone"), Some("fp-1")),
            entry("term-auth", ChannelType::Terminal, true, Some("Phone"), Some("fp-1")),
            entry("ev-anon", ChannelType::Event, false, None, None),
        ]);
        let targets = broadcast_targets(&map, None);
        assert_eq!(targets, vec!["ev-auth"], "仅已认证 Event 通道是广播目标");
    }

    #[actix_rt::test]
    async fn broadcast_targets_dedup_by_fingerprint() {
        // 同一设备两条 Event 通道 → 只发一条（去重）；匿名（fingerprint=None）不去重
        let map = entries_map(vec![
            entry("ev-1", ChannelType::Event, true, Some("Phone"), Some("fp-same")),
            entry("ev-2", ChannelType::Event, true, Some("Phone"), Some("fp-same")),
            entry("ev-3", ChannelType::Event, true, Some("Phone 2"), None),
            entry("ev-4", ChannelType::Event, true, Some("Phone 3"), None),
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
            entry("ev-1", ChannelType::Event, true, Some("operator"), Some("fp-op")),
            entry("ev-2", ChannelType::Event, true, Some("peer"), Some("fp-peer")),
        ]);
        let targets = broadcast_targets(&map, Some("operator"));
        assert_eq!(targets, vec!["ev-2"], "exclude_device_name 命中设备必须被排除");
    }

    // ==================== is_device_online / event_connection_count ====================
    //
    // 两个场景合并为一个测试：它们都直接改全局单例 sessions，且互为 clear_all()
    // 的并发干扰源（#[actix_rt::test] 并行跑会互相清空刚插入的条目）——串行化
    // 消除竞态；broadcast_targets 矩阵测试只用局部 map，无此约束

    #[actix_rt::test]
    async fn device_online_queries() {
        let registry = WsSessionRegistry::global();
        registry.clear_all().await;

        // 场景 1：Event 已认证（在线）/ Terminal 已认证（不算在线）/ Event 未认证（不算）
        for (cid, e) in [
            entry("ev-online", ChannelType::Event, true, Some("Phone"), Some("fp-dev-a")),
            entry("term-only", ChannelType::Terminal, true, Some("Phone"), Some("fp-dev-a")),
            entry("ev-unauthed", ChannelType::Event, false, None, Some("fp-dev-a")),
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
            entry("ev-a1", ChannelType::Event, true, Some("Phone"), Some("fp-multi")),
            entry("ev-a2", ChannelType::Event, true, Some("Phone"), Some("fp-multi")),
            entry("term-a", ChannelType::Terminal, true, Some("Phone"), Some("fp-multi")),
        ] {
            registry.sessions.write().await.insert(cid, e);
        }

        assert_eq!(registry.event_connection_count("fp-multi").await, 2);
        assert!(registry.is_device_online("fp-multi").await);

        registry.clear_all().await;
    }
}