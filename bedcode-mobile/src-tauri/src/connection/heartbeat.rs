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

use crate::system::constants::connection::BROADCAST_CHANNEL_CAPACITY;
use crate::system::constants::heartbeat::{DEFAULT_MAX_HEARTBEAT_TIMEOUTS, HEARTBEAT_TIMEOUT_MULTIPLIER};

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
            timeout: Duration::from_secs(30 * HEARTBEAT_TIMEOUT_MULTIPLIER),
            max_timeouts: DEFAULT_MAX_HEARTBEAT_TIMEOUTS,
        }
    }
}

impl HeartbeatConfig {
    pub fn new(interval_secs: u64, timeout_secs: u64) -> Self {
        Self {
            interval: Duration::from_secs(interval_secs),
            timeout: Duration::from_secs(timeout_secs),
            max_timeouts: DEFAULT_MAX_HEARTBEAT_TIMEOUTS,
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
        let (event_tx, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
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
            timeout: Duration::from_secs(heartbeat_interval_secs * HEARTBEAT_TIMEOUT_MULTIPLIER),
            max_timeouts: DEFAULT_MAX_HEARTBEAT_TIMEOUTS,
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
    ///
    /// 注意：必须在 async 上下文中调用（tokio RwLock 的 blocking_write 在
    /// 运行时任务内会 panic，见 ws_client 接收循环调用点）
    pub async fn on_pong_received(&self) {
        *self.last_pong.write().await = Some(std::time::Instant::now());

        // 重置连续超时计数
        *self.consecutive_timeouts.write().await = 0;

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

}

impl Default for HeartbeatManager {
    fn default() -> Self {
        Self {
            config: HeartbeatConfig::default(),
            event_tx: broadcast::channel(BROADCAST_CHANNEL_CAPACITY).0,
            is_running: Arc::new(RwLock::new(false)),
            last_pong: RwLock::new(None),
            consecutive_timeouts: RwLock::new(0),
            running_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_values() {
        // 默认：间隔 30s，超时 = 间隔 × 3 = 90s，最大连续超时 3 次
        let cfg = HeartbeatConfig::default();
        assert_eq!(cfg.interval, Duration::from_secs(30));
        assert_eq!(cfg.timeout, Duration::from_secs(90));
        assert_eq!(cfg.max_timeouts, DEFAULT_MAX_HEARTBEAT_TIMEOUTS);
    }

    #[test]
    fn test_from_client_config_applies_timeout_multiplier() {
        // 旧 API 兼容入口：超时时间 = 间隔 × HEARTBEAT_TIMEOUT_MULTIPLIER
        let mgr = HeartbeatManager::from_client_config(10);
        let cfg = mgr.config();
        assert_eq!(cfg.interval, Duration::from_secs(10));
        assert_eq!(cfg.timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_new_config_preserves_custom_values() {
        // 显式构造：间隔/超时按参数设置，最大超时次数保持默认
        let mgr = HeartbeatManager::new(HeartbeatConfig::new(5, 20));
        let cfg = mgr.config();
        assert_eq!(cfg.interval, Duration::from_secs(5));
        assert_eq!(cfg.timeout, Duration::from_secs(20));
        assert_eq!(cfg.max_timeouts, DEFAULT_MAX_HEARTBEAT_TIMEOUTS);
    }

    #[tokio::test]
    async fn test_increment_timeout_sequence() {
        // 连续超时计数从 1 递增，供调用方对照 max_timeouts 判定断连
        let mgr = HeartbeatManager::new(HeartbeatConfig::default());
        assert_eq!(mgr.increment_timeout().await, 1);
        assert_eq!(mgr.increment_timeout().await, 2);
        assert_eq!(mgr.increment_timeout().await, 3);
        assert_eq!(mgr.get_consecutive_timeouts().await, 3);
    }

    #[tokio::test]
    async fn test_pong_received_resets_timeouts_and_emits_event() {
        // 收到 Pong：连续超时计数清零，并广播 PongReceived 事件（latency 目前恒为 0）
        let mgr = HeartbeatManager::new(HeartbeatConfig::default());
        mgr.increment_timeout().await;
        mgr.increment_timeout().await;

        let mut rx = mgr.subscribe();
        mgr.on_pong_received().await;
        assert_eq!(mgr.get_consecutive_timeouts().await, 0);
        match rx.recv().await {
            Ok(HeartbeatEvent::PongReceived { latency_ms }) => assert_eq!(latency_ms, 0),
            other => panic!("期望 PongReceived 事件，实际: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers_all_receive_pong_event() {
        // broadcast 语义：所有订阅者都能收到同一事件
        let mgr = HeartbeatManager::new(HeartbeatConfig::default());
        let mut rx1 = mgr.subscribe();
        let mut rx2 = mgr.subscribe();
        mgr.on_pong_received().await;
        assert!(matches!(rx1.recv().await, Ok(HeartbeatEvent::PongReceived { .. })));
        assert!(matches!(rx2.recv().await, Ok(HeartbeatEvent::PongReceived { .. })));
    }

    #[tokio::test]
    async fn test_connection_lost_false_before_any_pong() {
        // 从未收到 Pong（last_pong=None）时不算断连——当前实现的语义约定
        let mgr = HeartbeatManager::new(HeartbeatConfig::default());
        assert!(!mgr.is_connection_lost().await);
    }

    #[tokio::test]
    async fn test_connection_lost_false_right_after_pong() {
        // 刚收到 Pong：elapsed 远小于 timeout，不算断连
        let mgr = HeartbeatManager::new(HeartbeatConfig::new(30, 30));
        mgr.on_pong_received().await;
        assert!(!mgr.is_connection_lost().await);
    }

    #[tokio::test]
    async fn test_connection_lost_true_after_timeout_elapses() {
        // 超时后判定断连。last_pong 使用 std::time::Instant，无注入缝，
        // 只能构造极小超时（1ms）做一次最小真实等待
        let mgr = HeartbeatManager::new(HeartbeatConfig {
            interval: Duration::from_secs(1),
            timeout: Duration::from_millis(1),
            max_timeouts: DEFAULT_MAX_HEARTBEAT_TIMEOUTS,
        });
        mgr.on_pong_received().await;
        assert!(!mgr.is_connection_lost().await);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(mgr.is_connection_lost().await);
    }

    #[tokio::test]
    async fn test_stop_is_idempotent() {
        // 停止可重复调用，不 panic；运行标记恒为 false（管理器为被动组件，
        // 运行循环在 ws_client 侧，此处仅验证 stop 路径稳定）
        let mgr = HeartbeatManager::new(HeartbeatConfig::default());
        mgr.stop().await;
        mgr.stop().await;
        assert!(!mgr.is_running().await);
    }
}