//! Tests for WebSocket message types

use bedcode_lib::websocket::message::{
    Message, OutputPayload, InputPayload, SpecialKey, AuthPayload, AuthStage,
    ControlPayload, ControlAction, SessionSummary, QuickActionSummary
};

mod message_tests {
    use super::*;

    #[test]
    fn test_create_output_message() {
        let msg = Message::output("session-123", b"Hello, World!", false);

        match &msg {
            Message::Output { session_id, payload, .. } => {
                assert_eq!(session_id, "session-123");
                assert!(!payload.is_waiting);
                assert!(!payload.data.is_empty());
            }
            _ => panic!("Expected Output message"),
        }
    }

    #[test]
    fn test_create_output_message_waiting() {
        let msg = Message::output("session-123", b"prompt: ", true);

        match &msg {
            Message::Output { payload, .. } => {
                assert!(payload.is_waiting);
            }
            _ => panic!("Expected Output message"),
        }
    }

    #[test]
    fn test_create_input_message() {
        let msg = Message::input("session-456", "Hello", None);

        match &msg {
            Message::Input { session_id, payload, .. } => {
                assert_eq!(session_id, "session-456");
                assert_eq!(payload.data, "Hello");
                assert!(payload.special_key.is_none());
            }
            _ => panic!("Expected Input message"),
        }
    }

    #[test]
    fn test_create_input_message_with_special_key() {
        let msg = Message::input("session-456", "", Some(SpecialKey::CtrlC));

        match &msg {
            Message::Input { payload, .. } => {
                assert_eq!(payload.special_key, Some(SpecialKey::CtrlC));
            }
            _ => panic!("Expected Input message"),
        }
    }

    #[test]
    fn test_create_control_message() {
        let msg = Message::control(ControlAction::ListSessions, None);

        match &msg {
            Message::Control { session_id, payload, .. } => {
                assert!(session_id.is_none());
                match &payload.action {
                    ControlAction::ListSessions => {}
                    _ => panic!("Expected ListSessions action"),
                }
            }
            _ => panic!("Expected Control message"),
        }
    }

    #[test]
    fn test_create_control_message_with_session() {
        let msg = Message::control(
            ControlAction::StartSession { config_id: "config-1".to_string() },
            Some("session-1")
        );

        match &msg {
            Message::Control { session_id, payload, .. } => {
                assert_eq!(session_id, &Some("session-1".to_string()));
                match &payload.action {
                    ControlAction::StartSession { config_id } => {
                        assert_eq!(config_id, "config-1");
                    }
                    _ => panic!("Expected StartSession action"),
                }
            }
            _ => panic!("Expected Control message"),
        }
    }

