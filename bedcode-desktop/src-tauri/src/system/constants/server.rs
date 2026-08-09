//! 服务器 / WebSocket / HTTP 相关常量

/// 默认服务器端口
pub const DEFAULT_SERVER_PORT: u16 = 8765;

/// WebSocket 终端路径
pub const WS_TERMINAL_PATH: &str = "/ws/terminal";

/// 本地 WebSocket 终端路径（仅限环回地址，免 JWT，供桌面端 WebView 消费 PTY 输出）
pub const LOCAL_WS_TERMINAL_PATH: &str = "/ws/terminal/local";

/// 健康检查 API 路径
pub const API_HEALTH_PATH: &str = "/api/health";

/// CORS 预检请求缓存时间（秒）
pub const CORS_MAX_AGE_SECS: usize = 3600;

/// WebSocket 心跳间隔（秒）
pub const HEARTBEAT_INTERVAL_SECS: u64 = 5;

/// WebSocket 客户端超时（秒）
///
/// 超过此时间未收到 Pong 则判定连接断开
pub const CLIENT_TIMEOUT_SECS: u64 = 10;

/// 远程客户端超时（秒）
///
/// 移动端在高负载（输出风暴 + 前端渲染 + 日志 I/O）下 Pong 回复可能延迟，
/// 本地环回通道（桌面 WebView）保持 CLIENT_TIMEOUT_SECS 即可，
/// 远程通道放宽到 45s 避免高负载误断导致的断连-重连-再订阅循环
pub const REMOTE_CLIENT_TIMEOUT_SECS: u64 = 45;

/// WebSocket 事件广播容量
///
/// 用于 WebSocketManager 内部 ServerEvent 广播
pub const WS_EVENT_BROADCAST_CAPACITY: usize = 16;

/// 服务器重启等待时间（毫秒）
///
/// stop() 后等待旧连接清理完毕再 start()
pub const SERVER_RESTART_DELAY_MS: u64 = 500;

/// 指标采样间隔（秒）
pub const METRICS_SAMPLING_INTERVAL_SECS: u64 = 5;

/// 指标历史记录容量
///
/// VecDeque 预分配容量，同时也是保留的最大条目数
pub const METRICS_HISTORY_CAPACITY: usize = 60;

/// TCP 绑定地址
pub const BIND_ADDRESS: &str = "0.0.0.0";

/// 无法获取对端地址时的占位值
pub const PLACEHOLDER_PEER_ADDR: &str = "0.0.0.0:0";

/// 最大合法端口号
pub const MAX_PORT: u16 = 65535;

/// 端口被占用时搜索替代端口的最大尝试次数
pub const PORT_SEARCH_MAX_ATTEMPTS: u16 = 10;
