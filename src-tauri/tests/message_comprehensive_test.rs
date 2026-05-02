//! Additional tests for WebSocket message types

use bedcode_lib::websocket::message::{
    Message, OutputPayload, InputPayload, SpecialKey, AuthPayload, AuthStage,
    ControlPayload, ControlAction, SessionSummary, QuickActionSummary
};

mod message_id_tests {
    use super::*;

    #[test]
    fn test_message_id_present() {
        let msg = Message::output("session-1", b"data", false);
        assert!(msg.message_id().is_some());
        assert!(!msg.message_id().unwrap().is_empty());
    }

    #[test]
    fn test_heartbeat_no_message_id() {
        let msg = Message::heartbeat();
        assert!(msg.message_id().is_none());
    }

    #[test]
    fn test_error_message_id() {
        // Error without associated message ID
        let msg = Message::error("ERR001", "Test error");
        assert!(msg.message_id().is_none());

        // Error with associated message ID
        let msg_with_id = Message::error_with_id("msg-123", "ERR002", "Related error");
        assert_eq!(msg_with_id.message_id(), Some("msg-123"));
    }

    #[test]
    fn test_all_message_types_have_unique_ids() {
        let messages = vec![
            Message::output("s1", b"data", false),
            Message::input("s1", "text", None),
            Message::control(ControlAction::ListSessions, None),
            Message::Auth {
                message_id: uuid::Uuid::new_v4().to_string(),
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: AuthPayload {
                    stage: AuthStage::RequestPairing,
                    device_id: None,
                    device_name: None,
                    device_fingerprint: None,
                    pairing_code: None,
                    session_token: None,
                    error: None,
                },
            },
        ];

        let ids: Vec<&str> = messages.iter()
            .filter_map(|m| m.message_id())
            .collect();

        // All IDs should be unique
        let unique_ids: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique_ids.len());
    }
}

mod special_key_comprehensive_tests {
    use super::*;

    #[test]
    fn test_all_special_keys() {
        let keys = vec![
            (SpecialKey::Tab, "tab"),
            (SpecialKey::Enter, "enter"),
            (SpecialKey::Escape, "escape"),
            (SpecialKey::CtrlC, "ctrl_c"),
            (SpecialKey::CtrlD, "ctrl_d"),
            (SpecialKey::CtrlZ, "ctrl_z"),
            (SpecialKey::ArrowUp, "arrow_up"),
            (SpecialKey::ArrowDown, "arrow_down"),
            (SpecialKey::ArrowLeft, "arrow_left"),
            (SpecialKey::ArrowRight, "arrow_right"),
            (SpecialKey::Backspace, "backspace"),
        ];

        for (key, expected_str) in keys {
            assert_eq!(key.as_str(), expected_str);
        }
    }

    #[test]
    fn test_special_key_aliases() {
        // Test all aliases
        assert_eq!(SpecialKey::from_str("esc"), Some(SpecialKey::Escape));
        assert_eq!(SpecialKey::from_str("ctrlc"), Some(SpecialKey::CtrlC));
        assert_eq!(SpecialKey::from_str("ctrld"), Some(SpecialKey::CtrlD));
        assert_eq!(SpecialKey::from_str("ctrlz"), Some(SpecialKey::CtrlZ));
        assert_eq!(SpecialKey::from_str("up"), Some(SpecialKey::ArrowUp));
        assert_eq!(SpecialKey::from_str("down"), Some(SpecialKey::ArrowDown));
        assert_eq!(SpecialKey::from_str("left"), Some(SpecialKey::ArrowLeft));
        assert_eq!(SpecialKey::from_str("right"), Some(SpecialKey::ArrowRight));
    }

    #[test]
    fn test_special_key_roundtrip() {
        let all_keys = [
            SpecialKey::Tab,
            SpecialKey::Enter,
            SpecialKey::Escape,
            SpecialKey::CtrlC,
            SpecialKey::CtrlD,
            SpecialKey::CtrlZ,
            SpecialKey::ArrowUp,
            SpecialKey::ArrowDown,
            SpecialKey::ArrowLeft,
            SpecialKey::ArrowRight,
            SpecialKey::Backspace,
        ];

        for key in all_keys {
            let str = key.as_str();
            let parsed = SpecialKey::from_str(str);
            assert_eq!(parsed, Some(key.clone()));

            // Also test JSON roundtrip
            let json = serde_json::to_string(&key).unwrap();
            let parsed_json: SpecialKey = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed_json, key);
        }
    }
}

mod auth_stage_tests {
    use super::*;

