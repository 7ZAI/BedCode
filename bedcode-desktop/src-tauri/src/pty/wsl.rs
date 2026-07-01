//! WSL2 Support
//!
//! 提供 WSL2 环境下的命令执行和路径转换功能

use crate::Result;
use crate::process::create_command;
use encoding_rs::UTF_16LE;

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
    // 尝试使用 UTF-8 编码，如果失败则使用系统默认编码
    let output = create_command("cmd.exe")
        .args(["/c", "chcp 65001 >nul 2>&1 && wsl --list --verbose"])
        .output()?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    // 尝试 UTF-8 解码，失败则尝试 UTF-16LE，最后回退到 GBK
    // 注意：wsl --list --verbose 输出的是 UTF-16LE 编码
    let stdout = match String::from_utf8(output.stdout.clone()) {
        Ok(s) => {
            // 检查是否包含空字节（UTF-16 特征）
            if s.contains('\x00') {
                s.replace('\x00', "")
            } else {
                s
            }
        },
        Err(_) => {
            // 先尝试 UTF-16LE（Windows 原生编码）
            let (decoded, _, had_errors) = UTF_16LE.decode(&output.stdout);
            if !had_errors {
                // 移除 UTF-16LE 解码后的空字节
                decoded.to_string().replace('\x00', "")
            } else {
                // 回退到 GBK
                let mutgbk = encoding_rs::GBK;
                let (gbk_decoded, _, _) = mutgbk.decode(&output.stdout);
                gbk_decoded.to_string()
            }
        }
    };
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

    let output = create_command("wsl.exe").args(&args).output()?;

    Ok(output)
}

/// 将 Windows 路径转换为 WSL 路径
///
/// C:\Users\test -> /mnt/c/Users/test
/// \\wsl$\Ubuntu\home -> /home
/// \\wsl.localhost\Ubuntu\home -> /home (WSL2 新格式)
pub fn windows_to_wsl_path(path: &str) -> String {
    // 检查是否是 WSL 路径 (\\wsl$\... 或 \\wsl.localhost\...)
    // 支持两种格式：
    // - \\wsl$\Ubuntu\home\user (旧格式)
    // - \\wsl.localhost\Ubuntu\home\user (新格式，WSL2 1903+)
    if path.starts_with("\\\\wsl.localhost\\") || path.starts_with("//wsl.localhost/") {
        // 新格式: \\wsl.localhost\Ubuntu\home\user -> /home/user
        let path = path.trim_start_matches('\\').trim_start_matches('/');
        let path = path.trim_start_matches("wsl.localhost").trim_start_matches('\\').trim_start_matches('/');
        let parts: Vec<&str> = path.splitn(2, '\\').collect();
        if parts.len() >= 2 {
            return format!("/{}", parts[1].replace('\\', "/"));
        }
        return path.replace('\\', "/");
    }

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
    create_command("cmd.exe")
        .args(["/c", "chcp 65001 >nul && wsl --version"])
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
        // WSL2 新格式: \\wsl.localhost\Ubuntu\home\user
        assert_eq!(
            windows_to_wsl_path("\\\\wsl.localhost\\Ubuntu\\home\\binblink\\project\\blink"),
            "/home/binblink/project/blink"
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

    /// 测试获取 WSL2 发行版列表
    /// 运行方式: cargo test --package bedcode_lib --lib pty::wsl::tests::test_list_wsl_distributions -- --nocapture
    #[test]
    fn test_list_wsl_distributions() {
        println!("\n========== Testing WSL Distribution List ==========");

        // 检查 WSL 是否可用
        let available = is_wsl_available();
        println!("WSL Available: {}", available);

        if !available {
            println!("WSL is not installed or not enabled. Skipping test.");
            return;
        }

        // 获取发行版列表
        let result = list_distributions();
        match result {
            Ok(distros) => {
                println!("Found {} distribution(s):", distros.len());
                for distro in &distros {
                    println!(
                        "  - {} (default: {}, state: {}, version: {})",
                        distro.name, distro.is_default, distro.state, distro.version
                    );
                }

                // 验证至少有一个发行版
                assert!(!distros.is_empty(), "Expected at least one WSL distribution");

                // 验证默认发行版
                let has_default = distros.iter().any(|d| d.is_default);
                assert!(has_default, "Expected a default WSL distribution");
            }
            Err(e) => {
                panic!("Failed to list distributions: {}", e);
            }
        }

        // 测试获取默认发行版
        let default_result = get_default_distro();
        match default_result {
            Ok(Some(default)) => {
                println!("Default distribution: {}", default);
            }
            Ok(None) => {
                println!("No default distribution found");
            }
            Err(e) => {
                println!("Failed to get default distribution: {}", e);
            }
        }

        println!("========== Test Completed ==========\n");
    }
}
