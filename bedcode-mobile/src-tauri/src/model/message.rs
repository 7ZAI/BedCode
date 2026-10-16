//! WebSocket Message Types
//!
//! 统一的业务消息类型，作为 WebSocket 客户端和服务端的业务传输类型
//!
//! # 票 04：信封协议退役（P3 / M3）
//!
//! 会话控制 / 终端输入 / 配置查询已迁桌面 HTTP 面，WS 上的业务 `Message`
//! 信封只剩**测试与协议级收尾**用途（`ws_protocol_integration` 的 legacy 场景
//! 驱动真实 WsClient→router→handler 链路）。本枚举裁剪为实际仍有消费者的
//! 变体：
//!
//! | 变体 | 保留理由 |
//! | --- | --- |
//! | `Auth` | `connect_and_pair` 等集成测试的 WS 首消息 JWT 认证（`AuthRequest`） |
//! | `SessionControl` | 集成测试的请求-响应匹配（`send_and_wait` + Ack 回包） |
//! | `Error` | codec 对 Close 帧 / 畸形帧的显性表达；`SystemHandler` 路由 |
//! | `ServerClosed` | 服务端主动断开通知（`SystemHandler`） |
//! | `Ack` | 请求-响应确认（`request_response` 按 `request_id` 匹配） |
//!
//! 已退役删除：`Terminal` / `SessionConfig` / `ClientDisconnected` /
//! `SessionEvent` / `SyncData`（WS 生产路径零使用；`rg Message::` 兜底见
//! `handler/plugin_event.rs` 结构锁）。

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::enums::auth::AuthPayload;
use crate::enums::control::{SessionControlAction, SessionControlPayload};

// ==================== Ack 响应代码常量 ====================

/// Ack 成功响应代码
pub const ACK_CODE_SUCCESS: u16 = 0;

/// Ack 失败响应代码 - 通用错误
pub const ACK_CODE_FAILURE: u16 = 1;

/// Ack 失败响应代码 - 认证失败
pub const ACK_CODE_AUTH_FAILED: u16 = 1001;

/// Ack 失败响应代码 - 会话不存在
pub const ACK_CODE_SESSION_NOT_FOUND: u16 = 1002;

/// Ack 失败响应代码 - 无效请求
pub const ACK_CODE_INVALID_REQUEST: u16 = 1003;

/// Ack 失败响应代码 - 操作超时
pub const ACK_CODE_TIMEOUT: u16 = 1004;

/// 生成唯一消息ID
pub(crate) fn generate_message_id() -> String {
    Uuid::new_v4().to_string()
}

/// 默认返回空字符串
fn default_token() -> String {
    String::new()
}

/// 统一的 WebSocket 消息类型
/// 作为 WebSocket 客户端和服务端的业务传输类型
/// 直接对应 JSON 序列化的结构
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Message {
    // ==================== 业务消息类型 ====================
    /// 认证消息 (双向)
    #[serde(rename = "auth")]
    Auth {
        #[serde(default = "generate_message_id")]
        message_id: String,
        #[serde(default)]
        expect_response: bool,
        timestamp: i64,
        session_id: Option<String>,
        /// 认证令牌
        #[serde(default = "default_token")]
        token: String,
        payload: AuthPayload,
    },

    /// 会话控制消息 (双向)
    /// 会话生命周期管理：启动/停止/删除等
    #[serde(rename = "session_control")]
    SessionControl {
        #[serde(default = "generate_message_id")]
        message_id: String,
        #[serde(default)]
        expect_response: bool,
        timestamp: i64,
        session_id: Option<String>,
        /// 认证令牌
        #[serde(default = "default_token")]
        token: String,
        payload: SessionControlPayload,
    },

    /// 错误消息 (服务端 → 客户端)
    #[serde(rename = "error")]
    Error {
        /// 关联的消息ID（如果有）
        #[serde(skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        /// 是否需要服务端响应
        #[serde(default)]
        expect_response: bool,
        timestamp: i64,
        /// 认证令牌
        #[serde(default = "default_token")]
        token: String,
        code: String,
        message: String,
    },

    /// 服务端关闭通知 (服务端 → 客户端)
    /// 桌面端退出时通知所有移动端连接已断开
    #[serde(rename = "server_closed")]
    ServerClosed {
        /// 关闭原因
        reason: String,
        /// 是否会重连（目前桌面端退出后不会重连）
        will_reconnect: bool,
        /// 认证令牌
        #[serde(default = "default_token")]
        token: String,
    },

    /// 确认响应 (服务端 → 客户端)
    /// 当 expect_response=true 但 handler 无具体返回值时的默认响应
    /// 表示消息已收到并处理
    #[serde(rename = "ack")]
    Ack {
        /// 关联的请求消息ID
        request_id: String,
        /// 时间戳（毫秒）
        timestamp: i64,
        /// 响应代码：0 表示成功，非 0 表示失败
        /// 使用 ACK_CODE_* 常量
        code: u16,
        /// 可选的错误消息，失败时应提供
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// 认证令牌
        #[serde(default = "default_token")]
        token: String,
    },
}

