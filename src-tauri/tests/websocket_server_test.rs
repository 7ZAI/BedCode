//! Tests for WebSocket server
//!
//! 测试 WebSocket 服务端的消息处理、认证流程和连接管理

use bedcode_lib::auth::PairingService;
use bedcode_lib::db::Database;
use bedcode_lib::session::SessionManager;
use bedcode_lib::websocket::message::{
    AuthPayload, AuthStage, ControlAction, Message, SessionSummary, SessionConfigSummary,
};
use tempfile::TempDir;

mod client_info_tests {
    // ClientInfo is in a private module, so we test it through integration tests
    // These tests verify the AuthPayload structure which is public
}

mod auth_payload_tests {
    use super::*;

    #[test]
    fn test_auth_payload_default() {
        let payload = AuthPayload::default();

        assert_eq!(payload.stage, AuthStage::RequestPairing);
        assert!(payload.device_id.is_none());
        assert!(payload.device_name.is_none());
        assert!(payload.pairing_code.is_none());
        assert!(payload.session_token.is_none());
        assert!(payload.error.is_none());
    }

    #[test]
    fn test_auth_payload_request_pairing() {
        let payload = AuthPayload {
            stage: AuthStage::RequestPairing,
            device_id: None,
            device_name: Some("My Phone".to_string()),
            device_fingerprint: Some("fp123".to_string()),
            pairing_code: None,
            session_token: None,
            error: None,
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"stage\":\"request_pairing\""));
        assert!(json.contains("\"device_name\":\"My Phone\""));
    }

    #[test]
    fn test_auth_payload_verify_code() {
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
        assert!(json.contains("\"stage\":\"verify_code\""));
        assert!(json.contains("\"pairing_code\":\"123456\""));
    }

    #[test]
    fn test_auth_payload_authenticated() {
        let payload = AuthPayload {
            stage: AuthStage::Authenticated,
            device_id: Some("device-1".to_string()),
            device_name: None,
            device_fingerprint: None,
            pairing_code: None,
            session_token: Some("token-abc".to_string()),
            error: None,
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"stage\":\"authenticated\""));
        assert!(json.contains("\"session_token\":\"token-abc\""));
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
        assert!(json.contains("\"stage\":\"failed\""));
        assert!(json.contains("\"error\":\"Invalid code\""));
    }
}

mod message_handling_tests {
    use super::*;

