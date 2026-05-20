//! Config Mapper Trait
//!
//! 配置映射 trait 定义

use crate::desktop::pty::SessionLaunchConfig;
use crate::shared::db::SessionConfig;
use crate::Result;

pub trait ConfigMapper: Send + Sync {
    fn to_launch_config(&self, config: &SessionConfig) -> Result<SessionLaunchConfig>;
}