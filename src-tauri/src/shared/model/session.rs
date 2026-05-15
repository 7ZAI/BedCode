//! Session Model - Session configuration

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfig {
    pub id: String,
    pub name: String,
    pub environment: String,
    pub wsl_distro: Option<String>,
    pub working_dir: String,
    pub command: String,
    pub tmux_session: Option<String>,
    pub auto_start: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SessionConfig {
    pub fn new(name: String, environment: String, working_dir: String, command: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            environment,
            wsl_distro: None,
            working_dir,
            command,
            tmux_session: None,
            auto_start: false,
            created_at: now,
            updated_at: now,
        }
    }
}