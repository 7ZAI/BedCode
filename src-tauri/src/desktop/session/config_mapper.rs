//! Config Mapper
//!
//! 配置映射服务 - 将数据库配置转换为启动配置

use crate::desktop::pty::{ExecutionEnvironment, SessionLaunchConfig, WindowsShell};
use crate::shared::db::SessionConfig;
use crate::Result;

/// 配置映射 trait
pub trait ConfigMapper: Send + Sync {
    fn to_launch_config(&self, config: &SessionConfig) -> Result<SessionLaunchConfig>;
}

/// 默认配置映射实现
pub struct DefaultConfigMapper;

impl DefaultConfigMapper {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultConfigMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigMapper for DefaultConfigMapper {
    fn to_launch_config(&self, config: &SessionConfig) -> Result<SessionLaunchConfig> {
        let environment = match config.environment.as_str() {
            "wsl2" => ExecutionEnvironment::Wsl2 {
                distro: config.wsl_distro.clone().unwrap_or_else(|| "Ubuntu".to_string()),
            },
            _ => ExecutionEnvironment::Windows {
                shell: WindowsShell::PowerShell,
            },
        };

        Ok(SessionLaunchConfig {
            name: config.name.clone(),
            environment,
            working_dir: config.working_dir.clone(),
            command: config.command.clone(),
            env_vars: std::collections::HashMap::new(),
            cols: 120,
            rows: 40,
        })
    }
}