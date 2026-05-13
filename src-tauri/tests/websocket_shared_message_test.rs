//! Unit tests for the shared WebSocket message module
//!
//! Tests WsMessage types

use bedcode_lib::{
    WsMessage, WsMessageType, TextPayload, BinaryPayload,
};

mod ws_message_tests {
    use super::*;

    #[test]
    fn test_ws_message_text_creation() {
        let msg = WsMessage::text("Hello, World!");

        match &msg {
            WsMessage::Text { message_id, timestamp, payload } => {
                assert!(!message_id.is_empty());
                assert!(*timestamp > 0);
                assert_eq!(payload.content, "Hello, World!");
            }
            _ => panic!("Expected Text message"),
        }
    }

    #[test]
    fn test_ws_message_binary_creation() {
        let msg = WsMessage::binary(b"Binary data");

        match &msg {
            WsMessage::Binary { message_id, timestamp, payload } => {
                assert!(!message_id.is_empty());
                assert!(*timestamp > 0);
                // Data should be base64 encoded
                let decoded = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    &payload.data
                ).unwrap();
                assert_eq!(decoded, b"Binary data");
            }
            _ => panic!("Expected Binary message"),
        }
    }

    #[test]
    fn test_ws_message_ping() {
        let msg = WsMessage::ping();

        match &msg {
            WsMessage::Ping { timestamp } => {
                assert!(*timestamp > 0);
            }
            _ => panic!("Expected Ping message"),
        }
    }

    #[test]
    fn test_ws_message_pong() {
        let msg = WsMessage::pong();

        match &msg {
            WsMessage::Pong { timestamp } => {
                assert!(*timestamp > 0);
            }
            _ => panic!("Expected Pong message"),
        }
    }

    #[test]
    fn test_ws_message_error() {
        let msg = WsMessage::error("ERR_CODE", "Error message");

        match &msg {
            WsMessage::Error { message_id, code, message } => {
                assert!(message_id.is_none());
                assert_eq!(code, "ERR_CODE");
                assert_eq!(message, "Error message");
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[test]
    fn test_ws_message_error_with_id() {
        let msg = WsMessage::error_with_id("msg-123", "ERR_CODE", "Error message");

        match &msg {
            WsMessage::Error { message_id, code, message } => {
                assert_eq!(message_id.as_deref(), Some("msg-123"));
                assert_eq!(code, "ERR_CODE");
                assert_eq!(message, "Error message");
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[test]
    fn test_ws_message_close() {
        let msg = WsMessage::close("User disconnected");

        match &msg {
            WsMessage::Close { reason } => {
                assert_eq!(reason, "User disconnected");
            }
            _ => panic!("Expected Close message"),
        }
    }

    #[test]
    fn test_ws_message_ack() {
        let msg = WsMessage::ack("original-msg-id");

        match &msg {
            WsMessage::Ack { original_id, timestamp } => {
                assert_eq!(original_id, "original-msg-id");
                assert!(*timestamp > 0);
            }
            _ => panic!("Expected Ack message"),
        }
    }

    #[test]
    fn test_ws_message_message_id() {
        let text_msg = WsMessage::text("test");
        assert!(text_msg.message_id().is_some());

        let binary_msg = WsMessage::binary(b"test");
        assert!(binary_msg.message_id().is_some());

        let ping_msg = WsMessage::ping();
        assert!(ping_msg.message_id().is_none());

        let pong_msg = WsMessage::pong();
        assert!(pong_msg.message_id().is_none());

        let close_msg = WsMessage::close("test");
        assert!(close_msg.message_id().is_none());
    }

    #[test]
    fn test_ws_message_message_type() {
        let text_msg = WsMessage::text("test");
        assert_eq!(text_msg.message_type(), WsMessageType::Text);

        let binary_msg = WsMessage::binary(b"test");
        assert_eq!(binary_msg.message_type(), WsMessageType::Binary);

        let ping_msg = WsMessage::ping();
        assert_eq!(ping_msg.message_type(), WsMessageType::Ping);

        let pong_msg = WsMessage::pong();
        assert_eq!(pong_msg.message_type(), WsMessageType::Pong);

        let error_msg = WsMessage::error("code", "msg");
        assert_eq!(error_msg.message_type(), WsMessageType::Error);

        let close_msg = WsMessage::close("reason");
        assert_eq!(close_msg.message_type(), WsMessageType::Close);

        let ack_msg = WsMessage::ack("id");
        assert_eq!(ack_msg.message_type(), WsMessageType::Ack);
    }

    #[test]
    fn test_ws_message_serialization() {
        let msg = WsMessage::text("Hello");
        let json = msg.to_json().unwrap();

        // Debug: print actual JSON
        eprintln!("JSON: {}", json);

        // Using serde(rename_all = "snake_case"), the type field becomes "Text" not "text"
        assert!(json.contains("\"type\":\"Text\""));
        assert!(json.contains("\"content\":\"Hello\""));
    }

    #[test]
    fn test_ws_message_deserialization() {
        // Create a message, serialize it, then deserialize it
        let original = WsMessage::text("Hello");
        let json = original.to_json().unwrap();
        let msg = WsMessage::from_json(&json).unwrap();

        match msg {
            WsMessage::Text { message_id, payload, .. } => {
                assert!(!message_id.is_empty());
                assert_eq!(payload.content, "Hello");
            }
            _ => panic!("Expected Text message"),
        }
    }

    #[test]
    fn test_ws_message_roundtrip() {
        let original = WsMessage::text("Roundtrip test");
        let json = original.to_json().unwrap();
        let parsed = WsMessage::from_json(&json).unwrap();

        match parsed {
            WsMessage::Text { payload, .. } => {
                assert_eq!(payload.content, "Roundtrip test");
            }
            _ => panic!("Expected Text message"),
        }
    }

    #[test]
    fn test_ws_message_to_ws_message() {
        let msg = WsMessage::text("Test");
        let ws_msg = msg.to_ws_message().unwrap();

        match ws_msg {
            tokio_tungstenite::tungstenite::Message::Text(text) => {
                assert!(text.contains("Test"));
            }
            _ => panic!("Expected Text message"),
        }
    }

    #[test]
    fn test_text_payload() {
        let payload = TextPayload {
            content: "test".to_string(),
        };
        assert_eq!(payload.content, "test");
    }

    #[test]
    fn test_binary_payload() {
        let payload = BinaryPayload {
            data: "SGVsbG8=".to_string(), // "Hello" in base64
        };
        let decoded = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &payload.data
        ).unwrap();
        assert_eq!(decoded, b"Hello");
    }
}

mod ws_message_type_tests {
    use super::*;

    #[test]
    fn test_ws_message_type_display() {
        assert_eq!(WsMessageType::Text.to_string(), "text");
        assert_eq!(WsMessageType::Binary.to_string(), "binary");
        assert_eq!(WsMessageType::Ping.to_string(), "ping");
        assert_eq!(WsMessageType::Pong.to_string(), "pong");
        assert_eq!(WsMessageType::Error.to_string(), "error");
        assert_eq!(WsMessageType::Close.to_string(), "close");
        assert_eq!(WsMessageType::Ack.to_string(), "ack");
    }

    #[test]
    fn test_ws_message_type_equality() {
        assert_eq!(WsMessageType::Text, WsMessageType::Text);
        assert_eq!(WsMessageType::Ping, WsMessageType::Ping);
        assert_ne!(WsMessageType::Text, WsMessageType::Binary);
    }
}