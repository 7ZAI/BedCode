//! 安装命令 recipe 白名单（纯函数，双平台可测）
//!
//! 仅接受白名单 cli 名 + 固定模板，method/installed 来自探测状态而非前端
//! 传参：claude native → `claude update`（镜像无关）；claude npm-global →
//! npm 包安装；opencode standalone 拒绝自动安装（官方脚本手动，v1 提示）。

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
}
