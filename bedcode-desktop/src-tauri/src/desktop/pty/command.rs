//! Command Builder
//!
//! 构建不同执行环境的命令

use portable_pty::CommandBuilder;

use crate::desktop::enums::{ExecutionEnvironment, SessionLaunchConfig};
use crate::desktop::pty::wsl::windows_to_wsl_path;

/// 构建命令（Windows/WSL）
pub fn build_command(config: &SessionLaunchConfig) -> crate::Result<CommandBuilder> {
    let mut cmd = match &config.environment {
        ExecutionEnvironment::Windows { shell } => {
            match shell {
                crate::desktop::enums::WindowsShell::PowerShell => {
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
                crate::desktop::enums::WindowsShell::Cmd => {
                    // 构建完整的 CMD 命令
                    let full_command = format!(
                        "@chcp 65001 > nul && cd /d \"{}\" && echo Working directory: %cd% && {}",
                        config.working_dir,
                        config.command
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
            let wsl_command = format!(
                "cd '{}' && pwd && {}",
                wsl_path,
                config.command
            );
            cmd.arg(wsl_command);
            cmd
        }
    };

    // 设置进程工作目录（作为备选，确保进程启动位置正确）
    if matches!(config.environment, ExecutionEnvironment::Windows { .. }) {
        cmd.cwd(&config.working_dir);
    }

    // 设置环境变量
    for (key, value) in &config.env_vars {
        cmd.env(key, value);
    }

    Ok(cmd)
}