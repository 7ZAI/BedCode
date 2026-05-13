//! WebSocket Message Types
//!
//! 不包含业务逻辑的基础消息抽象
//! 提供消息序列化/反序列化的基础设施

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 生成唯一消息ID
pub(crate) fn generate_message_id() -> String {
    Uuid::new_v4().to_string()
}

/// 获取当前时间戳（毫秒）
pub(crate) fn current_timestamp() -> i64 {
    Utc::now().timestamp_millis()
}

/// WebSocket 消息（顶级抽象）
/// 不包含任何业务相关的字段，仅提供基础框架
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum WsMessage {
    /// 文本消息
    Text {
        #[serde(default = "generate_message_id")]
        message_id: String,
        timestamp: i64,
        payload: TextPayload,
    },

    /// 二进制消息
    Binary {
        #[serde(default = "generate_message_id")]
        message_id: String,
        timestamp: i64,
        payload: BinaryPayload,
    },

    /// 心跳/ping 消息
    Ping {
        timestamp: i64,
    },

    /// 心跳/ pong 响应
    Pong {
        timestamp: i64,
    },

    /// 错误消息
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        code: String,
        message: String,
    },

    /// 连接关闭通知
    Close {
        reason: String,
    },

    /// 确认消息（用于请求-响应模式）
    Ack {
        /// 对应的请求消息ID
        original_id: String,
        timestamp: i64,
    },
}

/// 文本消息载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPayload {
    /// 消息内容
    pub content: String,
}

/// 二进制消息载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryPayload {
    /// Base64 编码的二进制数据
    pub data: String,
}

impl WsMessage {
    /// 创建文本消息
    pub fn text(content: impl Into<String>) -> Self {
        WsMessage::Text {
            message_id: generate_message_id(),
            timestamp: Utc::now().timestamp_millis(),
            payload: TextPayload {
                content: content.into(),
            },
        }
    }

    /// 创建二进制消息
    pub fn binary(data: impl AsRef<[u8]>) -> Self {
        WsMessage::Binary {
            message_id: generate_message_id(),
            timestamp: Utc::now().timestamp_millis(),
            payload: BinaryPayload {
                data: base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    data.as_ref(),
                ),
            },
        }
    }

    /// 创建 Ping 消息
    pub fn ping() -> Self {
        WsMessage::Ping {
            timestamp: Utc::now().timestamp_millis(),
        }
    }

    /// 创建 Pong 消息
    pub fn pong() -> Self {
        WsMessage::Pong {
            timestamp: Utc::now().timestamp_millis(),
        }
    }

    /// 创建错误消息
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        WsMessage::Error {
            message_id: None,
            code: code.into(),
            message: message.into(),
        }
    }

    /// 创建带消息ID的错误消息
    pub fn error_with_id(
        message_id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        WsMessage::Error {
            message_id: Some(message_id.into()),
            code: code.into(),
            message: message.into(),
        }
    }

    /// 创建关闭消息
    pub fn close(reason: impl Into<String>) -> Self {
        WsMessage::Close {
            reason: reason.into(),
        }
    }

    /// 创建确认消息
    pub fn ack(original_id: impl Into<String>) -> Self {
        WsMessage::Ack {
            original_id: original_id.into(),
            timestamp: Utc::now().timestamp_millis(),
        }
    }

    /// 获取消息ID
    pub fn message_id(&self) -> Option<&str> {
        match self {
            WsMessage::Text { message_id, .. } => Some(message_id),
            WsMessage::Binary { message_id, .. } => Some(message_id),
            WsMessage::Error { message_id, .. } => message_id.as_deref(),
            WsMessage::Ack { original_id, .. } => Some(original_id),
            WsMessage::Ping { .. } => None,
            WsMessage::Pong { .. } => None,
            WsMessage::Close { .. } => None,
        }
    }

    /// 获取消息类型
    pub fn message_type(&self) -> WsMessageType {
        match self {
            WsMessage::Text { .. } => WsMessageType::Text,
            WsMessage::Binary { .. } => WsMessageType::Binary,
            WsMessage::Ping { .. } => WsMessageType::Ping,
            WsMessage::Pong { .. } => WsMessageType::Pong,
            WsMessage::Error { .. } => WsMessageType::Error,
            WsMessage::Close { .. } => WsMessageType::Close,
            WsMessage::Ack { .. } => WsMessageType::Ack,
        }
    }

    /// 序列化为 JSON 字符串
    pub fn to_json(&self) -> crate::Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    /// 从 JSON 字符串反序列化
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
                // 将二进制数据作为 base64 编码的文本消息处理
                let text = String::from_utf8_lossy(&data);
                Ok(Some(serde_json::from_str(&text)?))
            }
            tokio_tungstenite::tungstenite::Message::Ping(_) => {
                // 收到 ping，返回 pong
                Ok(Some(WsMessage::pong()))
            }
            tokio_tungstenite::tungstenite::Message::Pong(_) => Ok(Some(WsMessage::pong())),
            tokio_tungstenite::tungstenite::Message::Close(reason) => {
                Ok(Some(WsMessage::close(reason.map(|r| r.to_string()).unwrap_or_default())))
            }
            tokio_tungstenite::tungstenite::Message::Frame(_) => Ok(None),
        }
    }
}

/// WebSocket 消息类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsMessageType {
    Text,
    Binary,
    Ping,
    Pong,
    Error,
    Close,
    Ack,
}

impl std::fmt::Display for WsMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WsMessageType::Text => write!(f, "text"),
            WsMessageType::Binary => write!(f, "binary"),
            WsMessageType::Ping => write!(f, "ping"),
            WsMessageType::Pong => write!(f, "pong"),
            WsMessageType::Error => write!(f, "error"),
            WsMessageType::Close => write!(f, "close"),
            WsMessageType::Ack => write!(f, "ack"),
        }
    }
}