//! Reconnect Module - Reconnection Strategy
//!
//! 职责：错误处理与重连策略
//! 1. 指数退避算法
//! 2. 最大重试次数
//! 3. 重连条件判断

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

use crate::system::constants::reconnect::{
    DEFAULT_BACKOFF_MULTIPLIER, DEFAULT_INITIAL_DELAY_MS, DEFAULT_MAX_DELAY_MS, DEFAULT_MAX_RETRIES,
};

/// 重连配置
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// 最大重试次数（0 表示无限重试）
    pub max_retries: u32,
    /// 初始重连延迟（毫秒）
    pub initial_delay_ms: u64,
    /// 最大重连延迟（毫秒）
    pub max_delay_ms: u64,
    /// 退避倍数
    pub backoff_multiplier: f64,
    /// 是否启用抖动
    pub jitter: bool,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            initial_delay_ms: DEFAULT_INITIAL_DELAY_MS,
            max_delay_ms: DEFAULT_MAX_DELAY_MS,
            backoff_multiplier: DEFAULT_BACKOFF_MULTIPLIER,
            jitter: true,
        }
    }
}

impl ReconnectConfig {
    pub fn new(max_retries: u32, initial_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            max_retries,
            initial_delay_ms,
            max_delay_ms,
            backoff_multiplier: DEFAULT_BACKOFF_MULTIPLIER,
            jitter: true,
        }
    }
}

/// 重连状态
#[derive(Debug, Clone)]
pub enum ReconnectState {
    /// 空闲（未在重连）
    Idle,
    /// 重连中
    Reconnecting {
        attempt: u32,
        next_delay: Duration,
    },
    /// 重连成功
    Success,
    /// 重连失败（达到最大重试次数）
    Failed {
        attempts: u32,
        last_error: String,
    },
    /// 已放弃（手动放弃）
    Abandoned,
}

/// 重连事件
#[derive(Debug, Clone)]
pub enum ReconnectEvent {
    /// 开始重连
    Started {
        attempt: u32,
    },
    /// 重试
    Retrying {
        attempt: u32,
        delay: Duration,
    },
    /// 重连成功
    Succeeded {
        attempts: u32,
    },
    /// 重连失败
    Failed {
        attempts: u32,
        error: String,
    },
    /// 放弃重连
    Abandoned {
        attempts: u32,
        reason: String,
    },
}

/// 重连策略管理器
pub struct ReconnectManager {
    config: ReconnectConfig,
    /// 当前状态
    state: RwLock<ReconnectState>,
    /// 当前重试次数
    retry_count: RwLock<u32>,
    /// 当前延迟
    current_delay: RwLock<Duration>,
    /// 是否已放弃
    abandoned: RwLock<bool>,
}

impl ReconnectManager {
    /// 创建新的重连管理器
    pub fn new(config: ReconnectConfig) -> Arc<Self> {
        let initial_delay = config.initial_delay_ms;
        Arc::new(Self {
            config,
            state: RwLock::new(ReconnectState::Idle),
            retry_count: RwLock::new(0),
            current_delay: RwLock::new(Duration::from_millis(initial_delay)),
            abandoned: RwLock::new(false),
        })
    }

    /// 创建默认配置的重连管理器
    pub fn with_default_config() -> Arc<Self> {
        Self::new(ReconnectConfig::default())
    }

    /// 从客户端配置创建
    pub fn from_client_config(_heartbeat_interval_secs: u64) -> Arc<Self> {
        Self::new(ReconnectConfig::default())
    }

    /// 获取配置
    pub fn config(&self) -> &ReconnectConfig {
        &self.config
    }

    /// 获取当前状��
    pub async fn get_state(&self) -> ReconnectState {
        self.state.read().await.clone()
    }

    /// 检查是否应该重连
    pub async fn should_retry(&self) -> bool {
        if *self.abandoned.read().await {
            return false;
        }

        let retry_count = *self.retry_count.read().await;
        let max_retries = self.config.max_retries;

        if max_retries == 0 {
            return true;
        }

        retry_count < max_retries
    }

    /// 获取下次重连延迟
    pub async fn get_delay(&self) -> Duration {
        let delay = *self.current_delay.read().await;

        if self.config.jitter {
            let jitter_range = delay.as_millis() as f64 * 0.1;
            let jitter = (rand_simple() * jitter_range) as u64;
            delay + Duration::from_millis(jitter)
        } else {
            delay
        }
    }

    /// 开始重连
    pub async fn start(&self) -> Option<Duration> {
        if !self.should_retry().await {
            *self.state.write().await = ReconnectState::Failed {
                attempts: *self.retry_count.read().await,
                last_error: "Max retries exceeded".to_string(),
            };
            return None;
        }

        let mut retry_count = self.retry_count.write().await;
        *retry_count += 1;
        let attempt = *retry_count;

        let delay = self.calculate_delay(attempt).await;
        *self.current_delay.write().await = delay;

        *self.state.write().await = ReconnectState::Reconnecting {
            attempt,
            next_delay: delay,
        };

        info!(
            "[ReconnectManager] Starting reconnect attempt {} (delay: {:?})",
            attempt, delay
        );

        Some(delay)
    }

