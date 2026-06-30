//! WebSocket Message Codec
//!
//! 可插拔的消息编解码器，支持 JSON、Protobuf 等格式

use crate::shared::model::message::Message;
use crate::Result;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

/// 消息编解码器 trait
/// 支持自定义编解码格式，如 JSON、MessagePack、Protobuf 等
pub trait MessageCodec: Send + Sync {
    /// 编码消息为 WebSocket 消息
    fn encode(&self, msg: &Message) -> Result<WsMsg>;

    /// 解码 WebSocket 消息为 Message
    fn decode(&self, msg: WsMsg) -> Result<Option<Message>>;

    /// 编解码器名称
    fn name(&self) -> &str;
}

/// JSON 编解码器（默认实现）
#[derive(Debug, Clone, Default)]
pub struct JsonCodec;

impl JsonCodec {
    pub fn new() -> Self {
        Self
    }
}

impl MessageCodec for JsonCodec {
    fn encode(&self, msg: &Message) -> Result<WsMsg> {
        let json = msg.to_json()?;
        Ok(WsMsg::Text(json))
    }

    fn decode(&self, msg: WsMsg) -> Result<Option<Message>> {
        match msg {
            WsMsg::Text(text) => {
                let msg = Message::from_json(&text)?;
                Ok(Some(msg))
            }
            WsMsg::Binary(data) => {
                // 将二进制数据作为 base64 编码的文本处理
                let text = String::from_utf8_lossy(&data);
                let msg = Message::from_json(&text)?;
                Ok(Some(msg))
            }
            WsMsg::Ping(_) | WsMsg::Pong(_) => Ok(None), // 协议层心跳由 tungstenite 自动处理
            WsMsg::Close(reason) => {
                Ok(Some(Message::error("close", &reason.map(|r| r.to_string()).unwrap_or_default())))
            }
            WsMsg::Frame(_) => Ok(None),
        }
    }

    fn name(&self) -> &str {
        "JsonCodec"
    }
}

/// 默认编解码器
pub type DefaultCodec = JsonCodec;