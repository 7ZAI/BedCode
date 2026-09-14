//! CLI 探测域（票据 02）
//!
//! 经 host-process 平台分派采集（Windows / Linux 双兼容）：
//! - unix：登录 shell `bash -lc` + PATH 引导（登录 shell 不读 `~/.bashrc`，
//!   nvm 等 PATH 注入由交互子 shell 提取，见 `path_bootstrap_unix`），
//!   `which -a` 全命中
//! - Windows：`cmd /C` 链式命令（GUI 进程 PATH 来自注册表用户环境，npm shim
//!   经 PATHEXT 解析），`where` 全命中
//! - 每个 CLI：全部 PATH 命中（→ 双安装检测）+ `--version`
//! - 环境：node / npm / pnpm 版本 + 当前 npm registry
//! - 路径统一规范化为 `/` 分隔后再归类（Windows `where` 输出反斜杠）
//!
//! 流程：spawn_all 将各项标记 detecting 并持久化推送 → 宿主进程结束回灌
//! `on_process_done` → 读输出文件解析 → 更新 storage → emit 事件推前端。
//! 状态 JSON 为唯一真源（host-storage `detection` 键），每次读-改-写。

use super::{host, pending, shell_invocation, PendingRun, AUTH_KEY, DATA_DIR, STATE_KEY};
use bedcode_plugin_api::events::ProcessDoneEvent;
use bedcode_plugin_api::host::{HostEvents, HostFs, HostLog, HostProcess, HostStorage};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};

/// 探测项：4 个 CLI + 1 个环境采集
pub(crate) const ENV_KIND: &str = "env";
pub(crate) const CLI_KINDS: [&str; 4] = ["claude", "codex", "opencode", "pi"];
/// 登录 shell + nvm 注入可能偏慢，`--version` 类采集 20s 足够
const TIMEOUT_MS: u64 = 20_000;

static RUN_SEQ: AtomicU32 = AtomicU32::new(0);
/// 状态推送序号（push_state 单调自增，前端按 seq 过滤乱序旧事件）
static STATE_SEQ: AtomicU32 = AtomicU32::new(0);

// ==================== 状态（读-改-写） ====================

fn read_state(h: &WasmHost) -> Value {
    h.storage_get(STATE_KEY).ok().flatten().unwrap_or_else(|| {
        json!({
            "authGranted": false,
            "envStatus": "idle",
            "env": null,
            "clis": default_clis(),
        })
    })
}

fn default_clis() -> Value {
    let mut map = serde_json::Map::new();
    for kind in CLI_KINDS {
        map.insert(
            kind.to_string(),
            json!({
                "status": "idle",
                "installed": null,
                "version": null,
                "method": "unknown",
                "paths": [],
                "dual": false,
                "error": null,
            }),
        );
    }
    Value::Object(map)
}

fn write_state(h: &WasmHost, state: &Value) {
    if let Err(e) = h.storage_set(STATE_KEY, state) {
        h.log_warn(&format!("detect: persist state failed: {e}"));
    }
}

/// 全量状态推送前端（guest 传事件全名，与前端订阅端一致）
pub(crate) fn push_state(h: &WasmHost) {
    let mut state = read_state(h);
    // 授权状态每次以 storage 标记为准（request-auth 命令可随时改写）
    if let Some(auth) = h.storage_get(AUTH_KEY).ok().flatten() {
        state["authGranted"] = json!(auth == json!("granted"));
    }
    // 单调递增序号：探测期间多次推送全量状态，前端按 seq 过滤乱序旧事件，
    // 只接受最新全量——避免中间态（某探测项仍 detecting）覆盖最终态，
    // 导致 UI 永久"检测中"（实测复现：storage 已 ok，界面卡 detecting）
    state["seq"] = json!(STATE_SEQ.fetch_add(1, Ordering::Relaxed) + 1);
    h.emit_event("plugin:agent-hub:detection", &state);
}

fn mark_detecting(state: &mut Value) {
    state["envStatus"] = json!("detecting");
    if let Some(clis) = state["clis"].as_object_mut() {
        for kind in CLI_KINDS {
            if let Some(info) = clis.get_mut(kind) {
                info["status"] = json!("detecting");
            }
        }
    }
}

