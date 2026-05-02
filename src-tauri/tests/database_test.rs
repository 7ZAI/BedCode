//! Tests for database operations

use bedcode_lib::db::{Database, SessionConfig, QuickAction, Message, MessageType};
use tempfile::TempDir;
use std::path::PathBuf;

fn create_test_db() -> (Database, TempDir) {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("test.db");
    let db = Database::new(&path).unwrap();
    db.init_schema().unwrap();
    (db, temp_dir)
}

#[test]
fn test_database_init() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("test.db");

    let db = Database::new(&path).unwrap();
    db.init_schema().unwrap();

    // Verify tables exist
    let count: i32 = db.conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
            [],
            |row| row.get(0)
        )
        .unwrap();

    assert!(count >= 5, "Expected at least 5 tables to be created");
}

#[test]
fn test_pairing_crud() {
    let (db, _temp_dir) = create_test_db();

    // Create pairing
    let id = db.add_pairing(
        "Test Device",
        "fingerprint123",
        "public_key_data"
    ).unwrap();

    assert!(!id.is_empty());

    // Get pairings
    let pairings = db.get_pairings().unwrap();
    assert_eq!(pairings.len(), 1);
    assert_eq!(pairings[0].device_name, "Test Device");
    assert_eq!(pairings[0].device_fingerprint, "fingerprint123");
    assert!(pairings[0].is_active);

    // Verify pairing
    let is_valid = db.verify_pairing("fingerprint123").unwrap();
    assert!(is_valid);

    let is_invalid = db.verify_pairing("wrong_fingerprint").unwrap();
    assert!(!is_invalid);

    // Remove pairing
    db.remove_pairing(&id).unwrap();

    let pairings = db.get_pairings().unwrap();
    assert_eq!(pairings.len(), 0);
}

#[test]
fn test_session_config_crud() {
    let (db, _temp_dir) = create_test_db();

    let config = SessionConfig::new(
        "Test Session".to_string(),
        "windows".to_string(),
        "C:\\Users\\test".to_string(),
        "claude".to_string()
    );

    let id = config.id.clone();

    // Create
    db.create_session_config(&config).unwrap();

    // Read
    let configs = db.get_session_configs().unwrap();
    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0].name, "Test Session");

    // Read single
    let loaded = db.get_session_config(&id).unwrap();
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().name, "Test Session");

    // Delete
    db.delete_session_config(&id).unwrap();

    let configs = db.get_session_configs().unwrap();
    assert_eq!(configs.len(), 0);
}

#[test]
fn test_session_config_with_wsl() {
    let (db, _temp_dir) = create_test_db();

    let mut config = SessionConfig::new(
        "WSL Session".to_string(),
        "wsl2".to_string(),
        "/home/user".to_string(),
        "claude".to_string()
    );
    config.wsl_distro = Some("Ubuntu".to_string());
    config.tmux_session = Some("existing_session".to_string());

    db.create_session_config(&config).unwrap();

    let loaded = db.get_session_config(&config.id).unwrap().unwrap();

    assert_eq!(loaded.environment, "wsl2");
    assert_eq!(loaded.wsl_distro, Some("Ubuntu".to_string()));
    assert_eq!(loaded.tmux_session, Some("existing_session".to_string()));
}

#[test]
fn test_quick_action_crud() {
    let (db, _temp_dir) = create_test_db();

    let action = QuickAction::new(
        "Continue".to_string(),
        "Please continue".to_string()
    )
    .with_icon("▶️".to_string())
    .with_color("#22c55e".to_string());

    // Create
    db.create_quick_action(&action).unwrap();

    // Read
    let actions = db.get_quick_actions().unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].name, "Continue");
    assert_eq!(actions[0].content, "Please continue");
    assert_eq!(actions[0].icon, Some("▶️".to_string()));
    assert_eq!(actions[0].color, Some("#22c55e".to_string()));

    // Delete
    db.delete_quick_action(&action.id).unwrap();

    let actions = db.get_quick_actions().unwrap();
    assert_eq!(actions.len(), 0);
}

