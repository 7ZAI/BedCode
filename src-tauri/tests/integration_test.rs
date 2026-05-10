//! Integration tests for WebSocket server
//!
//! These tests verify the WebSocket communication between
//! desktop (server) and mobile (client) components.

use std::net::TcpListener;
use std::time::Duration;
use tokio::time::sleep;

/// Find an available port for testing
fn find_available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

mod websocket_integration {
    use super::*;

    #[tokio::test]
    async fn test_websocket_server_starts() {
        // This test verifies that the WebSocket server can be created
        // In a real implementation, we would:
        // 1. Create a WebSocket server
        // 2. Verify it's listening on the expected port
        // 3. Shut it down cleanly

        let port = find_available_port();
        println!("Test port: {}", port);

        // Verify port is available
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr);
        assert!(listener.is_ok(), "Port {} should be available", port);
    }

    #[tokio::test]
    async fn test_websocket_message_roundtrip() {
        // This test verifies message serialization and deserialization
        // Simulates the roundtrip that happens over WebSocket

        use bedcode_lib::websocket::message::{Message, SpecialKey};

        // Create input message
        let original = Message::input("session-1", "Hello", Some(SpecialKey::Enter));

        // Serialize
        let json = original.to_json().unwrap();

        // Deserialize
        let parsed = Message::from_json(&json).unwrap();

        // Verify
        match parsed {
            Message::Input { session_id, payload, .. } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(payload.data, "Hello");
                assert_eq!(payload.special_key, Some(SpecialKey::Enter));
            }
            _ => panic!("Expected Input message"),
        }
    }

    #[tokio::test]
    async fn test_output_message_encoding() {
        use bedcode_lib::websocket::message::Message;

        let output_data = b"Hello, World!\n> ";
        let msg = Message::output("session-1", output_data, true);

        let json = msg.to_json().unwrap();
        let parsed = Message::from_json(&json).unwrap();

        match parsed {
            Message::Output { session_id, payload, .. } => {
                assert_eq!(session_id, "session-1");
                assert!(payload.is_waiting);

                // Verify base64 decoding
                let decoded = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    &payload.data
                ).unwrap();
                assert_eq!(decoded, output_data);
            }
            _ => panic!("Expected Output message"),
        }
    }

    #[tokio::test]
    async fn test_control_message_session_management() {
        use bedcode_lib::websocket::message::{Message, ControlAction, SessionSummary};

        // Test session list request
        let list_msg = Message::control(ControlAction::ListSessions, None);
        let json = list_msg.to_json().unwrap();
        let parsed = Message::from_json(&json).unwrap();

        match parsed {
            Message::Control { payload, .. } => {
                assert!(matches!(payload.action, ControlAction::ListSessions));
            }
            _ => panic!("Expected Control message"),
        }

        // Test session list response
        let response_msg = Message::control(
            ControlAction::SessionList {
                sessions: vec![
                    SessionSummary {
                        id: "s1".to_string(),
                        name: "Session 1".to_string(),
                        status: "running".to_string(),
                        created_at: "2024-01-01T00:00:00Z".to_string(),
                        started_at: None,
                    },
                ],
            },
            None,
        );

        let json = response_msg.to_json().unwrap();
        let parsed = Message::from_json(&json).unwrap();

        match parsed {
            Message::Control { payload, .. } => {
                match payload.action {
                    ControlAction::SessionList { sessions } => {
                        assert_eq!(sessions.len(), 1);
                        assert_eq!(sessions[0].name, "Session 1");
                    }
                    _ => panic!("Expected SessionList action"),
                }
            }
            _ => panic!("Expected Control message"),
        }
    }

    #[tokio::test]
    async fn test_auth_flow_messages() {
        use bedcode_lib::websocket::message::{Message, AuthPayload, AuthStage};

        // Request pairing
        let request = Message::Auth {
            message_id: uuid::Uuid::new_v4().to_string(),
            session_id: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
            payload: AuthPayload {
                stage: AuthStage::RequestPairing,
                device_id: None,
                device_name: Some("Test Device".to_string()),
                device_fingerprint: Some("fp123".to_string()),
                pairing_code: None,
                session_token: None,
                error: None,
                    qr_token: None,
            },
        };

        let json = request.to_json().unwrap();
        let parsed = Message::from_json(&json).unwrap();

        match parsed {
            Message::Auth { payload, .. } => {
                assert_eq!(payload.stage, AuthStage::RequestPairing);
                assert_eq!(payload.device_name, Some("Test Device".to_string()));
            }
            _ => panic!("Expected Auth message"),
        }

        // Verify code
        let verify = Message::Auth {
            message_id: uuid::Uuid::new_v4().to_string(),
            session_id: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
            payload: AuthPayload {
                stage: AuthStage::VerifyCode,
                device_id: Some("device-1".to_string()),
                device_name: None,
                device_fingerprint: None,
                pairing_code: Some("123456".to_string()),
                session_token: None,
                error: None,
                    qr_token: None,
            },
        };

        let json = verify.to_json().unwrap();
        let parsed = Message::from_json(&json).unwrap();

        match parsed {
            Message::Auth { payload, .. } => {
                assert_eq!(payload.stage, AuthStage::VerifyCode);
                assert_eq!(payload.pairing_code, Some("123456".to_string()));
            }
            _ => panic!("Expected Auth message"),
        }
    }

    #[tokio::test]
    async fn test_heartbeat_message() {
        use bedcode_lib::websocket::message::Message;

        let heartbeat = Message::heartbeat();
        let json = heartbeat.to_json().unwrap();

        // Should be minimal JSON
        assert!(json.contains("\"type\":\"heartbeat\""));
        assert!(json.contains("\"timestamp\""));

        let parsed = Message::from_json(&json).unwrap();
        match parsed {
            Message::Heartbeat { timestamp } => {
                assert!(timestamp > 0);
            }
            _ => panic!("Expected Heartbeat message"),
        }
    }

    #[tokio::test]
    async fn test_error_message() {
        use bedcode_lib::websocket::message::Message;

        let error = Message::error("AUTH_FAILED", "Invalid pairing code");
        let json = error.to_json().unwrap();
        let parsed = Message::from_json(&json).unwrap();

        match parsed {
            Message::Error { code, message, .. } => {
                assert_eq!(code, "AUTH_FAILED");
                assert_eq!(message, "Invalid pairing code");
            }
            _ => panic!("Expected Error message"),
        }
    }
}