fn set_cli_error(state: &mut Value, kind: &str, message: &str) {
    if kind == ENV_KIND {
        state["envStatus"] = json!("error");
        state["envError"] = json!(message);
    } else if let Some(clis) = state["clis"].as_object_mut() {
        clis.insert(
            kind.to_string(),
            json!({
                "status": "error",
                "installed": null,
                "version": null,
                "method": "unknown",
                "paths": [],
                "dual": false,
                "error": message,
            }),
        );
    }
}

// ==================== 采集入口 ====================

/// 触发全量探测：先全部标记 detecting 推送（前端即时进加载态），再逐项 spawn
pub(crate) fn spawn_all(h: &WasmHost) -> anyhow::Result<()> {
    let data_dir = DATA_DIR
        .get()
        .ok_or_else(|| anyhow::anyhow!("detect: data dir unavailable (activate incomplete)"))?;

    {
        let mut state = read_state(h);
        mark_detecting(&mut state);
        write_state(h, &state);
    }
    push_state(h);

    let mut kinds = vec![ENV_KIND];
    kinds.extend(CLI_KINDS);
    for kind in kinds {
        let seq = RUN_SEQ.fetch_add(1, Ordering::Relaxed);
        let output_path = format!("{data_dir}/runs/{kind}-{seq}.log");
        let script = detection_script(kind);
        let (command, args) = shell_invocation(script, super::is_windows());
        let request = json!({
            "command": command,
            "args": args,
            "output_path": output_path,
            "timeout_ms": TIMEOUT_MS,
        });
        match h.process_run(&request.to_string()) {
            Ok(run_id) => {
                let mut map = pending()
                    .lock()
                    .map_err(|e| anyhow::anyhow!("detect: poisoned pending map: {e}"))?;
                map.insert(
                    run_id,
                    PendingRun {
                        kind: kind.to_string(),
                        output_path,
                        cli: None,
                        source: None,
                    },
                );
            }
            Err(e) => {
                h.log_warn(&format!("detect: spawn {kind} failed: {e}"));
                let mut state = read_state(h);
                set_cli_error(&mut state, kind, &e.to_string());
                write_state(h, &state);
            }
        }
    }
    push_state(h);
    Ok(())
}

/// 进程完成回灌：按 run_id 归属 → 读输出 → 解析 → 持久化 → 推送
pub(crate) fn handle_process_done(event: &ProcessDoneEvent) -> anyhow::Result<()> {
    let h = host();
    let removed = pending()
        .lock()
        .map_err(|e| anyhow::anyhow!("process-done: poisoned pending map: {e}"))?
        .remove(&event.run_id);
    let Some(entry) = removed else {
        // 非本插件的进程回调，或停用清理后的迟到回调——放行
        return Ok(());
    };
    let kind = entry.kind;
    let output_path = entry.output_path;

    h.log_info(&format!(
        "detect: {kind} run finished (exit_code={:?}, timed_out={})",
        event.exit_code, event.timed_out
    ));

    let mut state = read_state(&h);
    if event.timed_out || event.exit_code != Some(0) {
        set_cli_error(
            &mut state,
            &kind,
            &format!("exit={:?} timed_out={}", event.exit_code, event.timed_out),
        );
    } else {
        match h.fs_read(&output_path) {
            Ok(Some(output)) => {
                apply_output(&mut state, &kind, &output);
                // 采集完成即清理输出文件，防 runs 目录堆积（尽力而为）
                let _ = h.fs_delete(&output_path);
            }
            Ok(None) => set_cli_error(&mut state, &kind, "output file missing"),
            Err(e) => set_cli_error(&mut state, &kind, &format!("read output failed: {e}")),
        }
    }
    write_state(&h, &state);
    push_state(&h);
    Ok(())
}

fn apply_output(state: &mut Value, kind: &str, output: &str) {
    if kind == ENV_KIND {
        state["envStatus"] = json!("ok");
        state["env"] = parse_env(output);
        // 清除历史失败残留（如修复前探测的 exit=127 envError），避免 UI 混淆
        state["envError"] = Value::Null;
    } else {
        let mut info = parse_cli(kind, output);
        info["status"] = json!(if info["installed"] == json!(true) {
            "ok"
        } else {
            "not-installed"
        });
        state["clis"][kind] = info;
    }
}

// ==================== 采集脚本（按平台分派，分段标记两端一致） ====================

