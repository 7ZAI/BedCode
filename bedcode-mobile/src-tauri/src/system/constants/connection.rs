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

// 注：原 `WS_PLUGIN_TERMINAL_PATH`（终端流端点 `/ws/plugin/com.bedcode.terminal-session/terminal`）
// 已随票 12 终端订阅协议客户端迁插件删除——端点 URL 的拼装真源在终端插件
// （`plugins/terminal-session/rust/src/lib.rs` 的 TERMINAL_WS_PATH），宿主零消费。

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

/// 认证类 WS 关闭码（桌面端语义，M1/ADR 0031）：4001 = 认证被拒（auth-policy
/// 拒绝/无中心 fail-closed）、4003 = 链路加密失败。收到这类 close 说明「重连前
/// 需重新配对/认证」，自愈重连无意义——ConnMonitor 与自愈监督按致命处理：
/// 不自愈、只发一次提示。非致命 code（1005/1006/网络断开等）走既有自愈路径。
pub const WS_AUTH_FATAL_CLOSE_CODES: &[u16] = &[4001, 4003];

/// 判定 WS 关闭码是否认证类致命（M1）
pub fn is_auth_fatal_close_code(code: u16) -> bool {
    WS_AUTH_FATAL_CLOSE_CODES.contains(&code)
}

/// 协议层不可重试关闭码（RFC 6455 §7.4.1）
///
/// 语义：**用相同参数重连必然得到相同结果**，重试只是浪费往返，并把真实原因
/// （两端版本 / 协议不匹配）伪装成网络抖动。与认证类致命的区别是**处置不同**
/// ——不是「重新配对」能救，而是「升级一端」才能救，故单列一档而不并入
/// `WS_AUTH_FATAL_CLOSE_CODES`。
///
/// | 码 | RFC 名称 | 为什么重试无用 |
/// | --- | --- | --- |
/// | 1002 | Protocol error | 对端按协议解析本端帧失败，重发同样的帧仍失败 |
/// | 1003 | Unsupported Data | 本端发了对端声明不支持的数据类型 |
/// | 1007 | Invalid frame payload | 负载不是对端接受的编码（如非 UTF-8 文本） |
/// | 1008 | Policy Violation | 对端按策略拒收该消息，重发同一消息仍被拒 |
/// | 1009 | Message Too Big | 消息超出对端上限，缩小内容才可能成功 |
/// | 1010 | Mandatory Ext | 缺少对端要求的扩展，协商不出即永远失败 |
///
/// **不在此列**（仍属可重试）：1000 正常关闭 / 1001 Going Away（服务端优雅关闭、
/// 滚动升级）/ 1011 Internal Error（服务端崩溃）/ 1012 Service Restart /
/// 1013 Try Again Later（过载，应加长退避而非放弃）。
pub const WS_NON_RETRYABLE_CLOSE_CODES: &[u16] = &[1002, 1003, 1007, 1008, 1009, 1010];

/// 判定 WS 关闭码是否属于「重试无用」类（协议/策略层不可自愈）
///
/// 与 [`is_auth_fatal_close_code`] 互斥；两者都**优先于**可重试判定。
pub fn is_non_retryable_close_code(code: u16) -> bool {
    WS_NON_RETRYABLE_CLOSE_CODES.contains(&code)
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 行为契约（RFC 6455 §7.4.1 + 2026-10-04 审计 P1-4）：
    /// 关闭码分三档，且后两档**互斥**；判错的后果是「白重连」或「本可自愈
    /// 却放弃」，两个方向都会让用户看到假的网络故障。
    ///
    /// 档位：① auth-fatal（需重新配对）② non-retryable（需升级一端）③ 其余可重连
    #[test]
    fn test_auth_fatal_close_codes_are_recognized() {
        // 正例：桌面端两个致命码
        assert!(is_auth_fatal_close_code(4001));
        assert!(is_auth_fatal_close_code(4003));
    }

    #[test]
    fn test_auth_fatal_and_non_retryable_are_mutually_exclusive() {
        // 反例：两档不得重叠，否则同一 close 会同时拿到「重新配对」和
        // 「升级版本」两种互斥提示文案
        for code in WS_AUTH_FATAL_CLOSE_CODES {
            assert!(!is_non_retryable_close_code(*code), "关闭码 {} 同时落入两档", code);
        }
        for code in WS_NON_RETRYABLE_CLOSE_CODES {
            assert!(!is_auth_fatal_close_code(*code), "关闭码 {} 同时落入两档", code);
        }
    }

    #[test]
    fn test_non_retryable_protocol_codes_are_recognized() {
        // 正例：协议/策略层六个码——用相同参数重连必然同样失败
        for code in [1002u16, 1003, 1007, 1008, 1009, 1010] {
            assert!(is_non_retryable_close_code(code), "{} 应判为不可重试", code);
            assert!(!is_auth_fatal_close_code(code));
        }
    }

    #[test]
    fn test_server_side_and_transient_codes_stay_retryable() {
        // 反例（最关键的一条）：这些码**必须**保持可重连，否则把自愈能力
        // 关掉了。服务端优雅关闭 / 滚动升级 / 崩溃 / 过载都是「等一会就好」，
        // 1006 更是移动网络切换的常态
        for code in [
            1000u16, // 正常关闭
            1001,    // Going Away
            1005,    // 无状态码
            1006,    // 异常断开（TCP 半开 / 网络切换）
            1011,    // 服务端内部错误
            1012,    // 服务重启
            1013,    // Try Again Later（过载）
        ] {
            assert!(
                !is_non_retryable_close_code(code),
                "{} 必须保持可重连（服务端侧/瞬态）",
                code
            );
            assert!(!is_auth_fatal_close_code(code));
        }
    }

    #[test]
    fn test_unknown_business_codes_default_to_retryable() {
        // 反例：未知业务码（如 4002/4999）不得被默认判死——误判会让一次偶发
        // 断开永远不再自愈。fail-open 于重试（不同于认证的 fail-closed）
        for code in [4000u16, 4002, 4004, 4999, 5000] {
            assert!(!is_non_retryable_close_code(code));
            assert!(!is_auth_fatal_close_code(code));
        }
    }
}
