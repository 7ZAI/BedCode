//! WSL2 Support
//!
//! 提供 WSL2 环境下的命令执行和路径转换功能

use crate::process::create_command;
use crate::Result;
use encoding_rs::UTF_16LE;

/// WSL 发行版信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WslDistro {
    pub name: String,
    pub is_default: bool,
    pub state: String,
    pub version: u8,
}

/// 解码 `wsl --list --verbose` 原始输出字节为可解析文本
///
/// 解码回退链：UTF-8 → UTF-16LE（Windows 原生编码，实际输出恒为 UTF-16LE）→ GBK。
/// UTF-16LE 解码结果会夹带 `\x00` 空字节，此处一并清除。
fn decode_wsl_output(stdout: &[u8]) -> String {
    // 尝试 UTF-8 解码，失败则尝试 UTF-16LE，最后回退到 GBK
    match String::from_utf8(stdout.to_vec()) {
        Ok(s) => {
            // 检查是否包含空字节（UTF-16 特征）
            if s.contains('\x00') {
                s.replace('\x00', "")
            } else {
                s
            }
        }
        Err(_) => {
            // 先尝试 UTF-16LE（Windows 原生编码）
            let (decoded, _, had_errors) = UTF_16LE.decode(stdout);
            if !had_errors {
                // 移除 UTF-16LE 解码后的空字节
                decoded.to_string().replace('\x00', "")
            } else {
                // 回退到 GBK
                let gbk = encoding_rs::GBK;
                let (gbk_decoded, _, _) = gbk.decode(stdout);
                gbk_decoded.to_string()
            }
        }
    }
}

/// 解析 `wsl --list --verbose` 输出文本为发行版列表（纯函数，可单测）
///
/// 首行为表头，跳过；每行结构：`[*] 名字 状态 版本`。
/// 版本列解析失败时回退 2（默认 WSL2）。
pub fn parse_wsl_list_output(stdout: &str) -> Vec<WslDistro> {
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

    distros
}

/// 列出已安装的 WSL 发行版
pub fn list_distributions() -> Result<Vec<WslDistro>> {
    let output = create_command("cmd.exe")
        .args(["/c", "chcp 65001 >nul 2>&1 && wsl --list --verbose"])
        .output()?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    Ok(parse_wsl_list_output(&decode_wsl_output(&output.stdout)))
}

/// 在 WSL 中执行命令
pub fn execute_command(distro: &str, command: &str, working_dir: Option<&str>) -> Result<std::process::Output> {
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
    // 先把正斜杠统一为反斜杠，使 / 与 \ 两种写法走同一套解析（末尾分支会转回）
    let path = path.replace('/', "\\");

    // 检查是否是 WSL 路径 (\\wsl$\... 或 \\wsl.localhost\...)
    if path.starts_with("\\\\wsl.localhost\\") {
        // 新格式: \\wsl.localhost\Ubuntu\home\user -> /home/user（WSL2 1903+）
        let rest = path.trim_start_matches('\\').trim_start_matches("wsl.localhost\\");
        let parts: Vec<&str> = rest.splitn(2, '\\').collect();
        if parts.len() >= 2 {
            return format!("/{}", parts[1].replace('\\', "/"));
        }
        return rest.replace('\\', "/");
    }

    if path.starts_with("\\\\wsl$") {
        // 旧格式: \\wsl$\Ubuntu\home\user -> /home/user
        let rest = path.trim_start_matches('\\');
        let parts: Vec<&str> = rest.splitn(3, '\\').collect();
        if parts.len() >= 3 {
            return format!("/{}", parts[2].replace('\\', "/"));
        }
        return rest.replace('\\', "/");
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
        assert_eq!(windows_to_wsl_path("C:\\Users\\test"), "/mnt/c/Users/test");
        assert_eq!(windows_to_wsl_path("D:\\Projects\\my-app"), "/mnt/d/Projects/my-app");
        assert_eq!(windows_to_wsl_path("\\\\wsl$\\Ubuntu\\home\\user"), "/home/user");
        // WSL2 新格式: \\wsl.localhost\Ubuntu\home\user
        assert_eq!(
            windows_to_wsl_path("\\\\wsl.localhost\\Ubuntu\\home\\binblink\\project\\blink"),
            "/home/binblink/project/blink"
        );
    }

    #[test]
    fn test_windows_to_wsl_path_forward_slash_forms() {
        // 正斜杠形式必须与反斜杠形式等价（票据 01：曾解析出发行版名残留）
        assert_eq!(windows_to_wsl_path("//wsl.localhost/Ubuntu/home/user"), "/home/user");
        assert_eq!(windows_to_wsl_path("//wsl$/Ubuntu/home/user"), "/home/user");
        // 正斜杠磁盘路径同样归一化
        assert_eq!(windows_to_wsl_path("C:/Users/test"), "/mnt/c/Users/test");
        // 类 Unix 路径透传不受归一化影响
        assert_eq!(windows_to_wsl_path("/home/user"), "/home/user");
    }

    #[test]
    fn test_parse_wsl_list_output_default_marker_and_state() {
        // `*` 前缀 → is_default=true，名字干净无星号；状态列、版本列解析
        let out = "  NAME      STATE           VERSION\n* Ubuntu    Running         2\n Debian     Stopped         1\n";
        let distros = parse_wsl_list_output(out);
        assert_eq!(distros.len(), 2);
        assert!(distros[0].is_default);
        assert_eq!(distros[0].name, "Ubuntu");
        assert_eq!(distros[0].state, "Running");
        assert_eq!(distros[0].version, 2);
        assert!(!distros[1].is_default);
        assert_eq!(distros[1].name, "Debian");
        assert_eq!(distros[1].version, 1);
    }

    #[test]
    fn test_parse_wsl_list_output_version_fallback_and_line_skips() {
        // 版本列非法 → 回退 2；首行表头 / 空行 / 少于 3 列的行跳过
        let out =
            "  NAME      STATE           VERSION\n\n* Unknown   Running         ???\n    OnlyName\n  Two      Cols\n";
        let distros = parse_wsl_list_output(out);
        assert_eq!(distros.len(), 1);
        assert_eq!(distros[0].name, "Unknown");
        assert!(distros[0].is_default);
        assert_eq!(distros[0].version, 2);
    }

    #[test]
    fn test_parse_wsl_list_output_empty() {
        assert!(parse_wsl_list_output("").is_empty());
        assert!(parse_wsl_list_output("  NAME      STATE           VERSION\n").is_empty());
    }

    #[test]
    fn test_decode_wsl_output_utf16le_strips_nul_bytes() {
        // UTF-16LE 编码样本（含 \x00），解码后空字节被清除
        let utf16: Vec<u8> = b"* Ubuntu Running 2\n".iter().flat_map(|&b| [b, 0]).collect();
        let decoded = decode_wsl_output(&utf16);
        assert_eq!(decoded, "* Ubuntu Running 2\n");
    }

    #[test]
    fn test_decode_wsl_output_utf8_passthrough() {
        let decoded = decode_wsl_output(b"* Ubuntu Running 2\n");
        assert_eq!(decoded, "* Ubuntu Running 2\n");
    }

    #[test]
    fn test_wsl_to_windows_path() {
        assert_eq!(wsl_to_windows_path("/mnt/c/Users/test", None), "C:\\Users\\test");
        assert_eq!(
            wsl_to_windows_path("/home/user", Some("Ubuntu")),
            "\\\\wsl$\\Ubuntu\\home\\user"
        );
    }
}