    fn create_test_db() -> (Database, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.db");
        let db = Database::new(&path).unwrap();
        db.init_schema().unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_heartbeat_message_response() {
        let heartbeat = Message::heartbeat();

        match heartbeat {
            Message::Heartbeat { timestamp } => {
                assert!(timestamp > 0);
            }
            _ => panic!("Expected Heartbeat message"),
        }
    }

    #[test]
    fn test_error_message_creation() {
        let error = Message::error("TEST_ERROR", "Test error message");

        match error {
            Message::Error { code, message, .. } => {
                assert_eq!(code, "TEST_ERROR");
                assert_eq!(message, "Test error message");
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[test]
    fn test_control_message_list_sessions() {
        let msg = Message::control(ControlAction::ListSessions, None);

        match msg {
            Message::Control { payload, .. } => {
                assert!(matches!(payload.action, ControlAction::ListSessions));
            }
            _ => panic!("Expected Control message"),
        }
    }

    #[test]
    fn test_control_message_session_list_response() {
        let msg = Message::control(
            ControlAction::SessionList {
                sessions: vec![SessionSummary {
                    id: "s1".to_string(),
                    name: "Session 1".to_string(),
                    status: "running".to_string(),
                }],
            },
            None,
        );

        let json = msg.to_json().unwrap();
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

    #[test]
    fn test_input_message_creation() {
        let msg = Message::input("session-1", "Hello, World!", None);

        match msg {
            Message::Input { session_id, payload, .. } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(payload.data, "Hello, World!");
            }
            _ => panic!("Expected Input message"),
        }
    }

    #[test]
    fn test_output_message_creation() {
        let msg = Message::output("session-1", b"Output text", true);

        match msg {
            Message::Output { session_id, payload, .. } => {
                assert_eq!(session_id, "session-1");
                assert!(payload.is_waiting);
                // Data should be base64 encoded
                assert!(!payload.data.is_empty());
            }
            _ => panic!("Expected Output message"),
        }
    }
}

mod auth_flow_tests {
    use super::*;

    #[tokio::test]
    async fn test_pairing_service_generate_code() {
        let service = PairingService::new();

        let code = service.generate_code().await;

        assert_eq!(code.code.len(), 6);
        assert!(code.code.chars().all(|c| c.is_ascii_digit()));
    }

    #[tokio::test]
    async fn test_pairing_service_get_current_code() {
        let service = PairingService::new();

        // No code initially
        let no_code = service.get_current_code().await;
        assert!(no_code.is_none());

        // Generate code
        let code = service.generate_code().await;
        let current = service.get_current_code().await;

        assert!(current.is_some());
        assert_eq!(current.unwrap().code, code.code);
    }

    #[tokio::test]
    async fn test_pairing_service_verify_correct_code() {
        let service = PairingService::new();

        let code = service.generate_code().await;
        let code_str = code.code.clone();

        let is_valid = service.verify_code(&code_str).await;
        assert!(is_valid);
    }

    #[tokio::test]
    async fn test_pairing_service_verify_incorrect_code() {
        let service = PairingService::new();

        service.generate_code().await;

        let is_valid = service.verify_code("000000").await;
        assert!(!is_valid);
    }

    #[tokio::test]
    async fn test_pairing_service_verify_no_code() {
        let service = PairingService::new();

        let is_valid = service.verify_code("123456").await;
        assert!(!is_valid);
    }

    #[tokio::test]
    async fn test_pairing_service_clear_code() {
        let service = PairingService::new();

        service.generate_code().await;
        service.clear_code().await;

        let current = service.get_current_code().await;
        assert!(current.is_none());
    }
}

mod session_manager_tests {
    use super::*;

    fn create_test_session_manager() -> (SessionManager, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.db");
        let db = Database::new(&path).unwrap();
        db.init_schema().unwrap();
        (SessionManager::from_database(db), temp_dir)
    }

    #[tokio::test]
    async fn test_session_manager_default() {
        let manager = SessionManager::default();

        let sessions = manager.list_sessions().await;
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_session_manager_list_sessions_empty() {
        let (manager, _temp_dir) = create_test_session_manager();

        let sessions = manager.list_sessions().await;
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_session_manager_get_session_not_found() {
        let (manager, _temp_dir) = create_test_session_manager();

        let session = manager.get_session("nonexistent").await;
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn test_session_manager_get_status_not_found() {
        let (manager, _temp_dir) = create_test_session_manager();

        let status = manager.get_session_status("nonexistent").await;
        assert!(status.is_none());
    }

    #[tokio::test]
    async fn test_session_manager_subscribe_output() {
        let (manager, _temp_dir) = create_test_session_manager();

        // Should be able to subscribe even without sessions
        let mut rx = manager.subscribe_output();

        // The receiver should be valid
        drop(rx);
    }

    #[tokio::test]
    async fn test_session_manager_cleanup_stopped_sessions() {
        let (manager, _temp_dir) = create_test_session_manager();

        // Cleanup on empty manager should work
        manager.cleanup_stopped_sessions().await;

        let sessions = manager.list_sessions().await;
        assert!(sessions.is_empty());
    }
}

mod control_action_tests {
    use super::*;

    #[test]
    fn test_control_action_list_sessions_serialization() {
        let action = ControlAction::ListSessions;
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"list_sessions\""));
    }

    #[test]
    fn test_control_action_start_session_serialization() {
        let action = ControlAction::StartSession {
            config_id: "config-123".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"start_session\""));
        assert!(json.contains("\"config_id\":\"config-123\""));
    }

    #[test]
    fn test_control_action_stop_session_serialization() {
        let action = ControlAction::StopSession {
            session_id: "session-456".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"stop_session\""));
        assert!(json.contains("\"session_id\":\"session-456\""));
    }

    #[test]
    fn test_control_action_resize_session_serialization() {
        let action = ControlAction::ResizeSession {
            session_id: "session-123".to_string(),
            cols: 120,
            rows: 40,
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"resize_session\""));
        assert!(json.contains("\"session_id\":\"session-123\""));
        assert!(json.contains("\"cols\":120"));
        assert!(json.contains("\"rows\":40"));
    }

    #[test]
    fn test_control_action_list_quick_actions_serialization() {
        let action = ControlAction::ListQuickActions;
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"list_quick_actions\""));
    }

