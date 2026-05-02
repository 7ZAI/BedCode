//! WSL2 Support
//!
//! 提供 WSL2 环境下的命令执行和路径转换功能

use crate::Result;
use std::process::Command;

/// WSL 发行版信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WslDistro {
    pub name: String,
    pub is_default: bool,
    pub state: String,
    pub version: u8,
}

/// 列出已安装的 WSL 发行版
pub fn list_distributions() -> Result<Vec<WslDistro>> {
    let output = Command::new("wsl.exe")
        .args(["--list", "--verbose"])
        .output()?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    // 解析输出
    // 格式: "  NAME            STATE           VERSION"
    //       "* Ubuntu         Running         2"
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut distros = Vec::new();

    for line in stdout.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let is_default = line.starts_with('*');
        let line = line.trim_start_matches('*').trim();

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let name = parts[0].to_string();
            let state = parts[1].to_string();
            let version: u8 = parts[2].parse().unwrap_or(2);

            distros.push(WslDistro {
                name,
                is_default,
                state,
                version,
            });
        }
    }

    Ok(distros)
}

/// 在 WSL 中执行命令
pub fn execute_command(
    distro: &str,
    command: &str,
    working_dir: Option<&str>,
) -> Result<std::process::Output> {
    let mut args: Vec<String> = vec!["-d".to_string(), distro.to_string()];

    if let Some(dir) = working_dir {
        // 将 Windows 路径转换为 WSL 路径
        let wsl_path = windows_to_wsl_path(dir);
        args.push("--cd".to_string());
        args.push(wsl_path);
    }

    args.push("--".to_string());
    args.push("bash".to_string());
    args.push("-c".to_string());
    args.push(command.to_string());

    let output = Command::new("wsl.exe").args(&args).output()?;

    Ok(output)
}

/// 将 Windows 路径转换为 WSL 路径
///
/// C:\Users\test -> /mnt/c/Users/test
/// \\wsl$\Ubuntu\home -> /home
pub fn windows_to_wsl_path(path: &str) -> String {
    // 检查是否是 WSL 路径 (\\wsl$\...)
    if path.starts_with("\\\\wsl$") || path.starts_with("//wsl$") {
        let path = path.trim_start_matches('\\').trim_start_matches('/');
        let parts: Vec<&str> = path.splitn(3, '\\').collect();
        if parts.len() >= 3 {
            return format!("/{}", parts[2].replace('\\', "/"));
        }
        return path.replace('\\', "/");
    }

    // 检查是否是 Windows 驱动器路径 (C:\...)
    if path.len() >= 2 && path.chars().nth(1) == Some(':') {
        let drive = path.chars().next().unwrap().to_ascii_lowercase();
        let rest = &path[2..].replace('\\', "/");
        return format!("/mnt/{}{}", drive, rest);
    }

    // 已经是类 Unix 路径
    path.replace('\\', "/")
}

/// 将 WSL 路径转换为 Windows 路径
///
/// /mnt/c/Users/test -> C:\Users\test
/// /home -> \\wsl$\Ubuntu\home (需要发行版名称)
pub fn wsl_to_windows_path(path: &str, distro: Option<&str>) -> String {
    // 检查是否是 /mnt/... 路径
    if path.starts_with("/mnt/") && path.len() >= 6 {
        let drive = path.chars().nth(5).unwrap().to_ascii_uppercase();
        let rest = &path[6..].replace('/', "\\");
        return format!("{}:{}", drive, rest);
    }

    // 其他路径需要通过 WSL 发行版访问
    if let Some(d) = distro {
        return format!("\\\\wsl$\\{}{}", d, path.replace('/', "\\"));
    }

    path.replace('/', "\\")
}

/// 检查 WSL 是否可用
pub fn is_wsl_available() -> bool {
    Command::new("wsl.exe")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 获取默认 WSL 发行版
pub fn get_default_distro() -> Result<Option<String>> {
    let distros = list_distributions()?;
    Ok(distros.into_iter().find(|d| d.is_default).map(|d| d.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_to_wsl_path() {
        assert_eq!(
            windows_to_wsl_path("C:\\Users\\test"),
            "/mnt/c/Users/test"
        );
        assert_eq!(
            windows_to_wsl_path("D:\\Projects\\my-app"),
            "/mnt/d/Projects/my-app"
        );
        assert_eq!(
            windows_to_wsl_path("\\\\wsl$\\Ubuntu\\home\\user"),
            "/home/user"
        );
    }

    #[test]
    fn test_wsl_to_windows_path() {
        assert_eq!(
            wsl_to_windows_path("/mnt/c/Users/test", None),
            "C:\\Users\\test"
        );
        assert_eq!(
            wsl_to_windows_path("/home/user", Some("Ubuntu")),
            "\\\\wsl$\\Ubuntu\\home\\user"
        );
    }
}