    #[test]
    fn test_create_error_message() {
        let msg = Message::error("AUTH_FAILED", "Invalid pairing code");

        match &msg {
            Message::Error { code, message, .. } => {
                assert_eq!(code, "AUTH_FAILED");
                assert_eq!(message, "Invalid pairing code");
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[test]
    fn test_create_heartbeat_message() {
        let msg = Message::heartbeat();

        match &msg {
            Message::Heartbeat { timestamp } => {
                assert!(*timestamp > 0);
            }
            _ => panic!("Expected Heartbeat message"),
        }
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::input("session-1", "test input", Some(SpecialKey::Enter));
        let json = msg.to_json().unwrap();

        assert!(json.contains("\"type\":\"input\""));
        assert!(json.contains("\"session_id\":\"session-1\""));
        assert!(json.contains("\"data\":\"test input\""));
        assert!(json.contains("\"special_key\":\"enter\""));
    }

    #[test]
    fn test_message_deserialization() {
        let json = r#"{"type":"output","session_id":"s1","timestamp":1234567890,"payload":{"data":"SGVsbG8=","is_waiting":false}}"#

;
        let msg = Message::from_json(json).unwrap();

        match msg {
            Message::Output { session_id, payload, .. } => {
                assert_eq!(session_id, "s1");
                assert!(!payload.is_waiting);
            }
            _ => panic!("Expected Output message"),
        }
    }

    #[test]
    fn test_message_roundtrip() {
        let original = Message::control(
            ControlAction::ResizeSession {
                session_id: "session-1".to_string(),
                cols: 120,
                rows: 40,
            },
            Some("session-1"),
        );

        let json = original.to_json().unwrap();
        let parsed = Message::from_json(&json).unwrap();

        match parsed {
            Message::Control { session_id, payload, .. } => {
                assert_eq!(session_id, Some("session-1".to_string()));
                match payload.action {
                    ControlAction::ResizeSession {
                        session_id: sid,
                        cols,
                        rows,
                    } => {
                        assert_eq!(sid, "session-1");
                        assert_eq!(cols, 120);
                        assert_eq!(rows, 40);
                    }
                    _ => panic!("Expected ResizeSession action"),
                }
            }
            _ => panic!("Expected Control message"),
        }
    }
}

mod special_key_tests {
    use super::*;

    #[test]
    fn test_special_key_as_str() {
        assert_eq!(SpecialKey::Tab.as_str(), "tab");
        assert_eq!(SpecialKey::Enter.as_str(), "enter");
        assert_eq!(SpecialKey::Escape.as_str(), "escape");
        assert_eq!(SpecialKey::CtrlC.as_str(), "ctrl_c");
        assert_eq!(SpecialKey::ArrowUp.as_str(), "arrow_up");
    }

    #[test]
    fn test_special_key_from_str() {
        assert_eq!(SpecialKey::from_str("tab"), Some(SpecialKey::Tab));
        assert_eq!(SpecialKey::from_str("enter"), Some(SpecialKey::Enter));
        assert_eq!(SpecialKey::from_str("escape"), Some(SpecialKey::Escape));
        assert_eq!(SpecialKey::from_str("esc"), Some(SpecialKey::Escape));
        assert_eq!(SpecialKey::from_str("ctrl_c"), Some(SpecialKey::CtrlC));
        assert_eq!(SpecialKey::from_str("ctrlc"), Some(SpecialKey::CtrlC));
        assert_eq!(SpecialKey::from_str("up"), Some(SpecialKey::ArrowUp));
        assert_eq!(SpecialKey::from_str("invalid"), None);
    }

    #[test]
    fn test_special_key_serialization() {
        let key = SpecialKey::CtrlD;
        let json = serde_json::to_string(&key).unwrap();
        assert_eq!(json, "\"ctrl_d\"");

        let parsed: SpecialKey = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, SpecialKey::CtrlD);
    }
}

mod auth_payload_tests {
    use super::*;

    #[test]
    fn test_auth_stage_serialization() {
        let stages = vec![
            AuthStage::RequestPairing,
            AuthStage::VerifyCode,
            AuthStage::ExchangeCertificate,
            AuthStage::Authenticated,
            AuthStage::Failed,
        ];

        for stage in stages {
            let json = serde_json::to_string(&stage).unwrap();
            let parsed: AuthStage = serde_json::from_str(&json).unwrap();
            assert_eq!(stage, parsed);
        }
    }

    #[test]
    fn test_auth_payload_request() {
        let payload = AuthPayload {
            stage: AuthStage::RequestPairing,
            device_id: None,
            device_name: Some("My Device".to_string()),
            device_fingerprint: Some("fp123".to_string()),
            pairing_code: None,
            session_token: None,
            error: None,
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"stage\":\"request_pairing\""));
        assert!(json.contains("\"device_name\":\"My Device\""));
    }

    #[test]
    fn test_auth_payload_verify() {
        let payload = AuthPayload {
            stage: AuthStage::VerifyCode,
            device_id: Some("device-1".to_string()),
            device_name: None,
            device_fingerprint: None,
            pairing_code: Some("123456".to_string()),
            session_token: None,
            error: None,
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"pairing_code\":\"123456\""));
    }

    #[test]
    fn test_auth_payload_failed() {
        let payload = AuthPayload {
            stage: AuthStage::Failed,
            device_id: None,
            device_name: None,
            device_fingerprint: None,
            pairing_code: None,
            session_token: None,
            error: Some("Invalid code".to_string()),
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"error\":\"Invalid code\""));
    }
}

mod control_action_tests {
    use super::*;

