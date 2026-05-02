//! Configuration management

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Network configuration
    pub network: NetworkConfig,
    /// Session defaults
    pub session: SessionConfig,
    /// UI preferences
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// WebSocket server port
    pub port: u16,
    /// mDNS service name
    pub service_name: String,
    /// Enable mDNS discovery
    pub enable_discovery: bool,
    /// Heartbeat interval in seconds (client should send heartbeat)
    pub heartbeat_interval_secs: u64,
    /// Heartbeat timeout in seconds (server disconnects if no heartbeat)
    pub heartbeat_timeout_secs: u64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            port: 8765,
            service_name: "bedcode".to_string(),
            enable_discovery: true,
            heartbeat_interval_secs: 30,
            heartbeat_timeout_secs: 90,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Default environment (windows/wsl2)
    pub default_environment: String,
    /// Default WSL distribution
    pub default_wsl_distro: Option<String>,
    /// Default working directory
    pub default_working_dir: Option<String>,
    /// Default command to run
    pub default_command: Option<String>,
    /// Session timeout in seconds
    pub session_timeout: u64,
}

impl Default for SessionConfig {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Theme (light/dark/system)
    pub theme: String,
    /// Terminal font size
    pub terminal_font_size: u8,
    /// Terminal font family
    pub terminal_font_family: String,
    /// Show terminal preview
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

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            network: NetworkConfig::default(),
            session: SessionConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load configuration from file
    pub fn load(path: &PathBuf) -> crate::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self, path: &PathBuf) -> crate::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
