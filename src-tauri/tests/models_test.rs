//! Tests for database models

use bedcode_lib::db::{
    SessionConfig, QuickAction, Message, MessageType,
    History, Pairing, Setting
};
use chrono::Utc;

mod session_config_tests {
    use super::*;

    #[test]
    fn test_session_config_new() {
        let config = SessionConfig::new(
            "Test Session".to_string(),
            "windows".to_string(),
            "C:\\Users\\test".to_string(),
            "claude".to_string(),
        );

        assert!(!config.id.is_empty());
        assert_eq!(config.name, "Test Session");
        assert_eq!(config.environment, "windows");
        assert_eq!(config.working_dir, "C:\\Users\\test");
        assert_eq!(config.command, "claude");
        assert!(config.wsl_distro.is_none());
        assert!(config.tmux_session.is_none());
        assert!(!config.auto_start);
    }

    #[test]
    fn test_session_config_id_unique() {
        let config1 = SessionConfig::new(
            "Session 1".to_string(),
            "windows".to_string(),
            "C:\\".to_string(),
            "cmd".to_string(),
        );

        let config2 = SessionConfig::new(
            "Session 2".to_string(),
            "windows".to_string(),
            "C:\\".to_string(),
            "cmd".to_string(),
        );

        assert_ne!(config1.id, config2.id);
    }

    #[test]
    fn test_session_config_timestamps() {
        let before = Utc::now();
        let config = SessionConfig::new(
            "Test".to_string(),
            "windows".to_string(),
            "C:\\".to_string(),
            "claude".to_string(),
        );
        let after = Utc::now();

        assert!(config.created_at >= before);
        assert!(config.created_at <= after);
        assert_eq!(config.created_at, config.updated_at);
    }

    #[test]
    fn test_session_config_serialization() {
        let config = SessionConfig::new(
            "Test".to_string(),
            "wsl2".to_string(),
            "/home/user".to_string(),
            "bash".to_string(),
        );

        let json = serde_json::to_string(&config).unwrap();
        let parsed: SessionConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.id, parsed.id);
        assert_eq!(config.name, parsed.name);
        assert_eq!(config.environment, parsed.environment);
        assert_eq!(config.working_dir, parsed.working_dir);
    }
}

mod message_tests {
    use super::*;

    #[test]
    fn test_message_new_input() {
        let msg = Message::new_input(
            "session-123".to_string(),
            "Hello, Claude!".to_string(),
        );

        assert!(!msg.id.is_empty());
        assert_eq!(msg.session_id, "session-123");
        assert_eq!(msg.message_type, MessageType::Input);
        assert_eq!(msg.content, "Hello, Claude!");
        assert!(msg.history_id.is_none());
        assert!(msg.metadata.is_none());
    }

    #[test]
    fn test_message_new_output() {
        let msg = Message::new_output(
            "session-456".to_string(),
            "Response text".to_string(),
        );

        assert_eq!(msg.message_type, MessageType::Output);
        assert_eq!(msg.content, "Response text");
    }

    #[test]
    fn test_message_with_history() {
        let msg = Message::new_input(
            "session-1".to_string(),
            "test".to_string(),
        ).with_history("history-1".to_string());

        assert_eq!(msg.history_id, Some("history-1".to_string()));
    }

    #[test]
    fn test_message_with_metadata() {
        let metadata = serde_json::json!({
            "token_count": 150,
            "model": "claude-3"
        });

        let msg = Message::new_output(
            "session-1".to_string(),
            "Response".to_string(),
        ).with_metadata(metadata.clone());

        assert!(msg.metadata.is_some());
        let meta = msg.metadata.unwrap();
        assert_eq!(meta["token_count"], 150);
    }

    #[test]
    fn test_message_type_as_str() {
        assert_eq!(MessageType::Input.as_str(), "input");
        assert_eq!(MessageType::Output.as_str(), "output");
    }

    #[test]
    fn test_message_type_from_str() {
        assert_eq!(MessageType::from_str("input"), Some(MessageType::Input));
        assert_eq!(MessageType::from_str("output"), Some(MessageType::Output));
        assert_eq!(MessageType::from_str("invalid"), None);
    }