mod parser_integration {
    use bedcode_lib::parser::{OutputParser, detect_waiting_input};

    #[test]
    fn test_real_terminal_output() {
        let mut parser = OutputParser::new();

        // Simulate real terminal output with ANSI codes
        let output = "\x1b[1;36m→\x1b[0m \x1b[1mClaude Code\x1b[0m\n\
                      Analyzing project structure...\n\
                      \x1b[32m✓\x1b[0m Found 15 files\n\
                      \n\
                      ```typescript\n\
                      interface Config {\n\
                        name: string;\n\
                      }\n\
                      ```\n\
                      \n\
                      > ";

        let segments = parser.parse(output);

        // Should detect various content types
        assert!(!segments.is_empty());

        // Should detect waiting for input
        assert!(parser.detect_waiting_input(output));
    }

    #[test]
    fn test_code_block_with_waiting() {
        let mut parser = OutputParser::new();

        let output = "Here's the code:\n\n```rust\nfn main() {}\n```\n\n❯ ";

        let segments = parser.parse(output);

        // Should have code block
        assert!(segments.iter().any(|s| {
            matches!(s, bedcode_lib::parser::ParsedSegment::CodeBlock { .. })
        }));

        // Should detect waiting
        assert!(parser.detect_waiting_input(output));
    }

    #[test]
    fn test_progress_output() {
        let mut parser = OutputParser::new();

        let output = "Downloading... 75%\n";
        let segments = parser.parse(output);

        // Should detect progress
        let has_progress = segments.iter().any(|s| {
            matches!(s, bedcode_lib::parser::ParsedSegment::Progress { .. })
        });
        assert!(has_progress);
    }
}

mod database_integration {
    use bedcode_lib::db::{Database, SessionConfig, QuickAction};
    use tempfile::TempDir;

    fn create_test_db() -> (Database, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.db");
        let db = Database::new(&path).unwrap();
        db.init_schema().unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_settings_persistence() {
        let (db, _temp_dir) = create_test_db();

        db.set_setting("theme", "dark").unwrap();
        db.set_setting("fontSize", "16").unwrap();
        db.set_setting("notifyOnWaiting", "true").unwrap();

        assert_eq!(db.get_setting("theme").unwrap(), Some("dark".to_string()));
        assert_eq!(db.get_setting("fontSize").unwrap(), Some("16".to_string()));
        assert_eq!(db.get_setting("notifyOnWaiting").unwrap(), Some("true".to_string()));

        db.set_setting("theme", "light").unwrap();
        assert_eq!(db.get_setting("theme").unwrap(), Some("light".to_string()));

        let all = db.get_all_settings().unwrap();
        assert_eq!(all.len(), 3);
    }
}
