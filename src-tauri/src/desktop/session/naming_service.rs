//! Naming Service
//!
//! 会话命名服务 - 生成唯一的会话名称

use crate::desktop::model::SessionInfo;
use crate::shared::enums::SessionStatus;

/// 命名服务 trait
pub trait NamingService: Send + Sync {
    fn generate_unique_name(
        &self,
        config_id: &str,
        base_name: &str,
        sessions: &[SessionInfo],
    ) -> String;
}

/// 默认命名服务实现
pub struct DefaultNamingService;

impl DefaultNamingService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultNamingService {
    fn default() -> Self {
        Self::new()
    }
}

impl NamingService for DefaultNamingService {
    fn generate_unique_name(
        &self,
        config_id: &str,
        base_name: &str,
        sessions: &[SessionInfo],
    ) -> String {
        let count = sessions
            .iter()
            .filter(|s| s.config_id == config_id && s.status != SessionStatus::Stopped)
            .count();

        if count == 0 {
            base_name.to_string()
        } else {
            format!("{}({})", base_name, count)
        }
    }
}