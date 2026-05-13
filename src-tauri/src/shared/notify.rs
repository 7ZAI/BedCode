//! Notification Service

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::{DateTime, Utc};

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
    pub quiet_hours_start: String, // HH:MM format
    pub quiet_hours_end: String,   // HH:MM format
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

/// Notification service
pub struct NotificationService {
    settings: Arc<Mutex<NotificationSettings>>,
    history: Arc<Mutex<Vec<Notification>>>,
}

impl NotificationService {
    pub fn new() -> Self {
        Self {
            settings: Arc::new(Mutex::new(NotificationSettings::default())),
            history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Send a notification (uses Tauri notification plugin)
    pub async fn send(&self, notification: &Notification) -> crate::Result<()> {
        let settings = self.settings.lock().await;

        if !settings.enabled {
            return Ok(());
        }

        // Check if notification type is enabled
        let should_notify = match notification.notification_type {
            NotificationType::WaitingInput => settings.waiting_input,
            NotificationType::SessionStarted | NotificationType::SessionEnded => settings.session_events,
            NotificationType::DeviceConnected | NotificationType::DeviceDisconnected | NotificationType::PairingRequest => settings.device_events,
            _ => true,
        };

        if !should_notify {
            return Ok(());
        }

        // Check quiet hours
        if settings.quiet_hours_enabled && self.is_quiet_hours(&settings) {
            tracing::debug!("Quiet hours - notification suppressed");
            return Ok(());
        }

        // Store in history
        drop(settings);
        let mut history = self.history.lock().await;
        history.push(notification.clone());

        // Keep only last 100 notifications
        if history.len() > 100 {
            history.remove(0);
        }

        tracing::info!("Notification: {} - {}", notification.title, notification.body);

        // In actual implementation, this would call Tauri notification plugin
        // Example:
        // use tauri_plugin_notification::NotificationExt;
        // app.notification().builder()
        //     .title(&notification.title)
        //     .body(&notification.body)
        //     .show()?;

        Ok(())
    }

    /// Check if current time is within quiet hours
    fn is_quiet_hours(&self, settings: &NotificationSettings) -> bool {
        let now = chrono::Local::now();
        let current_time = now.format("%H:%M").to_string();

        // Handle overnight quiet hours (e.g., 22:00 - 08:00)
        if settings.quiet_hours_start > settings.quiet_hours_end {
            current_time >= settings.quiet_hours_start || current_time <= settings.quiet_hours_end
        } else {
            current_time >= settings.quiet_hours_start && current_time <= settings.quiet_hours_end
        }
    }

    /// Get notification settings
    pub async fn get_settings(&self) -> NotificationSettings {
        self.settings.lock().await.clone()
    }

    /// Update notification settings
    pub async fn update_settings(&self, settings: NotificationSettings) {
        let mut current = self.settings.lock().await;
        *current = settings;
    }

    /// Get notification history
    pub async fn get_history(&self, limit: Option<usize>) -> Vec<Notification> {
        let history = self.history.lock().await;
        match limit {
            Some(n) => history.iter().rev().take(n).cloned().collect(),
            None => history.clone(),
        }
    }

    /// Mark notification as read
    pub async fn mark_read(&self, notification_id: &str) {
        let mut history = self.history.lock().await;
        if let Some(notification) = history.iter_mut().find(|n| n.id == notification_id) {
            notification.read = true;
        }
    }

    /// Clear all notifications
    pub async fn clear_history(&self) {
        let mut history = self.history.lock().await;
        history.clear();
    }

    /// Create waiting input notification
    pub fn create_waiting_input_notification(session_name: &str, session_id: &str) -> Notification {
        Notification::new(
            NotificationType::WaitingInput,
            "Claude Code 等待输入".to_string(),
            format!("会话 '{}' 正在等待您的输入", session_name),
        )
        .with_session(session_id.to_string())
        .with_priority(NotificationPriority::High)
    }

    /// Create device connected notification
    pub fn create_device_connected_notification(device_name: &str, device_id: &str) -> Notification {
        Notification::new(
            NotificationType::DeviceConnected,
            "设备已连接".to_string(),
            format!("设备 '{}' 已连接", device_name),
        )
        .with_device(device_id.to_string())
    }

    /// Create pairing request notification
    pub fn create_pairing_request_notification(code: &str) -> Notification {
        Notification::new(
            NotificationType::PairingRequest,
            "配对请求".to_string(),
            format!("配对码: {}", code),
        )
        .with_priority(NotificationPriority::Urgent)
    }
}

impl Default for NotificationService {
    fn default() -> Self {
        Self::new()
    }
}
