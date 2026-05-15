//! WebSocket Model - WebSocket message types

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub(crate) fn generate_message_id() -> String {
    Uuid::new_v4().to_string()
}

pub(crate) fn current_timestamp() -> i64 {
    Utc::now().timestamp_millis()
}

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum WsMessage {
    Text {
        #[serde(default = "generate_message_id")]
        message_id: String,
        timestamp: i64,
        payload: TextPayload,
    },
    Binary {
        #[serde(default = "generate_message_id")]
        message_id: String,
        timestamp: i64,
        payload: BinaryPayload,
    },
    Ping {
        timestamp: i64,
    },
    Pong {
        timestamp: i64,
    },
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        code: String,
        message: String,
    },
    Close {
        reason: String,
    },
    Ack {
        original_id: String,
        timestamp: i64,
    },
}

/// Text payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPayload {
    pub content: String,
}

/// Binary payload (Base64 encoded)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryPayload {
    pub data: String,
}

/// WebSocket message type enumeration
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

impl WsMessage {
    pub fn text(content: impl Into<String>) -> Self {
        WsMessage::Text {
            message_id: generate_message_id(),
            timestamp: Utc::now().timestamp_millis(),
            payload: TextPayload {
                content: content.into(),
            },
        }
    }

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

    pub fn ping() -> Self {
        WsMessage::Ping {
            timestamp: Utc::now().timestamp_millis(),
        }
    }

    pub fn pong() -> Self {
        WsMessage::Pong {
            timestamp: Utc::now().timestamp_millis(),
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        WsMessage::Error {
            message_id: None,
            code: code.into(),
            message: message.into(),
        }
    }

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

    pub fn close(reason: impl Into<String>) -> Self {
        WsMessage::Close {
            reason: reason.into(),
        }
    }

    pub fn ack(original_id: impl Into<String>) -> Self {
        WsMessage::Ack {
            original_id: original_id.into(),
            timestamp: Utc::now().timestamp_millis(),
        }
    }

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

    pub fn to_json(&self) -> crate::Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(json: &str) -> crate::Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn to_ws_message(&self) -> crate::Result<tokio_tungstenite::tungstenite::Message> {
        let json = self.to_json()?;
        Ok(tokio_tungstenite::tungstenite::Message::Text(json))
    }

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
            tokio_tungstenite::tungstenite::Message::Ping(_) => {
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