    #[test]
    fn test_message_type_serialization() {
        let input = MessageType::Input;
        let json = serde_json::to_string(&input).unwrap();
        // MessageType serializes with PascalCase (as defined)
        assert!(json.contains("Input") || json.contains("input"));

        let parsed: MessageType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, MessageType::Input);
    }
}

mod quick_action_tests {
    use super::*;

    #[test]
    fn test_quick_action_new() {
        let action = QuickAction::new(
            "Continue".to_string(),
            "Please continue".to_string(),
        );

        assert!(!action.id.is_empty());
        assert_eq!(action.name, "Continue");
        assert_eq!(action.content, "Please continue");
        assert!(action.icon.is_none());
        assert!(action.color.is_none());
        assert!(action.category.is_none());
        assert_eq!(action.sort_order, 0);
    }

    #[test]
    fn test_quick_action_with_icon() {
        let action = QuickAction::new(
            "Test".to_string(),
            "Content".to_string(),
        ).with_icon("▶️".to_string());

        assert_eq!(action.icon, Some("▶️".to_string()));
    }

    #[test]
    fn test_quick_action_with_color() {
        let action = QuickAction::new(
            "Test".to_string(),
            "Content".to_string(),
        ).with_color("#22c55e".to_string());

        assert_eq!(action.color, Some("#22c55e".to_string()));
    }

    #[test]
    fn test_quick_action_with_category() {
        let action = QuickAction::new(
            "Test".to_string(),
            "Content".to_string(),
        ).with_category("navigation".to_string());

        assert_eq!(action.category, Some("navigation".to_string()));
    }

    #[test]
    fn test_quick_action_fluent_api() {
        let action = QuickAction::new(
            "Full".to_string(),
            "Content".to_string(),
        )
        .with_icon("🔧".to_string())
        .with_color("#3b82f6".to_string())
        .with_category("tools".to_string());

        assert_eq!(action.icon, Some("🔧".to_string()));
        assert_eq!(action.color, Some("#3b82f6".to_string()));
        assert_eq!(action.category, Some("tools".to_string()));
    }

    #[test]
    fn test_quick_action_serialization() {
        let action = QuickAction::new(
            "Test".to_string(),
            "Content".to_string(),
        ).with_icon("✅".to_string());

        let json = serde_json::to_string(&action).unwrap();
        let parsed: QuickAction = serde_json::from_str(&json).unwrap();

        assert_eq!(action.id, parsed.id);
        assert_eq!(action.name, parsed.name);
        assert_eq!(action.icon, parsed.icon);
    }
}

mod history_tests {
    use super::*;

    #[test]
    fn test_history_new() {
        let history = History::new(
            "session-1".to_string(),
            "My Session".to_string(),
        );

        assert!(!history.id.is_empty());
        assert_eq!(history.session_id, "session-1");
        assert_eq!(history.session_name, "My Session");
        assert!(history.device_id.is_none());
        assert!(history.ended_at.is_none());
        assert!(history.output_summary.is_none());
    }
}

mod setting_tests {
    use super::*;

    #[test]
    fn test_setting_new() {
        let setting = Setting::new(
            "theme".to_string(),
            "dark".to_string(),
        );

        assert_eq!(setting.key, "theme");
        assert_eq!(setting.value, "dark");
    }
}

mod pairing_tests {
    use super::*;

    #[test]
    fn test_pairing_struct() {
        let now = Utc::now();
        let pairing = Pairing {
            id: "pairing-1".to_string(),
            device_name: "My Phone".to_string(),
            device_fingerprint: "fp123".to_string(),
            public_key: "pk123".to_string(),
            paired_at: now,
            last_seen: None,
            is_active: true,
        };

        assert_eq!(pairing.id, "pairing-1");
        assert_eq!(pairing.device_name, "My Phone");
        assert!(pairing.is_active);
    }

    #[test]
    fn test_pairing_serialization() {
        let pairing = Pairing {
            id: "p1".to_string(),
            device_name: "Device".to_string(),
            device_fingerprint: "fp".to_string(),
            public_key: "pk".to_string(),
            paired_at: Utc::now(),
            last_seen: Some(Utc::now()),
            is_active: true,
        };

        let json = serde_json::to_string(&pairing).unwrap();
        let parsed: Pairing = serde_json::from_str(&json).unwrap();

        assert_eq!(pairing.id, parsed.id);
        assert_eq!(pairing.device_name, parsed.device_name);
    }
}
