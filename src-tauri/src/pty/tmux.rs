//! Tmux Integration
//!
//! 提供 Tmux 会话管理功能

use crate::Result;
use std::process::Command;

/// Tmux 会话信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TmuxSession {
    pub name: String,
    pub windows: usize,
    pub is_attached: bool,
    pub created: Option<String>,
}

/// 检查 Tmux 是否可用
pub fn is_tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 获取 Tmux 版本
pub fn get_tmux_version() -> Result<String> {
    let output = Command::new("tmux").arg("-V").output()?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout);
        Ok(version.trim().to_string())
    } else {
        Err(crate::AppError::NotFound("tmux not found".into()))
    }
}

/// 列出所有 Tmux 会话
pub fn list_sessions() -> Result<Vec<TmuxSession>> {
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}:#{session_windows}:#{session_attached}:#{session_created}"])
        .output()?;

    if !output.status.success() {
        // 没有会话时返回空列表
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut sessions = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 3 {
            let name = parts[0].to_string();
            let windows: usize = parts[1].parse().unwrap_or(1);
            let is_attached = parts[2] == "1";
            let created = parts.get(3).map(|s| s.to_string());

            sessions.push(TmuxSession {
                name,
                windows,
                is_attached,
                created,
            });
        }
    }

    Ok(sessions)
}

/// 检查会话是否存在
pub fn session_exists(name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 创建新的 Tmux 会话
pub fn create_session(name: &str, command: Option<&str>) -> Result<()> {
    let mut args = vec!["new-session", "-d", "-s", name];

    if let Some(cmd) = command {
        args.push(cmd);
    }

    let status = Command::new("tmux").args(&args).status()?;

    if !status.success() {
        return Err(crate::AppError::Session(format!(
            "Failed to create tmux session: {}",
            name
        )));
    }

    tracing::info!("Created tmux session: {}", name);
    Ok(())
}

/// 创建带工作目录的 Tmux 会话
pub fn create_session_in_dir(name: &str, working_dir: &str, command: Option<&str>) -> Result<()> {
    let mut args = vec!["new-session", "-d", "-s", name, "-c", working_dir];

    if let Some(cmd) = command {
        args.push(cmd);
    }

    let status = Command::new("tmux").args(&args).status()?;

    if !status.success() {
        return Err(crate::AppError::Session(format!(
            "Failed to create tmux session: {}",
            name
        )));
    }

    tracing::info!("Created tmux session '{}' in '{}'", name, working_dir);
    Ok(())
}

/// 杀死 Tmux 会话
pub fn kill_session(name: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["kill-session", "-t", name])
        .status()?;

    if !status.success() {
        return Err(crate::AppError::Session(format!(
            "Failed to kill tmux session: {}",
            name
        )));
    }

    tracing::info!("Killed tmux session: {}", name);
    Ok(())
}

/// 发送命令到 Tmux 会话
pub fn send_keys(session: &str, keys: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["send-keys", "-t", session, keys, "Enter"])
        .status()?;

    if !status.success() {
        return Err(crate::AppError::Session(format!(
            "Failed to send keys to tmux session: {}",
            session
        )));
    }

    Ok(())
}

/// 发送特殊键到 Tmux 会话
pub fn send_special_key(session: &str, key: &str) -> Result<()> {
    let tmux_key = match key.to_lowercase().as_str() {
        "enter" => "Enter",
        "escape" | "esc" => "Escape",
        "tab" => "Tab",
        "backspace" => "BSpace",
        "up" | "arrow_up" => "Up",
        "down" | "arrow_down" => "Down",
        "left" | "arrow_left" => "Left",
        "right" | "arrow_right" => "Right",
        "ctrl_c" | "ctrlc" => "C-c",
        "ctrl_d" | "ctrld" => "C-d",
        "ctrl_z" | "ctrlz" => "C-z",
        _ => {
            return Err(crate::AppError::InvalidInput(format!(
                "Unknown special key: {}",
                key
            )))
        }
    };

    let status = Command::new("tmux")
        .args(["send-keys", "-t", session, tmux_key])
        .status()?;

    if !status.success() {
        return Err(crate::AppError::Session(format!(
            "Failed to send special key to tmux session: {}",
            session
        )));
    }

    Ok(())
}

/// 获取 Tmux 会话的输出（最近 N 行）
pub fn capture_pane(session: &str, lines: Option<usize>) -> Result<String> {
    let mut args = vec!["capture-pane".to_string(), "-t".to_string(), session.to_string(), "-p".to_string()];

    if let Some(n) = lines {
        let start_arg = format!("-{}", n);
        args.push("-S".to_string());
        args.push(start_arg);
    }

    let output = Command::new("tmux").args(&args).output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(crate::AppError::Session(format!(
            "Failed to capture pane: {}",
            session
        )))
    }
}

/// 获取用于附加到 Tmux 会话的命令
pub fn get_attach_command(session: &str) -> String {
    format!("tmux attach -t {}", session)
}

/// 获取用于在 Tmux 会话中运行命令的完整命令
pub fn get_tmux_command(session: &str, working_dir: &str, command: &str) -> Vec<String> {
    if session_exists(session) {
        // 会话已存在，发送命令
        vec![
            "tmux".to_string(),
            "send-keys".to_string(),
            "-t".to_string(),
            session.to_string(),
            command.to_string(),
            "Enter".to_string(),
        ]
    } else {
        // 创建新会话
        vec![
            "tmux".to_string(),
            "new-session".to_string(),
            "-s".to_string(),
            session.to_string(),
            "-c".to_string(),
            working_dir.to_string(),
            command.to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tmux_available() {
        // 这个测试依赖于系统是否安装了 tmux
        let available = is_tmux_available();
        println!("Tmux available: {}", available);
    }
}
