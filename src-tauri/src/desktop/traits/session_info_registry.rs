//! Session Info Registry Trait
//!
//! 会话信息注册表 trait 定义

use crate::desktop::model::SessionInfo;
use crate::shared::enums::SessionStatus;

pub trait SessionInfoRegistry: Send + Sync {
    async fn insert(&self, info: SessionInfo);
    async fn remove(&self, id: &str) -> Option<SessionInfo>;
    async fn get(&self, id: &str) -> Option<SessionInfo>;
    async fn list(&self) -> Vec<SessionInfo>;
    async fn update_status(&self, id: &str, status: SessionStatus);
    async fn update_status_with_time(&self, id: &str, status: SessionStatus);
    async fn get_status(&self, id: &str) -> Option<SessionStatus>;
    async fn filter_by_config(&self, config_id: &str) -> Vec<SessionInfo>;
    async fn filter_active_by_config(&self, config_id: &str) -> Vec<SessionInfo>;
}