    /// 计算延迟（指数退避）
    async fn calculate_delay(&self, attempt: u32) -> Duration {
        let max = Duration::from_millis(self.config.max_delay_ms);
        let multiplier = self.config.backoff_multiplier;

        let delay_ms = (self.config.initial_delay_ms as f64)
            * (multiplier.powi(attempt as i32 - 1));

        let delay = Duration::from_millis(delay_ms as u64);

        if delay > max { max } else { delay }
    }

    /// 重连成功
    pub async fn on_success(&self) {
        let attempts = *self.retry_count.read().await;
        *self.state.write().await = ReconnectState::Success;
        info!("[ReconnectManager] Reconnected successfully after {} attempts", attempts);
        self.reset().await;
    }

    /// 重连失败
    pub async fn on_failure(&self, error: String) {
        let attempts = *self.retry_count.read().await;

        if self.config.max_retries > 0 && attempts >= self.config.max_retries {
            *self.state.write().await = ReconnectState::Failed {
                attempts,
                last_error: error.clone(),
            };
        } else {
            *self.state.write().await = ReconnectState::Reconnecting {
                attempt: attempts,
                next_delay: *self.current_delay.read().await,
            };
        }
    }

    /// 放弃重连
    pub async fn abandon(&self, reason: impl Into<String>) {
        let attempts = *self.retry_count.read().await;
        *self.abandoned.write().await = true;
        *self.state.write().await = ReconnectState::Abandoned;

        info!(
            "[ReconnectManager] Reconnect abandoned after {} attempts: {}",
            attempts,
            reason.into()
        );
    }

    /// 重置重连状态
    pub async fn reset(&self) {
        *self.retry_count.write().await = 0;
        *self.current_delay.write().await = Duration::from_millis(self.config.initial_delay_ms);
        *self.abandoned.write().await = false;
        *self.state.write().await = ReconnectState::Idle;
    }

    /// 获取当前重试次数
    pub async fn get_retry_count(&self) -> u32 {
        *self.retry_count.read().await
    }

    /// 检查是否已放弃
    pub async fn is_abandoned(&self) -> bool {
        *self.abandoned.read().await
    }

    /// 检查是否正在重连
    pub async fn is_reconnecting(&self) -> bool {
        matches!(
            *self.state.read().await,
            ReconnectState::Reconnecting { .. }
        )
    }
}

impl Default for ReconnectManager {
    fn default() -> Self {
        let config = ReconnectConfig::default();
        let initial_delay = config.initial_delay_ms;
        Self {
            config,
            state: RwLock::new(ReconnectState::Idle),
            retry_count: RwLock::new(0),
            current_delay: RwLock::new(Duration::from_millis(initial_delay)),
            abandoned: RwLock::new(false),
        }
    }
}

