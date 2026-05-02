//! Tests for Session Manager
//!
//! 测试会话管理器的生命周期、状态转换和客户端连接

use bedcode_lib::db::{Database, SessionConfig};
use bedcode_lib::session::{SessionInfo, SessionManager, SessionStatus};
use chrono::Utc;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

mod session_status_tests {
    use super::*;

    #[test]
    fn test_session_status_default() {
        let status = SessionStatus::default();
        assert_eq!(status, SessionStatus::Starting);
    }

    #[test]
    fn test_session_status_variants() {
        let statuses = vec![
            SessionStatus::Starting,
            SessionStatus::Running,
            SessionStatus::WaitingInput,
            SessionStatus::Stopped,
            SessionStatus::Error,
        ];

        // All variants should be distinct
        for i in 0..statuses.len() {
            for j in (i + 1)..statuses.len() {
                assert_ne!(statuses[i], statuses[j]);
            }
        }
    }

    #[test]
    fn test_session_status_equality() {
        assert_eq!(SessionStatus::Running, SessionStatus::Running);
        assert_ne!(SessionStatus::Running, SessionStatus::Stopped);
    }

    #[test]
    fn test_session_status_serialization() {
        let test_cases = vec![
            (SessionStatus::Starting, "Starting"),
            (SessionStatus::Running, "Running"),
            (SessionStatus::WaitingInput, "WaitingInput"),
            (SessionStatus::Stopped, "Stopped"),
            (SessionStatus::Error, "Error"),
        ];

        for (status, expected) in test_cases {
            let json = serde_json::to_string(&status).unwrap();
            assert!(json.contains(expected), "Expected {} in {}", expected, json);

            let parsed: SessionStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, parsed);
        }
    }
}

mod session_info_tests {
    use super::*;

    #[test]
    fn test_session_info_new() {
        let info = SessionInfo::new("config-123", "My Session");

        // Should have valid UUID
        assert!(!info.id.is_empty());
        assert!(uuid::Uuid::parse_str(&info.id).is_ok());

        // Should have correct config reference
        assert_eq!(info.config_id, "config-123");
        assert_eq!(info.name, "My Session");

        // Should start in Starting state
        assert_eq!(info.status, SessionStatus::Starting);

        // Should have valid timestamps
        assert!(info.created_at <= Utc::now());

        // Should not have start/stop times yet
        assert!(info.started_at.is_none());
        assert!(info.stopped_at.is_none());
    }

    #[test]
    fn test_session_info_unique_ids() {
        let info1 = SessionInfo::new("cfg1", "Session 1");
        let info2 = SessionInfo::new("cfg2", "Session 2");

        assert_ne!(info1.id, info2.id);
    }

    #[test]
    fn test_session_info_clone() {
        let info = SessionInfo::new("cfg-1", "Test");
        let cloned = info.clone();

        assert_eq!(info.id, cloned.id);
        assert_eq!(info.config_id, cloned.config_id);
        assert_eq!(info.name, cloned.name);
        assert_eq!(info.status, cloned.status);
    }

    #[test]
    fn test_session_info_serialization() {
        let info = SessionInfo::new("cfg-1", "Test Session");
        let json = serde_json::to_string(&info).unwrap();
        let parsed: SessionInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(info.id, parsed.id);
        assert_eq!(info.config_id, parsed.config_id);
        assert_eq!(info.name, parsed.name);
        assert_eq!(info.status, parsed.status);
    }
}

mod session_manager_tests {
    use super::*;

    fn create_test_db() -> (Database, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.db");
        let db = Database::new(&path).unwrap();
        db.init_schema().unwrap();
        (db, temp_dir)
    }

    fn create_test_session_manager() -> (SessionManager, TempDir) {
        let (db, temp_dir) = create_test_db();
        (SessionManager::from_database(db), temp_dir)
    }

    #[test]
    fn test_session_manager_default() {
        let _manager = SessionManager::default();
        // Default manager should be usable
        // It creates an in-memory database
    }