    #[test]
    fn test_control_action_list_sessions() {
        let action = ControlAction::ListSessions;
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"list_sessions\""));
    }

    #[test]
    fn test_control_action_session_list() {
        let action = ControlAction::SessionList {
            sessions: vec![
                SessionSummary {
                    id: "s1".to_string(),
                    name: "Session 1".to_string(),
                    status: "running".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"session_list\""));
        assert!(json.contains("\"name\":\"Session 1\""));
    }

    #[test]
    fn test_control_action_start_session() {
        let action = ControlAction::StartSession {
            config_id: "config-123".to_string(),
        };

        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"start_session\""));
        assert!(json.contains("\"config_id\":\"config-123\""));
    }

    #[test]
    fn test_control_action_resize() {
        let action = ControlAction::ResizeSession {
            session_id: "session-1".to_string(),
            cols: 100,
            rows: 30,
        };

        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"resize_session\""));
        assert!(json.contains("\"session_id\":\"session-1\""));
        assert!(json.contains("\"cols\":100"));
        assert!(json.contains("\"rows\":30"));
    }

    #[test]
    fn test_control_action_quick_actions() {
        let action = ControlAction::QuickActionList {
            actions: vec![
                QuickActionSummary {
                    id: "qa1".to_string(),
                    name: "Continue".to_string(),
                    content: "Please continue".to_string(),
                    icon: Some("▶️".to_string()),
                    color: Some("#22c55e".to_string()),
                },
            ],
        };

        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"quick_action_list\""));
        assert!(json.contains("\"name\":\"Continue\""));
    }
}

mod output_payload_tests {
    use super::*;

    #[test]
    fn test_output_payload_base64() {
        let payload = OutputPayload {
            data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"test"),
            is_waiting: false,
        };

        // Verify base64 decoding works
        let decoded = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &payload.data
        ).unwrap();
        assert_eq!(decoded, b"test");
    }
}

mod message_id_tests {
    use super::*;

    #[test]
    fn test_message_id_present_in_output() {
        let msg = Message::output("session-1", b"data", false);

        match &msg {
            Message::Output { message_id, .. } => {
                assert!(!message_id.is_empty());
                // Message ID should be a UUID format
                assert!(message_id.contains('-'));
            }
            _ => panic!("Expected Output message"),
        }
    }

    #[test]
    fn test_message_id_present_in_input() {
        let msg = Message::input("session-1", "test", None);

        match &msg {
            Message::Input { message_id, .. } => {
                assert!(!message_id.is_empty());
            }
            _ => panic!("Expected Input message"),
        }
    }

    #[test]
    fn test_message_id_present_in_auth() {
        let msg = Message::Auth {
            message_id: "test-msg-id".to_string(),
            session_id: None,
            timestamp: 1234567890,
            payload: AuthPayload::default(),
        };

        assert_eq!(msg.message_id(), Some("test-msg-id"));
    }

    #[test]
    fn test_message_id_present_in_control() {
        let msg = Message::control(ControlAction::ListSessions, None);

        match &msg {
            Message::Control { message_id, .. } => {
                assert!(!message_id.is_empty());
            }
            _ => panic!("Expected Control message"),
        }
    }

    #[test]
    fn test_error_message_with_id() {
        let msg = Message::error_with_id("req-123", "TEST_ERROR", "Test error message");

        match &msg {
            Message::Error { message_id, code, message, .. } => {
                assert_eq!(message_id, &Some("req-123".to_string()));
                assert_eq!(code, "TEST_ERROR");
                assert_eq!(message, "Test error message");
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[test]
    fn test_message_id_method() {
        let output_msg = Message::output("s1", b"data", false);
        assert!(output_msg.message_id().is_some());

        let input_msg = Message::input("s1", "test", None);
        assert!(input_msg.message_id().is_some());

        let control_msg = Message::control(ControlAction::ListSessions, None);
        assert!(control_msg.message_id().is_some());

        let error_msg = Message::error("ERR", "message");
        assert!(error_msg.message_id().is_none());

        let error_with_id_msg = Message::error_with_id("id-123", "ERR", "message");
        assert_eq!(error_with_id_msg.message_id(), Some("id-123"));

        let heartbeat_msg = Message::heartbeat();
        assert!(heartbeat_msg.message_id().is_none());
    }

    #[test]
    fn test_message_id_preserved_in_serialization() {
        let original = Message::Auth {
            message_id: "custom-message-id-123".to_string(),
            session_id: Some("session-abc".to_string()),
            timestamp: 1234567890,
            payload: AuthPayload {
                stage: AuthStage::Authenticated,
                device_id: Some("device-1".to_string()),
                device_name: None,
                device_fingerprint: None,
                pairing_code: None,
                session_token: Some("token-xyz".to_string()),
                error: None,
            },
        };

        let json = original.to_json().unwrap();
        let parsed = Message::from_json(&json).unwrap();

        assert_eq!(parsed.message_id(), Some("custom-message-id-123"));
    }
}
