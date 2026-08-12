//! 连接相关常量
//!
//! WebSocket 连接、Channel 容量、轮询间隔、日志截断等

/// Broadcast channel 默认容量
///
/// 用于所有 broadcast::channel() 创建，统一缓冲区大小
/// 客户端事件广播容量：回放洪峰时（历史全量重播）短时间涌入大量输出帧，
/// 容量过小 + 转发循环被慢路径（插件回调）阻塞会溢出丢帧（移动端游标连续性破坏）
pub const BROADCAST_CHANNEL_CAPACITY: usize = 8192;

/// WebSocket 接收任务轮询间隔（毫秒）
pub const RECEIVER_POLL_INTERVAL_MS: u64 = 50;

/// WebSocket 发送任务轮询间隔（毫秒）
pub const SENDER_POLL_INTERVAL_MS: u64 = 10;

/// 事件转发器轮询间隔（毫秒）
pub const EVENT_FORWARDER_POLL_INTERVAL_MS: u64 = 100;

/// 日志预览最大长度（字符数）
///
/// 发送/接收日志截断到此长度，避免日志刷屏
pub const LOG_PREVIEW_MAX_LEN: usize = 500;

/// 默认心跳间隔（秒）
pub const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// 默认消息队列大小
pub const DEFAULT_MESSAGE_QUEUE_SIZE: usize = 256;

/// 默认连接超时（毫秒）
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 10000;

/// 连接建立后稳定等待时间（毫秒）
///
/// WebSocket 握手成功后短暂等待，确保底层通道就绪
pub const CONNECTION_STABILIZE_DELAY_MS: u64 = 100;

/// 断开连接时等待任务结束的超时（秒）
pub const DISCONNECT_TASK_TIMEOUT_SECS: u64 = 3;

/// 客户端模式占位地址
///
/// 移动端作为 WS 客户端无真实对端地址，使用此占位符
pub const PLACEHOLDER_CLIENT_ADDR: &str = "0.0.0.0:0";

/// WebSocket 默认路径（WsClientConfig 默认值）
pub const WS_DEFAULT_PATH: &str = "/";

/// 移动端连接桌面端的 WebSocket 路径
pub const WS_TERMINAL_PATH: &str = "/ws/terminal";
