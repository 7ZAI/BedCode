//! Agent Hub Plugin (WASM, Desktop) — Agent CLI 统一管理台
//!
//! 票据 02（`.scratch/agent-hub/issues/02`）：插件骨架 + 概览探测。
//! 票据 03（`.scratch/agent-hub/issues/03`）：测速换源 + 一键安装/更新。
//! 票据 04（`.scratch/agent-hub/issues/04`）：Skills 管理（浏览/编辑/分发/
//! GitHub 安装/本地导入）。
//! 票据 06（`.scratch/agent-hub/issues/06`）：使用统计 + 会话日志（claude/pi
//! JSONL 适配器、水位增量、看板聚合、日志主从视图）。
//! 票据 05（`.scratch/agent-hub/issues/05`）：供应商统一管理（预设 CRUD/
//! 反向导入/应用到目标 CLI/claude 桥接冲突）。
//!
//! 探测与安装均经 host-process 以平台分派的 shell 执行：unix 登录 shell
//! （`bash -lc`，与宿主 PTY 命令构建同模式，保住 nvm 等 PATH 注入），
//! Windows `cmd /C`。宿主 OS 在 activate 时经 `ConfigKey::OsPlatform`
//! 缓存（wasm 目标下 `cfg!(windows)` 恒为 false，不可用于运行时平台分派）。
//! 进程为异步执行（run-id + output_path 落盘），完成事件 `on_process_done`
//! 按 run_id 归属驱动解析 → storage 持久化 → 事件推送前端。
//!
//! 目录授权（AC3）：activate 时对 CLI 配置目录做一次批量 request-auth 弹窗；
//! 拒绝**不阻断激活**（区别于 ai-chatbox 的激活门控 ADR 0007），降级由概览页
//! 横幅呈现，用户可随时经 `agent-hub.request-auth` 命令重试。`~/.claude` 走
//! 宿主 fs_auth 路径白名单免审，不在申请列。

use bedcode_plugin_api::events::ProcessDoneEvent;
use bedcode_plugin_api::host::{ConfigKey, HostConfig, HostFs, HostLog, HostStorage};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm_host::WasmHost;
use bedcode_plugin_api::WasmPlugin;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

mod detect;
mod install;
mod providers;
mod skills;
mod usage;
mod usage_parse;

pub(crate) const PLUGIN_ID: &str = "com.bedcode.agent-hub";
/// host-storage 键：探测状态 JSON（前端 `AgentHubState` wire 形状）
const STATE_KEY: &str = "detection";
/// host-storage 键：目录授权结果（"granted" | "declined"）
const AUTH_KEY: &str = "auth";
/// host-storage 键：安装/更新域复合状态（active/last/updates/mirror）
const INSTALL_KEY: &str = "install";

/// 插件数据目录：`{HomeDir}/.bedcode/agent-hub/`（探测/安装输出落盘 + 后续统计库）
static DATA_DIR: OnceLock<String> = OnceLock::new();
/// 用户主目录（npmrc 改写/还原用）
static HOME: OnceLock<String> = OnceLock::new();
/// 宿主 OS（activate 时经 `ConfigKey::OsPlatform` 缓存，见模块注释）
static OS_PLATFORM: OnceLock<String> = OnceLock::new();
/// 在途进程归属：run_id → 归属描述；on_process_done 按 run_id 分发
pub(crate) struct PendingRun {
    /// "env" | CLI 名（探测）| "install" | "skills-scan" | "skills-import"
    pub kind: String,
    pub output_path: String,
    /// kind == "install" 时为 Some(cli)
    pub cli: Option<String>,
    /// kind == "skills-import" 时为 "源目录\n入库目录名"
    pub source: Option<String>,
}
static PENDING: OnceLock<Mutex<HashMap<String, PendingRun>>> = OnceLock::new();

fn pending() -> &'static Mutex<HashMap<String, PendingRun>> {
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

fn host() -> WasmHost {
    WasmHost
}

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

/// 批量授权目录清单：全部为家目录绝对路径前缀；`~/.claude` 走宿主路径
/// 白名单免审，不在申请列（fs_auth 对含 `.claude/` 段的路径直接放行）；
/// `~/.claude.json` 是文件且不含 `.claude/` 段，白名单不覆盖，需单独申请
/// （同意后 usage 扫描可读配置提取「正在使用的项目会话」）。
fn auth_dirs(home: &str) -> Vec<String> {
    vec![
        format!("{home}/.codex"),
        format!("{home}/.pi"),
        format!("{home}/.config/opencode"),
        format!("{home}/.local/share/opencode"),
        format!("{home}/.agents"),
        format!("{home}/.npmrc"),
        format!("{home}/.claude.json"),
    ]
}

