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
    /// 启动命令（**仅诊断/日志用的人类可读命令行**；pty 引擎不解释它——
    /// 实际 exec 的是 `command_args`）
    pub command: String,
    /// 完整 argv（**必需**）：`argv[0]` 为主程序，其余为参数。pty 引擎按 argv
    /// 原样 exec，**不做 shell 包装**（无 bash -lic / PowerShell -Command / CMD /K、
    /// 无 WSL 路径转换）。业务会话的 argv 由插件 `launch.rs::build_argv` 算好经
    /// `create-with-spec` 的 `commandArgs` 传入；**宿主旧 shell 包装路径已退役**
    /// （2026-09-23 PTY 解耦票），缺省即报错、不再有静默回退分支。
    pub command_args: Vec<String>,
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
            command_args: vec!["bash".to_string(), "-lic".to_string(), "ls".to_string()],
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
        assert_eq!(back.command_args.len(), 3);
        assert_eq!(back.env_vars.get("FOO").map(String::as_str), Some("bar"));
    }

    /// `commandArgs` 必填（PTY 解耦票：宿主 shell 包装旧路径已退役）
    ///
    /// 缺该字段的 JSON 必须**反序列化即失败**，而不是落一个「argv 为空」的配置
    /// 让运行期再炸——旧路径的存在性由类型本身否证。
    #[test]
    fn session_launch_config_requires_command_args() {
        let json = r#"{"name":"x","environment":{"type":"Linux"},"working_dir":"/tmp","command":"ls"}"#;
        let err = serde_json::from_str::<SessionLaunchConfig>(json).unwrap_err();
        assert!(
            err.to_string().contains("command_args"),
            "缺 commandArgs 必须显性失败: {err}"
        );
    }

    /// 缺省字段反序列化：env_vars / cols / rows 有 serde default 兜底
    /// （`command_args` 除外——它必填，缺省即失败，见上一用例）
    #[test]
    fn session_launch_config_missing_fields_default() {
        let json =
            r#"{"name":"x","environment":{"type":"Linux"},"working_dir":"/tmp","command":"ls","command_args":["ls"]}"#;
        let cfg: SessionLaunchConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.env_vars.is_empty());
        assert!(cfg.cols > 0 && cfg.rows > 0);
        assert_eq!(cfg.command_args, vec!["ls".to_string()]);
    }
}
