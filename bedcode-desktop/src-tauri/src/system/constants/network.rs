//! 网络 / 连接相关常量

/// 本地回环地址（用于 QR 码 IP fallback）
pub const LOCALHOST_IP: &str = "127.0.0.1";

/// IPv4 回环地址前缀（用于过滤非外部 IP）
pub const IP_LOOPBACK_PREFIX: &str = "127.";

/// IPv4 链路本地地址前缀（用于过滤非外部 IP）
pub const IP_LINK_LOCAL_PREFIX: &str = "169.254.";

/// 同步事件广播容量
///
/// 用于 DesktopSyncEvent 的 broadcast channel
pub const SYNC_EVENT_BROADCAST_CAPACITY: usize = 64;