/// 采集脚本分派：Windows 用 `cmd /C` 链式（`where` 全命中、`2>nul` 抑制未装
/// 报错）；unix 用 `which -a` + `head` 截断版本噪声
fn detection_script(kind: &str) -> String {
    if cfg!(windows) {
        if kind == ENV_KIND {
            "echo == node == & node --version 2>&1 & echo == npm == & npm --version 2>&1 & echo == pnpm == & pnpm --version 2>&1 & echo == registry == & npm config get registry 2>&1".to_string()
        } else {
            format!("echo == paths == & where {kind} 2>nul & echo == version == & {kind} --version 2>&1")
        }
    } else if kind == ENV_KIND {
        env_script_unix()
    } else {
        cli_script_unix(kind)
    }
}

/// unix PATH 引导前缀：登录 shell 不读 `~/.bashrc`（其交互守卫
/// `case $- in *i*)` 在非交互 shell 下直接 return，`~/.profile` 的 source
/// 同样被拦截），nvm 等 PATH 注入因而失效，node/npm/pnpm 探测全部
/// not found。改为从交互子 shell 提取 PATH（`2>/dev/null` 吞无 tty 的
/// ioctl 警告；`tail -n 1` 取末行，防 rc 启动输出污染），再在当前非交互
/// shell 继续采集——两侧段标记契约不受影响。
fn path_bootstrap_unix() -> &'static str {
    "export PATH=\"$(bash -ic 'printf \"%s\\n\" \"$PATH\"' 2>/dev/null | tail -n 1)\"\n"
}

/// unix 单 CLI 采集：`which -a` 全部 PATH 命中 + 版本行；未安装时 paths 为空、
/// version 段只有 shell 报错行 → not-installed
fn cli_script_unix(kind: &str) -> String {
    format!(
        "{}echo '== paths =='\nwhich -a {kind} 2>/dev/null\necho '== version =='\n{kind} --version 2>&1 | head -n 2\n",
        path_bootstrap_unix()
    )
}

/// unix 环境采集：node / npm / pnpm 版本 + 当前 npm registry
fn env_script_unix() -> String {
    format!(
        "{}echo '== node =='\nnode --version 2>&1\necho '== npm =='\nnpm --version 2>&1\necho '== pnpm =='\npnpm --version 2>&1\necho '== registry =='\nnpm config get registry 2>&1\n",
        path_bootstrap_unix()
    )
}

// ==================== 输出解析 ====================

/// 解析单 CLI 输出（分段：== paths == / == version ==）
pub(crate) fn parse_cli(kind: &str, output: &str) -> Value {
    let mut paths: Vec<String> = vec![];
    let mut version: Option<String> = None;
    let mut section = "";
    for line in output.lines() {
        let line = line.trim();
        match line {
            "== paths ==" => {
                section = "paths";
                continue;
            }
            "== version ==" => {
                section = "version";
                continue;
            }
            _ => {}
        }
        if line.is_empty() {
            continue;
        }
        match section {
            // which/where 命中行：unix 以 `/` 开头，Windows 为盘符路径；未命中时该段为空
            "paths" => {
                if let Some(p) = normalize_path_line(line) {
                    if !paths.contains(&p) {
                        paths.push(p);
                    }
                }
            }
            "version" => {
                if version.is_none() {
                    version = extract_version(line);
                }
            }
            _ => {}
        }
    }
    let installed = !paths.is_empty() && version.is_some();
    json!({
        "status": "ok",
        "installed": installed,
        "version": version,
        "method": classify(kind, &paths),
        "paths": paths,
        "dual": paths.len() > 1,
        "error": null,
    })
}

/// 路径行识别 + 规范化：unix 以 `/` 开头；Windows 为盘符路径（`X:\...`，
/// where 输出反斜杠）。统一转 `/` 分隔存储——归类与前端展示共用
fn normalize_path_line(line: &str) -> Option<String> {
    let is_path = line.starts_with('/')
        || (line.len() >= 2
            && line.as_bytes()[0].is_ascii_alphabetic()
            && line.as_bytes()[1] == b':');
    if is_path {
        Some(line.replace('\\', "/"))
    } else {
        None
    }
}

/// 从版本行提取语义版本 token，兼容 `codex-cli 0.153.4` / `2.1.263 (Claude Code)` /
/// `0.85.1` 等形态（无 regex 依赖：取首个「至少两段、首段纯数字」的 token）
fn extract_version(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|tok| {
            let mut parts = tok.split('.');
            let Some(first) = parts.next() else {
                return false;
            };
            if first.is_empty() || !first.chars().all(|c| c.is_ascii_digit()) {
                return false;
            }
            parts
                .next()
                .and_then(|p| p.chars().next())
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        })
        .map(|tok| {
            tok.trim_matches(|c: char| c == '(' || c == ')' || c == ',')
                .to_string()
        })
}

