//! WebSocket Message Types
//!
//! 统一的业务消息类型，作为 WebSocket 客户端和服务端的业务传输类型

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::enums::auth::AuthPayload;
use crate::shared::enums::control::{SessionConfigAction, SessionConfigPayload, SessionControlAction, SessionControlPayload};
use crate::shared::enums::special_key::SpecialKey;
use crate::shared::enums::sumary::SessionSummary;

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

/// 统一的 WebSocket 消息类型
/// 作为 WebSocket 客户端和服务端的业务传输类型
/// 直接对应 JSON 序列化的结构
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Message {
    // ==================== 业务消息类型 ====================

    /// 输出消息 (服务端 → 客户端)
    /// PTY 输出数据推送到客户端
    #[serde(rename = "output")]
    Output {
        /// 唯一消息ID，用于请求-响应跟踪
        #[serde(default = "generate_message_id")]
        message_id: String,
        /// 是否需要服务端响应
        #[serde(default)]
        expect_response: bool,
        /// 时间戳（毫秒）
        timestamp: i64,
        session_id: String,
        payload: OutputPayload,
    },

    /// 输入消息 (客户端 → 服务端)
    /// 客户端发送输入到 PTY
    #[serde(rename = "input")]
    Input {
        #[serde(default = "generate_message_id")]
        message_id: String,
        #[serde(default)]
        expect_response: bool,
        timestamp: i64,
        session_id: String,
        payload: InputPayload,
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
    },

    /// 订阅输出 (客户端 → 服务端)
    /// 客户端订阅会话输出，实现增量同步
    #[serde(rename = "subscribe")]
    Subscribe {
        #[serde(default = "generate_message_id")]
        message_id: String,
        #[serde(default)]
        expect_response: bool,
        timestamp: i64,
        session_id: String,
        /// 起始序号，不指定则从头补完
        #[serde(skip_serializing_if = "Option::is_none")]
        start_seq: Option<u64>,
    },

    /// 订阅响应 (服务端 → 客户端)
    #[serde(rename = "subscribe_response")]
    SubscribeResponse {
        #[serde(default = "generate_message_id")]
        message_id: String,
        #[serde(default)]
        expect_response: bool,
        timestamp: i64,
        session_id: String,
        /// 当前最大序号
        current_max_seq: u64,
        /// 历史消息数量
        history_count: usize,
    },

    /// 取消订阅 (客户端 → 服务端)
    #[serde(rename = "unsubscribe")]
    Unsubscribe {
        #[serde(default = "generate_message_id")]
        message_id: String,
        #[serde(default)]
        expect_response: bool,
        timestamp: i64,
        session_id: String,
    },

    /// 取消订阅响应 (服务端 → 客户端)
    #[serde(rename = "unsubscribe_response")]
    UnsubscribeResponse {
        #[serde(default = "generate_message_id")]
        message_id: String,
        #[serde(default)]
        expect_response: bool,
        timestamp: i64,
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

// ==================== Payload 类型 ====================

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

// ==================== 辅助方法 ====================

impl Message {
    /// 创建输出消息
    pub fn output(session_id: &str, data: &[u8], is_waiting: bool, index: usize) -> Self {
        Message::Output {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
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
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            payload: InputPayload {
                data: data.to_string(),
                special_key,
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
            payload,
        }
    }

    /// 创建错误消息
    pub fn error(code: &str, message: &str) -> Self {
        Message::Error {
            message_id: None,
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
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
            code: code.to_string(),
            message: message.to_string(),
        }
    }

    /// 创建服务端关闭消息
    pub fn server_closed(reason: &str, will_reconnect: bool) -> Self {
        Message::ServerClosed {
            reason: reason.to_string(),
            will_reconnect,
        }
    }

    /// 创建订阅消息
    pub fn subscribe(session_id: &str, start_seq: Option<u64>) -> Self {
        Message::Subscribe {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            start_seq,
        }
    }

    /// 创建订阅响应消息
    pub fn subscribe_response(session_id: &str, current_max_seq: u64, history_count: usize) -> Self {
        Message::SubscribeResponse {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
            current_max_seq,
            history_count,
        }
    }

    /// 创建取消订阅消息
    pub fn unsubscribe(session_id: &str) -> Self {
        Message::Unsubscribe {
            message_id: generate_message_id(),
            expect_response: false,
            timestamp: Utc::now().timestamp_millis(),
            session_id: session_id.to_string(),
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
            Message::SessionControl { message_id, .. } => Some(message_id),
            Message::SessionConfig { message_id, .. } => Some(message_id),
            Message::Error { message_id, .. } => message_id.as_deref(),
            Message::ServerClosed { .. } => None,
            Message::ClientDisconnected { .. } => None,
            Message::SessionEvent { .. } => None,
            Message::Subscribe { message_id, .. } => Some(message_id),
            Message::SubscribeResponse { message_id, .. } => Some(message_id),
            Message::Unsubscribe { message_id, .. } => Some(message_id),
            Message::UnsubscribeResponse { message_id, .. } => Some(message_id),
        }
    }

    /// 获取 expect_response 标记
    pub fn expect_response(&self) -> bool {
        match self {
            Message::Output { expect_response, .. } => *expect_response,
            Message::Input { expect_response, .. } => *expect_response,
            Message::Auth { expect_response, .. } => *expect_response,
            Message::SessionControl { expect_response, .. } => *expect_response,
            Message::SessionConfig { expect_response, .. } => *expect_response,
            Message::Error { expect_response, .. } => *expect_response,
            Message::Subscribe { expect_response, .. } => *expect_response,
            Message::SubscribeResponse { expect_response, .. } => *expect_response,
            Message::Unsubscribe { expect_response, .. } => *expect_response,
            Message::UnsubscribeResponse { expect_response, .. } => *expect_response,
            Message::ServerClosed { .. } => false,
            Message::ClientDisconnected { .. } => false,
            Message::SessionEvent { .. } => false,
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