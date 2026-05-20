//! Naming Service Trait
//!
//! 命名服务 trait 定义

use crate::desktop::model::SessionInfo;
use crate::shared::enums::SessionStatus;

pub trait NamingService: Send + Sync {
    fn generate_unique_name(
        &self,
        config_id: &str,
        base_name: &str,
        sessions: &[SessionInfo],
    ) -> String;
}