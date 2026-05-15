//! Config Model - Application and network configuration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub network: NetworkConfig,
    pub session: SessionConfigDefaults,
    pub ui: UiConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            network: NetworkConfig::default(),
            session: SessionConfigDefaults::default(),
            ui: UiConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load(path: &PathBuf) -> crate::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self, path: &PathBuf) -> crate::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub port: u16,
    pub heartbeat_interval_secs: u64,
    pub heartbeat_timeout_secs: u64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            port: 8765,
            heartbeat_interval_secs: 30,
            heartbeat_timeout_secs: 90,
        }
    }
}

/// Session defaults configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfigDefaults {
    pub default_environment: String,
    pub default_wsl_distro: Option<String>,
    pub default_working_dir: Option<String>,
    pub default_command: Option<String>,
    pub session_timeout: u64,
}

impl Default for SessionConfigDefaults {
    fn default() -> Self {
        Self {
            default_environment: "windows".to_string(),
            default_wsl_distro: None,
            default_working_dir: None,
            default_command: Some("claude".to_string()),
            session_timeout: 3600,
        }
    }
}

/// UI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: String,
    pub terminal_font_size: u8,
    pub terminal_font_family: String,
    pub show_preview: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            terminal_font_size: 14,
            terminal_font_family: "Consolas".to_string(),
            show_preview: true,
        }
    }
}