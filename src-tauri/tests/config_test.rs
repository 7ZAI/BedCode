//! Tests for configuration management

use bedcode_lib::config::{AppConfig, NetworkConfig, SessionConfig, UiConfig};
use tempfile::NamedTempFile;
use std::path::PathBuf;

#[test]
fn test_default_network_config() {
    let config = NetworkConfig::default();

    assert_eq!(config.port, 8765);
    assert_eq!(config.service_name, "bedcode");
    assert!(config.enable_discovery);
    assert_eq!(config.heartbeat_interval_secs, 30);
    assert_eq!(config.heartbeat_timeout_secs, 90);
}

#[test]
fn test_default_session_config() {
    let config = SessionConfig::default();

    assert_eq!(config.default_environment, "windows");
    assert!(config.default_wsl_distro.is_none());
    assert!(config.default_working_dir.is_none());
    assert_eq!(config.default_command, Some("claude".to_string()));
    assert_eq!(config.session_timeout, 3600);
}

#[test]
fn test_default_ui_config() {
    let config = UiConfig::default();

    assert_eq!(config.theme, "system");
    assert_eq!(config.terminal_font_size, 14);
    assert_eq!(config.terminal_font_family, "Consolas");
    assert!(config.show_preview);
}

#[test]
fn test_default_app_config() {
    let config = AppConfig::default();

    assert_eq!(config.network.port, 8765);
    assert_eq!(config.session.default_environment, "windows");
    assert_eq!(config.ui.theme, "system");
}

#[test]
fn test_config_serialization() {
    let config = AppConfig::default();

    // Serialize
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"port\":8765"));
    assert!(json.contains("\"theme\":\"system\""));

    // Deserialize
    let deserialized: AppConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.network.port, config.network.port);
    assert_eq!(deserialized.ui.theme, config.ui.theme);
}

#[test]
fn test_config_save_and_load() {
    let config = AppConfig {
        network: NetworkConfig {
            port: 9999,
            service_name: "test-service".to_string(),
            enable_discovery: false,
            heartbeat_interval_secs: 20,
            heartbeat_timeout_secs: 60,
        },
        session: SessionConfig {
            default_environment: "wsl2".to_string(),
            default_wsl_distro: Some("Ubuntu".to_string()),
            default_working_dir: Some("/home/user".to_string()),
            default_command: Some("bash".to_string()),
            session_timeout: 7200,
        },
        ui: UiConfig {
            theme: "dark".to_string(),
            terminal_font_size: 16,
            terminal_font_family: "Fira Code".to_string(),
            show_preview: false,
        },
    };

    // Save to temp file
    let temp_file = NamedTempFile::new().unwrap();
    let path = PathBuf::from(temp_file.path());

    config.save(&path).unwrap();

    // Load from file
    let loaded = AppConfig::load(&path).unwrap();

    assert_eq!(loaded.network.port, 9999);
    assert_eq!(loaded.network.service_name, "test-service");
    assert!(!loaded.network.enable_discovery);

    assert_eq!(loaded.session.default_environment, "wsl2");
    assert_eq!(loaded.session.default_wsl_distro, Some("Ubuntu".to_string()));
    assert_eq!(loaded.session.session_timeout, 7200);

    assert_eq!(loaded.ui.theme, "dark");
    assert_eq!(loaded.ui.terminal_font_size, 16);
    assert_eq!(loaded.ui.terminal_font_family, "Fira Code");
    assert!(!loaded.ui.show_preview);
}

#[test]
fn test_config_load_nonexistent_file() {
    let path = PathBuf::from("/nonexistent/path/config.json");
    let config = AppConfig::load(&path).unwrap();

    // Should return default config
    assert_eq!(config.network.port, 8765);
    assert_eq!(config.ui.theme, "system");
}

#[test]
fn test_network_config_custom() {
    let config = NetworkConfig {
        port: 8080,
        service_name: "custom-service".to_string(),
        enable_discovery: false,
        heartbeat_interval_secs: 15,
        heartbeat_timeout_secs: 45,
    };

    assert_eq!(config.port, 8080);
    assert_eq!(config.service_name, "custom-service");
    assert!(!config.enable_discovery);
    assert_eq!(config.heartbeat_interval_secs, 15);
    assert_eq!(config.heartbeat_timeout_secs, 45);
}

#[test]
fn test_session_config_with_wsl() {
    let config = SessionConfig {
        default_environment: "wsl2".to_string(),
        default_wsl_distro: Some("Ubuntu-22.04".to_string()),
        default_working_dir: Some("/home/user/projects".to_string()),
        default_command: Some("claude".to_string()),
        session_timeout: 1800,
    };

    assert_eq!(config.default_environment, "wsl2");
    assert_eq!(config.default_wsl_distro, Some("Ubuntu-22.04".to_string()));
}
