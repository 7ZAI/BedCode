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

/// 终端流端点（票 05：新协议 `/ws/plugin/{plugin-id}/terminal`，对齐桌面插件
/// `ws_terminal.rs` 的订阅/输入/流控/重锚/停止帧；旧 `/ws/terminal/session/{id}`
/// 直连路径已随桌面 WS 硬切退役 → 404）
pub const WS_PLUGIN_TERMINAL_PATH: &str = "/ws/plugin/com.bedcode.terminal-session/terminal";

/// 桌面 wasm 应用 `com.bedcode.terminal-session` 的 WS 端点基础路径
///
/// 宿主路由形如 `/ws/plugin/{plugin-id}/{path}`（桌面 WS 业务硬切后的唯一形态，
/// 旧 `/ws/event` 与 `/ws/terminal/session/{id}` 已删除 → 404）。
/// **WS 路径字面量唯一出处**：新增端点在此追加，各调用点只引常量。
pub const WS_PLUGIN_BASE_PATH: &str = "/ws/plugin/com.bedcode.terminal-session";

/// 常驻事件通道端点（票 03：`session-control`，替代已删除的 `/ws/event`）
///
/// 认证首帧 `{"type":"auth","token":"<jwt>"}`；入站只有事件帧
/// `{"type":"event","event":"<name>","payload":{...}}`（票 02 定稿帧壳）。
pub const WS_PLUGIN_SESSION_CONTROL_PATH: &str = "/ws/plugin/com.bedcode.terminal-session/session-control";
