//! PTY (Pseudo Terminal) Manager
//!
//! 提供跨平台的 PTY 管理功能，支持 Windows 原生和 WSL2 环境

mod pty_process;
mod wsl;
mod tmux;

pub use pty_process::*;
pub use wsl::*;
pub use tmux::*;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 执行环境类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ExecutionEnvironment {
    /// Windows 原生环境
    Windows {
        shell: WindowsShell,
    },
    /// WSL2 环境
    Wsl2 {
        distro: String,
    },
}

/// Windows Shell 类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WindowsShell {
    PowerShell,
    Cmd,
}

impl Default for ExecutionEnvironment {
    fn default() -> Self {
        Self::Windows {
            shell: WindowsShell::PowerShell,
        }
    }
}

/// 会话启动配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLaunchConfig {
    /// 会话名称
    pub name: String,
    /// 执行环境
    pub environment: ExecutionEnvironment,
    /// 工作目录
    pub working_dir: String,
    /// 启动命令
    pub command: String,
    /// 环境变量
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
    /// Tmux 会话名（可选）
    pub tmux_session: Option<String>,
    /// 终端列数
    #[serde(default = "default_cols")]
    pub cols: u16,
    /// 终端行数
    #[serde(default = "default_rows")]
    pub rows: u16,
}

fn default_cols() -> u16 { 120 }
fn default_rows() -> u16 { 40 }

impl SessionLaunchConfig {
    /// 创建新的启动配置
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            environment: ExecutionEnvironment::default(),
            working_dir: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            command: command.into(),
            env_vars: HashMap::new(),
            tmux_session: None,
            cols: default_cols(),
            rows: default_rows(),
        }
    }

    /// 设置执行环境
    pub fn with_environment(mut self, env: ExecutionEnvironment) -> Self {
        self.environment = env;
        self
    }

    /// 设置工作目录
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = dir.into();
        self
    }

    /// 设置 Tmux 会话
    pub fn with_tmux_session(mut self, session: impl Into<String>) -> Self {
        self.tmux_session = Some(session.into());
        self
    }
}
