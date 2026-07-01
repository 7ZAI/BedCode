//! WebSocket Message Types
//!
//! 统一的业务消息类型，作为 WebSocket 客户端和服务端的业务传输类型

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::enums::auth::AuthPayload;
use crate::enums::control::{SessionConfigAction, SessionConfigPayload, SessionControlAction, SessionControlPayload, TerminalAction, TerminalPayload};
use crate::enums::special_key::KeyCombo;
use crate::enums::sumary::SessionSummary;
use crate::enums::SyncPayload;

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

/// 获取当前时间戳（毫秒）
pub(crate) fn current_timestamp() -> i64 {
    Utc::now().timestamp_millis()
}

/// 默认返回 false
fn default_false() -> bool {
    false
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

    /// 终端消息 (双向)
    /// 统一的终端操作类型：输出、输入、订阅、取消订阅等
    #[serde(rename = "terminal")]
    Terminal {
        /// 唯一消息ID，用于请求-响应跟踪
        #[serde(default = "generate_message_id")]
        message_id: String,
        /// 是否需要服务端响应
        #[serde(default)]
        expect_response: bool,
        /// 时间戳（毫秒）
        timestamp: i64,
        /// 会话ID（终端操作必须关联会话）
        session_id: String,
        /// 认证令牌
        #[serde(default = "default_token")]
        token: String,
        /// 终端载荷
        payload: TerminalPayload,
    },

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
    /// 会话生命周期管理：启动/停止/调整大小等
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

    /// 会话配置消息 (双向)
    /// 会话配置查询：列出配置/快捷指令等
    #[serde(rename = "session_config")]
    SessionConfig {
        #[serde(default = "generate_message_id")]
        message_id: String,
        #[serde(default)]
        expect_response: bool,
        timestamp: i64,
        session_id: Option<String>,
        /// 认证令牌
        #[serde(default = "default_token")]
        token: String,
        payload: SessionConfigPayload,
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

    /// 客户端断开通知 (服务端 → 客户端)
    /// 移动端断开连接时通知其他客户端
    #[serde(rename = "client_disconnected")]
    ClientDisconnected {
        /// 断开的设备名称
        device_name: String,
        /// 断开原因
        reason: String,
        /// 认证令牌
        #[serde(default = "default_token")]
        token: String,
    },

    /// 客户端会话变更通知 (服务端 → 客户端)
    /// 移动端创建/停止会话时通知所有客户端
    #[serde(rename = "session_event")]
    SessionEvent {
        /// 事件类型: created, stopped, removed
        event_type: String,
        /// 会话信息
        session: SessionSummary,
        /// 触发设备名称
        device_name: String,
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

    /// 数据同步消息 (服务端 → 客户端)
    /// 用于向客户端推送增量数据变更
    #[serde(rename = "sync_data")]
    SyncData {
        /// 时间戳（毫秒）
        timestamp: i64,
        /// 同步载荷
        payload: SyncPayload,
        /// 认证令牌
        #[serde(default = "default_token")]
        token: String,
    },
}

// ==================== 辅助方法 ====================

impl Message {
    /// 创建终端输出消息
    pub fn output(session_id: &str, data: &[u8], is_waiting: bool, index: usize) -> Self {
        Message::Terminal {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            token: String::new(),
            payload: TerminalPayload {
                action: TerminalAction::Output {
                    data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data),
                    is_waiting,
                    index,
                    end_index: None,
                },
            },
        }
    }

    /// 创建终端输出消息（使用已编码的 Base64 数据）
    /// 用于数据已经经过 Base64 编码的场景（如从 PTY 输出缓冲区转发）
    /// end_index 在合并多条事件时提供结束索引，前端可用其精确更新去重游标
    pub fn output_from_base64(session_id: &str, data_base64: &str, is_waiting: bool, index: usize, end_index: Option<usize>) -> Self {
        Message::Terminal {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            token: String::new(),
            payload: TerminalPayload {
                action: TerminalAction::Output {
                    data: data_base64.to_string(),
                    is_waiting,
                    index,
                    end_index,
                },
            },
        }
    }

    /// 创建终端输入消息
    pub fn input(session_id: &str, data: &str, special_key: Option<KeyCombo>) -> Self {
        Message::Terminal {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            token: String::new(),
            payload: TerminalPayload {
                action: TerminalAction::Input {
                    data: data.to_string(),
                    special_key,
                },
            },
        }
    }

    /// 创建终端输入消息（带响应期望）
    /// 用于需要确认输入已被处理的场景
    pub fn input_with_response(session_id: &str, data: &str, special_key: Option<KeyCombo>) -> Self {
        Message::Terminal {
            message_id: generate_message_id(),
            expect_response: true,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            token: String::new(),
            payload: TerminalPayload {
                action: TerminalAction::Input {
                    data: data.to_string(),
                    special_key,
                },
            },
        }
    }

    /// 创建终端订阅消息
    pub fn subscribe(session_id: &str, start_seq: Option<u64>) -> Self {
        Message::Terminal {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            token: String::new(),
            payload: TerminalPayload {
                action: TerminalAction::Subscribe { start_seq },
            },
        }
    }

    /// 创建终端订阅消息（带响应期望）
    pub fn subscribe_with_response(session_id: &str, start_seq: Option<u64>) -> Self {
        Message::Terminal {
            message_id: generate_message_id(),
            expect_response: true,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            token: String::new(),
            payload: TerminalPayload {
                action: TerminalAction::Subscribe { start_seq },
            },
        }
    }

    /// 创建终端订阅响应消息
    pub fn subscribe_response(session_id: &str, min_seq: u64, max_seq: u64, history_count: usize) -> Self {
        Self::subscribe_response_with_request_id(session_id, min_seq, max_seq, history_count, &generate_message_id())
    }

    /// 创建终端订阅响应消息（携带原始 request_id）
    ///
    /// 用于回复 `expect_response=true` 的订阅请求，使客户端能匹配 pending 请求
    pub fn subscribe_response_with_request_id(session_id: &str, min_seq: u64, max_seq: u64, history_count: usize, request_id: &str) -> Self {
        Message::Terminal {
            message_id: request_id.to_string(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            token: String::new(),
            payload: TerminalPayload {
                action: TerminalAction::SubscribeResponse {
                    min_seq,
                    max_seq,
                    history_count,
                },
            },
        }
    }

    /// 创建终端取消订阅消息
    pub fn unsubscribe(session_id: &str) -> Self {
        Message::Terminal {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            token: String::new(),
            payload: TerminalPayload {
                action: TerminalAction::Unsubscribe,
            },
        }
    }

    /// 创建终端取消订阅消息（带响应期望）
    pub fn unsubscribe_with_response(session_id: &str) -> Self {
        Message::Terminal {
            message_id: generate_message_id(),
            expect_response: true,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            token: String::new(),
            payload: TerminalPayload {
                action: TerminalAction::Unsubscribe,
            },
        }
    }

    /// 创建终端取消订阅响应消息
    pub fn unsubscribe_response(session_id: &str) -> Self {
        Self::unsubscribe_response_with_request_id(session_id, &generate_message_id())
    }

    /// 创建终端取消订阅响应消息（携带原始 request_id）
    ///
    /// 用于回复 `expect_response=true` 的取消订阅请求，使客户端能匹配 pending 请求
    pub fn unsubscribe_response_with_request_id(session_id: &str, request_id: &str) -> Self {
        Message::Terminal {
            message_id: request_id.to_string(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            token: String::new(),
            payload: TerminalPayload {
                action: TerminalAction::UnsubscribeResponse,
            },
        }
    }

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

    /// 创建会话配置消息
    pub fn session_config(action: SessionConfigAction, session_id: Option<&str>) -> Self {
        Message::SessionConfig {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.map(|s| s.to_string()),
            token: String::new(),
            payload: SessionConfigPayload { action },
        }
    }

    /// 创建会话配置消息（带响应期望）
    pub fn session_config_with_response(action: SessionConfigAction, session_id: Option<&str>) -> Self {
        Message::SessionConfig {
            message_id: generate_message_id(),
            expect_response: true,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.map(|s| s.to_string()),
            token: String::new(),
            payload: SessionConfigPayload { action },
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

    /// 创建客户端断开通知
    pub fn client_disconnected(device_name: &str, reason: &str) -> Self {
        Message::ClientDisconnected {
            device_name: device_name.to_string(),
            reason: reason.to_string(),
            token: String::new(),
        }
    }

    /// 创建会话事件通知
    pub fn session_event(event_type: &str, session: SessionSummary, device_name: &str) -> Self {
        Message::SessionEvent {
            event_type: event_type.to_string(),
            session,
            device_name: device_name.to_string(),
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

    /// 创建数据同步消息
    pub fn sync_data(payload: SyncPayload) -> Self {
        Message::SyncData {
            timestamp: Utc::now().timestamp_millis(),
            payload,
            token: String::new(),
        }
    }

    /// 获取消息ID
    pub fn message_id(&self) -> Option<&str> {
        match self {
            Message::Terminal { message_id, .. } => Some(message_id),
            Message::Auth { message_id, .. } => Some(message_id),
            Message::SessionControl { message_id, .. } => Some(message_id),
            Message::SessionConfig { message_id, .. } => Some(message_id),
            Message::Error { message_id, .. } => message_id.as_deref(),
            Message::ServerClosed { .. } => None,
            Message::ClientDisconnected { .. } => None,
            Message::SessionEvent { .. } => None,
            Message::Ack { .. } => None,
            Message::SyncData { .. } => None,
        }
    }

    /// 获取消息类型名称（用于调试日志）
    pub fn message_type(&self) -> Option<&'static str> {
        match self {
            Message::Terminal { .. } => Some("terminal"),
            Message::Auth { .. } => Some("auth"),
            Message::SessionControl { .. } => Some("session_control"),
            Message::SessionConfig { .. } => Some("session_config"),
            Message::Error { .. } => Some("error"),
            Message::ServerClosed { .. } => Some("server_closed"),
            Message::ClientDisconnected { .. } => Some("client_disconnected"),
            Message::SessionEvent { .. } => Some("session_event"),
            Message::Ack { .. } => Some("ack"),
            Message::SyncData { .. } => Some("sync_data"),
        }
    }

    /// 获取 expect_response 标记
    pub fn expect_response(&self) -> bool {
        match self {
            Message::Terminal { expect_response, .. } => *expect_response,
            Message::Auth { expect_response, .. } => *expect_response,
            Message::SessionControl { expect_response, .. } => *expect_response,
            Message::SessionConfig { expect_response, .. } => *expect_response,
            Message::Error { expect_response, .. } => *expect_response,
            Message::ServerClosed { .. } => false,
            Message::ClientDisconnected { .. } => false,
            Message::SessionEvent { .. } => false,
            Message::Ack { .. } => false,
            Message::SyncData { .. } => false,
        }
    }

    /// 获取 token
    pub fn token(&self) -> &str {
        match self {
            Message::Terminal { token, .. } => token,
            Message::Auth { token, .. } => token,
            Message::SessionControl { token, .. } => token,
            Message::SessionConfig { token, .. } => token,
            Message::Error { token, .. } => token,
            Message::ServerClosed { token, .. } => token,
            Message::ClientDisconnected { token, .. } => token,
            Message::SessionEvent { token, .. } => token,
            Message::Ack { token, .. } => token,
            Message::SyncData { token, .. } => token,
        }
    }

    /// 设置响应消息的关联 ID（用于请求-响应跟踪）
    /// 仅对支持响应关联的消息类型有效
    pub fn with_request_id(self, request_id: &str) -> Self {
        match self {
            Message::Terminal { message_id, expect_response, timestamp, session_id, token, payload } => {
                Message::Terminal {
                    message_id: request_id.to_string(),
                    expect_response,
                    timestamp,
                    session_id,
                    token,
                    payload,
                }
            }
            Message::Auth { message_id, expect_response, timestamp, session_id, token, payload } => {
                Message::Auth {
                    message_id: request_id.to_string(),
                    expect_response,
                    timestamp,
                    session_id,
                    token,
                    payload,
                }
            }
            Message::SessionControl { message_id, expect_response, timestamp, session_id, token, payload } => {
                Message::SessionControl {
                    message_id: request_id.to_string(),
                    expect_response,
                    timestamp,
                    session_id,
                    token,
                    payload,
                }
            }
            Message::SessionConfig { message_id, expect_response, timestamp, session_id, token, payload } => {
                Message::SessionConfig {
                    message_id: request_id.to_string(),
                    expect_response,
                    timestamp,
                    session_id,
                    token,
                    payload,
                }
            }
            Message::Error { message_id, expect_response, timestamp, token, code, message } => {
                Message::Error {
                    message_id: Some(request_id.to_string()),
                    expect_response,
                    timestamp,
                    token,
                    code,
                    message,
                }
            }
            // 其他类型不支持设置 request_id，直接返回
            other => other,
        }
    }

    /// 设置 token
    pub fn with_token(self, token: &str) -> Self {
        match self {
            Message::Terminal { message_id, expect_response, timestamp, session_id, payload, .. } => {
                Message::Terminal {
                    message_id,
                    expect_response,
                    timestamp,
                    session_id,
                    token: token.to_string(),
                    payload,
                }
            }
            Message::Auth { message_id, expect_response, timestamp, session_id, payload, .. } => {
                Message::Auth {
                    message_id,
                    expect_response,
                    timestamp,
                    session_id,
                    token: token.to_string(),
                    payload,
                }
            }
            Message::SessionControl { message_id, expect_response, timestamp, session_id, payload, .. } => {
                Message::SessionControl {
                    message_id,
                    expect_response,
                    timestamp,
                    session_id,
                    token: token.to_string(),
                    payload,
                }
            }
            Message::SessionConfig { message_id, expect_response, timestamp, session_id, payload, .. } => {
                Message::SessionConfig {
                    message_id,
                    expect_response,
                    timestamp,
                    session_id,
                    token: token.to_string(),
                    payload,
                }
            }
            Message::Error { message_id, expect_response, timestamp, code, message, .. } => {
                Message::Error {
                    message_id,
                    expect_response,
                    timestamp,
                    token: token.to_string(),
                    code,
                    message,
                }
            }
            Message::ServerClosed { reason, will_reconnect, .. } => {
                Message::ServerClosed {
                    reason,
                    will_reconnect,
                    token: token.to_string(),
                }
            }
            Message::ClientDisconnected { device_name, reason, .. } => {
                Message::ClientDisconnected {
                    device_name,
                    reason,
                    token: token.to_string(),
                }
            }
            Message::SessionEvent { event_type, session, device_name, .. } => {
                Message::SessionEvent {
                    event_type,
                    session,
                    device_name,
                    token: token.to_string(),
                }
            }
            Message::Ack { request_id, timestamp, code, message, .. } => {
                Message::Ack {
                    request_id,
                    timestamp,
                    code,
                    message,
                    token: token.to_string(),
                }
            }
            Message::SyncData { timestamp, payload, .. } => {
                Message::SyncData {
                    timestamp,
                    payload,
                    token: token.to_string(),
                }
            }
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
    pub fn from_ws_message(
        msg: tokio_tungstenite::tungstenite::Message,
    ) -> crate::Result<Option<Self>> {
        match msg {
            tokio_tungstenite::tungstenite::Message::Text(text) => {
                Ok(Some(serde_json::from_str(&text)?))
            }
            tokio_tungstenite::tungstenite::Message::Binary(data) => {
                let text = String::from_utf8_lossy(&data);
                Ok(Some(serde_json::from_str(&text)?))
            }
            tokio_tungstenite::tungstenite::Message::Ping(_) => Ok(None), // 协议层心跳由 tungstenite 自动处理
            tokio_tungstenite::tungstenite::Message::Pong(_) => Ok(None),
            tokio_tungstenite::tungstenite::Message::Close(reason) => {
                Ok(Some(Message::error("close", &reason.map(|r| r.to_string()).unwrap_or_default())))
            }
            tokio_tungstenite::tungstenite::Message::Frame(_) => Ok(None),
        }
    }
}