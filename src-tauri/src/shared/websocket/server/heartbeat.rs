//! WebSocket Heartbeat Manager
//!
//! 心跳管理模块，负责：
//! 1. 僵尸连接检测 - 定期检查超时连接
//! 2. 主动 Ping 探测 - 在超时前发送 Ping 检测连接存活
//! 3. 协议层 Ping/Pong 自动处理（tungstenite 负责）

use crate::shared::websocket::server::connection_manager::{ConnectionEvent, ConnectionId, ConnectionManager};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tokio::time::interval;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tracing::{debug, warn};

/// 心跳配置
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// 心跳检查间隔
    pub check_interval: Duration,
    /// 心跳超时时间
    pub timeout: Duration,
    /// 主动 Ping 阈值（超过此时间无活动则发送 Ping）
    pub ping_threshold: Duration,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(20),
            timeout: Duration::from_secs(60),
            ping_threshold: Duration::from_secs(30), // 30 秒无活动则发送 Ping
        }
    }
}

impl HeartbeatConfig {
    pub fn new(interval_secs: u64, timeout_secs: u64) -> Self {
        // Ping 阈值为超时时间的一半，确保有足够时间等待 Pong
        let ping_threshold = Duration::from_secs(timeout_secs / 2);
        Self {
            check_interval: Duration::from_secs(interval_secs),
            timeout: Duration::from_secs(timeout_secs),
            ping_threshold,
        }
    }
}

/// 心跳事件
#[derive(Debug, Clone)]
pub enum HeartbeatEvent {
    /// 客户端心跳超时
    Timeout {
        id: ConnectionId,
        addr: std::net::SocketAddr,
    },
    /// 客户端认证成功
    Authenticated {
        id: ConnectionId,
        client_id: String,
    },
    /// 连接已断开
    Disconnected {
        id: ConnectionId,
        addr: std::net::SocketAddr,
    },
}

/// 心跳管理器
pub struct HeartbeatManager {
    config: HeartbeatConfig,
    /// 连接管理器引用
    connection_manager: Arc<ConnectionManager>,
    /// 事件发送器
    event_tx: broadcast::Sender<HeartbeatEvent>,
    /// 是否正在运行
    is_running: Arc<RwLock<bool>>,
}

impl HeartbeatManager {
    /// 创建心跳管理器
    pub fn new(config: HeartbeatConfig, connection_manager: Arc<ConnectionManager>) -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Self {
            config,
            connection_manager,
            event_tx,
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    /// 从 WsServerConfig 创建（兼容旧 API）
    pub fn from_ws_config(
        interval_secs: u64,
        timeout_secs: u64,
        connection_manager: Arc<ConnectionManager>,
    ) -> Self {
        Self::new(HeartbeatConfig::new(interval_secs, timeout_secs), connection_manager)
    }

    /// 获取配置
    pub fn config(&self) -> &HeartbeatConfig {
        &self.config
    }

    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<HeartbeatEvent> {
        self.event_tx.subscribe()
    }

    /// 检查连接是否超时
    pub async fn check_timeout(&self) -> Vec<ConnectionId> {
        let now = std::time::Instant::now();
        let all_ids = self.connection_manager.all_ids().await;
        let mut timeout_ids = Vec::new();

        for id in all_ids {
            if let Some(conn) = self.connection_manager.get(id).await {
                if now.duration_since(conn.last_heartbeat) > self.config.timeout {
                    timeout_ids.push(id);
                }
            }
        }

        timeout_ids
    }

    /// 启动心跳检测任务
    pub fn spawn_checker(&self) {
        let connection_manager = Arc::clone(&self.connection_manager);
        let event_tx = self.event_tx.clone();
        let is_running = Arc::clone(&self.is_running);
        let timeout = self.config.timeout;
        let check_interval = self.config.check_interval;
        let ping_threshold = self.config.ping_threshold;

        tokio::spawn(async move {
            {
                let mut running = is_running.write().await;
                *running = true;
            }

            let mut tick = interval(check_interval);
            debug!(
                "Heartbeat checker started: interval={:?}, timeout={:?}, ping_threshold={:?}",
                check_interval, timeout, ping_threshold
            );

            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        let now = std::time::Instant::now();
                        let all_ids = connection_manager.all_ids().await;

                        for id in all_ids {
                            if let Some(conn) = connection_manager.get(id).await {
                                let elapsed = now.duration_since(conn.last_heartbeat);

                                if elapsed > timeout {
                                    // 心跳超时，断开连接
                                    warn!(
                                        "Heartbeat timeout for connection: {} ({}), last activity {:?}s ago, disconnecting",
                                        id, conn.addr, elapsed.as_secs()
                                    );

                                    // 发送超时事件
                                    let _ = event_tx.send(HeartbeatEvent::Timeout {
                                        id,
                                        addr: conn.addr,
                                    });

                                    // 发送断开事件，通知上层清理
                                    let _ = event_tx.send(HeartbeatEvent::Disconnected {
                                        id,
                                        addr: conn.addr,
                                    });

                                    // 从连接管理器中移除连接，触发实际断开
                                    connection_manager.unregister(id).await;
                                } else if elapsed > ping_threshold {
                                    // 超过 Ping 阈值，主动发送 Ping 探测
                                    debug!(
                                        "Sending Ping to connection {} ({}), last activity {:?}s ago",
                                        id, conn.addr, elapsed.as_secs()
                                    );

                                    if let Some(sender) = connection_manager.get_sender(id).await {
                                        // 发送 Ping，携带时间戳作为 payload
                                        let timestamp = now.elapsed().as_millis().to_be_bytes().to_vec();
                                        if sender.send(WsMsg::Ping(timestamp)).await.is_err() {
                                            warn!("Failed to send Ping to connection {}, send channel closed", id);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ = tokio::signal::ctrl_c() => {
                        debug!("Heartbeat checker received Ctrl+C");
                        break;
                    }
                }
            }

            {
                let mut running = is_running.write().await;
                *running = false;
            }
            debug!("Heartbeat checker stopped");
        });
    }

    /// 启动连接事件监听（同步 ConnectionManager 的事件到 HeartbeatEvent）
    pub fn spawn_event_forwarder(&self) {
        let connection_manager = Arc::clone(&self.connection_manager);
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            let mut rx = connection_manager.subscribe();

            loop {
                match rx.recv().await {
                    Ok(event) => {
                        match event {
                            ConnectionEvent::Authenticated { id, client_id } => {
                                let _ = event_tx.send(HeartbeatEvent::Authenticated { id, client_id });
                            }
                            ConnectionEvent::Disconnected { id, addr, .. } => {
                                let _ = event_tx.send(HeartbeatEvent::Disconnected { id, addr });
                            }
                            ConnectionEvent::Connected { .. } => {
                                // 新连接不需要特殊处理
                            }
                            ConnectionEvent::Heartbeat { .. } => {
                                // 这个事件在新的 ConnectionManager 中可能不存在
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // 跳过滞后的消息
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        });
    }

    /// 停止心跳管理器
    pub async fn stop(&self) {
        let mut running = self.is_running.write().await;
        *running = false;
    }

    /// 检查是否正在运行
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }
}