// ==================== 辅助方法 ====================

impl Message {
    /// 创建会话控制消息
    pub fn session_control(action: SessionControlAction, session_id: Option<&str>) -> Self {
        Message::SessionControl {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.map(|s| s.to_string()),
            token: String::new(),
            payload: SessionControlPayload { action },
        }
    }

    /// 创建会话控制消息（带响应期望）
    pub fn session_control_with_response(action: SessionControlAction, session_id: Option<&str>) -> Self {
        Message::SessionControl {
            message_id: generate_message_id(),
            expect_response: true,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.map(|s| s.to_string()),
            token: String::new(),
            payload: SessionControlPayload { action },
        }
    }

    /// 创建认证消息
    pub fn auth(session_id: Option<String>, payload: AuthPayload) -> Self {
        Message::Auth {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id,
            token: String::new(),
            payload,
        }
    }

    /// 创建错误消息
    pub fn error(code: &str, message: &str) -> Self {
        Message::Error {
            message_id: None,
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            token: String::new(),
            code: code.to_string(),
            message: message.to_string(),
        }
    }

    /// 创建错误消息（关联到特定消息ID）
    pub fn error_with_id(message_id: &str, code: &str, message: &str) -> Self {
        Message::Error {
            message_id: Some(message_id.to_string()),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            token: String::new(),
            code: code.to_string(),
            message: message.to_string(),
        }
    }

    /// 创建服务端关闭消息
    pub fn server_closed(reason: &str, will_reconnect: bool) -> Self {
        Message::ServerClosed {
            reason: reason.to_string(),
            will_reconnect,
            token: String::new(),
        }
    }

    /// 创建确认响应消息（成功）
    /// 当 expect_response=true 但 handler 无具体返回值时使用
    pub fn ack(request_id: &str) -> Self {
        Message::Ack {
            request_id: request_id.to_string(),
            timestamp: Utc::now().timestamp_millis(),
            code: ACK_CODE_SUCCESS,
            message: None,
            token: String::new(),
        }
    }

    /// 创建确认响应消息（失败）
    pub fn ack_failure(request_id: &str, code: u16, message: &str) -> Self {
        Message::Ack {
            request_id: request_id.to_string(),
            timestamp: Utc::now().timestamp_millis(),
            code,
            message: Some(message.to_string()),
            token: String::new(),
        }
    }

    /// 获取消息ID
    pub fn message_id(&self) -> Option<&str> {
        match self {
            Message::Auth { message_id, .. } => Some(message_id),
            Message::SessionControl { message_id, .. } => Some(message_id),
            Message::Error { message_id, .. } => message_id.as_deref(),
            Message::ServerClosed { .. } => None,
            Message::Ack { .. } => None,
        }
    }

