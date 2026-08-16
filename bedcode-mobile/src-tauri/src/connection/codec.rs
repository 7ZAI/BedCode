//! WebSocket Message Codec
//!
//! 可插拔的消息编解码器，支持 JSON、Protobuf 等格式

use crate::model::message::Message;
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

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::control::{TerminalAction, TerminalPayload};
    use crate::enums::special_key::KeyCombo;
    use std::borrow::Cow;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::{CloseCode, Data, OpCode};
    use tokio_tungstenite::tungstenite::protocol::frame::Frame;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;

    fn codec() -> JsonCodec {
        JsonCodec::new()
    }

    #[test]
    fn test_name() {
        assert_eq!(codec().name(), "JsonCodec");
    }

    #[test]
    fn test_encode_returns_text_frame() {
        // 编码结果必须是 Text 帧，且 JSON 含类型与关键字段
        let msg = Message::input("sess-1", "ls -la", None);
        match codec().encode(&msg).unwrap() {
            WsMsg::Text(json) => {
                let text = json.to_string();
                assert!(text.contains(r#""type":"terminal""#), "实际: {}", text);
                assert!(text.contains(r#""session_id":"sess-1""#), "实际: {}", text);
            }
            other => panic!("期望 Text 帧，实际: {:?}", other),
        }
    }

    #[test]
    fn test_text_roundtrip_preserves_message() {
        // Text 帧 round-trip：序列化结果应逐字节一致（时间戳在构造时固定）
        let msg = Message::input_with_response("s", "pwd", None).with_request_id("req-77");
        let ws = codec().encode(&msg).unwrap();
        match codec().decode(ws).unwrap() {
            Some(back) => {
                assert_eq!(back.to_json().unwrap(), msg.to_json().unwrap());
                assert_eq!(back.message_id(), Some("req-77"));
            }
            None => panic!("文本消息应解码出业务消息"),
        }
    }

    #[test]
    fn test_roundtrip_preserves_special_key() {
        // 特殊按键（Ctrl+C）随载荷无损往返
        let msg = Message::input("s", "", Some(KeyCombo::parse("ctrl+c").unwrap()));
        let ws = codec().encode(&msg).unwrap();
        match codec().decode(ws).unwrap().unwrap() {
            Message::Terminal {
                session_id,
                payload: TerminalPayload { action: TerminalAction::Input { data, special_key }, .. },
                ..
            } => {
                assert_eq!(session_id, "s");
                assert_eq!(data, "");
                assert_eq!(special_key, Some(KeyCombo::parse("ctrl+c").unwrap()));
            }
            other => panic!("期望 terminal input，实际: {:?}", other),
        }
    }

    #[test]
    fn test_decode_ping_pong_returns_none() {
        // 协议层心跳帧不产生业务消息
        assert!(codec().decode(WsMsg::Ping(vec![].into())).unwrap().is_none());
        assert!(codec().decode(WsMsg::Pong(vec![].into())).unwrap().is_none());
    }

    #[test]
    fn test_decode_raw_frame_returns_none() {
        // 原始帧透传场景（底层已被拆帧）不产生业务消息
        let frame = Frame::message(b"ignored".to_vec(), OpCode::Data(Data::Text), true);
        assert!(codec().decode(WsMsg::Frame(frame)).unwrap().is_none());
    }

    #[test]
    fn test_decode_binary_json_message() {
        // Binary 帧按 UTF-8 文本解析，合法 JSON 可解码为业务消息
        let msg = Message::input("s", "echo hi", None);
        let json = msg.to_json().unwrap();
        let ws = WsMsg::Binary(json.into_bytes().into());
        match codec().decode(ws).unwrap() {
            Some(back) => assert_eq!(back.to_json().unwrap(), msg.to_json().unwrap()),
            None => panic!("合法 JSON 的 Binary 帧应解码成功"),
        }
    }

    #[test]
    fn test_decode_invalid_json_errors() {
        // 非法 JSON：应返回 Err 而非静默吞掉（调用方据此记日志/跳帧）
        let err = codec().decode(WsMsg::Text("{not valid json".into())).unwrap_err();
        assert!(
            matches!(err, crate::AppError::Serialization(_)),
            "期望 Serialization 错误，实际: {}",
            err
        );
    }

    #[test]
    fn test_decode_invalid_utf8_binary_errors() {
        // 非法 UTF-8 的 Binary 经 lossy 转换后仍是非法 JSON → Err
        assert!(codec().decode(WsMsg::Binary(vec![0xff, 0xfe, 0x00].into())).is_err());
    }

    #[test]
    fn test_decode_close_converts_to_error() {
        // Close 帧转 error 消息（code=close）。
        // 注意：tungstenite 0.24 的 CloseFrame Display 为 "{reason} ({code})"，
        // 当前实现用 to_string() 导致 code 被拼进 message（既有 bug，见
        // .scratch/test-coverage-bugs.md），此处锁定实际行为
        let frame = CloseFrame {
            code: CloseCode::Normal,
            reason: Cow::Owned("going away".into()),
        };
        match codec().decode(WsMsg::Close(Some(frame))).unwrap().unwrap() {
            Message::Error { code, message, .. } => {
                assert_eq!(code, "close");
                assert_eq!(message, "going away (1000)");
            }
            other => panic!("期望 error 消息，实际: {:?}", other),
        }

        // 无原因的 Close 帧 → 空字符串
        match codec().decode(WsMsg::Close(None)).unwrap().unwrap() {
            Message::Error { message, .. } => assert_eq!(message, ""),
            other => panic!("期望 error 消息，实际: {:?}", other),
        }
    }
}