/// 批量申请 CLI 配置目录授权（一次弹窗），结果持久化并返回是否全部同意
fn request_auth(h: &WasmHost, home: &str) -> bool {
    let dirs = auth_dirs(home);
    let granted = h.fs_request_auth(&dirs).unwrap_or(false);
    let _ = h.storage_set(
        AUTH_KEY,
        &serde_json::json!(if granted { "granted" } else { "declined" }),
    );
    // 同步持久化 authGranted 到 detection：get-state 直接读 storage，若
    // 不写回，前端横幅恒显示"尚未授权"（push_state 只在事件 payload 更新、
    // 从不落库，实测 auth 键已 granted 但 detection.authGranted 恒 false）
    if let Ok(Some(mut state)) = h.storage_get(STATE_KEY) {
        state["authGranted"] = serde_json::json!(granted);
        let _ = h.storage_set(STATE_KEY, &state);
    }
    if granted {
        h.log_info("directory authorization granted");
    } else {
        h.log_warn("directory authorization declined; agent-hub degrades until granted");
    }
    granted
}

struct AgentHubPlugin;

impl WasmPlugin for AgentHubPlugin {
    const ID: &'static str = PLUGIN_ID;

    fn manifest() -> PluginManifest {
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let h = host();
        h.log_info("Agent Hub plugin activating (tickets 02-04)");

        let home = h
            .config_get(ConfigKey::HomeDir)?
            .ok_or_else(|| anyhow::anyhow!("activate: home_dir config unavailable"))?;
        let _ = DATA_DIR.set(format!("{home}/.bedcode/agent-hub"));
        let _ = HOME.set(home.clone());
        // 宿主 OS 运行时缓存：shell 分派/安装 recipe 均依赖（wasm 下 cfg 恒 unix）
        match h.config_get(ConfigKey::OsPlatform)? {
            Some(p) => {
                let _ = OS_PLATFORM.set(p);
            }
            None => h.log_warn("os.platform config unavailable; falling back to compile-time os"),
        }
        h.log_info("data dir resolved");

        // 授权拒绝不阻断激活（AC3）：结果落 storage，前端横幅降级呈现
        request_auth(&h, &home);

        // 供应商预设表（票据 05，幂等建表）；失败不阻断激活（命令入口会重试）
        if let Err(e) = providers::ensure_schema(&h) {
            h.log_warn(&format!("activate: providers schema init failed: {e}"));
        }
        // 使用统计三表（票据 06，幂等建表）；失败不阻断激活（命令入口会重试）
        if let Err(e) = usage::ensure_schema(&h) {
            h.log_warn(&format!("activate: usage schema init failed: {e}"));
        }

        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        // 清空在途进程归属：停用后到达的完成事件不再被误消费
        pending()
            .lock()
            .map(|mut p| p.clear())
            .map_err(|e| anyhow::anyhow!("deactivate: poisoned pending map: {e}"))?;
        // 在途安装进程尽力终止并落终态，避免 storage 里残留永久 "running"
        if let Err(e) = install::abort_active(&host()) {
            host().log_warn(&format!("deactivate: abort active run failed: {e}"));
        }
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let h = host();
        match name {
            "agent-hub.get-state" => {
                let state = h.storage_get(STATE_KEY)?;
                Ok(serde_json::json!({ "state": state }))
            }
            "agent-hub.request-auth" => {
                let home = HOME
                    .get()
                    .ok_or_else(|| anyhow::anyhow!("request-auth: home unavailable"))?;
                let granted = request_auth(&h, home);
                detect::push_state(&h);
                Ok(serde_json::json!({ "granted": granted }))
            }
            "agent-hub.detect" => {
                detect::spawn_all(&h)?;
                Ok(serde_json::json!({ "started": true }))
            }
            // ==================== 票据 03：安装/更新与镜像 ====================
            "agent-hub.get-install-state" => install::get_state(&h),
            "agent-hub.speed-test" => install::speed_test(&h),
            "agent-hub.apply-mirror" => install::apply_mirror(&h, &args),
            "agent-hub.add-custom-source" => install::add_custom_source(&h, &args),
            "agent-hub.remove-custom-source" => install::remove_custom_source(&h, &args),
            "agent-hub.restore-npmrc" => install::restore_npmrc(&h),
            "agent-hub.check-updates" => install::check_updates(&h),
            "agent-hub.install" => install::start(&h, &args),
            "agent-hub.describe-install" => install::describe_install(&h, &args),
            "agent-hub.get-run-output" => install::run_output(&h),
            "agent-hub.cancel-run" => install::cancel_run(&h),
            // ==================== 票据 04：Skills 管理 ====================
            "agent-hub.get-skills-state" => skills::get_state(&h),
            "agent-hub.scan-skills" => skills::scan(&h),
            "agent-hub.read-skill" => skills::read_skill(&h, &args),
            "agent-hub.save-skill" => skills::save_skill(&h, &args),
            "agent-hub.distribute-skill" => skills::distribute(&h, &args),
            "agent-hub.install-github-skill" => skills::install_github(&h, &args),
            "agent-hub.import-skill" => skills::import_local(&h, &args),
            // ==================== 票据 05：供应商统一管理 ====================
            "agent-hub.get-providers-state" => providers::get_state(&h),
            "agent-hub.save-preset" => providers::save_preset(&h, &args),
            "agent-hub.delete-preset" => providers::delete_preset(&h, &args),
            "agent-hub.import-providers" => providers::import_providers(&h),
            "agent-hub.apply-provider" => providers::apply_provider(&h, &args),
            // ==================== 票据 06：使用统计与会话日志 ====================
            "agent-hub.get-usage-state" => usage::get_state(&h),
            "agent-hub.scan-usage" => usage::scan(&h),
            "agent-hub.get-usage-stats" => usage::get_stats(&h),
            "agent-hub.list-usage-sources" => usage::list_sources(&h),
            "agent-hub.add-usage-source" => usage::add_source(&h, &args),
            "agent-hub.remove-usage-source" => usage::remove_source(&h, &args),
            "agent-hub.list-usage-sessions" => usage::list_sessions(&h, &args),
            "agent-hub.read-usage-session" => usage::read_session(&h, &args),
            other => Err(anyhow::anyhow!("unknown command: {other}")),
        }
    }

