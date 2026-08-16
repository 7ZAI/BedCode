//! WebSocket Message Types
//!
//! 统一的业务消息类型，作为 WebSocket 客户端和服务端的业务传输类型

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::enums::auth::AuthPayload;
use crate::enums::control::{SessionConfigAction, SessionConfigPayload, SessionControlAction, SessionControlPayload, TerminalAction, TerminalPayload};
use crate::enums::file_service::FileServicePayload;
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

    /// 文件服务控制面消息 (移动端 → 桌面端，内网文件传输插件规格阶段 2)
    ///
    /// 承载 Announce（端口/token/挂载公告）与 Withdraw（服务撤回）。
    /// 与桌面端 `server/ws/message.rs` 的同名变体双写互引：两端
    /// 新增/变更字段必须同步
    #[serde(rename = "file_service")]
    FileService {
        #[serde(default = "generate_message_id")]
        message_id: String,
        #[serde(default)]
        expect_response: bool,
        timestamp: i64,
        /// 认证令牌
        #[serde(default = "default_token")]
        token: String,
        payload: FileServicePayload,
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
                    start_offset: None,
                    end_offset: None,
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
                    start_offset: None,
                    end_offset: None,
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
                    mode: None,
                    min_offset: None,
                    max_offset: None,
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

    /// 创建文件服务控制面消息（Announce / Withdraw，见 [`FileServicePayload`]）
    pub fn file_service(payload: FileServicePayload) -> Self {
        Message::FileService {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            token: String::new(),
            payload,
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
            Message::FileService { message_id, .. } => Some(message_id),
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
            Message::FileService { .. } => Some("file_service"),
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
            Message::FileService { expect_response, .. } => *expect_response,
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
            Message::FileService { token, .. } => token,
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
            Message::FileService { message_id, expect_response, timestamp, payload, .. } => {
                Message::FileService {
                    message_id,
                    expect_response,
                    timestamp,
                    token: token.to_string(),
                    payload,
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
                // 注意：tungstenite 的 CloseFrame Display 会追加关闭码（"reason (code)"），
                // 这里只取 reason 字段，避免关闭码混入错误消息
                Ok(Some(Message::error("close", &reason.map(|r| r.reason.to_string()).unwrap_or_default())))
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

    /// 构造固定值的终端输入消息（测试辅助）
    fn terminal_input_literal() -> Message {
        Message::Terminal {
            message_id: "m-001".to_string(),
            expect_response: false,
            timestamp: FIXED_TS,
            session_id: "sess-1".to_string(),
            token: "tk".to_string(),
            payload: TerminalPayload {
                action: TerminalAction::Input {
                    data: "ls -la".to_string(),
                    special_key: None,
                },
            },
        }
    }

    /// 构造固定值的会话摘要（测试辅助）
    fn sample_session_summary() -> SessionSummary {
        SessionSummary {
            id: "s1".to_string(),
            name: "dev".to_string(),
            status: "running".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            started_at: Some("2024-01-02T00:00:00Z".to_string()),
            session_type: None,
            config_id: None,
            task_status: None,
            task_reason: None,
        }
    }

    // ==================== 构造器测试 ====================

    #[test]
    fn test_output_constructor_fields() {
        // 输出消息：data 应为 Base64 编码，其余字段原样透传
        let msg = Message::output("sess-1", b"hello", true, 42);
        assert_eq!(msg.message_type(), Some("terminal"));
        assert!(msg.message_id().is_some_and(|id| !id.is_empty()));
        assert!(!msg.expect_response());
        assert_eq!(msg.token(), "");
        match msg {
            Message::Terminal {
                session_id,
                payload:
                    TerminalPayload {
                        action: TerminalAction::Output { data, is_waiting, index, end_index, start_offset, end_offset },
                    },
                ..
            } => {
                assert_eq!(session_id, "sess-1");
                // b"hello" 的 Base64 手算值
                assert_eq!(data, "aGVsbG8=");
                assert!(is_waiting);
                assert_eq!(index, 42);
                assert_eq!(end_index, None);
                assert_eq!(start_offset, None);
                assert_eq!(end_offset, None);
            }
            _ => panic!("expected terminal output message"),
        }
    }

    #[test]
    fn test_output_base64_encoding() {
        // 手算 Base64 真源："hello world" → aGVsbG8gd29ybGQ=
        let msg = Message::output("s", b"hello world", false, 0);
        match msg {
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::Output { data, .. } },
                ..
            } => assert_eq!(data, "aGVsbG8gd29ybGQ="),
            _ => panic!(),
        }

        // 空字节 → 空字符串
        let msg = Message::output("s", b"", false, 0);
        match msg {
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::Output { data, .. } },
                ..
            } => assert_eq!(data, ""),
            _ => panic!(),
        }

        // 非 ASCII 字节 [0x00,0x01,0x02,0xFF] 手算 → AAEC/w==
        let msg = Message::output("s", &[0x00, 0x01, 0x02, 0xff], false, 0);
        match msg {
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::Output { data, .. } },
                ..
            } => assert_eq!(data, "AAEC/w=="),
            _ => panic!(),
        }
    }

    #[test]
    fn test_output_from_base64_passthrough() {
        // 已编码数据不应二次编码，直接透传
        let msg = Message::output_from_base64("s", "QUJDRA==", false, 7, Some(9));
        match msg {
            Message::Terminal {
                payload:
                    TerminalPayload {
                        action: TerminalAction::Output { data, is_waiting, index, end_index, start_offset, end_offset },
                    },
                ..
            } => {
                assert_eq!(data, "QUJDRA==");
                assert!(!is_waiting);
                assert_eq!(index, 7);
                assert_eq!(end_index, Some(9));
                assert_eq!(start_offset, None);
                assert_eq!(end_offset, None);
            }
            _ => panic!(),
        }

        // end_index 缺省时仍为 None
        let msg = Message::output_from_base64("s", "QUJDRA==", true, 1, None);
        match msg {
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::Output { end_index, .. } },
                ..
            } => assert_eq!(end_index, None),
            _ => panic!(),
        }
    }

    #[test]
    fn test_input_constructor() {
        // 带特殊键（Ctrl+C）
        let combo = KeyCombo::parse("ctrl+c").unwrap();
        let msg = Message::input("sess-1", "ls", Some(combo.clone()));
        assert_eq!(msg.message_type(), Some("terminal"));
        assert!(!msg.expect_response());
        match msg {
            Message::Terminal {
                session_id,
                payload: TerminalPayload { action: TerminalAction::Input { data, special_key } },
                ..
            } => {
                assert_eq!(session_id, "sess-1");
                assert_eq!(data, "ls");
                assert_eq!(special_key, Some(combo));
            }
            _ => panic!(),
        }

        // 无特殊键
        let msg = Message::input("sess-1", "echo hi", None);
        match msg {
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::Input { special_key, .. } },
                ..
            } => assert_eq!(special_key, None),
            _ => panic!(),
        }
    }

    #[test]
    fn test_input_with_response() {
        // 与 input 唯一区别是 expect_response=true
        let msg = Message::input_with_response("s", "y", None);
        assert!(msg.expect_response());
        match msg {
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::Input { data, .. } },
                ..
            } => assert_eq!(data, "y"),
            _ => panic!(),
        }
    }

    #[test]
    fn test_subscribe_start_seq() {
        // 显式起始序号
        let msg = Message::subscribe("s", Some(100));
        assert!(!msg.expect_response());
        match msg {
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::Subscribe { start_seq } },
                ..
            } => assert_eq!(start_seq, Some(100)),
            _ => panic!(),
        }

        // 从头补完（None）
        let msg = Message::subscribe("s", None);
        match msg {
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::Subscribe { start_seq } },
                ..
            } => assert_eq!(start_seq, None),
            _ => panic!(),
        }
    }

    #[test]
    fn test_subscribe_with_response() {
        let msg = Message::subscribe_with_response("s", Some(5));
        assert!(msg.expect_response());
        match msg {
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::Subscribe { start_seq } },
                ..
            } => assert_eq!(start_seq, Some(5)),
            _ => panic!(),
        }
    }

    #[test]
    fn test_subscribe_response_with_request_id() {
        // 响应消息的 message_id 必须回填请求 ID，便于客户端匹配 pending 请求
        let msg = Message::subscribe_response_with_request_id("s", 10, 20, 3, "req-1");
        assert_eq!(msg.message_id(), Some("req-1"));
        assert!(!msg.expect_response());
        match msg {
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::SubscribeResponse { min_seq, max_seq, history_count, mode, min_offset, max_offset } },
                ..
            } => {
                assert_eq!(min_seq, 10);
                assert_eq!(max_seq, 20);
                assert_eq!(history_count, 3);
                // 移动端构造器不裁决订阅模式，三个扩展字段应为 None
                assert_eq!(mode, None);
                assert_eq!(min_offset, None);
                assert_eq!(max_offset, None);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_subscribe_response_auto_id() {
        // 未指定 request_id 时自动生成，载荷字段与显式版本一致
        let msg = Message::subscribe_response("s", 1, 2, 0);
        assert!(msg.message_id().is_some_and(|id| !id.is_empty()));
        match msg {
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::SubscribeResponse { min_seq, max_seq, history_count, .. } },
                ..
            } => {
                assert_eq!(min_seq, 1);
                assert_eq!(max_seq, 2);
                assert_eq!(history_count, 0);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_unsubscribe_variants() {
        // 普通取消订阅
        let msg = Message::unsubscribe("s");
        assert!(!msg.expect_response());
        assert!(matches!(
            msg,
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::Unsubscribe },
                ..
            }
        ));

        // 带响应期望
        let msg = Message::unsubscribe_with_response("s");
        assert!(msg.expect_response());
        assert!(matches!(
            msg,
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::Unsubscribe },
                ..
            }
        ));

        // 取消订阅响应（自动 ID）
        let msg = Message::unsubscribe_response("s");
        assert!(!msg.expect_response());
        assert!(msg.message_id().is_some_and(|id| !id.is_empty()));
        assert!(matches!(
            msg,
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::UnsubscribeResponse },
                ..
            }
        ));

        // 取消订阅响应（回填请求 ID）
        let msg = Message::unsubscribe_response_with_request_id("s", "req-2");
        assert_eq!(msg.message_id(), Some("req-2"));
        assert!(matches!(
            msg,
            Message::Terminal {
                payload: TerminalPayload { action: TerminalAction::UnsubscribeResponse },
                ..
            }
        ));
    }

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
        let msg = Message::session_control(SessionControlAction::StopSession { session_id: "s9".into() }, None);
        match msg {
            Message::SessionControl { session_id, .. } => assert_eq!(session_id, None),
            _ => panic!(),
        }

        // 带响应期望
        let msg = Message::session_control_with_response(SessionControlAction::ListSessions, None);
        assert!(msg.expect_response());
    }

    #[test]
    fn test_session_config_variants() {
        let msg = Message::session_config(SessionConfigAction::ListSessionConfigs, Some("s1"));
        assert_eq!(msg.message_type(), Some("session_config"));
        assert!(!msg.expect_response());
        match msg {
            Message::SessionConfig {
                session_id,
                payload: SessionConfigPayload { action },
                ..
            } => {
                assert_eq!(session_id.as_deref(), Some("s1"));
                assert!(matches!(action, SessionConfigAction::ListSessionConfigs));
            }
            _ => panic!(),
        }

        let msg = Message::session_config_with_response(SessionConfigAction::ListQuickActions, None);
        assert!(msg.expect_response());
        match msg {
            Message::SessionConfig {
                payload: SessionConfigPayload { action },
                ..
            } => assert!(matches!(action, SessionConfigAction::ListQuickActions)),
            _ => panic!(),
        }
    }

    #[test]
    fn test_auth_constructor() {
        // 配对码验证阶段，携带设备信息
        let payload = AuthPayload {
            stage: AuthStage::VerifyCode,
            device_id: Some("dev-1".to_string()),
            device_name: Some("phone".to_string()),
            pairing_code: Some("123456".to_string()),
            ..Default::default()
        };
        let msg = Message::auth(Some("s1".to_string()), payload.clone());
        assert_eq!(msg.message_type(), Some("auth"));
        assert!(!msg.expect_response());
        assert!(msg.message_id().is_some_and(|id| !id.is_empty()));
        match msg {
            Message::Auth { session_id, payload: got, .. } => {
                assert_eq!(session_id.as_deref(), Some("s1"));
                assert_eq!(got.stage, AuthStage::VerifyCode);
                assert_eq!(got.device_id.as_deref(), Some("dev-1"));
                assert_eq!(got.device_name.as_deref(), Some("phone"));
                assert_eq!(got.pairing_code.as_deref(), Some("123456"));
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
            Message::ServerClosed { reason, will_reconnect, token } => {
                assert_eq!(reason, "host exiting");
                assert!(will_reconnect);
                assert_eq!(token, "");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_client_disconnected_constructor() {
        let msg = Message::client_disconnected("phone", "user quit");
        assert_eq!(msg.message_type(), Some("client_disconnected"));
        assert_eq!(msg.message_id(), None);
        assert!(!msg.expect_response());
        match msg {
            Message::ClientDisconnected { device_name, reason, .. } => {
                assert_eq!(device_name, "phone");
                assert_eq!(reason, "user quit");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_session_event_constructor() {
        let session = sample_session_summary();
        let msg = Message::session_event("created", session.clone(), "phone");
        assert_eq!(msg.message_type(), Some("session_event"));
        assert_eq!(msg.message_id(), None);
        assert!(!msg.expect_response());
        match msg {
            Message::SessionEvent { event_type, session: got, device_name, .. } => {
                assert_eq!(event_type, "created");
                assert_eq!(got.id, "s1");
                assert_eq!(got.name, "dev");
                assert_eq!(got.status, "running");
                assert_eq!(device_name, "phone");
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
            Message::Ack { request_id, code, message, .. } => {
                assert_eq!(request_id, "req-1");
                assert_eq!(code, ACK_CODE_SUCCESS);
                assert_eq!(message, None);
            }
            _ => panic!(),
        }

        // 失败：非 0 code 且带错误信息
        let msg = Message::ack_failure("req-1", ACK_CODE_TIMEOUT, "timed out");
        match msg {
            Message::Ack { request_id, code, message, .. } => {
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

    #[test]
    fn test_sync_data_constructor() {
        let payload = SyncPayload::SessionRemoved {
            session_id: "s9".to_string(),
            session_name: "dev".to_string(),
        };
        let msg = Message::sync_data(payload.clone());
        assert_eq!(msg.message_type(), Some("sync_data"));
        assert_eq!(msg.message_id(), None);
        assert!(!msg.expect_response());
        match msg {
            Message::SyncData { payload: got, .. } => match got {
                SyncPayload::SessionRemoved { session_id, session_name } => {
                    assert_eq!(session_id, "s9");
                    assert_eq!(session_name, "dev");
                }
                _ => panic!(),
            },
            _ => panic!(),
        }
    }

    #[test]
    fn test_file_service_constructor() {
        let payload = FileServicePayload::Announce {
            port: 41234,
            token: "file-tok".to_string(),
            device_name: "phone".to_string(),
            mounts: vec![],
        };
        let msg = Message::file_service(payload.clone());
        assert_eq!(msg.message_type(), Some("file_service"));
        assert!(msg.message_id().is_some_and(|id| !id.is_empty()));
        assert!(!msg.expect_response());
        match msg {
            Message::FileService { payload: got, .. } => match got {
                FileServicePayload::Announce { port, token, device_name, mounts } => {
                    assert_eq!(port, 41234);
                    assert_eq!(token, "file-tok");
                    assert_eq!(device_name, "phone");
                    assert!(mounts.is_empty());
                }
                _ => panic!(),
            },
            _ => panic!(),
        }
    }

    // ==================== 访问器测试 ====================

    #[test]
    fn test_message_type_all_variants() {
        // 逐一锁死 wire 上的 type 标签，防止 serde rename 被误改导致协议不兼容
        let cases = vec![
            (terminal_input_literal(), Some("terminal")),
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
                    payload: SessionControlPayload { action: SessionControlAction::ListSessions },
                },
                Some("session_control"),
            ),
            (
                Message::SessionConfig {
                    message_id: "m".into(),
                    expect_response: false,
                    timestamp: FIXED_TS,
                    session_id: None,
                    token: String::new(),
                    payload: SessionConfigPayload { action: SessionConfigAction::ListSessionConfigs },
                },
                Some("session_config"),
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
            (Message::client_disconnected("d", "r"), Some("client_disconnected")),
            (Message::session_event("created", sample_session_summary(), "d"), Some("session_event")),
            (Message::ack("r"), Some("ack")),
            (
                Message::sync_data(SyncPayload::SessionRemoved {
                    session_id: "s".into(),
                    session_name: "n".into(),
                }),
                Some("sync_data"),
            ),
            (Message::file_service(FileServicePayload::Withdraw {}), Some("file_service")),
        ];
        for (msg, expected) in cases {
            assert_eq!(msg.message_type(), expected);
        }
    }

    #[test]
    fn test_message_id_none_variants() {
        // 无消息 ID 语义的变体（通知/响应类）必须返回 None
        assert_eq!(Message::server_closed("r", false).message_id(), None);
        assert_eq!(Message::client_disconnected("d", "r").message_id(), None);
        assert_eq!(Message::session_event("created", sample_session_summary(), "d").message_id(), None);
        assert_eq!(Message::ack("r").message_id(), None);
        assert_eq!(
            Message::sync_data(SyncPayload::SessionRemoved {
                session_id: "s".into(),
                session_name: "n".into(),
            })
            .message_id(),
            None
        );
    }

    #[test]
    fn test_expect_response_none_variants() {
        // 通知/响应类变体没有 expect_response 概念，恒为 false
        assert!(!Message::server_closed("r", false).expect_response());
        assert!(!Message::client_disconnected("d", "r").expect_response());
        assert!(!Message::session_event("created", sample_session_summary(), "d").expect_response());
        assert!(!Message::ack("r").expect_response());
        assert!(!Message::sync_data(SyncPayload::SessionRemoved {
            session_id: "s".into(),
            session_name: "n".into(),
        })
        .expect_response());
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
        let msg = Message::output("s", b"x", false, 0);
        let after = Utc::now().timestamp_millis();
        let Message::Terminal { timestamp, .. } = msg else {
            panic!()
        };
        assert!(timestamp >= before, "timestamp {timestamp} < before {before}");
        assert!(timestamp <= after, "timestamp {timestamp} > after {after}");
    }

    #[test]
    fn test_token_accessor_and_with_token() {
        // with_token 覆盖 token，token() 读回
        let msg = Message::input("s", "ls", None).with_token("tok-1");
        assert_eq!(msg.token(), "tok-1");

        let msg = Message::server_closed("r", false).with_token("tok-2");
        assert_eq!(msg.token(), "tok-2");

        let msg = Message::ack("r").with_token("tok-3");
        assert_eq!(msg.token(), "tok-3");

        // 未设置时为空字符串
        assert_eq!(Message::sync_data(SyncPayload::SessionRemoved {
            session_id: "s".into(),
            session_name: "n".into(),
        })
        .token(), "");
    }

    #[test]
    fn test_with_request_id_supported_variants() {
        // 请求类变体：message_id 被替换为 request_id
        for msg in [
            Message::input("s", "ls", None),
            Message::auth(None, AuthPayload::default()),
            Message::session_control(SessionControlAction::ListSessions, None),
            Message::session_config(SessionConfigAction::ListSessionConfigs, None),
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
        let req = Message::input_with_response("s", "y", None).with_request_id("req-abc");
        let ack = Message::ack("req-abc");
        match ack {
            Message::Ack { request_id, .. } => assert_eq!(request_id, req.message_id().unwrap()),
            _ => panic!(),
        }
        // 订阅响应 / 错误响应同样回填请求 ID
        assert_eq!(Message::subscribe_response_with_request_id("s", 0, 1, 0, "req-abc").message_id(), Some("req-abc"));
        assert_eq!(Message::error_with_id("req-abc", "E", "m").message_id(), Some("req-abc"));
    }

    // ==================== 序列化测试 ====================

    #[test]
    fn test_to_json_terminal_output_exact() {
        // 手写完整 wire JSON 作为真源，锁死字段名/顺序/嵌套结构
        let msg = Message::Terminal {
            message_id: "m-001".to_string(),
            expect_response: false,
            timestamp: FIXED_TS,
            session_id: "sess-1".to_string(),
            token: "".to_string(),
            payload: TerminalPayload {
                action: TerminalAction::Output {
                    data: "aGVsbG8gd29ybGQ=".to_string(),
                    is_waiting: true,
                    index: 3,
                    end_index: Some(5),
                    start_offset: Some(100),
                    end_offset: Some(200),
                },
            },
        };
        let json = msg.to_json().unwrap();
        assert_eq!(
            json,
            "{\"type\":\"terminal\",\"payload\":{\"message_id\":\"m-001\",\"expect_response\":false,\"timestamp\":1700000000000,\"session_id\":\"sess-1\",\"token\":\"\",\"payload\":{\"action\":{\"type\":\"output\",\"data\":\"aGVsbG8gd29ybGQ=\",\"is_waiting\":true,\"index\":3,\"end_index\":5,\"start_offset\":100,\"end_offset\":200}}}}"
        );
    }

    #[test]
    fn test_to_json_auth_exact() {
        // AuthPayload::default() 的 stage 为 request_pairing，可选字段全部省略；
        // session_id 为 None 时无 skip 属性 → 序列化为 null
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
            "{\"type\":\"auth\",\"payload\":{\"message_id\":\"m-002\",\"expect_response\":false,\"timestamp\":1700000000000,\"session_id\":null,\"token\":\"\",\"payload\":{\"stage\":\"request_pairing\"}}}"
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
    fn test_to_json_session_event_exact() {
        // SessionSummary 的 session_type=None 无 skip 属性 → null；config_id/task_* 省略
        let msg = Message::session_event("created", sample_session_summary(), "phone");
        assert_eq!(
            msg.to_json().unwrap(),
            "{\"type\":\"session_event\",\"payload\":{\"event_type\":\"created\",\"session\":{\"id\":\"s1\",\"name\":\"dev\",\"status\":\"running\",\"created_at\":\"2024-01-01T00:00:00Z\",\"started_at\":\"2024-01-02T00:00:00Z\",\"session_type\":null},\"device_name\":\"phone\",\"token\":\"\"}}"
        );
    }

    #[test]
    fn test_to_json_file_service_exact() {
        // FileServicePayload 为 action/data 相邻标签格式
        let msg = Message::FileService {
            message_id: "m-003".to_string(),
            expect_response: false,
            timestamp: FIXED_TS,
            token: "".to_string(),
            payload: FileServicePayload::Announce {
                port: 41234,
                token: "file-tok".to_string(),
                device_name: "phone".to_string(),
                mounts: vec![],
            },
        };
        assert_eq!(
            msg.to_json().unwrap(),
            "{\"type\":\"file_service\",\"payload\":{\"message_id\":\"m-003\",\"expect_response\":false,\"timestamp\":1700000000000,\"token\":\"\",\"payload\":{\"action\":\"announce\",\"data\":{\"port\":41234,\"token\":\"file-tok\",\"device_name\":\"phone\",\"mounts\":[]}}}}"
        );
    }

    #[test]
    fn test_from_json_applies_defaults() {
        // 旧端/简化端可省略 message_id/expect_response/token，反序列化必须兜底
        let json = r#"{"type":"terminal","payload":{"timestamp":1700000000000,"session_id":"s1","payload":{"action":{"type":"input","data":"ls"}}}}"#;
        let msg = Message::from_json(json).unwrap();
        assert!(msg.message_id().is_some_and(|id| !id.is_empty()));
        assert!(!msg.expect_response());
        assert_eq!(msg.token(), "");
    }

    #[test]
    fn test_serde_roundtrip_all_variants() {
        // 全部 11 个变体：to_value → from_value → to_value 必须保持一致
        let variants = vec![
            Message::Terminal {
                message_id: "m1".into(),
                expect_response: true,
                timestamp: FIXED_TS,
                session_id: "s1".into(),
                token: "tk".into(),
                payload: TerminalPayload {
                    action: TerminalAction::Input {
                        data: "ls".into(),
                        special_key: Some(KeyCombo::parse("ctrl+c").unwrap()),
                    },
                },
            },
            Message::Auth {
                message_id: "m2".into(),
                expect_response: false,
                timestamp: FIXED_TS,
                session_id: Some("s1".into()),
                token: "tk".into(),
                payload: AuthPayload {
                    stage: AuthStage::VerifyCode,
                    pairing_code: Some("123456".into()),
                    ..Default::default()
                },
            },
            Message::SessionControl {
                message_id: "m3".into(),
                expect_response: false,
                timestamp: FIXED_TS,
                session_id: None,
                token: "".into(),
                payload: SessionControlPayload { action: SessionControlAction::ListSessions },
            },
            Message::SessionConfig {
                message_id: "m4".into(),
                expect_response: false,
                timestamp: FIXED_TS,
                session_id: None,
                token: "".into(),
                payload: SessionConfigPayload { action: SessionConfigAction::ListQuickActions },
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
            Message::ClientDisconnected {
                device_name: "phone".into(),
                reason: "quit".into(),
                token: "".into(),
            },
            Message::SessionEvent {
                event_type: "created".into(),
                session: sample_session_summary(),
                device_name: "phone".into(),
                token: "".into(),
            },
            Message::Ack {
                request_id: "r1".into(),
                timestamp: FIXED_TS,
                code: ACK_CODE_SUCCESS,
                message: None,
                token: "".into(),
            },
            Message::SyncData {
                timestamp: FIXED_TS,
                payload: SyncPayload::SessionRemoved {
                    session_id: "s9".into(),
                    session_name: "dev".into(),
                },
                token: "".into(),
            },
            Message::FileService {
                message_id: "m11".into(),
                expect_response: false,
                timestamp: FIXED_TS,
                token: "".into(),
                payload: FileServicePayload::Withdraw {},
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
            Message::output("s", b"\x00\x01\xfe\xff", true, 9),
            Message::input("s", "echo hi", Some(KeyCombo::parse("ctrl+shift+up").unwrap())),
            Message::subscribe("s", Some(42)),
            Message::subscribe_response_with_request_id("s", 0, 100, 10, "req-7"),
            Message::session_control(SessionControlAction::ResizeSession {
                session_id: "s1".into(),
                cols: 80,
                rows: 24,
            }, Some("s1")),
            Message::auth(
                None,
                AuthPayload {
                    stage: AuthStage::Authenticated,
                    session_token: Some("jwt-token".into()),
                    ..Default::default()
                },
            ),
            Message::ack_failure("req-8", ACK_CODE_AUTH_FAILED, "auth failed"),
            Message::sync_data(SyncPayload::TaskStatusChanged {
                session_id: "s1".into(),
                task_status: "completed".into(),
                task_reason: None,
                task_questions: None,
            }),
            Message::file_service(FileServicePayload::Query {}),
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
        let parsed = Message::from_ws_message(WsMessage::Text(json.clone())).unwrap().unwrap();
        assert_eq!(parsed.message_type(), Some("server_closed"));
        assert_eq!(parsed.to_json().unwrap(), json);

        // Binary 帧按 UTF-8 解析
        let parsed = Message::from_ws_message(WsMessage::Binary(json.clone().into_bytes())).unwrap().unwrap();
        assert_eq!(parsed.to_json().unwrap(), json);
    }

    #[test]
    fn test_from_ws_message_control_frames_return_none() {
        // 心跳/帧等协议控制帧不应产生业务消息
        assert!(Message::from_ws_message(WsMessage::Ping(vec![].into())).unwrap().is_none());
        assert!(Message::from_ws_message(WsMessage::Pong(vec![].into())).unwrap().is_none());
        assert!(Message::from_ws_message(WsMessage::Frame(tokio_tungstenite::tungstenite::protocol::frame::Frame::ping(vec![]))).unwrap().is_none());
    }

    #[test]
    fn test_from_ws_message_close_reason() {
        // Close 帧转换为 error 消息（code=close），reason 透传纯文本（不含关闭码）
        let frame = CloseFrame {
            code: CloseCode::Normal,
            reason: Cow::Owned("going away".into()),
        };
        let msg = Message::from_ws_message(WsMessage::Close(Some(frame))).unwrap().unwrap();
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
        let msg = Message::input_with_response("s", "pwd", None).with_request_id("req-77");
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