    #[test]
    fn test_control_action_list_session_configs_serialization() {
        let action = ControlAction::ListSessionConfigs;
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"list_session_configs\""));
    }

    #[test]
    fn test_control_action_session_config_list_serialization() {
        let action = ControlAction::SessionConfigList {
            configs: vec![
                SessionConfigSummary {
                    id: "cfg-1".to_string(),
                    name: "Test Config".to_string(),
                    environment: "windows".to_string(),
                    wsl_distro: None,
                    working_dir: "C:\\test".to_string(),
                    command: "claude".to_string(),
                },
            ],
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"session_config_list\""));
        assert!(json.contains("\"name\":\"Test Config\""));
    }

    #[test]
    fn test_control_action_join_session_serialization() {
        let action = ControlAction::JoinSession {
            session_id: "session-789".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"join_session\""));
        assert!(json.contains("\"session_id\":\"session-789\""));
    }

    #[test]
    fn test_control_action_leave_session_serialization() {
        let action = ControlAction::LeaveSession {
            session_id: "session-789".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"leave_session\""));
        assert!(json.contains("\"session_id\":\"session-789\""));
    }
}

mod join_leave_session_tests {
    use super::*;

    #[test]
    fn test_join_session_action_deserialization() {
        let json = r#"{"type":"join_session","session_id":"test-session-id"}"#;
        let action: ControlAction = serde_json::from_str(json).unwrap();

        match action {
            ControlAction::JoinSession { session_id } => {
                assert_eq!(session_id, "test-session-id");
            }
            _ => panic!("Expected JoinSession action"),
        }
    }

    #[test]
    fn test_leave_session_action_deserialization() {
        let json = r#"{"type":"leave_session","session_id":"test-session-id"}"#;
        let action: ControlAction = serde_json::from_str(json).unwrap();

        match action {
            ControlAction::LeaveSession { session_id } => {
                assert_eq!(session_id, "test-session-id");
            }
            _ => panic!("Expected LeaveSession action"),
        }
    }

    #[test]
    fn test_join_session_message_creation() {
        let msg = Message::control(
            ControlAction::JoinSession {
                session_id: "s1".to_string(),
            },
            Some("session-context"),
        );

        match msg {
            Message::Control { session_id, payload, .. } => {
                assert_eq!(session_id, Some("session-context".to_string()));
                match payload.action {
                    ControlAction::JoinSession { session_id } => {
                        assert_eq!(session_id, "s1");
                    }
                    _ => panic!("Expected JoinSession"),
                }
            }
            _ => panic!("Expected Control message"),
        }
    }

    #[test]
    fn test_leave_session_message_creation() {
        let msg = Message::control(
            ControlAction::LeaveSession {
                session_id: "s1".to_string(),
            },
            None,
        );

        match msg {
            Message::Control { payload, .. } => {
                match payload.action {
                    ControlAction::LeaveSession { session_id } => {
                        assert_eq!(session_id, "s1");
                    }
                    _ => panic!("Expected LeaveSession"),
                }
            }
            _ => panic!("Expected Control message"),
        }
    }
}
