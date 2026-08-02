//! WSL 文件系统桥接
//!
//! WSL2 发行版处于 Stopped 状态时，`\\wsl.localhost\<distro>\...` 与 `\\wsl$\<distro>\...`
//! UNC 路径在 Windows 侧不可访问（std::fs 返回 ERROR_FILE_NOT_FOUND）。
//!
//! 通过 `wsl.exe` 桥接读写可自动启动发行版（wsl.exe 会按需启动 Stopped 的发行版），
//! 保证任意状态下 WSL 文件可访问，同时支持 Windows 路径 ↔ WSL 路径的跨域复制
//! （hook 脚本安装场景：源为插件安装目录，目标为 WSL 项目 .claude/）。

use crate::process::create_command;
use std::io::Write;
use std::process::Stdio;

/// 判断路径是否为 WSL UNC 路径（兼容 / 与 \ 分隔符混合）
pub fn is_wsl_unc_path(path: &str) -> bool {
    let normalized = path.replace('/', "\\");
    normalized.starts_with("\\\\wsl.localhost\\") || normalized.starts_with("\\\\wsl$\\")
}

/// 从 WSL UNC 路径解析发行版名称与 WSL 内部路径
///
/// `\\wsl.localhost\Ubuntu\home\binblink\project\blink` → `("Ubuntu", "/home/binblink/project/blink")`
pub fn parse_wsl_unc_path(path: &str) -> Option<(String, String)> {
    let normalized = path.replace('/', "\\");
    let rest = if normalized.starts_with("\\\\wsl.localhost\\") {
        &normalized["\\\\wsl.localhost\\".len()..]
    } else if normalized.starts_with("\\\\wsl$\\") {
        &normalized["\\\\wsl$\\".len()..]
    } else {
        return None;
    };

    let (distro, inner) = rest.split_once('\\')?;
    if distro.is_empty() || inner.is_empty() {
        return None;
    }
    Some((distro.to_string(), format!("/{}", inner.replace('\\', "/"))))
}

/// 通过 wsl.exe 读取文件原始字节（等价于 std::fs::read）
pub fn read_bytes_via_wsl(distro: &str, wsl_path: &str) -> std::io::Result<Vec<u8>> {
    let output = create_command("wsl.exe")
        .args(["-d", distro, "--", "cat", wsl_path])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("wsl cat failed (distro={}): {}", distro, stderr.trim()),
        ));
    }

    Ok(output.stdout)
}

/// 通过 wsl.exe 读取文本文件（等价于 std::fs::read_to_string）
pub fn read_to_string_via_wsl(distro: &str, wsl_path: &str) -> std::io::Result<String> {
    let bytes = read_bytes_via_wsl(distro, wsl_path)?;
    String::from_utf8(bytes).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("wsl cat output is not UTF-8 (distro={}): {}", distro, e),
        )
    })
}

/// 通过 wsl.exe 写入文件原始字节，自动创建父目录（等价于 std::fs::write + create_dir_all）
///
/// 使用 `mkdir -p` + `tee` 组合而非 shell 重定向：wsl.exe 对 `--` 后的参数会
/// 重新拼接，含引号/$ 的 shell 脚本会丢失语义，普通参数则原样传递。
pub fn write_bytes_via_wsl(distro: &str, wsl_path: &str, content: &[u8]) -> std::io::Result<()> {
    // 1. 创建父目录（Rust 侧计算 dirname，避免 shell 引号问题）
    if let Some(parent) = wsl_path.rsplit_once('/').map(|(d, _)| d) {
        if !parent.is_empty() {
            let mkdir = create_command("wsl.exe")
                .args(["-d", distro, "--", "mkdir", "-p", parent])
                .output()?;
            if !mkdir.status.success() {
                let stderr = String::from_utf8_lossy(&mkdir.stderr);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("wsl mkdir failed (distro={}): {}", distro, stderr.trim()),
                ));
            }
        }
    }

    // 2. tee 从 stdin 写入目标文件（路径作为普通参数传递）
    let mut child = create_command("wsl.exe")
        .args(["-d", distro, "--", "tee", wsl_path])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        // 写入后显式 drop 关闭 stdin，让 tee 收到 EOF
        stdin.write_all(content)?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("wsl write failed (distro={}): {}", distro, stderr.trim()),
        ));
    }
    Ok(())
}

/// 通过 wsl.exe 删除文件（等价于 std::fs::remove_file，文件不存在视为成功）
pub fn delete_via_wsl(distro: &str, wsl_path: &str) -> std::io::Result<()> {
    // rm -f 对不存在的文件不报错，天然幂等
    let output = create_command("wsl.exe")
        .args(["-d", distro, "--", "rm", "-f", wsl_path])
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("wsl rm failed (distro={}): {}", distro, stderr.trim()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_wsl_unc_path() {
        assert!(is_wsl_unc_path("\\\\wsl.localhost\\Ubuntu\\home\\user"));
        assert!(is_wsl_unc_path("\\\\wsl$\\Ubuntu\\home\\user"));
        assert!(is_wsl_unc_path("\\\\wsl.localhost\\Ubuntu\\home\\user/.claude/settings.json"));
        assert!(!is_wsl_unc_path("C:\\Users\\test"));
        assert!(!is_wsl_unc_path("D:\\Projects\\my-app/.claude/settings.json"));
        assert!(!is_wsl_unc_path("/home/user"));
    }

    #[test]
    fn test_parse_wsl_unc_path() {
        // wsl.localhost 新格式
        let (distro, wsl_path) =
            parse_wsl_unc_path("\\\\wsl.localhost\\Ubuntu\\home\\binblink\\project\\blink")
                .expect("parse failed");
        assert_eq!(distro, "Ubuntu");
        assert_eq!(wsl_path, "/home/binblink/project/blink");

        // 混合分隔符（Windows 前缀 + 正斜杠后续）
        let (distro, wsl_path) = parse_wsl_unc_path(
            "\\\\wsl.localhost\\Ubuntu\\home\\binblink\\project\\blink/.claude/settings.json",
        )
        .expect("parse failed");
        assert_eq!(distro, "Ubuntu");
        assert_eq!(wsl_path, "/home/binblink/project/blink/.claude/settings.json");

        // wsl$ 旧格式
        let (distro, wsl_path) =
            parse_wsl_unc_path("\\\\wsl$\\Ubuntu\\home\\user").expect("parse failed");
        assert_eq!(distro, "Ubuntu");
        assert_eq!(wsl_path, "/home/user");

        // 非 WSL 路径返回 None
        assert!(parse_wsl_unc_path("C:\\Users\\test").is_none());
        assert!(parse_wsl_unc_path("/home/user").is_none());
    }
}