/// 安装方式归类：以 PATH 首位（实际生效者）为准
///
/// 输入已规范化为 `/` 分隔；Windows 路径大小写不敏感，统一小写后匹配特征。
/// - standalone：opencode 官方安装脚本落 `~/.opencode/bin`
/// - npm-global：nvm（unix/nvm-windows）/ node 全局 bin（Windows 为
///   `%APPDATA%\Roaming\npm` shim 目录，另含 volta、pnpm 全局目录特征）
/// - native：claude 官方安装器落 `~/.local/bin`（两端同位）
fn classify(kind: &str, paths: &[String]) -> &'static str {
    let Some(primary) = paths.first() else {
        return "unknown";
    };
    let p = primary.to_lowercase();
    if p.contains("/.opencode/bin/") {
        return "standalone";
    }
    if p.contains("nvm")
        || p.contains("/node-v")
        || p.contains("node_modules")
        || p.contains("/.npm-global")
        || p.contains("/.local/share/pnpm")
        || p.contains("volta")
        || p.contains("/roaming/npm/")
        || p.contains("/program files/nodejs")
    {
        return "npm-global";
    }
    if kind == "claude" && p.contains("/.local/bin/") {
        return "native";
    }
    "unknown"
}

/// 解析环境输出（分段：== node == / == npm == / == pnpm == / == registry ==）；
/// 各段取首个非空行，pnpm 未安装的 shell 报错行不算版本
pub(crate) fn parse_env(output: &str) -> Value {
    let mut node: Option<String> = None;
    let mut npm: Option<String> = None;
    let mut pnpm: Option<String> = None;
    let mut registry: Option<String> = None;
    let mut section = "";
    for line in output.lines() {
        let line = line.trim();
        match line {
            "== node ==" => {
                section = "node";
                continue;
            }
            "== npm ==" => {
                section = "npm";
                continue;
            }
            "== pnpm ==" => {
                section = "pnpm";
                continue;
            }
            "== registry ==" => {
                section = "registry";
                continue;
            }
            _ => {}
        }
        if line.is_empty() {
            continue;
        }
        match section {
            "node" => {
                if node.is_none() {
                    node = Some(line.to_string());
                }
            }
            "npm" => {
                if npm.is_none() {
                    npm = Some(line.to_string());
                }
            }
            "pnpm" => {
                // pnpm 未安装：unix「command not found」/ Windows「'pnpm' is not
                // recognized」/ 中文 locale 报错行，均不算版本
                if pnpm.is_none()
                    && !line.contains("not found")
                    && !line.contains("not recognized")
                    && !line.contains("未找到")
                {
                    pnpm = Some(line.to_string());
                }
            }
            "registry" => {
                if registry.is_none() {
                    registry = Some(line.to_string());
                }
            }
            _ => {}
        }
    }
    json!({ "node": node, "npm": npm, "pnpm": pnpm, "registry": registry })
}

