//! mDNS 相关常量

/// mDNS 事件接收超时（秒）
///
/// 每次 recv_timeout 的等待时间，超时后继续循环检查是否应停止
pub const RECV_TIMEOUT_SECS: u64 = 1;