    fn on_process_done(event: &ProcessDoneEvent) -> anyhow::Result<()> {
        let kind = pending()
            .lock()
            .map_err(|e| anyhow::anyhow!("process-done: poisoned pending map: {e}"))?
            .get(&event.run_id)
            .map(|e| e.kind.clone());
        match kind.as_deref() {
            Some("install") => install::handle_process_done(event),
            Some(kind) if kind.starts_with("skills-") => skills::handle_process_done(event, kind),
            Some("usage-scan") => usage::handle_scan_done(event),
            Some(_) => detect::handle_process_done(event),
            // 非本插件的进程回调，或停用清理后的迟到回调——放行
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// manifest 经 include_str! 反序列化必须成功（注册即契约）
    #[test]
    fn manifest_parses() {
        let m = AgentHubPlugin::manifest();
        assert_eq!(m.id, PLUGIN_ID);
        assert_eq!(m.rust_library, "bedcode_plugin_agent_hub");
        assert!(m.permissions.contains(&"process:run".to_string()));
    }

    /// 授权目录清单：家目录绝对路径前缀、覆盖四家 CLI 配置落点，且不含
    /// `~/.claude`（fs_auth 路径白名单对含 `.claude/` 段的路径直接放行）
    #[test]
    fn auth_dirs_cover_cli_homes() {
        let dirs = auth_dirs("/home/u");
        assert_eq!(dirs.len(), 7);
        assert!(dirs.iter().all(|d| d.starts_with("/home/u/")));
        assert!(dirs.iter().any(|d| d.ends_with("/.codex")));
        assert!(dirs.iter().any(|d| d.ends_with("/.pi")));
        // ~/.claude 目录走白名单免审；仅 .claude.json 文件需授权（白名单不含文件）
        assert!(dirs.iter().any(|d| d.ends_with("/.claude.json")));
        assert!(dirs.iter().all(|d| !d.contains("/.claude/")));
    }

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

bedcode_plugin_api::wasm_entry!(AgentHubPlugin);
