//! Tests for database operations
#![allow(dead_code)]

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
fn test_database_init() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("test.db");

    let db = Database::new(&path).unwrap();
    db.init_schema().unwrap();

    let count: i32 = db.conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
            [],
            |row| row.get(0)
        )
        .unwrap();

    assert!(count >= 4, "Expected at least 4 tables to be created");
}

#[test]
fn test_pairing_crud() {
    let (db, _temp_dir) = create_test_db();

    let id = db.add_pairing(
        "Test Device",
        "fingerprint123",
        "public_key_data",
        None,
    ).unwrap();

    assert!(!id.is_empty());

    let pairings = db.get_pairings().unwrap();
    assert_eq!(pairings.len(), 1);
    assert_eq!(pairings[0].device_name, "Test Device");
    assert_eq!(pairings[0].device_fingerprint, "fingerprint123");
    assert!(pairings[0].is_active);

    let is_valid = db.verify_pairing("fingerprint123").unwrap();
    assert!(is_valid);

    let is_invalid = db.verify_pairing("wrong_fingerprint").unwrap();
    assert!(!is_invalid);

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

    db.create_session_config(&config).unwrap();

    let configs = db.get_session_configs().unwrap();
    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0].name, "Test Session");

    let loaded = db.get_session_config(&id).unwrap();
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().name, "Test Session");

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

    db.create_quick_action(&action).unwrap();

    let actions = db.get_quick_actions().unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].name, "Continue");
    assert_eq!(actions[0].content, "Please continue");
    assert_eq!(actions[0].icon, Some("▶️".to_string()));
    assert_eq!(actions[0].color, Some("#22c55e".to_string()));

    db.delete_quick_action(&action.id).unwrap();

    let actions = db.get_quick_actions().unwrap();
    assert_eq!(actions.len(), 0);
}

#[test]
fn test_settings() {
    let (db, _temp_dir) = create_test_db();

    db.set_setting("theme", "dark").unwrap();
    db.set_setting("font_size", "16").unwrap();

    let theme = db.get_setting("theme").unwrap();
    assert_eq!(theme, Some("dark".to_string()));

    let font_size = db.get_setting("font_size").unwrap();
    assert_eq!(font_size, Some("16".to_string()));

    let nonexistent = db.get_setting("nonexistent").unwrap();
    assert!(nonexistent.is_none());

    db.set_setting("theme", "light").unwrap();
    let theme = db.get_setting("theme").unwrap();
    assert_eq!(theme, Some("light".to_string()));

    let all = db.get_all_settings().unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn test_multiple_pairings() {
    let (db, _temp_dir) = create_test_db();

    let _id1 = db.add_pairing("Device 1", "fp1", "pk1", None).unwrap();
    let id2 = db.add_pairing("Device 2", "fp2", "pk2", None).unwrap();
    let _id3 = db.add_pairing("Device 3", "fp3", "pk3", None).unwrap();

    let pairings = db.get_pairings().unwrap();
    assert_eq!(pairings.len(), 3);

    db.remove_pairing(&id2).unwrap();

    let pairings = db.get_pairings().unwrap();
    assert_eq!(pairings.len(), 2);

    let fingerprints: Vec<_> = pairings.iter()
        .map(|p| p.device_fingerprint.as_str())
        .collect();
    assert!(fingerprints.contains(&"fp1"));
    assert!(fingerprints.contains(&"fp3"));
    assert!(!fingerprints.contains(&"fp2"));
}