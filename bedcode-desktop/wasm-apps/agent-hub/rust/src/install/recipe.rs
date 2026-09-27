//! 安装/卸载命令 recipe 白名单（纯函数，双平台可测）
//!
//! 仅接受白名单 cli 名 + 固定模板，method/installed 来自探测状态而非前端
//! 传参：claude native → `claude update`（镜像无关）；claude npm-global →
//! npm 包安装；opencode standalone 拒绝自动安装（官方脚本手动，v1 提示）。
//! 卸载同样只走白名单：npm-global → `npm uninstall -g <pkg>`；claude native
//! → 官方文档卸载命令（rm launcher + 版本目录）；opencode standalone →
//! `opencode uninstall --force`（官方命令，非交互跳过确认）。

use super::registry::NPMMIRROR;

// ==================== recipe 白名单 ====================

/// npm 包名白名单（spec §4.2，包名已实机核实）；cli 名本身也经本表校验
pub(super) fn npm_package(cli: &str) -> Option<&'static str> {
    match cli {
        "pi" => Some("@earendil-works/pi-coding-agent"),
        "codex" => Some("@openai/codex"),
        "opencode" => Some("opencode-ai"),
        "claude" => Some("@anthropic-ai/claude-code"),
        _ => None,
    }
}

fn npm_install_cmd(pkg: &str, use_mirror: bool) -> String {
    if use_mirror {
        format!("npm install -g {pkg} --registry={NPMMIRROR}")
    } else {
        format!("npm install -g {pkg}")
    }
}

/// claude native 卸载命令（官方文档，双平台形态）：
/// unix `rm -f ~/.local/bin/claude` + `rm -rf ~/.local/share/claude`；
/// Windows cmd `del`/`rd` 等价（`%USERPROFILE%` 由 cmd 展开）
fn claude_native_uninstall(windows: bool) -> String {
    if windows {
        "del /f \"%USERPROFILE%\\.local\\bin\\claude.exe\" & rd /s /q \"%USERPROFILE%\\.local\\share\\claude\"".to_string()
    } else {
        "rm -f ~/.local/bin/claude\nrm -rf ~/.local/share/claude".to_string()
    }
}

/// 卸载命令构造（纯函数，双平台可测）
///
/// 仅接受白名单 cli 名 + 固定模板；method 来自探测状态而非前端传参。
/// - npm-global（全部 CLI）→ `npm uninstall -g <pkg>`（卸载本地完成，镜像无关）
/// - claude native → 官方文档卸载命令（unix / Windows 分派）
/// - opencode standalone → `opencode uninstall --force`（官方命令，非交互跳过确认）
///
/// 其余 method（unknown 等）→ 拒绝（v1 提示手动卸载）
pub(crate) fn build_uninstall_script(
    cli: &str,
    method: &str,
    windows: bool,
) -> Result<String, String> {
    if cli == "claude" && method == "native" {
        return Ok(claude_native_uninstall(windows));
    }
    if cli == "opencode" && method == "standalone" {
        return Ok("opencode uninstall --force".to_string());
    }
    if method != "npm-global" {
        return Err(format!("unsupported uninstall method: {cli} / {method}"));
    }
    let pkg = npm_package(cli).ok_or_else(|| format!("no uninstall recipe for cli: {cli}"))?;
    Ok(format!("npm uninstall -g {pkg}"))
}

/// 安装/更新命令构造（纯函数，双平台可测）
///
/// 仅接受白名单 cli 名 + 固定模板；method/installed 来自探测状态而非前端传参。
/// 返回 `(脚本, action)`；action 供 UI 展示（install/update）。
/// - claude native → `claude update`（双平台同命令，镜像无关）
/// - claude npm-global → npm 包安装
/// - opencode standalone 生效 → 拒绝（更新走官方脚本，v1 提示手动）
pub(crate) fn build_install_script(
    cli: &str,
    method: &str,
    installed: bool,
    use_mirror: bool,
) -> Result<(String, &'static str), String> {
    if cli == "claude" {
        return match method {
            "native" => Ok(("claude update".to_string(), "update")),
            "npm-global" => Ok((
                npm_install_cmd(
                    npm_package(cli).ok_or_else(|| "no package for claude".to_string())?,
                    use_mirror,
                ),
                if installed { "update" } else { "install" },
            )),
            other => Err(format!("unsupported claude install method: {other}")),
        };
    }
    if cli == "opencode" && method == "standalone" {
        return Err("opencode standalone: update via official script (manual)".to_string());
    }
    let pkg = npm_package(cli).ok_or_else(|| format!("no install recipe for cli: {cli}"))?;
    Ok((
        npm_install_cmd(pkg, use_mirror),
        if installed { "update" } else { "install" },
    ))
}

