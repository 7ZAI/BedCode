//! Database models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Paired device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pairing {
    pub id: String,
    pub device_name: String,
    pub device_fingerprint: String,
    pub public_key: String,
    pub paired_at: DateTime<Utc>,
    pub last_seen: Option<DateTime<Utc>>,
    pub is_active: bool,
}

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

/// Session history record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History {
    pub id: String,
    pub session_id: String,
    pub session_name: String,
    pub device_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub output_summary: Option<String>,
}

impl History {
    pub fn new(session_id: String, session_name: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id,
            session_name,
            device_id: None,
            started_at: Utc::now(),
            ended_at: None,
            output_summary: None,
        }
    }
}

/// Message type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageType {
    Input,
    Output,
}

impl MessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "input" => Some(Self::Input),
            "output" => Some(Self::Output),
            _ => None,
        }
    }
}

/// Message record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub history_id: Option<String>,
    pub message_type: MessageType,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

impl Message {
    pub fn new_input(session_id: String, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id,
            history_id: None,
            message_type: MessageType::Input,
            content,
            timestamp: Utc::now(),
            metadata: None,
        }
    }

    pub fn new_output(session_id: String, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id,
            history_id: None,
            message_type: MessageType::Output,
            content,
            timestamp: Utc::now(),
            metadata: None,
        }
    }

    pub fn with_history(mut self, history_id: String) -> Self {
        self.history_id = Some(history_id);
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Quick action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickAction {
    pub id: String,
    pub name: String,
    pub content: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub category: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

impl QuickAction {
    pub fn new(name: String, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            content,
            icon: None,
            color: None,
            category: None,
            sort_order: 0,
            created_at: Utc::now(),
        }
    }

    pub fn with_icon(mut self, icon: String) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn with_color(mut self, color: String) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_category(mut self, category: String) -> Self {
        self.category = Some(category);
        self
    }
}

/// App setting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub updated_at: DateTime<Utc>,
}

impl Setting {
    pub fn new(key: String, value: String) -> Self {
        Self {
            key,
            value,
            updated_at: Utc::now(),
        }
    }
}
