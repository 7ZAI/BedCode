//! 重连相关常量

/// 默认最大重试轮数（manager.rs 顶层重连循环 + ReconnectManager 默认配置共用）
pub const DEFAULT_MAX_RETRIES: u32 = 3;

/// 默认初始重连延迟（毫秒）
pub const DEFAULT_INITIAL_DELAY_MS: u64 = 1000;

/// 默认最大重连延迟（毫秒）
pub const DEFAULT_MAX_DELAY_MS: u64 = 30000;

/// 默认退避倍数（ReconnectManager 内部等比退避用；初始 1s × 2^n → 1s/2s/4s）
pub const DEFAULT_BACKOFF_MULTIPLIER: f64 = 2.0;

/// 默认等比退避延迟表（毫秒）：1s, 2s, 4s（3 轮，初始 1s × 2^n）
///
/// 供 manager.rs 顶层重连循环按轮取等待（轮数上限见 DEFAULT_MAX_RETRIES），
/// 与 ReconnectManager 默认等比序列一致（1s/2s/4s）
pub const DEFAULT_RETRY_DELAYS_MS: &[u64] = &[1000, 2000, 4000];
