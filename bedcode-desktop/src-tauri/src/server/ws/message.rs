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
use crate::enums::summary::SessionSummary;
use crate::enums::SubscribeMode;
use crate::enums::SyncPayload;

// ==================== Ack 响应代码常量 ====================

/// Ack 成功响应代码
pub const ACK_CODE_SUCCESS: u16 = 0;

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
    /// 承载移动文件服务的 Announce（端口/token/挂载公告）与 Withdraw（服务撤回）。
    /// 与移动端 `model/message.rs` 的同名变体双写互引：两端
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
    /// 创建文件服务控制面消息（桌面端 → 移动端：Query / 挂载快照补发）
    pub fn file_service(payload: FileServicePayload) -> Self {
        Message::FileService {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            token: String::new(),
            payload,
        }
    }

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
    /// start_offset/end_offset 提供字节级游标（会话流坐标），供增量续传
    pub fn output_from_base64(session_id: &str, data_base64: &str, is_waiting: bool, index: usize, end_index: Option<usize>, start_offset: Option<u64>, end_offset: Option<u64>) -> Self {
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
                    start_offset,
                    end_offset,
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
    pub fn subscribe_response(session_id: &str, min_seq: u64, max_seq: u64, history_count: usize, mode: SubscribeMode, min_offset: u64, max_offset: u64) -> Self {
        Self::subscribe_response_with_request_id(session_id, min_seq, max_seq, history_count, mode, min_offset, max_offset, &generate_message_id())
    }

    /// 创建终端订阅响应消息（携带原始 request_id）
    ///
    /// 用于回复 `expect_response=true` 的订阅请求，使客户端能匹配 pending 请求
    /// mode/min_offset/max_offset 为订阅裁决信息（见 SubscribeMode），
    /// 消费者据此决定清屏重播（reset）或从游标续传（incremental）
    pub fn subscribe_response_with_request_id(session_id: &str, min_seq: u64, max_seq: u64, history_count: usize, mode: SubscribeMode, min_offset: u64, max_offset: u64, request_id: &str) -> Self {
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
                    mode,
                    min_offset,
                    max_offset,
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

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::auth::AuthStage;
    use crate::enums::file_service::MountAnnouncement;
    use crate::enums::special_key::KeyCode;
    use bedcode_plugin_api::FileOperation;
    use serde_json::Value;
    // CloseCode 在 tungstenite 0.24 中不公开导出，需从 frame::coding 引入
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;

    /// 构造样本会话摘要（字段值仅作入参，与真源无关）
    fn sample_session() -> SessionSummary {
        SessionSummary {
            id: "sess-1".to_string(),
            name: "dev-shell".to_string(),
            status: "running".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            started_at: Some("2025-01-01T00:00:00Z".to_string()),
            session_type: Some("pty".to_string()),
            config_id: Some("cfg-1".to_string()),
            task_status: None,
            task_reason: None,
        }
    }

    /// 断言时间戳是近当前时间的毫秒值（允许 10 秒偏差，避免生成器时序抖动误报）
    fn assert_recent_timestamp(ts: i64) {
        let now = Utc::now().timestamp_millis();
        assert!((now - ts).abs() <= 10_000, "timestamp {ts} 偏离当前时间 {now}");
    }

    // ==================== 构造器 ====================

    #[test]
    fn file_service_constructor_sets_control_plane_fields() {
        let m = Message::file_service(FileServicePayload::Query {});
        assert_eq!(m.message_type(), Some("file_service"));
        // 消息 ID 由生成器产生，只验证非空
        assert!(!m.message_id().unwrap().is_empty());
        assert!(!m.expect_response());
        assert_eq!(m.token(), "");
        match &m {
            Message::FileService { payload, .. } => {
                assert!(matches!(payload, FileServicePayload::Query {}));
            }
            _ => panic!("期望 file_service 消息"),
        }
    }

    #[test]
    fn output_constructor_base64_encodes_payload() {
        let m = Message::output("sess-1", b"hello", false, 5);
        match &m {
            Message::Terminal {
                message_id,
                expect_response,
                timestamp,
                session_id,
                token,
                payload: TerminalPayload {
                    action:
                        TerminalAction::Output {
                            data,
                            is_waiting,
                            index,
                            end_index,
                            start_offset,
                            end_offset,
                        },
                },
            } => {
                assert_eq!(session_id, "sess-1");
                assert_eq!(token, "");
                assert!(!expect_response);
                assert!(!message_id.is_empty());
                assert_recent_timestamp(*timestamp);
                // 真源：手工计算 "hello" 的 Base64 编码
                assert_eq!(data, "aGVsbG8=");
                assert!(!is_waiting);
                assert_eq!(*index, 5);
                assert!(end_index.is_none());
                assert!(start_offset.is_none());
                assert!(end_offset.is_none());
                // 解码回原文，验证 Base64 载荷可逆
                assert_eq!(
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data).unwrap(),
                    b"hello"
                );
            }
            _ => panic!("期望 terminal 消息"),
        }
    }

    #[test]
    fn output_from_base64_constructor_preserves_cursors() {
        let m =
            Message::output_from_base64("sess-1", "aGVsbG8=", true, 3, Some(7), Some(100), Some(200));
        match &m {
            Message::Terminal {
                payload: TerminalPayload {
                    action:
                        TerminalAction::Output {
                            data,
                            is_waiting,
                            index,
                            end_index,
                            start_offset,
                            end_offset,
                        },
                },
                ..
            } => {
                assert_eq!(data, "aGVsbG8=");
                assert!(*is_waiting);
                assert_eq!(*index, 3);
                assert_eq!(*end_index, Some(7));
                assert_eq!(*start_offset, Some(100));
                assert_eq!(*end_offset, Some(200));
            }
            _ => panic!("期望 output 动作"),
        }
    }

    #[test]
    fn input_constructor_sets_data_without_response_expectation() {
        let m = Message::input("sess-1", "ls -la", None);
        assert!(!m.expect_response());
        assert_eq!(m.message_type(), Some("terminal"));
        match &m {
            Message::Terminal {
                payload: TerminalPayload {
                    action: TerminalAction::Input { data, special_key },
                },
                ..
            } => {
                assert_eq!(data, "ls -la");
                assert!(special_key.is_none());
            }
            _ => panic!("期望 input 动作"),
        }
    }

    #[test]
    fn input_with_response_constructor_marks_expectation_and_passes_special_key() {
        // 带响应期望的输入用于确认输入已被处理，expect_response 必须为 true
        let combo = KeyCombo::parse("ctrl+a").unwrap();
        let m = Message::input_with_response("sess-1", "", Some(combo));
        assert!(m.expect_response());
        match &m {
            Message::Terminal {
                payload: TerminalPayload {
                    action: TerminalAction::Input { data, special_key },
                },
                ..
            } => {
                assert_eq!(data, "");
                // 特殊键应原样透传，键名与解析结果一致
                assert_eq!(special_key.as_ref().map(|k| &k.key), Some(&KeyCode::Char('a')));
            }
            _ => panic!("期望 input 动作"),
        }
    }

    #[test]
    fn subscribe_constructors_carry_start_seq_and_expectation() {
        let m = Message::subscribe("sess-1", Some(42));
        assert!(!m.expect_response());
        let m2 = Message::subscribe_with_response("sess-1", None);
        assert!(m2.expect_response());
        match (&m, &m2) {
            (
                Message::Terminal {
                    payload: TerminalPayload {
                        action: TerminalAction::Subscribe { start_seq },
                    },
                    ..
                },
                Message::Terminal {
                    payload: TerminalPayload {
                        action: TerminalAction::Subscribe { start_seq: s2 },
                    },
                    ..
                },
            ) => {
                assert_eq!(*start_seq, Some(42));
                assert!(s2.is_none());
            }
            _ => panic!("期望 subscribe 动作"),
        }
    }

    #[test]
    fn subscribe_response_constructors_set_mode_and_request_id() {
        let m = Message::subscribe_response("sess-1", 10, 20, 5, SubscribeMode::Incremental, 100, 200);
        match &m {
            Message::Terminal {
                payload: TerminalPayload {
                    action:
                        TerminalAction::SubscribeResponse {
                            min_seq,
                            max_seq,
                            history_count,
                            mode,
                            min_offset,
                            max_offset,
                        },
                },
                ..
            } => {
                assert_eq!(*min_seq, 10);
                assert_eq!(*max_seq, 20);
                assert_eq!(*history_count, 5);
                assert_eq!(*mode, SubscribeMode::Incremental);
                assert_eq!(*min_offset, 100);
                assert_eq!(*max_offset, 200);
            }
            _ => panic!("期望 subscribe_response 动作"),
        }
        assert!(!m.expect_response());
        // 无 request_id 版本使用生成的随机 ID
        assert!(!m.message_id().unwrap().is_empty());

        // 携带 request_id 版本把请求 ID 用作消息 ID，客户端据此匹配 pending 请求
        let m2 = Message::subscribe_response_with_request_id(
            "sess-1", 1, 2, 0, SubscribeMode::Reset, 0, 0, "req-7",
        );
        assert_eq!(m2.message_id(), Some("req-7"));
    }

    #[test]
    fn unsubscribe_constructors_differ_in_response_expectation() {
        let plain = Message::unsubscribe("sess-1");
        assert!(!plain.expect_response());
        assert!(matches!(
            &plain,
            Message::Terminal {
                payload: TerminalPayload {
                    action: TerminalAction::Unsubscribe,
                },
                ..
            }
        ));

        let with_resp = Message::unsubscribe_with_response("sess-1");
        assert!(with_resp.expect_response());

        let resp = Message::unsubscribe_response("sess-1");
        assert!(!resp.expect_response());
        assert!(!resp.message_id().unwrap().is_empty());

        let resp2 = Message::unsubscribe_response_with_request_id("sess-1", "req-7");
        assert_eq!(resp2.message_id(), Some("req-7"));
        assert!(matches!(
            &resp2,
            Message::Terminal {
                payload: TerminalPayload {
                    action: TerminalAction::UnsubscribeResponse,
                },
                ..
            }
        ));
    }

    #[test]
    fn session_control_constructors_preserve_action_and_session_id() {
        let m = Message::session_control(SessionControlAction::ListSessions, None);
        assert!(!m.expect_response());
        match &m {
            Message::SessionControl {
                session_id,
                payload: SessionControlPayload { action },
                ..
            } => {
                assert!(session_id.is_none());
                assert!(matches!(action, SessionControlAction::ListSessions));
            }
            _ => panic!("期望 session_control 消息"),
        }

        let m2 = Message::session_control_with_response(
            SessionControlAction::StopSession {
                session_id: "s1".to_string(),
            },
            Some("s1"),
        );
        assert!(m2.expect_response());
        match &m2 {
            Message::SessionControl {
                session_id,
                payload: SessionControlPayload { action },
                ..
            } => {
                assert_eq!(session_id.as_deref(), Some("s1"));
                assert!(matches!(
                    action,
                    SessionControlAction::StopSession { session_id } if session_id == "s1"
                ));
            }
            _ => panic!("期望 session_control 消息"),
        }
    }

    #[test]
    fn session_config_constructors_preserve_action() {
        let m = Message::session_config(SessionConfigAction::ListQuickActions, Some("s1"));
        assert!(!m.expect_response());
        match &m {
            Message::SessionConfig {
                session_id,
                payload: SessionConfigPayload { action },
                ..
            } => {
                assert_eq!(session_id.as_deref(), Some("s1"));
                assert!(matches!(action, SessionConfigAction::ListQuickActions));
            }
            _ => panic!("期望 session_config 消息"),
        }

        let m2 = Message::session_config_with_response(SessionConfigAction::ListSessionConfigs, None);
        assert!(m2.expect_response());
    }

    #[test]
    fn auth_constructor_carries_stage_and_payload_fields() {
        let payload = AuthPayload {
            stage: AuthStage::VerifyCode,
            device_id: Some("dev-1".to_string()),
            pairing_code: Some("123456".to_string()),
            ..Default::default()
        };
        let m = Message::auth(Some("sess-1".to_string()), payload);
        assert_eq!(m.message_type(), Some("auth"));
        match &m {
            Message::Auth {
                session_id,
                payload:
                    AuthPayload {
                        stage,
                        device_id,
                        pairing_code,
                        ..
                    },
                ..
            } => {
                assert_eq!(session_id.as_deref(), Some("sess-1"));
                assert_eq!(*stage, AuthStage::VerifyCode);
                assert_eq!(device_id.as_deref(), Some("dev-1"));
                assert_eq!(pairing_code.as_deref(), Some("123456"));
            }
            _ => panic!("期望 auth 消息"),
        }
    }

    #[test]
    fn error_constructors_link_optional_request_id() {
        let e = Message::error("E_BAD", "bad thing");
        assert_eq!(e.message_type(), Some("error"));
        assert!(e.message_id().is_none());
        match &e {
            Message::Error { code, message, .. } => {
                assert_eq!(code, "E_BAD");
                assert_eq!(message, "bad thing");
            }
            _ => panic!("期望 error 消息"),
        }

        // 关联请求 ID 的变体应携带该 ID，供客户端匹配 pending 请求
        let e2 = Message::error_with_id("req-1", "E_BAD", "bad thing");
        assert_eq!(e2.message_id(), Some("req-1"));
    }

    #[test]
    fn server_closed_constructor_reports_reconnect_flag() {
        let m = Message::server_closed("server quitting", true);
        assert_eq!(m.message_type(), Some("server_closed"));
        assert_eq!(m.message_id(), None);
        assert!(!m.expect_response());
        match &m {
            Message::ServerClosed {
                reason,
                will_reconnect,
                ..
            } => {
                assert_eq!(reason, "server quitting");
                assert!(*will_reconnect);
            }
            _ => panic!("期望 server_closed 消息"),
        }
    }

    #[test]
    fn client_disconnected_constructor_reports_device_and_reason() {
        let m = Message::client_disconnected("pixel-9", "lost link");
        assert_eq!(m.message_type(), Some("client_disconnected"));
        match &m {
            Message::ClientDisconnected {
                device_name,
                reason,
                ..
            } => {
                assert_eq!(device_name, "pixel-9");
                assert_eq!(reason, "lost link");
            }
            _ => panic!("期望 client_disconnected 消息"),
        }
    }

    #[test]
    fn session_event_constructor_embeds_summary() {
        let m = Message::session_event("created", sample_session(), "pixel-9");
        assert_eq!(m.message_type(), Some("session_event"));
        assert_eq!(m.message_id(), None);
        match &m {
            Message::SessionEvent {
                event_type,
                session,
                device_name,
                ..
            } => {
                assert_eq!(event_type, "created");
                assert_eq!(session.id, "sess-1");
                assert_eq!(device_name, "pixel-9");
            }
            _ => panic!("期望 session_event 消息"),
        }
    }

    #[test]
    fn ack_constructors_use_success_code_and_optional_message() {
        let ok = Message::ack("req-1");
        assert_eq!(ok.message_type(), Some("ack"));
        assert_eq!(ok.message_id(), None);
        match &ok {
            Message::Ack {
                request_id,
                code,
                message,
                ..
            } => {
                assert_eq!(request_id, "req-1");
                assert_eq!(*code, ACK_CODE_SUCCESS);
                assert!(message.is_none());
            }
            _ => panic!("期望 ack 消息"),
        }

        let fail = Message::ack_failure("req-1", 1001, "timeout");
        match &fail {
            Message::Ack {
                request_id,
                code,
                message,
                ..
            } => {
                assert_eq!(request_id, "req-1");
                assert_eq!(*code, 1001);
                assert_eq!(message.as_deref(), Some("timeout"));
            }
            _ => panic!("期望 ack 消息"),
        }
    }

    #[test]
    fn sync_data_constructor_wraps_payload() {
        let m = Message::sync_data(SyncPayload::SessionStopped {
            session_id: "s1".to_string(),
            session_name: "dev".to_string(),
        });
        assert_eq!(m.message_type(), Some("sync_data"));
        assert_eq!(m.message_id(), None);
        match &m {
            Message::SyncData {
                payload: SyncPayload::SessionStopped {
                    session_id,
                    session_name,
                },
                ..
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(session_name, "dev");
            }
            _ => panic!("期望 sync_data 消息"),
        }
    }

    // ==================== 访问器 ====================

    #[test]
    fn message_type_mapping_covers_all_variants() {
        // 逐变体验证类型名映射，防止序列化标签与调试名漂移
        let cases: Vec<(Message, &str)> = vec![
            (
                Message::file_service(FileServicePayload::Withdraw {}),
                "file_service",
            ),
            (Message::output("s", b"x", false, 0), "terminal"),
            (Message::auth(None, AuthPayload::default()), "auth"),
            (
                Message::session_control(SessionControlAction::ListSessions, None),
                "session_control",
            ),
            (
                Message::session_config(SessionConfigAction::ListSessionConfigs, None),
                "session_config",
            ),
            (Message::error("E", "m"), "error"),
            (Message::server_closed("r", false), "server_closed"),
            (Message::client_disconnected("d", "r"), "client_disconnected"),
            (Message::session_event("created", sample_session(), "d"), "session_event"),
            (Message::ack("req"), "ack"),
            (
                Message::sync_data(SyncPayload::SessionModeChanged {
                    session_id: "s".to_string(),
                    auto_approve: true,
                }),
                "sync_data",
            ),
        ];
        for (m, expected) in cases {
            assert_eq!(m.message_type(), Some(expected));
        }
    }

    #[test]
    fn message_id_accessor_is_none_for_notifications_only() {
        // 仅请求/响应类消息携带 ID，通知类（server_closed/client_disconnected/session_event/ack/sync_data）为 None
        assert_eq!(Message::server_closed("r", false).message_id(), None);
        assert_eq!(Message::client_disconnected("d", "r").message_id(), None);
        assert_eq!(Message::session_event("c", sample_session(), "d").message_id(), None);
        assert_eq!(Message::ack("req").message_id(), None);
        assert_eq!(
            Message::sync_data(SyncPayload::ConfigRemoved {
                config_id: "c".to_string(),
                config_name: "n".to_string(),
            })
            .message_id(),
            None
        );
        // error 的消息 ID 可选：无关联请求时为 None，error_with_id 时为 Some
        assert!(Message::error("E", "m").message_id().is_none());
        assert_eq!(Message::error_with_id("req-1", "E", "m").message_id(), Some("req-1"));
    }

    #[test]
    fn expect_response_accessor_is_false_for_one_way_notifications() {
        assert!(Message::input_with_response("s", "x", None).expect_response());
        assert!(!Message::input("s", "x", None).expect_response());
        assert!(Message::subscribe_with_response("s", None).expect_response());
        assert!(
            Message::session_control_with_response(SessionControlAction::ListSessions, None)
                .expect_response()
        );
        assert!(
            Message::session_config_with_response(SessionConfigAction::ListSessionConfigs, None)
                .expect_response()
        );
        assert!(!Message::server_closed("r", false).expect_response());
        assert!(!Message::ack("req").expect_response());
        assert!(!Message::sync_data(SyncPayload::SessionModeChanged {
            session_id: "s".to_string(),
            auto_approve: true,
        })
        .expect_response());
        assert!(!Message::file_service(FileServicePayload::Query {}).expect_response());
    }

    #[test]
    fn with_token_applies_to_all_variants() {
        // 每种变体都应能设置 token，且不改变原有载荷语义
        let cases = vec![
            Message::input("s", "x", None),
            Message::auth(Some("s".into()), AuthPayload::default()),
            Message::session_control(SessionControlAction::ListSessions, None),
            Message::session_config(SessionConfigAction::ListSessionConfigs, None),
            Message::error("E", "m"),
            Message::server_closed("r", true),
            Message::client_disconnected("d", "r"),
            Message::session_event("c", sample_session(), "d"),
            Message::ack("req"),
            Message::sync_data(SyncPayload::SessionModeChanged {
                session_id: "s".to_string(),
                auto_approve: true,
            }),
            Message::file_service(FileServicePayload::Query {}),
        ];
        for m in cases {
            assert_eq!(m.with_token("tok-9").token(), "tok-9");
        }
    }

    // ==================== 序列化 ====================

    #[test]
    fn to_json_output_message_serializes_known_fields() {
        let m = Message::output("sess-1", b"hello", false, 5);
        let v: Value = serde_json::from_str(&m.to_json().unwrap()).unwrap();
        // 字段值与构造入参一一对应（"aGVsbG8=" 为手工计算的 Base64）。
        // 注意 `content = "payload"` 会把除 type 外的所有字段包进 payload 对象，
        // Terminal 自身的 payload 字段因此嵌套为 payload.payload
        assert_eq!(v["type"], "terminal");
        assert_eq!(v["payload"]["payload"]["action"]["type"], "output");
        assert_eq!(v["payload"]["payload"]["action"]["data"], "aGVsbG8=");
        assert_eq!(v["payload"]["payload"]["action"]["is_waiting"], false);
        assert_eq!(v["payload"]["payload"]["action"]["index"], 5);
        assert_eq!(v["payload"]["session_id"], "sess-1");
        assert_eq!(v["payload"]["expect_response"], false);
        assert_eq!(v["payload"]["token"], "");
        // 未提供的游标字段不应出现在 JSON 中（skip_serializing_if）
        assert!(v["payload"]["payload"]["action"].get("end_index").is_none());
        assert!(v["payload"]["payload"]["action"].get("start_offset").is_none());
        assert!(v["payload"]["payload"]["action"].get("end_offset").is_none());
        // 生成的 ID/时间戳只验证存在性与类型
        assert!(v["payload"]["message_id"].as_str().map(|s| !s.is_empty()).unwrap_or(false));
        assert!(v["payload"]["timestamp"].as_i64().is_some());
    }

    #[test]
    fn to_json_skips_absent_optional_fields() {
        let e = Message::error("E_BAD", "bad thing");
        let v: Value = serde_json::from_str(&e.to_json().unwrap()).unwrap();
        assert_eq!(v["type"], "error");
        assert_eq!(v["payload"]["code"], "E_BAD");
        assert_eq!(v["payload"]["message"], "bad thing");
        assert!(v["payload"].get("message_id").is_none());

        let ack = Message::ack("req-1");
        let v: Value = serde_json::from_str(&ack.to_json().unwrap()).unwrap();
        assert_eq!(v["type"], "ack");
        assert_eq!(v["payload"]["request_id"], "req-1");
        assert_eq!(v["payload"]["code"], 0);
        assert!(v["payload"].get("message").is_none());

        let fail = Message::ack_failure("req-1", 500, "db down");
        let v: Value = serde_json::from_str(&fail.to_json().unwrap()).unwrap();
        assert_eq!(v["payload"]["message"], "db down");
    }

    #[test]
    fn from_json_parses_handwritten_terminal_message() {
        // 手工书写的线格式 JSON，字段值独立于构造器推导。
        // 除 type 外的全部字段（含 timestamp/session_id/token）都位于 payload 对象内
        let json = r#"{"type":"terminal","payload":{"payload":{"action":{"type":"output","data":"aGVsbG8=","is_waiting":true,"index":7,"end_index":9,"start_offset":100,"end_offset":200}},"message_id":"m-1","expect_response":true,"timestamp":123456789,"session_id":"s1","token":"tok-1"}}"#;
        let m = Message::from_json(json).unwrap();
        match &m {
            Message::Terminal {
                message_id,
                expect_response,
                timestamp,
                session_id,
                token,
                payload: TerminalPayload {
                    action:
                        TerminalAction::Output {
                            data,
                            is_waiting,
                            index,
                            end_index,
                            start_offset,
                            end_offset,
                        },
                },
            } => {
                assert_eq!(message_id, "m-1");
                assert!(*expect_response);
                assert_eq!(*timestamp, 123456789);
                assert_eq!(session_id, "s1");
                assert_eq!(token, "tok-1");
                assert_eq!(data, "aGVsbG8=");
                assert!(*is_waiting);
                assert_eq!(*index, 7);
                assert_eq!(*end_index, Some(9));
                assert_eq!(*start_offset, Some(100));
                assert_eq!(*end_offset, Some(200));
            }
            _ => panic!("期望 terminal 消息"),
        }
    }

    #[test]
    fn from_json_fills_defaults_for_absent_fields() {
        // 线格式允许省略 message_id/token/expect_response，反序列化应补默认值
        let json = r#"{"type":"terminal","payload":{"payload":{"action":{"type":"unsubscribe"}},"timestamp":1,"session_id":"s1"}}"#;
        let m = Message::from_json(json).unwrap();
        assert!(!m.message_id().unwrap().is_empty());
        assert!(!m.expect_response());
        assert_eq!(m.token(), "");
        assert!(matches!(
            &m,
            Message::Terminal {
                payload: TerminalPayload {
                    action: TerminalAction::Unsubscribe,
                },
                ..
            }
        ));
    }

    #[test]
    fn from_json_rejects_unknown_message_type() {
        // 未知 type 标签应反序列化失败，防止静默吞掉协议错位
        assert!(Message::from_json(r#"{"type":"bogus","payload":{}}"#).is_err());
    }

    #[test]
    fn json_round_trip_preserves_all_variants() {
        // 构造器产物经 to_json/from_json 往返后，序列化结果应完全一致
        let cases = vec![
            Message::file_service(FileServicePayload::Withdraw {}),
            Message::output("s", b"abc", true, 1),
            Message::output_from_base64("s", "YWJj", false, 2, Some(3), Some(4), Some(5)),
            Message::input("s", "ls", None),
            Message::input_with_response("s", "cd", Some(KeyCombo::parse("enter").unwrap())),
            Message::subscribe("s", Some(9)),
            Message::subscribe_with_response("s", None),
            Message::subscribe_response_with_request_id(
                "s", 1, 2, 3, SubscribeMode::Reset, 4, 5, "req-1",
            ),
            Message::unsubscribe("s"),
            Message::unsubscribe_with_response("s"),
            Message::unsubscribe_response_with_request_id("s", "req-2"),
            Message::session_control(
                SessionControlAction::ResizeSession {
                    session_id: "s".to_string(),
                    cols: 80,
                    rows: 24,
                },
                Some("s"),
            ),
            Message::session_config(SessionConfigAction::ListQuickActions, None),
            Message::auth(
                Some("s".into()),
                AuthPayload {
                    stage: AuthStage::Authenticated,
                    device_id: Some("d1".into()),
                    ..Default::default()
                },
            ),
            Message::error("E", "m"),
            Message::error_with_id("req-3", "E", "m"),
            Message::server_closed("bye", false),
            Message::client_disconnected("pixel", "bye"),
            Message::session_event("stopped", sample_session(), "pixel"),
            Message::ack("req-4"),
            Message::ack_failure("req-4", 42, "nope"),
            Message::sync_data(SyncPayload::TaskQueueChanged {
                session_id: "s".to_string(),
                queue_count: 3,
                action: "add".to_string(),
                task_id: Some("t1".to_string()),
                status: Some("pending".to_string()),
            }),
        ];
        for m in cases {
            let json = m.to_json().unwrap();
            let back = Message::from_json(&json).unwrap();
            assert_eq!(
                serde_json::to_value(&back).unwrap(),
                serde_json::to_value(&m).unwrap(),
                "变体往返不一致: {json}"
            );
        }
    }

    #[test]
    fn serde_value_round_trip_preserves_rich_payloads() {
        // 含嵌套结构的代表变体走 serde to_value/from_value 往返，验证内部标签与内容分发
        let cases = vec![
            Message::file_service(FileServicePayload::Announce {
                port: 41234,
                token: "t".to_string(),
                device_name: "my-phone".to_string(),
                mounts: vec![MountAnnouncement {
                    plugin_id: "com.bedcode.file-transfer".to_string(),
                    mount_path: "files".to_string(),
                    operations: vec![FileOperation::List, FileOperation::Download],
                }],
            }),
            Message::session_control(
                SessionControlAction::SessionChanged {
                    change_type: "created".to_string(),
                    session: sample_session(),
                },
                None,
            ),
            Message::sync_data(SyncPayload::TaskStatusChanged {
                session_id: "s".to_string(),
                task_status: "asking".to_string(),
                task_reason: Some("needs approval".to_string()),
                task_questions: None,
            }),
            Message::session_event("created", sample_session(), "pixel"),
        ];
        for m in cases {
            let value = serde_json::to_value(&m).unwrap();
            let back: Message = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(&back).unwrap(), value);
        }
    }

    // ==================== WS 消息转换 ====================

    #[test]
    fn to_ws_message_wraps_json_text() {
        let m = Message::ack("req-1");
        match m.to_ws_message().unwrap() {
            tokio_tungstenite::tungstenite::Message::Text(text) => {
                // WS 载荷应为合法 JSON，且可反解析回同一语义消息
                let back: Message = serde_json::from_str(&text).unwrap();
                assert_eq!(back.message_type(), Some("ack"));
                assert_eq!(back.message_id(), None);
            }
            other => panic!("期望 Text 消息，得到 {other:?}"),
        }
    }

    #[test]
    fn from_ws_message_parses_text_and_binary() {
        let m = Message::input_with_response("s1", "echo hi", None);
        let json = m.to_json().unwrap();

        let from_text = Message::from_ws_message(tokio_tungstenite::tungstenite::Message::Text(
            json.clone().into(),
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            serde_json::to_value(&from_text).unwrap(),
            serde_json::to_value(&m).unwrap()
        );

        let from_binary = Message::from_ws_message(tokio_tungstenite::tungstenite::Message::Binary(
            json.into_bytes(),
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            serde_json::to_value(&from_binary).unwrap(),
            serde_json::to_value(&m).unwrap()
        );
    }

    #[test]
    fn from_ws_message_ignores_heartbeat_frames() {
        // 心跳帧由 tungstenite 自动处理，业务层应返回 None 而非构造伪消息
        assert!(
            Message::from_ws_message(tokio_tungstenite::tungstenite::Message::Ping(vec![]))
                .unwrap()
                .is_none()
        );
        assert!(
            Message::from_ws_message(tokio_tungstenite::tungstenite::Message::Pong(vec![]))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn from_ws_message_converts_close_to_error() {
        // 对端关闭时业务层需感知，转换为 error 消息（code="close"）
        let m = Message::from_ws_message(tokio_tungstenite::tungstenite::Message::Close(None))
            .unwrap()
            .unwrap();
        match &m {
            Message::Error { code, message, .. } => {
                assert_eq!(code, "close");
                assert_eq!(message, "");
            }
            _ => panic!("期望 error 消息"),
        }

        // 携带关闭帧时，reason 应透传到错误消息中（只取 reason 字段，不含关闭码）
        let frame = CloseFrame {
            code: CloseCode::Normal,
            reason: "server shutting down".into(),
        };
        let m2 = Message::from_ws_message(tokio_tungstenite::tungstenite::Message::Close(Some(frame)))
            .unwrap()
            .unwrap();
        match &m2 {
            Message::Error { code, message, .. } => {
                assert_eq!(code, "close");
                assert_eq!(message, "server shutting down");
            }
            _ => panic!("期望 error 消息"),
        }
    }

    #[test]
    fn from_ws_message_rejects_invalid_json() {
        assert!(
            Message::from_ws_message(tokio_tungstenite::tungstenite::Message::Text(
                "not-json".into()
            ))
            .is_err()
        );
    }

    // ==================== 请求-响应关联 ====================

    #[test]
    fn with_request_id_correlates_ack_response() {
        // 请求-响应闭环：请求方生成的 ID 应能经 with_request_id 固化，再经 ack 回传匹配
        let req = Message::input_with_response("s1", "ls", None);
        let req_id = req.message_id().unwrap().to_string();
        assert!(!req_id.is_empty());

        let with_id = req.with_request_id("custom-req");
        assert_eq!(with_id.message_id(), Some("custom-req"));
        // 覆盖 request_id 不应改变其他字段语义
        assert!(with_id.expect_response());
        assert_eq!(with_id.message_type(), Some("terminal"));

        let ack = Message::ack("custom-req");
        match &ack {
            Message::Ack {
                request_id,
                code,
                ..
            } => {
                assert_eq!(request_id, "custom-req");
                assert_eq!(*code, ACK_CODE_SUCCESS);
            }
            _ => panic!("期望 ack 消息"),
        }

        // 错误响应同样可关联同一请求
        let err = Message::error_with_id("custom-req", "E_X", "failed");
        assert_eq!(err.message_id(), Some("custom-req"));
        let err2 = Message::error("E_X", "failed").with_request_id("custom-req");
        assert_eq!(err2.message_id(), Some("custom-req"));
    }

    #[test]
    fn with_request_id_is_noop_for_notifications() {
        // 通知类消息无 message_id 可覆盖，设置 request_id 应保持原样
        let closed = Message::server_closed("bye", false).with_request_id("req-9");
        assert_eq!(closed.message_type(), Some("server_closed"));
        assert_eq!(closed.message_id(), None);
        match &closed {
            Message::ServerClosed { reason, .. } => assert_eq!(reason, "bye"),
            _ => panic!("期望 server_closed 消息"),
        }

        let sync = Message::sync_data(SyncPayload::SessionModeChanged {
            session_id: "s".to_string(),
            auto_approve: false,
        })
        .with_request_id("req-9");
        assert_eq!(sync.message_id(), None);
    }
}