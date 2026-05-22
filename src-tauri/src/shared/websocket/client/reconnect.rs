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
            max_retries: 0,
            initial_delay_ms: 1000,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
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
            backoff_multiplier: 2.0,
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
        Arc::new(Self {
            config,
            state: RwLock::new(ReconnectState::Idle),
            retry_count: RwLock::new(0),
            current_delay: RwLock::new(Duration::from_millis(1000)),
            abandoned: RwLock::new(false),
        })
    }

    /// 创建默认配置的重连管理器
    pub fn with_default_config() -> Arc<Self> {
        Self::new(ReconnectConfig::default())
    }

    /// 从客户端配置创建
    pub fn from_client_config(_heartbeat_interval_secs: u64) -> Arc<Self> {
        Self::new(ReconnectConfig {
            max_retries: 0,
            initial_delay_ms: 1000,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
            jitter: true,
        })
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
        Self {
            config: ReconnectConfig::default(),
            state: RwLock::new(ReconnectState::Idle),
            retry_count: RwLock::new(0),
            current_delay: RwLock::new(Duration::from_millis(1000)),
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