    #[tokio::test]
    async fn test_session_manager_from_database() {
        let (db, _temp_dir) = create_test_db();
        let manager = SessionManager::from_database(db);

        let sessions = manager.list_sessions().await;
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_session_manager_new() {
        let (db, _temp_dir) = create_test_db();
        let manager = SessionManager::new(Arc::new(Mutex::new(db)));

        let sessions = manager.list_sessions().await;
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_list_sessions_empty() {
        let (manager, _temp_dir) = create_test_session_manager();

        let sessions = manager.list_sessions().await;
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_get_session_not_found() {
        let (manager, _temp_dir) = create_test_session_manager();

        let session = manager.get_session("nonexistent-id").await;
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn test_get_session_status_not_found() {
        let (manager, _temp_dir) = create_test_session_manager();

        let status = manager.get_session_status("nonexistent-id").await;
        assert!(status.is_none());
    }

    #[tokio::test]
    async fn test_update_session_status_not_found() {
        let (manager, _temp_dir) = create_test_session_manager();

        // Should not panic when updating non-existent session
        manager
            .update_session_status("nonexistent", SessionStatus::Running)
            .await;
    }

    #[tokio::test]
    async fn test_subscribe_output() {
        let (manager, _temp_dir) = create_test_session_manager();

        let rx = manager.subscribe_output();
        drop(rx);
    }

    #[tokio::test]
    async fn test_cleanup_stopped_sessions_empty() {
        let (manager, _temp_dir) = create_test_session_manager();

        manager.cleanup_stopped_sessions().await;

        let sessions = manager.list_sessions().await;
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_detect_waiting_input_no_session() {
        let (manager, _temp_dir) = create_test_session_manager();

        // Should return false for output without waiting pattern
        let waiting = manager.detect_waiting_input("nonexistent", "Some output").await;
        assert!(!waiting);
    }

    #[tokio::test]
    async fn test_detect_waiting_input_with_prompt() {
        let (manager, _temp_dir) = create_test_session_manager();

        // The detect_waiting_input method calls the parser internally
        // It should detect patterns like "> " or "❯ "
        // Since session doesn't exist, status won't be updated
        // but the parser logic should work
        let output_with_prompt = "Processing...\n> ";
        let waiting = manager
            .detect_waiting_input("nonexistent", output_with_prompt)
            .await;
        // Returns true because the pattern is detected, even though session doesn't exist
        assert!(waiting);
    }
}

mod session_lifecycle_tests {
    use super::*;

    fn create_test_db_with_config() -> (Database, TempDir, String) {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.db");
        let db = Database::new(&path).unwrap();
        db.init_schema().unwrap();

        let config = SessionConfig::new(
            "Test Session".to_string(),
            "windows".to_string(),
            "C:\\test".to_string(),
            "cmd".to_string(),
        );
        let config_id = config.id.clone();
        db.create_session_config(&config).unwrap();

        (db, temp_dir, config_id)
    }

    #[tokio::test]
    async fn test_session_manager_with_existing_config() {
        let (db, _temp_dir, config_id) = create_test_db_with_config();
        let manager = SessionManager::from_database(db);

        // Verify config exists
        let sessions = manager.list_sessions().await;
        assert!(sessions.is_empty());

        // Config ID should be valid
        assert!(!config_id.is_empty());
    }
}

mod pty_output_event_tests {
    use bedcode_lib::pty::PtyOutputEvent;

    #[test]
    fn test_pty_output_event_creation() {
        let event = PtyOutputEvent {
            session_id: "session-1".to_string(),
            data: "SGVsbG8sIFdvcmxkIQ==".to_string(), // Base64 encoded
            timestamp: chrono::Utc::now(),
        };

        assert_eq!(event.session_id, "session-1");
        assert_eq!(event.data, "SGVsbG8sIFdvcmxkIQ==");
    }

    #[test]
    fn test_pty_output_event_clone() {
        let event = PtyOutputEvent {
            session_id: "session-1".to_string(),
            data: "dGVzdA==".to_string(),
            timestamp: chrono::Utc::now(),
        };

        let cloned = event.clone();
        assert_eq!(event.session_id, cloned.session_id);
        assert_eq!(event.data, cloned.data);
    }

    #[test]
    fn test_pty_output_event_serialization() {
        let event = PtyOutputEvent {
            session_id: "session-1".to_string(),
            data: "dGVzdCBkYXRh".to_string(),
            timestamp: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("session-1"));
        assert!(json.contains("dGVzdCBkYXRh"));
    }
}

mod session_config_tests {
    use super::*;

    #[test]
    fn test_session_config_new() {
        let config = SessionConfig::new(
            "My Project".to_string(),
            "windows".to_string(),
            "C:\\Projects\\myapp".to_string(),
            "claude".to_string(),
        );

        assert!(!config.id.is_empty());
        assert_eq!(config.name, "My Project");
        assert_eq!(config.environment, "windows");
        assert_eq!(config.working_dir, "C:\\Projects\\myapp");
        assert_eq!(config.command, "claude");
        assert!(config.wsl_distro.is_none());
        assert!(config.tmux_session.is_none());
    }

    #[test]
    fn test_session_config_with_wsl() {
        let mut config = SessionConfig::new(
            "WSL Project".to_string(),
            "wsl2".to_string(),
            "/home/user/project".to_string(),
            "claude".to_string(),
        );
        config.wsl_distro = Some("Ubuntu".to_string());

        assert_eq!(config.environment, "wsl2");
        assert_eq!(config.wsl_distro, Some("Ubuntu".to_string()));
    }

    #[test]
    fn test_session_config_with_tmux() {
        let mut config = SessionConfig::new(
            "Tmux Session".to_string(),
            "wsl2".to_string(),
            "/home/user".to_string(),
            "claude".to_string(),
        );
        config.tmux_session = Some("existing-session".to_string());

        assert_eq!(config.tmux_session, Some("existing-session".to_string()));
    }

    #[test]
    fn test_session_config_serialization() {
        let config = SessionConfig::new(
            "Test".to_string(),
            "windows".to_string(),
            "C:\\test".to_string(),
            "cmd".to_string(),
        );

        let json = serde_json::to_string(&config).unwrap();
        let parsed: SessionConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.id, parsed.id);
        assert_eq!(config.name, parsed.name);
        assert_eq!(config.environment, parsed.environment);
    }
}

mod error_handling_tests {
    use bedcode_lib::error::AppError;

    #[test]
    fn test_session_not_found_error() {
        let err = AppError::NotFound("Session not found: session-123".to_string());
        assert!(err.to_string().contains("Session not found"));
    }

    #[test]
    fn test_config_not_found_error() {
        let err = AppError::NotFound("Config not found: config-456".to_string());
        assert!(err.to_string().contains("Config not found"));
    }

    #[test]
    fn test_session_error() {
        let err = AppError::Session("Failed to start PTY".to_string());
        assert!(err.to_string().contains("Session error"));
    }
}