    /// 获取消息类型名称（用于调试日志）
    pub fn message_type(&self) -> Option<&'static str> {
        match self {
            Message::Auth { .. } => Some("auth"),
            Message::SessionControl { .. } => Some("session_control"),
            Message::Error { .. } => Some("error"),
            Message::ServerClosed { .. } => Some("server_closed"),
            Message::Ack { .. } => Some("ack"),
        }
    }

    /// 获取 expect_response 标记
    pub fn expect_response(&self) -> bool {
        match self {
            Message::Auth { expect_response, .. } => *expect_response,
            Message::SessionControl { expect_response, .. } => *expect_response,
            Message::Error { expect_response, .. } => *expect_response,
            Message::ServerClosed { .. } => false,
            Message::Ack { .. } => false,
        }
    }

    /// 获取 token
    pub fn token(&self) -> &str {
        match self {
            Message::Auth { token, .. } => token,
            Message::SessionControl { token, .. } => token,
            Message::Error { token, .. } => token,
            Message::ServerClosed { token, .. } => token,
            Message::Ack { token, .. } => token,
        }
    }

    /// 设置响应消息的关联 ID（用于请求-响应跟踪）
    /// 仅对支持响应关联的消息类型有效
    pub fn with_request_id(self, request_id: &str) -> Self {
        match self {
            Message::Auth {
                message_id: _,
                expect_response,
                timestamp,
                session_id,
                token,
                payload,
            } => Message::Auth {
                message_id: request_id.to_string(),
                expect_response,
                timestamp,
                session_id,
                token,
                payload,
            },
            Message::SessionControl {
                message_id: _,
                expect_response,
                timestamp,
                session_id,
                token,
                payload,
            } => Message::SessionControl {
                message_id: request_id.to_string(),
                expect_response,
                timestamp,
                session_id,
                token,
                payload,
            },
            Message::Error {
                message_id: _,
                expect_response,
                timestamp,
                token,
                code,
                message,
            } => Message::Error {
                message_id: Some(request_id.to_string()),
                expect_response,
                timestamp,
                token,
                code,
                message,
            },
            // 其他类型不支持设置 request_id，直接返回
            other => other,
        }
    }

    /// 设置 token
    pub fn with_token(self, token: &str) -> Self {
        match self {
            Message::Auth {
                message_id,
                expect_response,
                timestamp,
                session_id,
                payload,
                ..
            } => Message::Auth {
                message_id,
                expect_response,
                timestamp,
                session_id,
                token: token.to_string(),
                payload,
            },
            Message::SessionControl {
                message_id,
                expect_response,
                timestamp,
                session_id,
                payload,
                ..
            } => Message::SessionControl {
                message_id,
                expect_response,
                timestamp,
                session_id,
                token: token.to_string(),
                payload,
            },
            Message::Error {
                message_id,
                expect_response,
                timestamp,
                code,
                message,
                ..
            } => Message::Error {
                message_id,
                expect_response,
                timestamp,
                token: token.to_string(),
                code,
                message,
            },
            Message::ServerClosed {
                reason, will_reconnect, ..
            } => Message::ServerClosed {
                reason,
                will_reconnect,
                token: token.to_string(),
            },
            Message::Ack {
                request_id,
                timestamp,
                code,
                message,
                ..
            } => Message::Ack {
                request_id,
                timestamp,
                code,
                message,
                token: token.to_string(),
            },
        }
    }

    /// 序列化为 JSON
    pub fn to_json(&self) -> crate::Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    /// 从 JSON 反序列化
    pub fn from_json(json: &str) -> crate::Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    /// 转换为 WebSocket 原生消息
    pub fn to_ws_message(&self) -> crate::Result<tokio_tungstenite::tungstenite::Message> {
        let json = self.to_json()?;
        Ok(tokio_tungstenite::tungstenite::Message::Text(json))
    }

    /// 从 WebSocket 原生消息转换
    pub fn from_ws_message(msg: tokio_tungstenite::tungstenite::Message) -> crate::Result<Option<Self>> {
        match msg {
            tokio_tungstenite::tungstenite::Message::Text(text) => Ok(Some(serde_json::from_str(&text)?)),
            tokio_tungstenite::tungstenite::Message::Binary(data) => {
                let text = String::from_utf8_lossy(&data);
                Ok(Some(serde_json::from_str(&text)?))
            }
            tokio_tungstenite::tungstenite::Message::Ping(_) => Ok(None), // 协议层心跳由 tungstenite 自动处理
            tokio_tungstenite::tungstenite::Message::Pong(_) => Ok(None),
            tokio_tungstenite::tungstenite::Message::Close(reason) => {
                // 注意：tungstenite 的 CloseFrame Display 会追加关闭码（"reason (code)"），
                // 这里只取 reason 字段，避免关闭码混入错误消息
                Ok(Some(Message::error(
                    "close",
                    &reason.map(|r| r.reason.to_string()).unwrap_or_default(),
                )))
            }
            tokio_tungstenite::tungstenite::Message::Frame(_) => Ok(None),
        }
    }
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    use super::*;
    use crate::enums::AuthStage;

    /// 固定时间戳（毫秒），避免依赖系统时钟，使精确 JSON 断言可复现
    const FIXED_TS: i64 = 1_700_000_000_000;

    // ==================== 构造器测试 ====================

    #[test]
    fn test_session_control_variants() {
        // 带 session_id
        let msg = Message::session_control(SessionControlAction::ListSessions, Some("s1"));
        assert_eq!(msg.message_type(), Some("session_control"));
        assert!(!msg.expect_response());
        match msg {
            Message::SessionControl {
                session_id,
                payload: SessionControlPayload { action },
                ..
            } => {
                assert_eq!(session_id.as_deref(), Some("s1"));
                assert!(matches!(action, SessionControlAction::ListSessions));
            }
            _ => panic!(),
        }

        // 无 session_id
        let msg = Message::session_control(
            SessionControlAction::StopSession {
                session_id: "s9".into(),
            },
            None,
        );
        match msg {
            Message::SessionControl { session_id, .. } => assert_eq!(session_id, None),
            _ => panic!(),
        }

        // 带响应期望
        let msg = Message::session_control_with_response(SessionControlAction::ListSessions, None);
        assert!(msg.expect_response());
    }

    #[test]
    fn test_auth_constructor() {
        // JWT 重认证阶段，携带设备信息与 session_token（旧 WS 配对 stage 已删）
        let payload = AuthPayload {
            stage: AuthStage::Reauthenticate,
            device_id: Some("dev-1".to_string()),
            device_name: Some("phone".to_string()),
            session_token: Some("jwt-1".to_string()),
            ..Default::default()
        };
        let msg = Message::auth(Some("s1".to_string()), payload.clone());
        assert_eq!(msg.message_type(), Some("auth"));
        assert!(!msg.expect_response());
        assert!(msg.message_id().is_some_and(|id| !id.is_empty()));
        match msg {
            Message::Auth {
                session_id,
                payload: got,
                ..
            } => {
                assert_eq!(session_id.as_deref(), Some("s1"));
                assert_eq!(got.stage, AuthStage::Reauthenticate);
                assert_eq!(got.device_id.as_deref(), Some("dev-1"));
                assert_eq!(got.device_name.as_deref(), Some("phone"));
                assert_eq!(got.session_token.as_deref(), Some("jwt-1"));
            }
            _ => panic!(),
        }

        // 会话无关的认证（无 session_id）
        let msg = Message::auth(None, payload);
        match msg {
            Message::Auth { session_id, .. } => assert_eq!(session_id, None),
            _ => panic!(),
        }
    }

    #[test]
    fn test_error_constructors() {
        // 无关联请求 ID
        let msg = Message::error("ERR_TIMEOUT", "operation timed out");
        assert_eq!(msg.message_type(), Some("error"));
        assert_eq!(msg.message_id(), None);
        assert!(!msg.expect_response());
        match msg {
            Message::Error { code, message, .. } => {
                assert_eq!(code, "ERR_TIMEOUT");
                assert_eq!(message, "operation timed out");
            }
            _ => panic!(),
        }

        // 关联请求 ID
        let msg = Message::error_with_id("req-9", "ERR", "boom");
        assert_eq!(msg.message_id(), Some("req-9"));
        match msg {
            Message::Error { code, message, .. } => {
                assert_eq!(code, "ERR");
                assert_eq!(message, "boom");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_server_closed_constructor() {
        let msg = Message::server_closed("host exiting", true);
        assert_eq!(msg.message_type(), Some("server_closed"));
        assert_eq!(msg.message_id(), None);
        assert!(!msg.expect_response());
        match msg {
            Message::ServerClosed {
                reason,
                will_reconnect,
                token,
            } => {
                assert_eq!(reason, "host exiting");
                assert!(will_reconnect);
                assert_eq!(token, "");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_ack_constructors() {
        // 成功：code=0 且不带 message
        let msg = Message::ack("req-1");
        assert_eq!(msg.message_type(), Some("ack"));
        assert_eq!(msg.message_id(), None);
        assert!(!msg.expect_response());
        match msg {
            Message::Ack {
                request_id,
                code,
                message,
                ..
            } => {
                assert_eq!(request_id, "req-1");
                assert_eq!(code, ACK_CODE_SUCCESS);
                assert_eq!(message, None);
            }
            _ => panic!(),
        }

        // 失败：非 0 code 且带错误信息
        let msg = Message::ack_failure("req-1", ACK_CODE_TIMEOUT, "timed out");
        match msg {
            Message::Ack {
                request_id,
                code,
                message,
                ..
            } => {
                assert_eq!(request_id, "req-1");
                assert_eq!(code, ACK_CODE_TIMEOUT);
                assert_eq!(message.as_deref(), Some("timed out"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_ack_code_constants() {
        // 协议常量是移动端与桌面端互通的公共契约，锁死取值防止误改
        assert_eq!(ACK_CODE_SUCCESS, 0);
        assert_eq!(ACK_CODE_FAILURE, 1);
        assert_eq!(ACK_CODE_AUTH_FAILED, 1001);
        assert_eq!(ACK_CODE_SESSION_NOT_FOUND, 1002);
        assert_eq!(ACK_CODE_INVALID_REQUEST, 1003);
        assert_eq!(ACK_CODE_TIMEOUT, 1004);
    }

    // ==================== 访问器测试 ====================

    #[test]
    fn test_message_type_all_variants() {
        // 逐一锁死 wire 上的 type 标签，防止 serde rename 被误改导致协议不兼容
        let cases = vec![
            (
                Message::Auth {
                    message_id: "m".into(),
                    expect_response: false,
                    timestamp: FIXED_TS,
                    session_id: None,
                    token: String::new(),
                    payload: AuthPayload::default(),
                },
                Some("auth"),
            ),
            (
                Message::SessionControl {
                    message_id: "m".into(),
                    expect_response: false,
                    timestamp: FIXED_TS,
                    session_id: None,
                    token: String::new(),
                    payload: SessionControlPayload {
                        action: SessionControlAction::ListSessions,
                    },
                },
                Some("session_control"),
            ),
            (
                Message::Error {
                    message_id: None,
                    expect_response: false,
                    timestamp: FIXED_TS,
                    token: String::new(),
                    code: "E".into(),
                    message: "m".into(),
                },
                Some("error"),
            ),
            (Message::server_closed("r", false), Some("server_closed")),
            (Message::ack("r"), Some("ack")),
        ];
        for (msg, expected) in cases {
            assert_eq!(msg.message_type(), expected);
        }
    }

    #[test]
    fn test_message_id_none_variants() {
        // 无消息 ID 语义的变体（通知/响应类）必须返回 None
        assert_eq!(Message::server_closed("r", false).message_id(), None);
        assert_eq!(Message::ack("r").message_id(), None);
    }

    #[test]
    fn test_expect_response_none_variants() {
        // 通知/响应类变体没有 expect_response 概念，恒为 false
        assert!(!Message::server_closed("r", false).expect_response());
        assert!(!Message::ack("r").expect_response());
    }

    #[test]
    fn test_message_id_is_uuid_v4() {
        // message_id 是 UUID v4 字符串（36 字符，4 个连字符）
        let id = generate_message_id();
        assert_eq!(id.len(), 36);
        assert_eq!(id.chars().filter(|&c| c == '-').count(), 4);
        assert!(Uuid::parse_str(&id).is_ok());
        // 两次生成不应相同
        assert_ne!(generate_message_id(), generate_message_id());
    }

    #[test]
    fn test_constructor_timestamp_near_now() {
        // 构造器时间戳应为当前毫秒级时间，与系统时钟偏差在 60 秒内
        let before = Utc::now().timestamp_millis();
        let msg = Message::session_control(SessionControlAction::ListSessions, None);
        let after = Utc::now().timestamp_millis();
        let Message::SessionControl { timestamp, .. } = msg else {
            panic!()
        };
        assert!(timestamp >= before, "timestamp {timestamp} < before {before}");
        assert!(timestamp <= after, "timestamp {timestamp} > after {after}");
    }

    #[test]
    fn test_token_accessor_and_with_token() {
        // with_token 覆盖 token，token() 读回
        let msg = Message::auth(None, AuthPayload::default()).with_token("tok-1");
        assert_eq!(msg.token(), "tok-1");

        let msg = Message::server_closed("r", false).with_token("tok-2");
        assert_eq!(msg.token(), "tok-2");

        let msg = Message::ack("r").with_token("tok-3");
        assert_eq!(msg.token(), "tok-3");

        // 未设置时为空字符串
        assert_eq!(Message::ack("r").token(), "");
    }

    #[test]
    fn test_with_request_id_supported_variants() {
        // 请求类变体：message_id 被替换为 request_id
        for msg in [
            Message::auth(None, AuthPayload::default()),
            Message::session_control(SessionControlAction::ListSessions, None),
        ] {
            let id = msg.message_id().unwrap().to_string();
            let rewritten = msg.with_request_id("rid-1");
            assert_eq!(rewritten.message_id(), Some("rid-1"));
            // 原 ID 未被复用（无副作用）
            assert_ne!(rewritten.message_id(), Some(id.as_str()));
        }

        // Error 变体：从 None 变成 Some(request_id)
        let msg = Message::error("E", "m").with_request_id("rid-2");
        assert_eq!(msg.message_id(), Some("rid-2"));
    }

    #[test]
    fn test_with_request_id_unsupported_returns_unchanged() {
        // 非请求类变体不支持关联，原样返回
        let msg = Message::ack("req-1").with_request_id("rid-x");
        match msg {
            Message::Ack { request_id, .. } => assert_eq!(request_id, "req-1"),
            _ => panic!(),
        }
        let msg = Message::server_closed("r", false).with_request_id("rid-x");
        assert_eq!(msg.message_id(), None);
    }

    #[test]
    fn test_request_id_correlation_with_ack() {
        // 请求（with_request_id）与 ack 通过 request_id 配对
        let req =
            Message::session_control_with_response(SessionControlAction::ListSessions, None).with_request_id("req-abc");
        let ack = Message::ack("req-abc");
        match ack {
            Message::Ack { request_id, .. } => assert_eq!(request_id, req.message_id().unwrap()),
            _ => panic!(),
        }
        // 错误响应同样回填请求 ID
        assert_eq!(
            Message::error_with_id("req-abc", "E", "m").message_id(),
            Some("req-abc")
        );
    }

    // ==================== 序列化测试 ====================

    #[test]
    fn test_to_json_auth_exact() {
        // AuthPayload::default() 的 stage 为 failed（占位，旧 request_pairing 已删），
        // 可选字段全部省略；session_id 为 None 时无 skip 属性 → 序列化为 null
        let msg = Message::Auth {
            message_id: "m-002".to_string(),
            expect_response: false,
            timestamp: FIXED_TS,
            session_id: None,
            token: "".to_string(),
            payload: AuthPayload::default(),
        };
        assert_eq!(
            msg.to_json().unwrap(),
            "{\"type\":\"auth\",\"payload\":{\"message_id\":\"m-002\",\"expect_response\":false,\"timestamp\":1700000000000,\"session_id\":null,\"token\":\"\",\"payload\":{\"stage\":\"failed\"}}}"
        );
    }

    #[test]
    fn test_to_json_session_control_exact() {
        let msg = Message::SessionControl {
            message_id: "m-003".to_string(),
            expect_response: false,
            timestamp: FIXED_TS,
            session_id: None,
            token: "".to_string(),
            payload: SessionControlPayload {
                action: SessionControlAction::StartSession {
                    config_id: "cfg-1".into(),
                },
            },
        };
        assert_eq!(
            msg.to_json().unwrap(),
            "{\"type\":\"session_control\",\"payload\":{\"message_id\":\"m-003\",\"expect_response\":false,\"timestamp\":1700000000000,\"session_id\":null,\"token\":\"\",\"payload\":{\"action\":{\"type\":\"start_session\",\"config_id\":\"cfg-1\"}}}}"
        );
    }

    #[test]
    fn test_to_json_error_and_ack_exact() {
        // Error：message_id=None 时字段整体省略（skip_serializing_if）
        let msg = Message::Error {
            message_id: None,
            expect_response: false,
            timestamp: FIXED_TS,
            token: "".to_string(),
            code: "ERR".to_string(),
            message: "boom".to_string(),
        };
        assert_eq!(
            msg.to_json().unwrap(),
            "{\"type\":\"error\",\"payload\":{\"expect_response\":false,\"timestamp\":1700000000000,\"token\":\"\",\"code\":\"ERR\",\"message\":\"boom\"}}"
        );

        // Error：message_id=Some 时字段出现
        let msg = Message::Error {
            message_id: Some("req-9".to_string()),
            expect_response: false,
            timestamp: FIXED_TS,
            token: "".to_string(),
            code: "ERR".to_string(),
            message: "boom".to_string(),
        };
        assert_eq!(
            msg.to_json().unwrap(),
            "{\"type\":\"error\",\"payload\":{\"message_id\":\"req-9\",\"expect_response\":false,\"timestamp\":1700000000000,\"token\":\"\",\"code\":\"ERR\",\"message\":\"boom\"}}"
        );

        // Ack：message=None 时省略
        let msg = Message::Ack {
            request_id: "req-001".to_string(),
            timestamp: FIXED_TS,
            code: ACK_CODE_SUCCESS,
            message: None,
            token: "".to_string(),
        };
        assert_eq!(
            msg.to_json().unwrap(),
            "{\"type\":\"ack\",\"payload\":{\"request_id\":\"req-001\",\"timestamp\":1700000000000,\"code\":0,\"token\":\"\"}}"
        );
    }

    #[test]
    fn test_to_json_ack_failure_exact() {
        // Ack：message=None 时省略；带 message 时出现
        let msg = Message::Ack {
            request_id: "req-001".to_string(),
            timestamp: FIXED_TS,
            code: ACK_CODE_TIMEOUT,
            message: Some("timeout".to_string()),
            token: "".to_string(),
        };
        assert_eq!(
            msg.to_json().unwrap(),
            "{\"type\":\"ack\",\"payload\":{\"request_id\":\"req-001\",\"timestamp\":1700000000000,\"code\":1004,\"message\":\"timeout\",\"token\":\"\"}}"
        );
    }

    #[test]
    fn test_from_json_applies_defaults() {
        // 旧端/简化端可省略 message_id/expect_response/token，反序列化必须兜底
        let json = r#"{"type":"session_control","payload":{"timestamp":1700000000000,"session_id":"s1","payload":{"action":{"type":"list_sessions"}}}}"#;
        let msg = Message::from_json(json).unwrap();
        assert!(msg.message_id().is_some_and(|id| !id.is_empty()));
        assert!(!msg.expect_response());
        assert_eq!(msg.token(), "");
    }

    #[test]
    fn test_serde_roundtrip_all_variants() {
        // 全部 5 个变体：to_value → from_value → to_value 必须保持一致
        let variants = vec![
            Message::Auth {
                message_id: "m2".into(),
                expect_response: false,
                timestamp: FIXED_TS,
                session_id: Some("s1".into()),
                token: "tk".into(),
                payload: AuthPayload {
                    stage: AuthStage::Reauthenticate,
                    session_token: Some("jwt-1".into()),
                    ..Default::default()
                },
            },
            Message::SessionControl {
                message_id: "m3".into(),
                expect_response: false,
                timestamp: FIXED_TS,
                session_id: None,
                token: "".into(),
                payload: SessionControlPayload {
                    action: SessionControlAction::ListSessions,
                },
            },
            Message::Error {
                message_id: Some("m5".into()),
                expect_response: false,
                timestamp: FIXED_TS,
                token: "".into(),
                code: "E1".into(),
                message: "err".into(),
            },
            Message::ServerClosed {
                reason: "bye".into(),
                will_reconnect: false,
                token: "".into(),
            },
            Message::Ack {
                request_id: "r1".into(),
                timestamp: FIXED_TS,
                code: ACK_CODE_SUCCESS,
                message: None,
                token: "".into(),
            },
        ];
        for v in variants {
            let value = serde_json::to_value(&v).unwrap();
            let back: Message = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(&back).unwrap(), value);
        }
    }

    #[test]
    fn test_to_json_from_json_roundtrip() {
        // 序列化 → 反序列化 → 再序列化，JSON 必须逐字节一致
        let variants = vec![
            Message::session_control(
                SessionControlAction::StopSession {
                    session_id: "s1".into(),
                },
                Some("s1"),
            ),
            Message::auth(
                None,
                AuthPayload {
                    stage: AuthStage::Authenticated,
                    session_token: Some("jwt-token".into()),
                    ..Default::default()
                },
            ),
            Message::ack_failure("req-8", ACK_CODE_AUTH_FAILED, "auth failed"),
            Message::server_closed("bye", false),
            Message::error_with_id("req-9", "E", "boom"),
        ];
        for v in variants {
            let json = v.to_json().unwrap();
            let back = Message::from_json(&json).unwrap();
            assert_eq!(back.to_json().unwrap(), json);
        }
    }

    // ==================== WebSocket 消息转换测试 ====================

    #[test]
    fn test_from_ws_message_text_and_binary() {
        let json = Message::server_closed("host exiting", false).to_json().unwrap();

        // Text 帧解析
        let parsed = Message::from_ws_message(WsMessage::Text(json.clone()))
            .unwrap()
            .unwrap();
        assert_eq!(parsed.message_type(), Some("server_closed"));
        assert_eq!(parsed.to_json().unwrap(), json);

        // Binary 帧按 UTF-8 解析
        let parsed = Message::from_ws_message(WsMessage::Binary(json.clone().into_bytes()))
            .unwrap()
            .unwrap();
        assert_eq!(parsed.to_json().unwrap(), json);
    }

    #[test]
    fn test_from_ws_message_control_frames_return_none() {
        // 心跳/帧等协议控制帧不应产生业务消息
        assert!(Message::from_ws_message(WsMessage::Ping(vec![].into()))
            .unwrap()
            .is_none());
        assert!(Message::from_ws_message(WsMessage::Pong(vec![].into()))
            .unwrap()
            .is_none());
        assert!(Message::from_ws_message(WsMessage::Frame(
            tokio_tungstenite::tungstenite::protocol::frame::Frame::ping(vec![])
        ))
        .unwrap()
        .is_none());
    }

    #[test]
    fn test_from_ws_message_close_reason() {
        // Close 帧转换为 error 消息（code=close），reason 透传纯文本（不含关闭码）
        let frame = CloseFrame {
            code: CloseCode::Normal,
            reason: Cow::Owned("going away".into()),
        };
        let msg = Message::from_ws_message(WsMessage::Close(Some(frame)))
            .unwrap()
            .unwrap();
        assert_eq!(msg.message_type(), Some("error"));
        match msg {
            Message::Error { code, message, .. } => {
                assert_eq!(code, "close");
                assert_eq!(message, "going away");
            }
            _ => panic!(),
        }

        // 无原因的 Close 帧 → 空字符串
        let msg = Message::from_ws_message(WsMessage::Close(None)).unwrap().unwrap();
        match msg {
            Message::Error { message, .. } => assert_eq!(message, ""),
            _ => panic!(),
        }
    }

    #[test]
    fn test_to_ws_message_roundtrip() {
        // 业务消息 → Text 帧 → 业务消息，内容无损
        let msg =
            Message::session_control_with_response(SessionControlAction::ListSessions, None).with_request_id("req-77");
        let ws = msg.to_ws_message().unwrap();
        match ws {
            WsMessage::Text(text) => {
                let back = Message::from_ws_message(WsMessage::Text(text)).unwrap().unwrap();
                assert_eq!(back.to_json().unwrap(), msg.to_json().unwrap());
            }
            _ => panic!("expected Text frame"),
        }
    }
}
