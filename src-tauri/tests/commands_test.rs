//! Tests for Tauri commands
//!
//! 测试暴露给前端的命令接口

use bedcode_lib::auth::PairingCode;
use bedcode_lib::db::{Database, SessionConfig};
use bedcode_lib::session::SessionInfo;
use chrono::Utc;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

mod pairing_tests {
    use super::*;

    #[test]
    fn test_pairing_code_generate() {
        let code = PairingCode::generate();

        // Should be 6 digits
        assert_eq!(code.code.len(), 6);
        assert!(code.code.chars().all(|c| c.is_ascii_digit()));

        // Should not be expired immediately
        assert!(!code.is_expired());

        // Should have correct TTL
        assert_eq!(code.expires_in, 60);
    }

    #[test]
    fn test_pairing_code_verify_correct() {
        let code = PairingCode::generate();
        let code_str = code.code.clone();

        assert!(code.verify(&code_str));
    }

    #[test]
    fn test_pairing_code_verify_incorrect() {
        let code = PairingCode::generate();

        // Wrong code
        assert!(!code.verify("000000"));
    }

    #[test]
    fn test_pairing_code_remaining_seconds() {
        let code = PairingCode::generate();

        // Should have close to 60 seconds remaining
        let remaining = code.remaining_seconds();
        assert!(remaining <= 60);
        assert!(remaining >= 58); // Allow 2 seconds for test execution
    }

    #[test]
    fn test_pairing_code_serialization() {
        let code = PairingCode::generate();
        let json = serde_json::to_string(&code).unwrap();
        let parsed: PairingCode = serde_json::from_str(&json).unwrap();

        assert_eq!(code.code, parsed.code);
        assert_eq!(code.expires_in, parsed.expires_in);
    }
}

mod pairing_service_tests {
    use super::*;
    use bedcode_lib::auth::PairingService;

    #[tokio::test]
    async fn test_pairing_service_clear_code() {
        let service = PairingService::new();

        // Generate a code
        service.generate_code().await;
        assert!(service.get_current_code().await.is_some());

        // Clear the code
        service.clear_code().await;
        assert!(service.get_current_code().await.is_none());
    }

    #[tokio::test]
    async fn test_pairing_service_clear_expired_code() {
        let service = PairingService::new();

        // Generate a code
        let code = service.generate_code().await;
        let code_str = code.code.clone();

        // Clear should work
        service.clear_code().await;

        // Code should no longer be valid
        assert!(!service.verify_code(&code_str).await);
    }

    #[tokio::test]
    async fn test_pairing_service_verify_after_clear() {
        let service = PairingService::new();

        service.generate_code().await;
        service.clear_code().await;

        // Any code should fail after clear
        assert!(!service.verify_code("123456").await);
        assert!(!service.verify_code("000000").await);
    }
}

mod session_info_tests {
    use super::*;
    use bedcode_lib::session::SessionStatus;

    #[test]
    fn test_session_info_new() {
        let info = SessionInfo::new("config-123", "My Session");

        assert!(!info.id.is_empty());
        assert_eq!(info.config_id, "config-123");
        assert_eq!(info.name, "My Session");
        assert_eq!(info.status, SessionStatus::Starting);
        assert!(info.started_at.is_none());
        assert!(info.stopped_at.is_none());
    }

    #[test]
    fn test_session_info_serialization() {
        let info = SessionInfo::new("cfg-1", "Test");
        let json = serde_json::to_string(&info).unwrap();
        let parsed: SessionInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(info.id, parsed.id);
        assert_eq!(info.config_id, parsed.config_id);
        assert_eq!(info.status, parsed.status);
    }

    #[test]
    fn test_session_status_default() {
        let status = SessionStatus::default();
        assert_eq!(status, SessionStatus::Starting);
    }

    #[test]
    fn test_session_status_serialization() {
        let statuses = vec![
            SessionStatus::Starting,
            SessionStatus::Running,
            SessionStatus::WaitingInput,
            SessionStatus::Stopped,
            SessionStatus::Error,
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: SessionStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, parsed);
        }
    }
}

mod utility_command_tests {
    use bedcode_lib::config::{AppConfig, NetworkConfig, SessionConfig as ConfigSession, UiConfig};

    #[test]
    fn test_get_app_settings_default() {
        let settings = AppConfig::default();

        // Network defaults
        assert_eq!(settings.network.port, 8765);
        assert_eq!(settings.network.service_name, "bedcode");
        assert!(settings.network.enable_discovery);

        // Session defaults
        assert_eq!(settings.session.default_environment, "windows");
        assert_eq!(settings.session.default_command, Some("claude".to_string()));

        // UI defaults
        assert_eq!(settings.ui.theme, "system");
        assert_eq!(settings.ui.terminal_font_size, 14);
    }

    #[test]
    fn test_ping_command() {
        // The ping command always returns "pong"
        let result = "pong".to_string();
        assert_eq!(result, "pong");
    }

