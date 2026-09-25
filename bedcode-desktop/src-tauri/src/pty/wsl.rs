//! WSL2 Support
//!
//! **只提供平台事实：已安装发行版列举**（`host-platform.wsl-distros` 原语的实现）。
//!
//! 曾同时承载「在 WSL 中执行命令」与「Windows → WSL 路径转换」——两者都是
//! **业务会话语义**，已随 2026-09-23 PTY 解耦票退役：shell 包装与路径转换归
//! 消费侧（业务会话 = 插件 `launch.rs::build_argv`，插件私有 PTY = 插件自己）。

use crate::system::process::create_command;
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

#[cfg(test)]
mod tests {
    use super::*;

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

}