#[test]
fn test_message_crud() {
    let (db, _temp_dir) = create_test_db();

    // Create session config first
    let config = SessionConfig::new(
        "Test Session".to_string(),
        "windows".to_string(),
        "C:\\test".to_string(),
        "claude".to_string()
    );
    db.create_session_config(&config).unwrap();

    // Create input message
    let input = Message::new_input(config.id.clone(), "Hello, Claude!".to_string());
    db.add_message(&input).unwrap();

    // Create output message
    let output = Message::new_output(config.id.clone(), "Hello! How can I help you?".to_string());
    db.add_message(&output).unwrap();

    // Get messages
    let messages = db.get_messages(&config.id, None, None).unwrap();
    assert_eq!(messages.len(), 2);

    assert_eq!(messages[0].message_type, MessageType::Input);
    assert_eq!(messages[0].content, "Hello, Claude!");

    assert_eq!(messages[1].message_type, MessageType::Output);
    assert_eq!(messages[1].content, "Hello! How can I help you?");
}

#[test]
fn test_message_search() {
    let (db, _temp_dir) = create_test_db();

    // Create session config
    let config = SessionConfig::new(
        "Search Test".to_string(),
        "windows".to_string(),
        "C:\\test".to_string(),
        "claude".to_string()
    );
    db.create_session_config(&config).unwrap();

    // Create messages
    let msg1 = Message::new_input(config.id.clone(), "Fix the bug in login".to_string());
    let msg2 = Message::new_output(config.id.clone(), "I'll help you fix the bug".to_string());
    let msg3 = Message::new_input(config.id.clone(), "Add new feature".to_string());

    db.add_message(&msg1).unwrap();
    db.add_message(&msg2).unwrap();
    db.add_message(&msg3).unwrap();

    // Search for "bug"
    let results = db.search_messages("bug", None).unwrap();
    assert_eq!(results.len(), 2);

    // Search for "feature"
    let results = db.search_messages("feature", None).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].content, "Add new feature");
}

#[test]
fn test_message_clear() {
    let (db, _temp_dir) = create_test_db();

    // Create session config
    let config = SessionConfig::new(
        "Clear Test".to_string(),
        "windows".to_string(),
        "C:\\test".to_string(),
        "claude".to_string()
    );
    db.create_session_config(&config).unwrap();

    // Create messages
    for i in 0..10 {
        let msg = Message::new_input(config.id.clone(), format!("Message {}", i));
        db.add_message(&msg).unwrap();
    }

    let messages = db.get_messages(&config.id, None, None).unwrap();
    assert_eq!(messages.len(), 10);

    // Clear messages for this session
    let deleted = db.clear_messages(Some(&config.id), None).unwrap();
    assert_eq!(deleted, 10);

    let messages = db.get_messages(&config.id, None, None).unwrap();
    assert_eq!(messages.len(), 0);
}

#[test]
fn test_settings() {
    let (db, _temp_dir) = create_test_db();

    // Set setting
    db.set_setting("theme", "dark").unwrap();
    db.set_setting("font_size", "16").unwrap();

    // Get setting
    let theme = db.get_setting("theme").unwrap();
    assert_eq!(theme, Some("dark".to_string()));

    let font_size = db.get_setting("font_size").unwrap();
    assert_eq!(font_size, Some("16".to_string()));

    // Get non-existent setting
    let nonexistent = db.get_setting("nonexistent").unwrap();
    assert!(nonexistent.is_none());

    // Update setting
    db.set_setting("theme", "light").unwrap();
    let theme = db.get_setting("theme").unwrap();
    assert_eq!(theme, Some("light".to_string()));

    // Get all settings
    let all = db.get_all_settings().unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn test_multiple_pairings() {
    let (db, _temp_dir) = create_test_db();

    // Create multiple pairings
    let id1 = db.add_pairing("Device 1", "fp1", "pk1").unwrap();
    let id2 = db.add_pairing("Device 2", "fp2", "pk2").unwrap();
    let id3 = db.add_pairing("Device 3", "fp3", "pk3").unwrap();

    let pairings = db.get_pairings().unwrap();
    assert_eq!(pairings.len(), 3);

    // Remove one
    db.remove_pairing(&id2).unwrap();

    let pairings = db.get_pairings().unwrap();
    assert_eq!(pairings.len(), 2);

    // Verify the correct one was removed
    let fingerprints: Vec<_> = pairings.iter()
        .map(|p| p.device_fingerprint.as_str())
        .collect();
    assert!(fingerprints.contains(&"fp1"));
    assert!(fingerprints.contains(&"fp3"));
    assert!(!fingerprints.contains(&"fp2"));
}
