//! 平台分派与脚本安全工具（纯函数，无宿主调用）
//!
//! 从 lib.rs 抽出的横切工具：wasm 目标下编译期 `cfg!` 不可用，宿主 OS 由
//! activate 时经 `ConfigKey::OsPlatform` 缓存进 [`OS_PLATFORM`]；shell 脚本
//! 构造与用户可控路径的转义/拒绝规则三端共用（detect / install / skills /
//! usage 的脚本都以 `== 分段 ==` 标记输出，共用同一套安全底线）。

use std::sync::OnceLock;

/// 宿主 OS（activate 时经 `ConfigKey::OsPlatform` 缓存，见模块注释）
pub(crate) static OS_PLATFORM: OnceLock<String> = OnceLock::new();

/// 宿主是否为 Windows（运行时分派依据：wasm 下编译期 cfg 不可用）
pub(crate) fn is_windows() -> bool {
    OS_PLATFORM
        .get()
        .map(|p| p == "windows")
        .unwrap_or_else(|| cfg!(windows))
}

/// 宿主平台名（`std::env::consts::OS` 值域：linux / windows / macos / …）。
/// wasm 目标下取 activate 缓存的 `os.platform`；native 编译（单测）回退编译期。
/// 概览环境条「系统」行的唯一来源——不经 shell 采集。
pub(crate) fn os_platform() -> String {
    OS_PLATFORM
        .get()
        .cloned()
        .unwrap_or_else(|| std::env::consts::OS.to_string())
}

/// POSIX shell 单引号包裹转义：`'` → `'\''`。
///
/// 用户可控路径（add-source 自定义来源 / import 目录）进枚举脚本前必须经此
/// 转义：单引号内 `"` / `$()` / 反引号 / `;` 全部中和，杜绝 shell 注入。
/// 路径为绝对路径（校验于 add_source / import_local），不存在前导 `-` 被
/// find 当作选项解释的面。
pub(crate) fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 路径进 shell 脚本前的双平台统一兜底校验：拒绝控制字符（含换行——会破坏
/// `== 分段 ==` 标记解析）、双引号与 `%`。
///
/// 双引号会逃出 Windows cmd 的双引号包裹；`%` 在 cmd 命令行上下文无法转义
/// （caret 不覆盖 %，`%%` 折叠是 batch 文件语义）——%VAR% 展开会把枚举根
/// 重定向，故在入口直接拒绝（合法 Windows 路径含 % 的极少，可接受）。
/// POSIX 单引号转义后本可承载这些字符，双平台统一收紧。
pub(crate) fn path_rejected_for_script(path: &str) -> bool {
    path.chars().any(|c| c.is_control() || c == '"' || c == '%')
}

/// 进程启动三要素按平台分派（纯函数，双平台可测）：
/// unix 登录 shell（`-lc`，bashrc/nvm PATH 注入，与宿主 PTY 命令构建同模式）；
/// Windows `cmd /C`（GUI 进程 PATH 来自注册表用户环境，npm shim 经 PATHEXT 解析）
pub(crate) fn shell_invocation(script: String, windows: bool) -> (String, Vec<String>) {
    if windows {
        ("cmd".to_string(), vec!["/C".to_string(), script])
    } else {
        ("/bin/bash".to_string(), vec!["-lc".to_string(), script])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// shell 分派纯函数：unix 登录 shell / Windows cmd /C（两端形态锁定）
    #[test]
    fn shell_invocation_platforms() {
        let (cmd, args) = shell_invocation("npm install -g pi".to_string(), false);
        assert_eq!(cmd, "/bin/bash");
        assert_eq!(
            args,
            vec!["-lc".to_string(), "npm install -g pi".to_string()]
        );

        let (cmd, args) = shell_invocation("npm install -g pi".to_string(), true);
        assert_eq!(cmd, "cmd");
        assert_eq!(
            args,
            vec!["/C".to_string(), "npm install -g pi".to_string()]
        );
    }

    /// POSIX 单引号转义：双引号 / $() / 反引号 / 分号均被包裹中和；
    /// 单引号自身经 '\'' 转义不逃逸
    #[test]
    fn sh_quote_neutralizes_shell_metachars() {
        assert_eq!(sh_quote("/tmp/a;$(x)"), "'/tmp/a;$(x)'");
        assert_eq!(sh_quote("/tmp/it's"), "'/tmp/it'\\''s'");
        assert_eq!(sh_quote("plain"), "'plain'");
    }

    /// 脚本入口拒绝集：控制字符（含换行）/ 双引号 / %（双平台统一收紧，
    /// Windows cmd 无法在命令行上下文转义 %，引号会逃出双引号包裹）
    #[test]
    fn path_rejected_for_script_set() {
        assert!(!path_rejected_for_script("/tmp/normal dir/with space"));
        assert!(path_rejected_for_script("/tmp/a\nb"));
        assert!(path_rejected_for_script("/tmp/a\"b"));
        assert!(path_rejected_for_script("/tmp/100%"));
        assert!(path_rejected_for_script("/tmp/\u{1}"));
    }
}