// ==================== Tests（native 编译下的纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// codex 形态：版本带 CLI 名前缀；本机已装 → npm-global
    #[test]
    fn parse_cli_codex() {
        let out = "== paths ==\n/home/u/.config/nvm/versions/node/v24.20.0/bin/codex\n== version ==\ncodex-cli 0.153.4\n";
        let info = parse_cli("codex", out);
        assert_eq!(info["installed"], json!(true));
        assert_eq!(info["version"], json!("0.153.4"));
        assert_eq!(info["method"], json!("npm-global"));
        assert_eq!(info["dual"], json!(false));
    }

    /// opencode 双安装：standalone 生效（PATH 首位）+ npm 并存 → dual + standalone
    #[test]
    fn parse_cli_opencode_dual_install() {
        let out = "== paths ==\n/home/u/.opencode/bin/opencode\n/home/u/.config/nvm/versions/node/v24.20.0/bin/opencode\n== version ==\nopencode 1.18.30\n";
        let info = parse_cli("opencode", out);
        assert_eq!(info["dual"], json!(true));
        assert_eq!(info["method"], json!("standalone"));
        assert_eq!(info["version"], json!("1.18.30"));
        assert_eq!(info["paths"].as_array().unwrap().len(), 2);
    }

    /// claude native：~/.local/bin 命中 + 括号后缀版本行
    #[test]
    fn parse_cli_claude_native() {
        let out = "== paths ==\n/home/u/.local/bin/claude\n== version ==\n2.1.263 (Claude Code)\n";
        let info = parse_cli("claude", out);
        assert_eq!(info["method"], json!("native"));
        assert_eq!(info["version"], json!("2.1.263"));
    }

    /// 未安装：paths 为空、version 段只有 shell 报错行 → not-installed 前置数据
    #[test]
    fn parse_cli_not_installed() {
        let out = "== paths ==\n== version ==\nbash: codex: command not found\n";
        let info = parse_cli("codex", out);
        assert_eq!(info["installed"], json!(false));
        assert_eq!(info["version"], json!(null));
        assert_eq!(info["paths"].as_array().unwrap().len(), 0);
    }

    /// pi 版本行为纯 token；版本 token 不吞括号（claude 形态）
    #[test]
    fn extract_version_variants() {
        assert_eq!(extract_version("0.85.1"), Some("0.85.1".to_string()));
        assert_eq!(
            extract_version("codex-cli 0.153.4"),
            Some("0.153.4".to_string())
        );
        assert_eq!(
            extract_version("2.1.263 (Claude Code)"),
            Some("2.1.263".to_string())
        );
        // 单段数字不是版本
        assert_eq!(extract_version("some note 42"), None);
    }

    /// 环境解析：pnpm 报错行不算版本；registry 原样透传
    #[test]
    fn parse_env_sections() {
        let out = "== node ==\nv24.20.0\n== npm ==\n12.0.2\n== pnpm ==\n== registry ==\nhttps://registry.npmjs.org/\n";
        let env = parse_env(out);
        assert_eq!(env["node"], json!("v24.20.0"));
        assert_eq!(env["npm"], json!("12.0.2"));
        assert_eq!(env["registry"], json!("https://registry.npmjs.org/"));
    }

    /// pnpm 未安装：报错行被过滤
    #[test]
    fn parse_env_pnpm_missing() {
        let out = "== node ==\nv24.20.0\n== npm ==\n12.0.2\n== pnpm ==\nbash: pnpm: command not found\n== registry ==\nhttps://registry.npmjs.org/\n";
        let env = parse_env(out);
        assert_eq!(env["pnpm"], json!(null));
    }

    /// Windows 形态：where 反斜杠输出规范化 + 盘符路径识别；npm 全局
    /// （Roaming/npm shim）、claude native（%USERPROFILE%\.local\bin）、
    /// opencode standalone（%USERPROFILE%\.opencode\bin）归类
    #[test]
    fn parse_cli_windows_paths() {
        let out =
            "== paths ==\nC:\\Users\\u\\AppData\\Roaming\\npm\\pi.cmd\n== version ==\n0.85.1\n";
        let info = parse_cli("pi", out);
        assert_eq!(info["installed"], json!(true));
        assert_eq!(info["method"], json!("npm-global"));
        assert_eq!(
            info["paths"][0],
            json!("C:/Users/u/AppData/Roaming/npm/pi.cmd")
        );

        let out = "== paths ==\nC:\\Users\\u\\.local\\bin\\claude.exe\n== version ==\n2.1.263 (Claude Code)\n";
        let info = parse_cli("claude", out);
        assert_eq!(info["method"], json!("native"));

        let out =
            "== paths ==\nC:\\Users\\u\\.opencode\\bin\\opencode.exe\n== version ==\n1.18.30\n";
        let info = parse_cli("opencode", out);
        assert_eq!(info["method"], json!("standalone"));
    }

    /// 脚本以 PATH 引导开头（登录 shell 不读 ~/.bashrc，nvm 注入依赖引导），
    /// 段标记保持后端解析契约不变
    #[test]
    fn scripts_carry_path_bootstrap() {
        assert!(env_script_unix().starts_with(path_bootstrap_unix()));
        assert!(cli_script_unix("pi").starts_with(path_bootstrap_unix()));
        assert!(env_script_unix().contains("== registry =="));
        assert!(cli_script_unix("pi").contains("== version =="));
    }

    /// Windows cmd：pnpm 未装的报错行（'pnpm' is not recognized）不算版本
    #[test]
    fn parse_env_pnpm_missing_windows() {
        let out = "== node ==\nv24.20.0\n== npm ==\n12.0.2\n== pnpm ==\n'pnpm' is not recognized as an internal or external command\n== registry ==\nhttps://registry.npmjs.org/\n";
        let env = parse_env(out);
        assert_eq!(env["pnpm"], json!(null));
    }
}
