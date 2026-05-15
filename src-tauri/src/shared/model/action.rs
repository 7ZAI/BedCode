//! Action Model - Quick actions for AI interactions

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Quick action for AI interactions
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