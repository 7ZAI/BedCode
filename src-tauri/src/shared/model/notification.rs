//! Notification Model - Notification types and structures

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Notification type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NotificationType {
    WaitingInput,
    SessionStarted,
    SessionEnded,
    DeviceConnected,
    DeviceDisconnected,
    PairingRequest,
    Error,
    Custom(String),
}

/// Notification priority
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl Default for NotificationPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Notification payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub notification_type: NotificationType,
    pub title: String,
    pub body: String,
    pub priority: NotificationPriority,
    pub session_id: Option<String>,
    pub device_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub read: bool,
    pub data: HashMap<String, String>,
}

impl Notification {
    pub fn new(notification_type: NotificationType, title: String, body: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            notification_type,
            title,
            body,
            priority: NotificationPriority::default(),
            session_id: None,
            device_id: None,
            timestamp: Utc::now(),
            read: false,
            data: HashMap::new(),
        }
    }

    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn with_device(mut self, device_id: String) -> Self {
        self.device_id = Some(device_id);
        self
    }

    pub fn with_priority(mut self, priority: NotificationPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_data(mut self, key: String, value: String) -> Self {
        self.data.insert(key, value);
        self
    }
}

/// Notification settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub enabled: bool,
    pub sound: bool,
    pub vibration: bool,
    pub waiting_input: bool,
    pub session_events: bool,
    pub device_events: bool,
    pub quiet_hours_enabled: bool,
    pub quiet_hours_start: String,
    pub quiet_hours_end: String,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            sound: true,
            vibration: true,
            waiting_input: true,
            session_events: true,
            device_events: true,
            quiet_hours_enabled: false,
            quiet_hours_start: "22:00".to_string(),
            quiet_hours_end: "08:00".to_string(),
        }
    }
}