// ==================== Tests（纯函数单测，双平台形态覆盖） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// recipe 白名单：npm 类 CLI 双平台同命令；镜像追加 --registry
    #[test]
    fn build_script_npm_clis() {
        let (script, action) =
            build_install_script("pi", "npm-global", false, false).expect("pi recipe");
        assert_eq!(script, "npm install -g @earendil-works/pi-coding-agent");
        assert_eq!(action, "install");

        let (script, action) =
            build_install_script("codex", "npm-global", true, false).expect("codex recipe");
        assert_eq!(script, "npm install -g @openai/codex");
        assert_eq!(action, "update");

        let (script, _) =
            build_install_script("pi", "npm-global", false, true).expect("pi mirror recipe");
        assert_eq!(
            script,
            "npm install -g @earendil-works/pi-coding-agent --registry=https://registry.npmmirror.com"
        );

        let (script, _) =
            build_install_script("opencode", "npm-global", true, false).expect("opencode recipe");
        assert_eq!(script, "npm install -g opencode-ai");
    }

    /// claude 分派：native → `claude update`（双平台同命令）；npm-global → npm 包；
    /// unknown method 拒绝
    #[test]
    fn build_script_claude() {
        let (script, action) =
            build_install_script("claude", "native", true, true).expect("native recipe");
        assert_eq!(script, "claude update");
        assert_eq!(action, "update");
        // 镜像对 claude update 无意义：命令不受 use_mirror 影响
        let (script, _) = build_install_script("claude", "native", true, false).expect("native");
        assert_eq!(script, "claude update");

        let (script, _) =
            build_install_script("claude", "npm-global", true, false).expect("npm recipe");
        assert_eq!(script, "npm install -g @anthropic-ai/claude-code");

        assert!(build_install_script("claude", "unknown", true, false).is_err());
    }

    /// opencode standalone 生效：拒绝自动安装/更新（官方脚本手动，v1 提示）
    #[test]
    fn build_script_opencode_standalone_rejected() {
        let err = build_install_script("opencode", "standalone", true, false).unwrap_err();
        assert!(err.contains("standalone"), "got: {err}");
    }

    /// 白名单外 cli 名拒绝（无用户自由输入拼接面）
    #[test]
    fn build_script_unknown_cli_rejected() {
        assert!(build_install_script("rm -rf /", "npm-global", false, false).is_err());
        assert!(build_install_script("", "npm-global", false, false).is_err());
    }

    // ==================== 卸载 recipe（票据：概览卸载） ====================

    /// npm-global 卸载：四家 CLI 均 `npm uninstall -g <pkg>`（包名同安装白名单），
    /// 镜像无关（本地完成）
    #[test]
    fn build_uninstall_npm_global() {
        for (cli, pkg) in [
            ("pi", "@earendil-works/pi-coding-agent"),
            ("codex", "@openai/codex"),
            ("opencode", "opencode-ai"),
            ("claude", "@anthropic-ai/claude-code"),
        ] {
            let script =
                build_uninstall_script(cli, "npm-global", false).expect("npm uninstall recipe");
            assert_eq!(script, format!("npm uninstall -g {pkg}"), "cli: {cli}");
        }
    }

    /// claude native 卸载：unix 移除 launcher + 版本目录；Windows cmd 等价形态
    #[test]
    fn build_uninstall_claude_native() {
        let unix = build_uninstall_script("claude", "native", false).expect("unix recipe");
        assert!(unix.contains("rm -f ~/.local/bin/claude"));
        assert!(unix.contains("rm -rf ~/.local/share/claude"));

        let win = build_uninstall_script("claude", "native", true).expect("win recipe");
        assert!(win.contains("claude.exe"));
        assert!(win.contains("rd /s /q"));
    }

    /// opencode standalone 卸载：官方 `opencode uninstall --force`（非交互跳过确认）
    #[test]
    fn build_uninstall_opencode_standalone() {
        let script =
            build_uninstall_script("opencode", "standalone", false).expect("standalone recipe");
        assert_eq!(script, "opencode uninstall --force");
    }

    /// 未知安装方式（unknown）拒绝自动卸载（v1 提示手动）
    #[test]
    fn build_uninstall_unknown_method_rejected() {
        let err = build_uninstall_script("pi", "unknown", false).unwrap_err();
        assert!(err.contains("unsupported uninstall method"), "got: {err}");
    }

    /// 白名单外 cli 名拒绝（无用户自由输入拼接面）
    #[test]
    fn build_uninstall_unknown_cli_rejected() {
        assert!(build_uninstall_script("rm -rf /", "npm-global", false).is_err());
        assert!(build_uninstall_script("", "npm-global", false).is_err());
    }
}
