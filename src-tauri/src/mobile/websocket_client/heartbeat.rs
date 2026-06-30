//! Heartbeat Module - Ping/Pong Keep-Alive
//!
//! 职责：心跳保活机制
//! 1. 定期发送 Ping（协议层）
//! 2. 检测 Pong 响应
//! 3. 超时检测

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tracing::debug;

/// 心跳配置
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// 心跳间隔
    pub interval: Duration,
    /// 超时时间（未收到 Pong 则认为连接失效）
    pub timeout: Duration,
    /// 最大连续超时次数
    pub max_timeouts: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(90),
            max_timeouts: 3,
        }
    }
}

impl HeartbeatConfig {
    pub fn new(interval_secs: u64, timeout_secs: u64) -> Self {
        Self {
            interval: Duration::from_secs(interval_secs),
            timeout: Duration::from_secs(timeout_secs),
            max_timeouts: 3,
        }
    }
}

/// 心跳事件
#[derive(Debug, Clone)]
pub enum HeartbeatEvent {
    /// 发送了 Ping
    PingSent,
    /// 收到 Pong 响应
    PongReceived {
        latency_ms: u64,
    },
    /// 心跳超时
    Timeout {
        consecutive: u32,
    },
    /// 心跳停止
    Stopped,
}

/// 心跳管理器
pub struct HeartbeatManager {
    config: HeartbeatConfig,
    /// 事件广播器
    event_tx: broadcast::Sender<HeartbeatEvent>,
    /// 是否正在运行
    is_running: Arc<RwLock<bool>>,
    /// 最后收到 Pong 的时间
    last_pong: RwLock<Option<std::time::Instant>>,
    /// 连续超时次数
    consecutive_timeouts: RwLock<u32>,
    /// 运行标记（用于原子操作）
    running_flag: Arc<std::sync::atomic::AtomicBool>,
}

impl HeartbeatManager {
    /// 创建新的心跳管理器
    pub fn new(config: HeartbeatConfig) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(1024);
        Arc::new(Self {
            config,
            event_tx,
            is_running: Arc::new(RwLock::new(false)),
            last_pong: RwLock::new(None),
            consecutive_timeouts: RwLock::new(0),
            running_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// 从客户端配置创建（兼容旧 API）
    pub fn from_client_config(heartbeat_interval_secs: u64) -> Arc<Self> {
        Self::new(HeartbeatConfig {
            interval: Duration::from_secs(heartbeat_interval_secs),
            timeout: Duration::from_secs(heartbeat_interval_secs * 3),
            max_timeouts: 3,
        })
    }

    /// 获取配置
    pub fn config(&self) -> &HeartbeatConfig {
        &self.config
    }

    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<HeartbeatEvent> {
        self.event_tx.subscribe()
    }

    /// 记录 Pong 响应（收到服务器 Pong 时调用）
    pub fn on_pong_received(&self) {
        let mut last_pong = self.last_pong.blocking_write();
        *last_pong = Some(std::time::Instant::now());

        // 重置连续超时计数
        *self.consecutive_timeouts.blocking_write() = 0;

        let _ = self.event_tx.send(HeartbeatEvent::PongReceived {
            latency_ms: 0,
        });

        debug!("[HeartbeatManager] Pong received");
    }

    /// 检查是否应该认为连接已断开
    pub async fn is_connection_lost(&self) -> bool {
        let last_pong = self.last_pong.read().await;
        if let Some(last) = *last_pong {
            last.elapsed() > self.config.timeout
        } else {
            false
        }
    }

    /// 停止心跳
    pub async fn stop(&self) {
        self.running_flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let mut running = self.is_running.write().await;
        *running = false;
    }

    /// 检查是否正在运行
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// 获取连续超时次数
    pub async fn get_consecutive_timeouts(&self) -> u32 {
        *self.consecutive_timeouts.read().await
    }

    /// 增加超时计数
    pub async fn increment_timeout(&self) -> u32 {
        let mut count = self.consecutive_timeouts.write().await;
        *count += 1;
        *count
    }

    /// 重置超时计数
    pub fn reset_timeouts(&self) {
        *self.consecutive_timeouts.blocking_write() = 0;
    }
}

impl Default for HeartbeatManager {
    fn default() -> Self {
        Self {
            config: HeartbeatConfig::default(),
            event_tx: broadcast::channel(1024).0,
            is_running: Arc::new(RwLock::new(false)),
            last_pong: RwLock::new(None),
            consecutive_timeouts: RwLock::new(0),
            running_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}