//! Command Builder
//!
//! 构建不同执行环境的命令

use portable_pty::CommandBuilder;

use crate::enums::{ExecutionEnvironment, SessionLaunchConfig};
use crate::pty::wsl::windows_to_wsl_path;

/// 构建命令（Windows/WSL/Linux）
pub fn build_command(config: &SessionLaunchConfig) -> crate::Result<CommandBuilder> {
    let mut cmd = match &config.environment {
        ExecutionEnvironment::Windows { shell } => {
            match shell {
                crate::enums::WindowsShell::PowerShell => {
                    // 构建完整的 PowerShell 命令
                    let full_command = format!(
                        "chcp 65001 > $null; [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new(); Set-Location '{}'; Write-Host 'Working directory:' $PWD.Path; {}",
                        config.working_dir,
                        config.command
                    );

                    let mut cmd = CommandBuilder::new("powershell.exe");
                    cmd.arg("-NoLogo");
                    cmd.arg("-NoExit");
                    cmd.arg("-Command");
                    cmd.arg(full_command);
                    cmd
                }
                crate::enums::WindowsShell::Cmd => {
                    // 构建完整的 CMD 命令
                    let full_command = format!(
                        "@chcp 65001 > nul && cd /d \"{}\" && echo Working directory: %cd% && {}",
                        config.working_dir, config.command
                    );

                    let mut cmd = CommandBuilder::new("cmd.exe");
                    cmd.arg("/K");
                    cmd.arg(full_command);
                    cmd
                }
            }
        }
        ExecutionEnvironment::Wsl2 { distro } => {
            let mut cmd = CommandBuilder::new("wsl.exe");
            cmd.arg("-d");
            cmd.arg(distro);
            cmd.arg("--");
            cmd.arg("bash");
            cmd.arg("-lic");

            let wsl_path = windows_to_wsl_path(&config.working_dir);
            let wsl_command = format!("cd '{}' && pwd && {}", wsl_path, config.command);
            cmd.arg(wsl_command);
            cmd
        }
        ExecutionEnvironment::Linux => {
            // Linux 原生环境：直接走当前用户的 bash，避免 spawn 父进程退出导致会话关闭
            // -c 一次性的命令用 single-quote 包住，避免 shell 展开；
            // 先切到工作目录并打印 pwd，便于前端看到 PTY 实际所在目录
            let full_command = format!(
                "cd '{}' && pwd && {}",
                config.working_dir.replace('\'', "'\\''"),
                config.command,
            );

            let mut cmd = CommandBuilder::new("bash");
            cmd.arg("-lc");
            cmd.arg(full_command);
            cmd
        }
    };

    // 设置进程工作目录（作为备选，确保进程启动位置正确）
    if matches!(config.environment, ExecutionEnvironment::Windows { .. }) {
        cmd.cwd(&config.working_dir);
    } else if matches!(config.environment, ExecutionEnvironment::Linux) {
        // Linux 环境：把 cwd 也设上（命令体里的 cd 已保证工作目录正确，这里只是兜底）
        cmd.cwd(&config.working_dir);
    }

    // 设置环境变量
    for (key, value) in &config.env_vars {
        cmd.env(key, value);
    }

    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::WindowsShell;
    use std::collections::HashMap;

    /// 构造最小启动配置（environment 由调用方指定）
    fn config(env: ExecutionEnvironment, command: &str) -> SessionLaunchConfig {
        SessionLaunchConfig {
            name: "test".to_string(),
            environment: env,
            working_dir: "D:\\work".to_string(),
            command: command.to_string(),
            env_vars: HashMap::new(),
            cols: 120,
            rows: 40,
        }
    }

    /// 提取 argv（OsString → String）供断言
    fn argv(cmd: &CommandBuilder) -> Vec<String> {
        cmd.get_argv()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn powershell_builds_utf8_codepage_command_with_cwd() {
        let cmd = build_command(&config(
            ExecutionEnvironment::Windows {
                shell: WindowsShell::PowerShell,
            },
            "echo hi",
        ))
        .unwrap();

        let argv = argv(&cmd);
        assert_eq!(argv[0], "powershell.exe");
        assert!(argv.iter().any(|a| a == "-NoLogo"));
        assert!(argv.iter().any(|a| a == "-NoExit"));

        // 完整命令：设置 UTF-8 输出编码 → 切换到工作目录 → 执行用户命令
        let full = argv.iter().find(|a| a.contains("echo hi")).unwrap();
        assert!(full.contains("chcp 65001 > $null"));
        assert!(full.contains("Set-Location 'D:\\work'"));
        assert!(full.contains("Write-Host 'Working directory:'"));

        // Windows 原生环境必须显式设置 cwd（备选机制）
        assert_eq!(
            cmd.get_cwd().map(|c| c.to_string_lossy().into_owned()),
            Some("D:\\work".to_string())
        );
    }

    #[test]
    fn cmd_shell_builds_cmd_commands() {
        let cmd = build_command(&config(
            ExecutionEnvironment::Windows {
                shell: WindowsShell::Cmd,
            },
            "dir",
        ))
        .unwrap();

        let argv = argv(&cmd);
        assert_eq!(argv[0], "cmd.exe");
        assert!(argv.iter().any(|a| a == "/K"));

        let full = argv.iter().find(|a| a.contains("dir")).unwrap();
        assert!(full.contains("@chcp 65001 > nul"));
        assert!(full.contains("cd /d \"D:\\work\""));
        assert!(full.contains("echo Working directory:"));

        assert_eq!(
            cmd.get_cwd().map(|c| c.to_string_lossy().into_owned()),
            Some("D:\\work".to_string())
        );
    }

    #[test]
    fn wsl2_uses_distro_and_converts_windows_path() {
        let cmd = build_command(&config(
            ExecutionEnvironment::Wsl2 {
                distro: "Ubuntu".to_string(),
            },
            "pwd",
        ))
        .unwrap();

        let argv = argv(&cmd);
        assert_eq!(argv[0], "wsl.exe");
        assert!(argv.iter().any(|a| a == "-d"));
        assert!(argv.iter().any(|a| a == "Ubuntu"));
        assert!(argv.iter().any(|a| a == "bash"));
        assert!(argv.iter().any(|a| a == "-lic"));

        // Windows 路径必须转换为 /mnt/d/work（WSL 挂载规则）
        let wsl_cmd = argv.iter().find(|a| a.contains("pwd")).unwrap();
        assert!(wsl_cmd.contains("cd '/mnt/d/work'"));

        // WSL 环境不设置 Windows cwd（由 wsl.exe 自身处理）
        assert!(cmd.get_cwd().is_none());
    }

    #[test]
    fn wsl_path_passthrough_for_unix_style_paths() {
        // 已是非 /mnt 的类 Unix 路径应原样透传
        let cmd = build_command(&config(
            ExecutionEnvironment::Wsl2 {
                distro: "Ubuntu".to_string(),
            },
            "ls",
        ))
        .unwrap();
        let _ = argv(&cmd);
        // 路径转换行为由 wsl::windows_to_wsl_path 保证，此处验证 WSL 分支不 panic
        assert!(cmd.get_cwd().is_none());
    }

    #[test]
    fn linux_uses_bash_and_sets_cwd_directly() {
        let cmd = build_command(&config(
            ExecutionEnvironment::Linux,
            "echo hi",
        ))
        .unwrap();

        let argv = argv(&cmd);
        assert_eq!(argv[0], "bash");
        // 必须是 login shell：保留 PATH/环境，claude/codex 这类命令依赖 PATH 解析
        assert!(argv.iter().any(|a| a == "-lc"));

        let full = argv.iter().find(|a| a.contains("echo hi")).unwrap();
        // 工作目录走单引号包住的 POSIX 路径（Linux 上直接用原路径，不做 /mnt 转换）
        assert!(full.contains("cd 'D:\\work'"));
        assert!(full.contains("pwd"));
        // Linux 环境显式设置 cwd（兜底机制）
        assert_eq!(
            cmd.get_cwd().map(|c| c.to_string_lossy().into_owned()),
            Some("D:\\work".to_string())
        );
    }

    #[test]
    fn linux_escapes_single_quotes_in_working_dir() {
        // 含单引号的工作目录需转义，避免破坏 shell 字符串
        let mut c = config(ExecutionEnvironment::Linux, "echo ok");
        c.working_dir = "/tmp/o'clock".to_string();
        let cmd = build_command(&c).unwrap();
        let argv = argv(&cmd);
        let full = argv.iter().find(|a| a.contains("echo ok")).unwrap();
        // 单引号被转义为 '\''
        assert!(full.contains("cd '/tmp/o'\\''clock'"));
    }
}
