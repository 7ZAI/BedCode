//! Session Info Registry
//!
//! 会话信息注册表 - 负责会话元数据的存储和状态管理

use crate::desktop::model::SessionInfo;
use crate::shared::enums::SessionStatus;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

/// 会话信息注册表 trait
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

/// 会话信息注册表实现
pub struct DefaultSessionInfoRegistry {
    info: Arc<RwLock<HashMap<String, SessionInfo>>>,
}

impl DefaultSessionInfoRegistry {
    pub fn new() -> Self {
        Self {
            info: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for DefaultSessionInfoRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionInfoRegistry for DefaultSessionInfoRegistry {
    async fn insert(&self, info: SessionInfo) {
        let mut map = self.info.write().await;
        map.insert(info.id.clone(), info);
    }

    async fn remove(&self, id: &str) -> Option<SessionInfo> {
        let mut map = self.info.write().await;
        map.remove(id)
    }

    async fn get(&self, id: &str) -> Option<SessionInfo> {
        let map = self.info.read().await;
        map.get(id).cloned()
    }

    async fn list(&self) -> Vec<SessionInfo> {
        let map = self.info.read().await;
        map.values().cloned().collect()
    }

    async fn update_status(&self, id: &str, status: SessionStatus) {
        let mut map = self.info.write().await;
        if let Some(info) = map.get_mut(id) {
            info.status = status;
        }
    }

    async fn update_status_with_time(&self, id: &str, status: SessionStatus) {
        let mut map = self.info.write().await;
        if let Some(info) = map.get_mut(id) {
            info.status = status.clone();
            match status {
                SessionStatus::Running => {
                    if info.started_at.is_none() {
                        info.started_at = Some(Utc::now());
                    }
                }
                SessionStatus::Stopped | SessionStatus::Error(_) => {
                    if info.stopped_at.is_none() {
                        info.stopped_at = Some(Utc::now());
                    }
                }
                _ => {}
            }
        }
    }

    async fn get_status(&self, id: &str) -> Option<SessionStatus> {
        let map = self.info.read().await;
        map.get(id).map(|i| i.status.clone())
    }

    async fn filter_by_config(&self, config_id: &str) -> Vec<SessionInfo> {
        let map = self.info.read().await;
        map.values()
            .filter(|s| s.config_id == config_id)
            .cloned()
            .collect()
    }

    async fn filter_active_by_config(&self, config_id: &str) -> Vec<SessionInfo> {
        let map = self.info.read().await;
        map.values()
            .filter(|s| s.config_id == config_id && s.status != SessionStatus::Stopped)
            .cloned()
            .collect()
    }
}