fn rand_simple() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    (nanos as f64) / (u32::MAX as f64)
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::constants::reconnect::DEFAULT_RETRY_DELAYS_MS;

    /// 构造确定性配置（禁用抖动），便于锁定退避序列
    fn deterministic_config(max_retries: u32, initial_delay_ms: u64, max_delay_ms: u64) -> ReconnectConfig {
        ReconnectConfig {
            max_retries,
            initial_delay_ms,
            max_delay_ms,
            backoff_multiplier: DEFAULT_BACKOFF_MULTIPLIER,
            jitter: false,
        }
    }

    #[tokio::test]
    async fn test_exponential_backoff_sequence() {
        // 指数退避：1s→2s→4s→8s→16s（默认倍数 2.0）
        let mgr = ReconnectManager::new(deterministic_config(5, 1000, 30_000));
        let expected = [1000u64, 2000, 4000, 8000, 16000];
        for expect_ms in expected {
            let delay = mgr.start().await.expect("重试次数未耗尽应返回延迟");
            assert_eq!(delay, Duration::from_millis(expect_ms));
        }
    }

    #[tokio::test]
    async fn test_backoff_capped_at_max_delay() {
        // 延迟封顶：第 5 次理论值 16s 超过 max 10s，应返回 10s
        let mgr = ReconnectManager::new(deterministic_config(5, 1000, 10_000));
        let expected = [1000u64, 2000, 4000, 8000, 10_000];
        for expect_ms in expected {
            let delay = mgr.start().await.unwrap();
            assert_eq!(delay, Duration::from_millis(expect_ms));
        }
    }

    #[tokio::test]
    async fn test_start_returns_none_when_retries_exhausted() {
        // 次数耗尽：start 返回 None 且状态置为 Failed
        let mgr = ReconnectManager::new(deterministic_config(2, 1000, 30_000));
        assert!(mgr.start().await.is_some());
        assert!(mgr.start().await.is_some());
        assert_eq!(mgr.get_retry_count().await, 2);
        assert!(mgr.start().await.is_none());
        match mgr.get_state().await {
            ReconnectState::Failed { attempts, last_error } => {
                assert_eq!(attempts, 2);
                assert_eq!(last_error, "Max retries exceeded");
            }
            other => panic!("期望 Failed 状态，实际: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_zero_max_retries_means_unlimited() {
        // max_retries=0 表示无限重试：should_retry 恒真，start 永不返回 None
        let mgr = ReconnectManager::new(deterministic_config(0, 100, 30_000));
        assert!(mgr.should_retry().await);
        for _ in 0..10 {
            assert!(mgr.start().await.is_some());
        }
        assert!(mgr.should_retry().await);
    }

    #[tokio::test]
    async fn test_default_retry_delays_match_constant_table() {
        // 默认配置（抖动仅影响 get_delay，不影响 start 的退避序列）
        // 与常量表 DEFAULT_RETRY_DELAYS_MS 锁死，防止两侧漂移
        let mgr = ReconnectManager::with_default_config();
        for expect_ms in DEFAULT_RETRY_DELAYS_MS {
            let delay = mgr.start().await.unwrap();
            assert_eq!(delay, Duration::from_millis(*expect_ms));
        }
    }

    #[tokio::test]
    async fn test_reset_restores_initial_state() {
        // reset 后计数、延迟、放弃标记全部还原，可重新开始新一轮退避
        let mgr = ReconnectManager::new(deterministic_config(5, 1000, 30_000));
        mgr.start().await.unwrap();
        mgr.start().await.unwrap();
        mgr.abandon("test").await;
        assert!(mgr.is_abandoned().await);

        mgr.reset().await;
        assert_eq!(mgr.get_retry_count().await, 0);
        assert!(!mgr.is_abandoned().await);
        assert!(matches!(mgr.get_state().await, ReconnectState::Idle));
        // 延迟复位为初始值：新一轮第一次重试又是 1s
        let delay = mgr.start().await.unwrap();
        assert_eq!(delay, Duration::from_millis(1000));
    }

    #[tokio::test]
    async fn test_on_success_resets_to_idle() {
        // 重连成功：状态短暂置 Success 后立即 reset，最终回到 Idle 且计数清零
        let mgr = ReconnectManager::new(deterministic_config(5, 1000, 30_000));
        mgr.start().await.unwrap();
        assert!(mgr.is_reconnecting().await);
        mgr.on_success().await;
        assert!(matches!(mgr.get_state().await, ReconnectState::Idle));
        assert_eq!(mgr.get_retry_count().await, 0);
        assert!(!mgr.is_reconnecting().await);
    }

    #[tokio::test]
    async fn test_on_failure_within_limit_stays_reconnecting() {
        // 未达上限的失败：保持 Reconnecting，携带当前延迟
        let mgr = ReconnectManager::new(deterministic_config(5, 1000, 30_000));
        mgr.start().await.unwrap();
        mgr.on_failure("temporary".to_string()).await;
        match mgr.get_state().await {
            ReconnectState::Reconnecting { attempt, next_delay } => {
                assert_eq!(attempt, 1);
                assert_eq!(next_delay, Duration::from_millis(1000));
            }
            other => panic!("期望 Reconnecting 状态，实际: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_on_failure_at_limit_marks_failed() {
        // 达到上限后的失败：置 Failed 并保留最后一次错误
        let mgr = ReconnectManager::new(deterministic_config(2, 1000, 30_000));
        mgr.start().await.unwrap();
        mgr.start().await.unwrap();
        mgr.on_failure("fatal".to_string()).await;
        match mgr.get_state().await {
            ReconnectState::Failed { attempts, last_error } => {
                assert_eq!(attempts, 2);
                assert_eq!(last_error, "fatal");
            }
            other => panic!("期望 Failed 状态，实际: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_abandon_stops_future_retries() {
        // 手动放弃：should_retry 为 false，start 返回 None
        let mgr = ReconnectManager::new(deterministic_config(5, 1000, 30_000));
        mgr.abandon("user cancelled").await;
        assert!(mgr.is_abandoned().await);
        assert!(!mgr.should_retry().await);
        assert!(mgr.start().await.is_none());
    }

    #[tokio::test]
    async fn test_jitter_within_ten_percent_bounds() {
        // 抖动范围：延迟 ∈ [base, base + 10%]（jitter = rand * base*0.1 截断）
        let mgr = ReconnectManager::new(ReconnectConfig {
            max_retries: 5,
            initial_delay_ms: 1000,
            max_delay_ms: 30_000,
            backoff_multiplier: DEFAULT_BACKOFF_MULTIPLIER,
            jitter: true,
        });
        let base = Duration::from_millis(1000);
        let upper = Duration::from_millis(1100);
        for _ in 0..50 {
            let delay = mgr.get_delay().await;
            assert!(delay >= base, "延迟不应小于基础值: {:?}", delay);
            assert!(delay <= upper, "延迟不应超过 +10%: {:?}", delay);
        }
    }

    #[tokio::test]
    async fn test_zero_initial_delay_ok() {
        // 边界：初始延迟为 0 时退避序列全为 0（不 panic 且可连续调用）
        let mgr = ReconnectManager::new(deterministic_config(3, 0, 30_000));
        for _ in 0..3 {
            let delay = mgr.start().await.unwrap();
            assert_eq!(delay, Duration::ZERO);
        }
    }
}