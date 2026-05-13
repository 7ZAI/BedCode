//! Notification Service
//!
//! 通知服务实现 - 跨平台共享

use std::sync::Arc;
use tokio::sync::Mutex;

pub use super::types::{Notification, NotificationPriority, NotificationSettings, NotificationType};

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

        // Check quiet hours - support overnight range (e.g., 22:00-08:00)
        if settings.quiet_hours_enabled && self.is_quiet_hours(&settings) {
            tracing::debug!("Quiet hours - notification suppressed");
            return Ok(());
        }

        // Store in history
        drop(settings);
        let mut history = self.history.lock().await;
        history.push(notification.clone());

        // Limit history to 100 entries to prevent unbounded memory growth
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