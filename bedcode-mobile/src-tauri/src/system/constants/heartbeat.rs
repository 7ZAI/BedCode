//! 心跳相关常量

/// 默认最大连续心跳超时次数
///
/// 连续超时达到此次数后判定连接已断开
pub const DEFAULT_MAX_HEARTBEAT_TIMEOUTS: u32 = 3;

/// 心跳超时倍数
///
/// 超时时间 = 心跳间隔 × 此倍数
pub const HEARTBEAT_TIMEOUT_MULTIPLIER: u64 = 3;