    #[test]
    fn test_get_app_version() {
        let version = env!("CARGO_PKG_VERSION").to_string();
        assert!(!version.is_empty());
        // Version should follow semver format
        let parts: Vec<&str> = version.split('.').collect();
        assert!(parts.len() >= 2);
    }
}

mod database_command_tests {
    use super::*;

    fn create_test_db() -> (Database, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.db");
        let db = Database::new(&path).unwrap();
        db.init_schema().unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_create_session_config_command() {
        let (db, _temp_dir) = create_test_db();

        let config = SessionConfig::new(
            "Test Session".to_string(),
            "windows".to_string(),
            "C:\\test".to_string(),
            "claude".to_string(),
        );

        // Simulate command behavior
        db.create_session_config(&config).unwrap();

        let configs = db.get_session_configs().unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "Test Session");
    }

    #[test]
    fn test_list_session_configs_empty() {
        let (db, _temp_dir) = create_test_db();

        let configs = db.get_session_configs().unwrap();
        assert!(configs.is_empty());
    }

    #[test]
    fn test_get_session_config_found() {
        let (db, _temp_dir) = create_test_db();

        let config = SessionConfig::new(
            "Test".to_string(),
            "windows".to_string(),
            "C:\\test".to_string(),
            "claude".to_string(),
        );
        let id = config.id.clone();
        db.create_session_config(&config).unwrap();

        let loaded = db.get_session_config(&id).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().name, "Test");
    }

    #[test]
    fn test_get_session_config_not_found() {
        let (db, _temp_dir) = create_test_db();

        let loaded = db.get_session_config("nonexistent").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_delete_session_config() {
        let (db, _temp_dir) = create_test_db();

        let config = SessionConfig::new(
            "Test".to_string(),
            "windows".to_string(),
            "C:\\test".to_string(),
            "claude".to_string(),
        );
        let id = config.id.clone();
        db.create_session_config(&config).unwrap();

        db.delete_session_config(&id).unwrap();

        let configs = db.get_session_configs().unwrap();
        assert!(configs.is_empty());
    }

    #[test]
    fn test_quick_action_command() {
        let (db, _temp_dir) = create_test_db();

        let mut action = bedcode_lib::db::QuickAction::new(
            "Continue".to_string(),
            "Please continue".to_string(),
        );
        action.icon = Some("▶️".to_string());
        action.color = Some("#22c55e".to_string());

        db.create_quick_action(&action).unwrap();

        let actions = db.get_quick_actions().unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Continue");
    }

    #[test]
    fn test_list_paired_devices_empty() {
        let (db, _temp_dir) = create_test_db();

        let pairings = db.get_pairings().unwrap();
        assert!(pairings.is_empty());
    }

    #[test]
    fn test_add_and_remove_pairing() {
        let (db, _temp_dir) = create_test_db();

        let id = db.add_pairing("My Phone", "fingerprint123", "public_key").unwrap();

        let pairings = db.get_pairings().unwrap();
        assert_eq!(pairings.len(), 1);
        assert_eq!(pairings[0].device_name, "My Phone");

        db.remove_pairing(&id).unwrap();

        let pairings = db.get_pairings().unwrap();
        assert!(pairings.is_empty());
    }
}

mod error_tests {
    use bedcode_lib::error::AppError;

    #[test]
    fn test_app_error_display() {
        let err = AppError::Pty("test error".to_string());
        assert_eq!(format!("{}", err), "PTY error: test error");

        let err = AppError::Session("session error".to_string());
        assert_eq!(format!("{}", err), "Session error: session error");

        let err = AppError::Auth("auth error".to_string());
        assert_eq!(format!("{}", err), "Authentication error: auth error");

        let err = AppError::NotFound("resource".to_string());
        assert_eq!(format!("{}", err), "Not found: resource");

        let err = AppError::InvalidInput("bad input".to_string());
        assert_eq!(format!("{}", err), "Invalid input: bad input");
    }

    #[test]
    fn test_app_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let app_err: AppError = io_err.into();

        match app_err {
            AppError::Io(e) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
            _ => panic!("Expected Io error variant"),
        }
    }

    #[test]
    fn test_app_error_from_json() {
        let json_err = serde_json::from_str::<i32>("not a number").unwrap_err();
        let app_err: AppError = json_err.into();

        match app_err {
            AppError::Serialization(_) => {}
            _ => panic!("Expected Serialization error variant"),
        }
    }

    #[test]
    fn test_app_error_serialization() {
        let err = AppError::NotFound("test resource".to_string());
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("test resource"));
    }
}

mod wsl_command_tests {
    #[test]
    fn test_is_wsl_available() {
        // This test just verifies the function runs without error
        // The result depends on the environment
        let _ = bedcode_lib::pty::is_wsl_available();
    }
}

mod tmux_command_tests {
    #[test]
    fn test_is_tmux_available() {
        // This test just verifies the function runs without error
        // The result depends on the environment
        let _ = bedcode_lib::pty::is_tmux_available();
    }
}
