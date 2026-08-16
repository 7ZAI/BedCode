//! 重连相关常量

/// 默认最大重试次数
pub const DEFAULT_MAX_RETRIES: u32 = 5;

/// 默认初始重连延迟（毫秒）
pub const DEFAULT_INITIAL_DELAY_MS: u64 = 1000;

/// 默认最大重连延迟（毫秒）
pub const DEFAULT_MAX_DELAY_MS: u64 = 30000;

/// 默认退避倍数
pub const DEFAULT_BACKOFF_MULTIPLIER: f64 = 2.0;

/// 默认指数退避延迟表（毫秒）
///
/// 对应 5 次重试的延迟：1s, 2s, 4s, 8s, 16s
pub const DEFAULT_RETRY_DELAYS_MS: &[u64] = &[1000, 2000, 4000, 8000, 16000];
