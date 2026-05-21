//! Message Types
//!
//! WebSocket 消息主类型定义

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::auth::AuthPayload;
use super::control::{ControlAction, ControlPayload};
use super::special_key::SpecialKey;
use super::sumary::SessionSummary;

/// 输出载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputPayload {
    /// Base64 编码的输出数据
    pub data: String,
    /// 是否等待输入
    pub is_waiting: bool,
    /// 全局递增索引，用于去重
    pub index: usize,
}

/// 输入载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputPayload {
    /// 输入数据
    pub data: String,
    /// 特殊键
    #[serde(skip_serializing_if = "Option::is_none")]
    pub special_key: Option<SpecialKey>,
}

/// WebSocket 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    /// 输出消息 (服务端 → 客户端)
    #[serde(rename = "output")]
    Output {
        /// Unique message ID for request-response tracking
        #[serde(default = "generate_message_id")]
        message_id: String,
        session_id: String,
        timestamp: i64,
        payload: OutputPayload,
    },

    /// 输入消息 (客户端 → 服务端)
    #[serde(rename = "input")]
    Input {
        /// Unique message ID for request-response tracking
        #[serde(default = "generate_message_id")]
        message_id: String,
        session_id: String,
        timestamp: i64,
        payload: InputPayload,
    },

    /// 认证消息 (双向)
    #[serde(rename = "auth")]
    Auth {
        /// Unique message ID for request-response tracking
        #[serde(default = "generate_message_id")]
        message_id: String,
        session_id: Option<String>,
        timestamp: i64,
        payload: AuthPayload,
    },

    /// 控制消息 (双向)
    #[serde(rename = "control")]
    Control {
        /// Unique message ID for request-response tracking
        #[serde(default = "generate_message_id")]
        message_id: String,
        session_id: Option<String>,
        timestamp: i64,
        payload: ControlPayload,
    },

    /// 错误消息 (服务端 → 客户端)
    #[serde(rename = "error")]
    Error {
        /// Message ID this error relates to (if any)
        #[serde(skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        code: String,
        message: String,
    },

    /// 心跳消息 (双向)
    #[serde(rename = "heartbeat")]
    Heartbeat {
        /// Unique message ID for request-response tracking
        #[serde(default = "generate_message_id")]
        message_id: String,
        timestamp: i64,
    },

    /// 服务端关闭通知 (服务端 → 客户端)
    /// 桌面端退出时通知所有移动端连接已断开
    #[serde(rename = "server_closed")]
    ServerClosed {
        /// 关闭原因
        reason: String,
        /// 是否会重连（目前桌面端退出后不会重连）
        will_reconnect: bool,
    },

    /// 订阅输出 (客户端 → 服务端)
    #[serde(rename = "subscribe")]
    Subscribe {
        /// Unique message ID for request-response tracking
        #[serde(default = "generate_message_id")]
        message_id: String,
        session_id: String,
        /// 起始序号，不指定则从头补完
        #[serde(skip_serializing_if = "Option::is_none")]
        start_seq: Option<u64>,
    },

    /// 订阅响应 (服务端 → 客户端)
    #[serde(rename = "subscribe_response")]
    SubscribeResponse {
        /// Unique message ID for request-response tracking
        #[serde(default = "generate_message_id")]
        message_id: String,
        session_id: String,
        /// 当前最大序号
        current_max_seq: u64,
        /// 历史消息数量
        history_count: usize,
    },

    /// 取消订阅 (客户端 → 服务端)
    #[serde(rename = "unsubscribe")]
    Unsubscribe {
        /// Unique message ID for request-response tracking
        #[serde(default = "generate_message_id")]
        message_id: String,
        session_id: String,
    },

    /// 客户端断开通知 (服务端 → 客户端)
    /// 移动端断开连接时通知其他客户端
    #[serde(rename = "client_disconnected")]
    ClientDisconnected {
        /// 断开的设备名称
        device_name: String,
        /// 断开原因
        reason: String,
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
    },
}

/// Generate a unique message ID
fn generate_message_id() -> String {
    Uuid::new_v4().to_string()
}

impl Message {
    /// 创建输出消息
    pub fn output(session_id: &str, data: &[u8], is_waiting: bool, index: usize) -> Self {
        Message::Output {
            message_id: generate_message_id(),
            session_id: session_id.to_string(),
            timestamp: Utc::now().timestamp_millis(),
            payload: OutputPayload {
                data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data),
                is_waiting,
                index,
            },
        }
    }

    /// 创建输入消息
    pub fn input(session_id: &str, data: &str, special_key: Option<SpecialKey>) -> Self {
        Message::Input {
            message_id: generate_message_id(),
            session_id: session_id.to_string(),
            timestamp: Utc::now().timestamp_millis(),
            payload: InputPayload {
                data: data.to_string(),
                special_key,
            },
        }
    }

    /// 创建控制消息
    pub fn control(action: ControlAction, session_id: Option<&str>) -> Self {
        Message::Control {
            message_id: generate_message_id(),
            session_id: session_id.map(|s| s.to_string()),
            timestamp: Utc::now().timestamp_millis(),
            payload: ControlPayload { action },
        }
    }

    /// 创建错误消息
    pub fn error(code: &str, message: &str) -> Self {
        Message::Error {
            message_id: None,
            code: code.to_string(),
            message: message.to_string(),
        }
    }

    /// 创建错误消息（关联到特定消息ID）
    pub fn error_with_id(message_id: &str, code: &str, message: &str) -> Self {
        Message::Error {
            message_id: Some(message_id.to_string()),
            code: code.to_string(),
            message: message.to_string(),
        }
    }

    /// 创建心跳消息
    pub fn heartbeat() -> Self {
        Message::Heartbeat {
            message_id: generate_message_id(),
            timestamp: Utc::now().timestamp_millis(),
        }
    }

    /// 创建服务端关闭消息
    pub fn server_closed(reason: &str, will_reconnect: bool) -> Self {
        Message::ServerClosed {
            reason: reason.to_string(),
            will_reconnect,
        }
    }

    /// 创建客户端断开通知
    pub fn client_disconnected(device_name: &str, reason: &str) -> Self {
        Message::ClientDisconnected {
            device_name: device_name.to_string(),
            reason: reason.to_string(),
        }
    }

    /// 创建会话事件通知
    pub fn session_event(event_type: &str, session: SessionSummary, device_name: &str) -> Self {
        Message::SessionEvent {
            event_type: event_type.to_string(),
            session,
            device_name: device_name.to_string(),
        }
    }

    /// 获取消息ID
    pub fn message_id(&self) -> Option<&str> {
        match self {
            Message::Output { message_id, .. } => Some(message_id),
            Message::Input { message_id, .. } => Some(message_id),
            Message::Auth { message_id, .. } => Some(message_id),
            Message::Control { message_id, .. } => Some(message_id),
            Message::Error { message_id, .. } => message_id.as_deref(),
            Message::Heartbeat { message_id, .. } => Some(message_id),
            Message::ServerClosed { .. } => None,
            Message::ClientDisconnected { .. } => None,
            Message::SessionEvent { .. } => None,
            Message::Subscribe { message_id, .. } => Some(message_id),
            Message::SubscribeResponse { message_id, .. } => Some(message_id),
            Message::Unsubscribe { message_id, .. } => Some(message_id),
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
}