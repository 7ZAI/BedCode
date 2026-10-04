//! 重连相关常量

/// 默认最大重试轮数（`ConnectionManager::reconnect` 顶层重连循环的上界）
pub const DEFAULT_MAX_RETRIES: u32 = 3;

/// 默认初始重连延迟（毫秒）
pub const DEFAULT_INITIAL_DELAY_MS: u64 = 1000;

/// 默认最大重连延迟（毫秒）
pub const DEFAULT_MAX_DELAY_MS: u64 = 30000;

/// 默认退避倍数（ReconnectManager 内部等比退避用；初始 1s × 2^n → 1s/2s/4s）
pub const DEFAULT_BACKOFF_MULTIPLIER: f64 = 2.0;

// 注：原 `DEFAULT_RETRY_DELAYS_MS: &[1000, 2000, 4000]` 已删除（2026-10-04）。
// 它与 ReconnectManager 的退避序列**内容相同、来源不同**——前者是 manager.rs
// 顶层循环的硬编码等待表，后者是 ReconnectManager 的算法输出，两者只靠注释
// 声称一致。审计发现 ReconnectManager 整条链挂在零调用者的
// `WsClient::reconnect()` 上（真实退避无抖动、无下限钳制），而这张表是当时真正
// 在跑的路径。接线修复后只剩一个来源，保留第二张表只会重新引入漂移面。

/// 重连最小退避下限（毫秒，M2/ADR 0031）：无论配置/轮次，单次等待不得小于 1s。
/// 杜绝「无退避」形态——2026-09-29 移动端 616 次/98 秒（≈6.3 Hz）自愈风暴正是
/// 退避下限缺失的后果（纵深防御：即使桌面端再出快速拒绝，也打不出风暴）。
pub const MIN_RECONNECT_DELAY_MS: u64 = 1000;

/// 同因熔断阈值（M2）：连续 N 次**相同原因**的重连失败即放弃（不再重试）。
/// 只熔断同因（网络抖动因超时后变化会被新原因重置计数），避免把临时抖动打成
/// 永久放弃。
///
/// **当前不可达**（`DEFAULT_MAX_RETRIES = 3` < 5，轮次先耗尽）。保留为纵深防御：
/// 日后调高轮数时同因熔断自动生效，无需再补接线。此值属于「业务默认值」，
/// 若要改为 3（与轮数齐平）需产品侧确认，不在本次改动范围。
pub const CIRCUIT_BREAKER_SAME_CAUSE_LIMIT: u32 = 5;