    #[test]
    fn test_all_auth_stages() {
        let stages = [
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
    fn test_auth_stage_json_format() {
        assert_eq!(
            serde_json::to_string(&AuthStage::RequestPairing).unwrap(),
            "\"request_pairing\""
        );
        assert_eq!(
            serde_json::to_string(&AuthStage::VerifyCode).unwrap(),
            "\"verify_code\""
        );
        assert_eq!(
            serde_json::to_string(&AuthStage::ExchangeCertificate).unwrap(),
            "\"exchange_certificate\""
        );
        assert_eq!(
            serde_json::to_string(&AuthStage::Authenticated).unwrap(),
            "\"authenticated\""
        );
        assert_eq!(
            serde_json::to_string(&AuthStage::Failed).unwrap(),
            "\"failed\""
        );
    }
}

mod control_action_comprehensive_tests {
    use super::*;

    #[test]
    fn test_all_control_actions_serialize() {
        let actions = vec![
            ControlAction::ListSessions,
            ControlAction::SessionList { sessions: vec![] },
            ControlAction::StartSession { config_id: "cfg1".to_string() },
            ControlAction::StopSession { session_id: "s1".to_string() },
            ControlAction::ResizeSession {
                session_id: "s1".to_string(),
                cols: 80,
                rows: 24,
            },
            ControlAction::ListQuickActions,
            ControlAction::QuickActionList { actions: vec![] },
        ];

        for action in actions {
            let json = serde_json::to_string(&action).unwrap();
            assert!(json.contains("\"type\""));
        }
    }

    #[test]
    fn test_stop_session_serialization() {
        let action = ControlAction::StopSession {
            session_id: "session-123".to_string(),
        };

        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"stop_session\""));
        assert!(json.contains("\"session_id\":\"session-123\""));
    }

    #[test]
    fn test_session_summary_serialization() {
        let summary = SessionSummary {
            id: "s1".to_string(),
            name: "Test Session".to_string(),
            status: "running".to_string(),
        };

        let json = serde_json::to_string(&summary).unwrap();
        let parsed: SessionSummary = serde_json::from_str(&json).unwrap();

        assert_eq!(summary.id, parsed.id);
        assert_eq!(summary.name, parsed.name);
        assert_eq!(summary.status, parsed.status);
    }

    #[test]
    fn test_quick_action_summary() {
        let summary = QuickActionSummary {
            id: "qa1".to_string(),
            name: "Continue".to_string(),
            content: "Please continue".to_string(),
            icon: Some("▶️".to_string()),
            color: Some("#22c55e".to_string()),
        };

        let json = serde_json::to_string(&summary).unwrap();
        let parsed: QuickActionSummary = serde_json::from_str(&json).unwrap();

        assert_eq!(summary.id, parsed.id);
        assert_eq!(summary.icon, parsed.icon);
        assert_eq!(summary.color, parsed.color);
    }
}

mod input_payload_tests {
    use super::*;

    #[test]
    fn test_input_payload_simple() {
        let payload = InputPayload {
            data: "Hello".to_string(),
            special_key: None,
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"data\":\"Hello\""));
        assert!(!json.contains("special_key")); // Should be skipped when None
    }

    #[test]
    fn test_input_payload_with_special_key() {
        let payload = InputPayload {
            data: "".to_string(),
            special_key: Some(SpecialKey::CtrlC),
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"special_key\":\"ctrl_c\""));
    }
}

mod output_payload_comprehensive_tests {
    use super::*;

    #[test]
    fn test_output_payload_encoding() {
        // Test various data
        let test_cases = vec![
            ("Hello, World!", false),
            ("❯ ", true),  // Prompt
            ("\x1b[31mRed\x1b[0m", false),  // ANSI codes
            ("Multi\nline\ntext", false),
            ("", false),  // Empty
        ];

        for (text, waiting) in test_cases {
            let payload = OutputPayload {
                data: base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    text.as_bytes()
                ),
                is_waiting: waiting,
            };

            // Verify decoding
            let decoded = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                &payload.data
            ).unwrap();
            assert_eq!(String::from_utf8_lossy(&decoded), text);
            assert_eq!(payload.is_waiting, waiting);
        }
    }
}

mod error_handling_tests {
    use super::*;

    #[test]
    fn test_invalid_json_deserialization() {
        let invalid_json = "{ not valid json }";
        let result = Message::from_json(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_fields_json() {
        // Missing required fields
        let incomplete = r#"{"type":"input"}"#;
        let result = Message::from_json(incomplete);
        assert!(result.is_err());
    }

    #[test]
    fn test_unknown_message_type() {
        let unknown = r#"{"type":"unknown","session_id":"s1","timestamp":0}"#;
        let result = Message::from_json(unknown);
        assert!(result.is_err());
    }
}
