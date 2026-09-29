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

/// 重连最小退避下限（毫秒，M2/ADR 0031）：无论配置/轮次，单次等待不得小于 1s。
/// 杜绝「无退避」形态——2026-09-29 移动端 616 次/98 秒（≈6.3 Hz）自愈风暴正是
/// 退避下限缺失的后果（纵深防御：即使桌面端再出快速拒绝，也打不出风暴）。
pub const MIN_RECONNECT_DELAY_MS: u64 = 1000;

/// 同因熔断阈值（M2）：连续 N 次**相同原因**的重连失败即放弃（不再重试）。
/// 只熔断同因（网络抖动因超时后变化会被新原因重置计数），避免把临时抖动打成
/// 永久放弃。
pub const CIRCUIT_BREAKER_SAME_CAUSE_LIMIT: u32 = 5;
