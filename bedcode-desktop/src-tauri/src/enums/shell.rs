//! Shell Types
//!
//! 执行环境和 Shell 类型定义

use crate::system::config::AppConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Windows Shell 类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WindowsShell {
    PowerShell,
    Cmd,
}

impl Default for WindowsShell {
    fn default() -> Self {
        Self::PowerShell
    }
}

/// 执行环境类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ExecutionEnvironment {
    /// Windows 原生环境
    Windows { shell: WindowsShell },
    /// WSL2 环境
    Wsl2 { distro: String },
    /// Linux 原生环境（仅当 BedCode 运行在 Linux 上时可用）
    ///
    /// 直接 fork 当前用户 shell（bash）执行命令，工作目录用 POSIX 路径。
    /// 与 Wsl2 不同：无需走 wsl.exe 桥接，路径无须 /mnt 转换。
    Linux,
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
    /// 裸 argv（可选，pty 票 1 新增路径）：非空时宿主按 argv 数组原样 exec，
    /// **不做 shell 包装**（无 bash -lic / PowerShell -Command / CMD /K、无 WSL 路径转换）；
    /// 缺省/空 → 走 `command` 字符串 + 宿主 `build_command` 包装的旧路径。
    /// 插件经 `create-with-spec` 的 `commandArgs` 字段传入。
    #[serde(default)]
    pub command_args: Option<Vec<String>>,
    /// 环境变量
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
    /// 终端列数
    #[serde(default = "default_cols")]
    pub cols: u16,
    /// 终端行数
    #[serde(default = "default_rows")]
    pub rows: u16,
}

fn default_cols() -> u16 {
    AppConfig::global().terminal.default_cols
}

fn default_rows() -> u16 {
    AppConfig::global().terminal.default_rows
}

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
            command_args: None,
            env_vars: HashMap::new(),
            cols: default_cols(),
            rows: default_rows(),
        }
    }

    /// 设置执行环���
    pub fn with_environment(mut self, env: ExecutionEnvironment) -> Self {
        self.environment = env;
        self
    }

    /// 设置工作目录
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = dir.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ExecutionEnvironment 全变体 serde 往返（票据 23：跨端协议表面零覆盖）
    #[test]
    fn execution_environment_roundtrip_all_variants() {
        let cases = vec![
            ExecutionEnvironment::Windows {
                shell: WindowsShell::PowerShell,
            },
            ExecutionEnvironment::Windows {
                shell: WindowsShell::Cmd,
            },
            ExecutionEnvironment::Wsl2 {
                distro: "Ubuntu".to_string(),
            },
            ExecutionEnvironment::Linux,
        ];
        for env in cases {
            let json = serde_json::to_string(&env).unwrap();
            let back: ExecutionEnvironment = serde_json::from_str(&json).unwrap();
            // 无 PartialEq，用序列化等值断言
            assert_eq!(serde_json::to_string(&back).unwrap(), json, "变体往返不一致: {json}");
        }
    }

    /// WindowsShell wire 标签锁
    #[test]
    fn windows_shell_wire_labels_locked() {
        assert_eq!(
            serde_json::to_string(&WindowsShell::PowerShell).unwrap(),
            "\"PowerShell\""
        );
        assert_eq!(serde_json::to_string(&WindowsShell::Cmd).unwrap(), "\"Cmd\"");
    }

    /// Default 行为：Windows + PowerShell
    #[test]
    fn defaults_are_windows_powershell() {
        assert_eq!(WindowsShell::default(), WindowsShell::PowerShell);
        assert!(matches!(
            ExecutionEnvironment::default(),
            ExecutionEnvironment::Windows {
                shell: WindowsShell::PowerShell
            }
        ));
    }

    /// SessionLaunchConfig：serde 往返
    #[test]
    fn session_launch_config_roundtrip() {
        let cfg = SessionLaunchConfig {
            name: "dev".to_string(),
            environment: ExecutionEnvironment::Linux,
            working_dir: "/home/u".to_string(),
            command: "bash".to_string(),
            command_args: None,
            env_vars: {
                let mut m = HashMap::new();
                m.insert("FOO".to_string(), "bar".to_string());
                m
            },
            cols: 120,
            rows: 40,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: SessionLaunchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), json, "往返不一致: {json}");
        assert_eq!(back.name, "dev");
        assert_eq!(back.env_vars.get("FOO").map(String::as_str), Some("bar"));
    }

    /// 缺省字段反序列化：env_vars / cols / rows 有 serde default 兜底
    #[test]
    fn session_launch_config_missing_fields_default() {
        let json = r#"{"name":"x","environment":{"type":"Linux"},"working_dir":"/tmp","command":"ls"}"#;
        let cfg: SessionLaunchConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.env_vars.is_empty());
        assert!(cfg.cols > 0 && cfg.rows > 0);
    }
}
