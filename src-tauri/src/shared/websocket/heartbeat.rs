//! WebSocket Heartbeat Manager
//!
//! 独立的心跳管理模块，支持配置化的心跳检测

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};
use tokio::time::interval;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

/// 心跳配置
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// 心跳发送间隔
    pub interval: Duration,
    /// 心跳超时时间
    pub timeout: Duration,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(90),
        }
    }
}

impl HeartbeatConfig {
    pub fn new(interval_secs: u64, timeout_secs: u64) -> Self {
        Self {
            interval: Duration::from_secs(interval_secs),
            timeout: Duration::from_secs(timeout_secs),
        }
    }
}

/// 心跳事件
#[derive(Debug, Clone)]
pub enum HeartbeatEvent {
    /// 客户端心跳超时
    Timeout { addr: SocketAddr },
    /// 客户端心跳响应
    Pong { addr: SocketAddr },
}

/// 心跳管理器
#[derive(Debug, Clone)]
pub struct HeartbeatManager {
    config: HeartbeatConfig,
    /// 客户端最后心跳时间
    last_heartbeat: Arc<RwLock<HashMap<SocketAddr, Instant>>>,
    /// 事件发送器
    event_tx: broadcast::Sender<HeartbeatEvent>,
    /// 关闭信号
    shutdown_tx: broadcast::Sender<()>,
}

impl HeartbeatManager {
    /// 创建心跳管理器
    pub fn new(config: HeartbeatConfig) -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            config,
            last_heartbeat: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            shutdown_tx,
        }
    }

    /// 获取配置
    pub fn config(&self) -> &HeartbeatConfig {
        &self.config
    }

    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<HeartbeatEvent> {
        self.event_tx.subscribe()
    }

    /// 更新客户端心跳时间
    pub async fn update_heartbeat(&self, addr: SocketAddr) {
        let mut heartbeats = self.last_heartbeat.write().await;
        heartbeats.insert(addr, Instant::now());
    }

    /// 移除客户端
    pub async fn remove_client(&self, addr: SocketAddr) {
        let mut heartbeats = self.last_heartbeat.write().await;
        heartbeats.remove(&addr);
    }

    /// 获取所有超时的客户端地址
    pub async fn get_timeout_clients(&self) -> Vec<SocketAddr> {
        let heartbeats = self.last_heartbeat.read().await;
        let now = Instant::now();

        heartbeats
            .iter()
            .filter(|(_, time)| now.duration_since(**time) > self.config.timeout)
            .map(|(addr, _)| *addr)
            .collect()
    }

    /// 启动心跳检测任务
    pub fn spawn_checker(&self) {
        let last_heartbeat = self.last_heartbeat.clone();
        let event_tx = self.event_tx.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let timeout = self.config.timeout;

        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(30));
            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        let now = Instant::now();
                        let heartbeats = last_heartbeat.read().await;

                        let timeout_addrs: Vec<SocketAddr> = heartbeats
                            .iter()
                            .filter(|(_, time)| now.duration_since(**time) > timeout)
                            .map(|(addr, _)| *addr)
                            .collect();

                        for addr in timeout_addrs {
                            tracing::warn!("Heartbeat timeout for client: {}", addr);
                            let _ = event_tx.send(HeartbeatEvent::Timeout { addr });
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
        });
    }

    /// 停止心跳管理器
    pub async fn stop(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

impl Default for HeartbeatManager {
    fn default() -> Self {
        Self::new(HeartbeatConfig::default())
    }
}

/// 心跳发送器 trait - 用于自定义心跳消息
pub trait HeartbeatSender: Send + Sync {
    /// 发送 Ping 消息
    fn send_ping(&self, addr: &SocketAddr) -> impl std::future::Future<Output = ()> + Send;

    /// 发送 Pong 消息
    fn send_pong(&self, addr: &SocketAddr, data: Vec<u8>) -> impl std::future::Future<Output = ()> + Send;
}