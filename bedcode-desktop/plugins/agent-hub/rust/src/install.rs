//! 安装/更新与镜像域（票据 03）
//!
//! 职责：
//! - **测速**：host-http 对 npm 官方源与 npmmirror 各做一次小 GET 计时
//!   （计时用 `ConfigKey::CurrentTimeMs`——wasm32-unknown-unknown 无系统时钟，
//!   `Instant::now()` 会 panic），给出换源推荐
//! - **最新版本**：registry HTTP API `GET /<pkg>/latest`（免 shell，官方源失败
//!   回落 npmmirror），与探测到的本地版本比较得 outdated
//! - **一键安装/更新**：recipe 白名单构造（cli 名 + 固定模板，无用户自由输入
//!   拼接），host-process 平台分派执行（unix `bash -lc` / Windows `cmd /C`），
//!   输出落盘供前端轮询回显；完成后自动触发全量重探测刷新卡片
//! - **持久换源**：改写 `{HomeDir}/.npmrc`（改前备份、UI 一键还原）；registry
//!   行之外的内容原样保留（含 authToken 行），文件内容不落日志
//!
//! 状态为单一真源（host-storage `install` 键，读-改-写）：
//! `{ active, last, updates, mirror: { speed, npmrc } }`，每次变更全量 emit
//! `plugin:agent-hub:install` 推送前端。

use super::{host, pending, shell_invocation, PendingRun, DATA_DIR, HOME, INSTALL_KEY};
use crate::detect;
use bedcode_plugin_api::events::ProcessDoneEvent;
use bedcode_plugin_api::host::{
    ConfigKey, HostConfig, HostEvents, HostFs, HostHttp, HostLog, HostProcess, HostStorage,
};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};
use std::cmp::Ordering;
use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

/// npm 官方源与 npmmirror（spec §4.2 固定两源）
pub(crate) const NPMJS: &str = "https://registry.npmjs.org";
pub(crate) const NPMMIRROR: &str = "https://registry.npmmirror.com";
/// 测速样本包：元数据小、两端长期存在，`/<pkg>/latest` 形态稳定
const PROBE_PKG: &str = "semver";
/// 安装/更新超时：npm 全局安装含完整依赖下载，15 分钟上限（host 默认 10 分钟）
const INSTALL_TIMEOUT_MS: u64 = 900_000;
/// 输出回显尾部截断：控制台只需尾部，限制 guest↔host 每次轮询的载荷
const OUTPUT_TAIL_BYTES: usize = 16 * 1024;

static RUN_SEQ: AtomicU32 = AtomicU32::new(0);

// ==================== 状态（读-改-写） ====================

fn read_state(h: &WasmHost) -> Value {
    h.storage_get(INSTALL_KEY)
        .ok()
        .flatten()
        .unwrap_or_else(default_state)
}

/// 默认复合状态（纯函数，可测）
fn default_state() -> Value {
    let updates = detect::CLI_KINDS
        .iter()
        .map(|c| (c.to_string(), json!(null)))
        .collect::<serde_json::Map<String, Value>>();
    json!({
        "active": null,
        "last": null,
        "updates": updates,
        "mirror": default_mirror(),
    })
}

fn default_mirror() -> Value {
    json!({
        "speed": {
            "status": "idle",
            "npmjsMs": null,
            "npmmirrorMs": null,
            "recommend": null,
            "error": null,
            "testedAt": null,
        },
        "npmrc": { "backupExists": false, "fileRegistry": null },
    })
}

fn write_state(h: &WasmHost, state: &Value) {
    if let Err(e) = h.storage_set(INSTALL_KEY, state) {
        h.log_warn(&format!("install: persist state failed: {e}"));
    }
}

/// 全量状态推送前端（事件名与前端订阅端一致）
fn emit(h: &WasmHost, state: &Value) {
    h.emit_event("plugin:agent-hub:install", state);
}

/// 推送并返回状态（命令返回值与事件载荷同形）
fn emit_and_return(h: &WasmHost, state: &Value) -> anyhow::Result<Value> {
    emit(h, state);
    Ok(json!({ "state": state }))
}

/// 宿主当前 Unix 毫秒时间戳（wasm 无系统时钟，一律经宿主获取）
pub(crate) fn now_ms(h: &WasmHost) -> Result<u64, String> {
    let s = h
        .config_get(ConfigKey::CurrentTimeMs)
        .map_err(|e| format!("time unavailable: {e}"))?
        .ok_or_else(|| "time unavailable".to_string())?;
    s.parse::<u64>()
        .map_err(|e| format!("time parse failed: {e}"))
}

// ==================== recipe 白名单 ====================

/// npm 包名白名单（spec §4.2，包名已实机核实）；cli 名本身也经本表校验
fn npm_package(cli: &str) -> Option<&'static str> {
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

// ==================== 版本比较 ====================

/// 宽松语义化版本比较（纯函数）：numeric core 逐段比较，pre-release < 正式版；
/// pre-release 之间不细比（本插件仅用于「是否落后」的粗判断）
pub(crate) fn compare_versions(a: &str, b: &str) -> Ordering {
    let (a_core, a_pre) = split_pre(a);
    let (b_core, b_pre) = split_pre(b);
    let mut sa = a_core.split('.').map(|s| s.parse::<u64>().unwrap_or(0));
    let mut sb = b_core.split('.').map(|s| s.parse::<u64>().unwrap_or(0));
    loop {
        match (sa.next(), sb.next()) {
            (None, None) => break,
            (x, y) => {
                let xv = x.unwrap_or(0);
                let yv = y.unwrap_or(0);
                if xv != yv {
                    return xv.cmp(&yv);
                }
            }
        }
    }
    match (a_pre.is_empty(), b_pre.is_empty()) {
        (true, true) | (false, false) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
    }
}

fn split_pre(v: &str) -> (&str, &str) {
    match v.split_once('-') {
        Some((core, pre)) => (core, pre),
        None => (v, ""),
    }
}

// ==================== 测速 ====================

/// 两源小 GET 计时（一次探测一个源，host-http 非流式）
fn probe_registry(h: &WasmHost, url: &str) -> Result<u64, String> {
    let t0 = now_ms(h)?;
    let request = json!({ "method": "GET", "url": url });
    let resp = h
        .http_fetch(&request)
        .map_err(|e| format!("http failed: {e}"))?
        .ok_or_else(|| "empty response".to_string())?;
    let t1 = now_ms(h)?;
    let status = resp.get("status").and_then(|v| v.as_u64()).unwrap_or(0);
    if status != 200 {
        return Err(format!("status {status}"));
    }
    Ok(t1.saturating_sub(t0).max(1))
}

/// 测速并给出换源推荐：npmjs 失败或 npmmirror 更快 → 推荐 npmmirror
pub(crate) fn speed_test(h: &WasmHost) -> anyhow::Result<Value> {
    let mut state = read_state(h);
    state["mirror"]["speed"] = json!({
        "status": "testing",
        "npmjsMs": null,
        "npmmirrorMs": null,
        "recommend": null,
        "error": null,
        "testedAt": null,
    });
    write_state(h, &state);
    emit(h, &state);
    h.log_info("registry speed test started");

    let npmjs = probe_registry(h, &format!("{NPMJS}/{PROBE_PKG}/latest"));
    let mirror = probe_registry(h, &format!("{NPMMIRROR}/{PROBE_PKG}/latest"));
    let tested_at = now_ms(h).unwrap_or(0);

    let (npmjs_ms, npmjs_err) = split_probe(&npmjs);
    let (mirror_ms, mirror_err) = split_probe(&mirror);
    let recommend = match (&npmjs, &mirror) {
        // 官方源失败而镜像可达 → 镜像；两者可达比快；仅官方可达 → 官方
        (Err(_), Ok(_)) => Some("npmmirror"),
        (Ok(n), Ok(m)) => Some(if m < n { "npmmirror" } else { "npmjs" }),
        (Ok(_), Err(_)) => Some("npmjs"),
        (Err(_), Err(_)) => None,
    };
    let error = match (&npmjs_err, &mirror_err) {
        (Some(a), Some(b)) => Some(format!("both registries unreachable: {a}; {b}")),
        _ => None,
    };

    state["mirror"]["speed"] = json!({
        "status": if recommend.is_some() { "ok" } else { "error" },
        "npmjsMs": npmjs_ms,
        "npmmirrorMs": mirror_ms,
        "recommend": recommend,
        "error": error,
        "testedAt": tested_at,
    });
    write_state(h, &state);
    h.log_info(&format!(
        "registry speed test done (npmjs_ms={npmjs_ms:?}, npmmirror_ms={mirror_ms:?}, recommend={recommend:?})"
    ));
    emit_and_return(h, &state)
}

fn split_probe(r: &Result<u64, String>) -> (Option<u64>, Option<String>) {
    match r {
        Ok(ms) => (Some(*ms), None),
        Err(e) => (None, Some(e.clone())),
    }
}

// ==================== 最新版本查询 ====================

/// registry HTTP API 查最新版本（免 shell）：官方源失败回落 npmmirror；
/// scoped 包名按 registry 约定转义 `/` → `%2f`
fn fetch_latest_version(h: &WasmHost, pkg: &str) -> Result<String, String> {
    let scoped = pkg.replace('/', "%2f");
    for base in [NPMJS, NPMMIRROR] {
        let request = json!({ "method": "GET", "url": format!("{base}/{scoped}/latest") });
        match h.http_fetch(&request) {
            Ok(Some(resp)) => {
                let status = resp.get("status").and_then(|v| v.as_u64()).unwrap_or(0);
                let body = resp.get("body").and_then(|v| v.as_str()).unwrap_or("");
                if status == 200 {
                    if let Ok(meta) = serde_json::from_str::<Value>(body) {
                        if let Some(v) = meta.get("version").and_then(|v| v.as_str()) {
                            return Ok(v.to_string());
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(e) => h.log_debug(&format!("install: latest query failed on {base}: {e}")),
        }
    }
    Err(format!("latest version unavailable for {pkg}"))
}

/// 批量检查更新：四家 CLI 各查一次 latest，与探测到的本地版本比较得 outdated
pub(crate) fn check_updates(h: &WasmHost) -> anyhow::Result<Value> {
    let mut state = read_state(h);
    let detection = h.storage_get(super::STATE_KEY).ok().flatten();
    let checked_at = now_ms(h).unwrap_or(0);

    for cli in detect::CLI_KINDS {
        let local = detection
            .as_ref()
            .and_then(|d| d["clis"][cli]["version"].as_str())
            .map(|s| s.to_string());
        let entry = match npm_package(cli) {
            Some(pkg) => match fetch_latest_version(h, pkg) {
                Ok(latest) => {
                    let outdated = local
                        .as_deref()
                        .map(|l| compare_versions(l, &latest) == Ordering::Less);
                    json!({ "latest": latest, "outdated": outdated, "checkedAt": checked_at, "error": null })
                }
                Err(e) => {
                    json!({ "latest": null, "outdated": null, "checkedAt": checked_at, "error": e })
                }
            },
            None => json!({
                "latest": null, "outdated": null, "checkedAt": checked_at,
                "error": format!("no package for {cli}"),
            }),
        };
        state["updates"][cli] = entry;
    }
    write_state(h, &state);
    h.log_info("update check finished");
    emit_and_return(h, &state)
}

// ==================== npmrc 持久换源 ====================

fn npmrc_path() -> anyhow::Result<String> {
    let home = HOME
        .get()
        .ok_or_else(|| anyhow::anyhow!("home unavailable"))?;
    Ok(format!("{home}/.npmrc"))
}

fn npmrc_backup_path() -> anyhow::Result<String> {
    let home = HOME
        .get()
        .ok_or_else(|| anyhow::anyhow!("home unavailable"))?;
    Ok(format!("{home}/.npmrc.agent-hub-backup"))
}

/// registry 行改写（纯函数）：移除全部 `registry=...` 行（key 两侧空白容忍，
/// 大小写不敏感），文末追加目标 registry；其余行（含 authToken 行）原样保留。
/// 统一 LF 行尾（npm 两平台均接受）。返回 (新内容, 是否有改动)
pub(crate) fn rewrite_registry(content: &str, target: &str) -> (String, bool) {
    let mut lines: Vec<String> = Vec::new();
    for line in content.split('\n') {
        let bare = line.strip_suffix('\r').unwrap_or(line);
        let key = bare.split('=').next().unwrap_or("").trim();
        if key.eq_ignore_ascii_case("registry") {
            continue;
        }
        lines.push(bare.to_string());
    }
    while lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        lines.pop();
    }
    let mut body = if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    };
    body.push_str(&format!("registry={target}\n"));
    let changed = body != content;
    (body, changed)
}

/// 从 npmrc 内容提取当前 registry 行的值（非敏感，可回显；无则 None）
pub(crate) fn extract_registry(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("registry")
            .then(|| value.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

/// 持久切换：备份（仅在无备份时——保留用户原始文件，镜像改写后的内容不得
/// 覆盖备份）→ 改写 → 持久化推送。target 白名单仅两源
pub(crate) fn apply_mirror(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let target = match args.get("target").and_then(|v| v.as_str()) {
        Some("npmmirror") => NPMMIRROR,
        Some("npmjs") => NPMJS,
        _ => return Err(anyhow::anyhow!("apply-mirror: invalid target")),
    };
    let path = npmrc_path()?;
    let backup = npmrc_backup_path()?;
    let existing = h
        .fs_read(&path)
        .map_err(|e| anyhow::anyhow!("apply-mirror: read npmrc failed: {e}"))?;

    if existing.is_some()
        && !h
            .fs_exists(&backup)
            .map_err(|e| anyhow::anyhow!("apply-mirror: backup check failed: {e}"))?
    {
        h.fs_copy(&path, &backup)
            .map_err(|e| anyhow::anyhow!("apply-mirror: backup failed: {e}"))?;
        h.log_info("npmrc backed up before registry rewrite");
    }

    let (new_content, changed) = rewrite_registry(existing.as_deref().unwrap_or(""), target);
    h.fs_write(&path, &new_content)
        .map_err(|e| anyhow::anyhow!("apply-mirror: write npmrc failed: {e}"))?;

    let mut state = read_state(h);
    state["mirror"]["npmrc"] = json!({
        "backupExists": h.fs_exists(&backup).unwrap_or(false),
        "fileRegistry": target,
        "appliedAt": now_ms(h).unwrap_or(0),
    });
    write_state(h, &state);
    h.log_info(&format!(
        "npmrc registry rewritten (changed={changed}, target={target})"
    ));
    emit_and_return(h, &state)
}

/// 一键还原：备份覆盖回 npmrc（备份保留，可重复还原）
pub(crate) fn restore_npmrc(h: &WasmHost) -> anyhow::Result<Value> {
    let path = npmrc_path()?;
    let backup = npmrc_backup_path()?;
    if !h
        .fs_exists(&backup)
        .map_err(|e| anyhow::anyhow!("restore-npmrc: backup check failed: {e}"))?
    {
        return Err(anyhow::anyhow!("restore-npmrc: no backup to restore"));
    }
    h.fs_copy(&backup, &path)
        .map_err(|e| anyhow::anyhow!("restore-npmrc: restore failed: {e}"))?;
    let file_registry = h
        .fs_read(&path)
        .ok()
        .flatten()
        .as_deref()
        .and_then(extract_registry);

    let mut state = read_state(h);
    state["mirror"]["npmrc"] = json!({
        "backupExists": true,
        "fileRegistry": file_registry,
        "restoredAt": now_ms(h).unwrap_or(0),
    });
    write_state(h, &state);
    h.log_info("npmrc restored from backup");
    emit_and_return(h, &state)
}

// ==================== 安装/更新执行 ====================

/// 读取安装域状态（命令入口，前端挂载时拉取）
pub(crate) fn get_state(h: &WasmHost) -> anyhow::Result<Value> {
    Ok(json!({ "state": read_state(h) }))
}

/// 解析安装/更新命令供展示（node 环境缺失时的「生成命令 + 复制」降级路径）：
/// 与 start 同一 recipe 白名单与探测状态来源，仅无副作用、不做 node 兜底校验
pub(crate) fn describe_install(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let cli = args
        .get("cli")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("describe-install: missing cli"))?;
    if !detect::CLI_KINDS.contains(&cli) {
        return Err(anyhow::anyhow!("describe-install: unknown cli: {cli}"));
    }
    let use_mirror = args.get("mirror").and_then(|v| v.as_bool()).unwrap_or(true);

    let detection = h
        .storage_get(super::STATE_KEY)
        .map_err(|e| anyhow::anyhow!("describe-install: read detection failed: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("describe-install: detection not ready"))?;
    let info = &detection["clis"][cli];
    let method = info["method"].as_str().unwrap_or("unknown");
    let installed = info["installed"].as_bool().unwrap_or(false);
    let (script, action) =
        build_install_script(cli, method, installed, use_mirror).map_err(anyhow::Error::msg)?;
    Ok(json!({ "command": script, "action": action }))
}

/// 触发安装/更新：recipe 白名单构造 → host-process 平台分派执行 →
/// 状态落 storage 推送前端；输出经 output_path 由前端轮询回显
pub(crate) fn start(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let cli = args
        .get("cli")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("install: missing cli"))?;
    if !detect::CLI_KINDS.contains(&cli) {
        return Err(anyhow::anyhow!("install: unknown cli: {cli}"));
    }
    let use_mirror = args.get("mirror").and_then(|v| v.as_bool()).unwrap_or(true);

    let mut state = read_state(h);
    if state["active"].is_object() {
        return Err(anyhow::anyhow!("install: another run is active"));
    }

    // method/installed 取自探测状态（不信任前端传参）；node 环境缺失时前端
    // 降级为展示命令，这里兜底拒绝
    let detection = h
        .storage_get(super::STATE_KEY)
        .map_err(|e| anyhow::anyhow!("install: read detection failed: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("install: detection not ready"))?;
    let node_ok = detection["env"]["node"].is_string();
    if !node_ok {
        return Err(anyhow::anyhow!("install: node environment not detected"));
    }
    let info = &detection["clis"][cli];
    let method = info["method"].as_str().unwrap_or("unknown");
    let installed = info["installed"].as_bool().unwrap_or(false);
    let (script, action) =
        build_install_script(cli, method, installed, use_mirror).map_err(anyhow::Error::msg)?;

    let data_dir = DATA_DIR
        .get()
        .ok_or_else(|| anyhow::anyhow!("install: data dir unavailable"))?;
    let seq = RUN_SEQ.fetch_add(1, AtomicOrdering::Relaxed);
    let output_path = format!("{data_dir}/runs/install-{cli}-{seq}.log");
    let (command, args_vec) = shell_invocation(script.clone(), super::is_windows());
    let request = json!({
        "command": command,
        "args": args_vec,
        "output_path": output_path,
        "timeout_ms": INSTALL_TIMEOUT_MS,
    });
    let run_id = h
        .process_run(&request.to_string())
        .map_err(|e| anyhow::anyhow!("install: spawn failed: {e}"))?;

    pending()
        .lock()
        .map_err(|e| anyhow::anyhow!("install: poisoned pending map: {e}"))?
        .insert(
            run_id.clone(),
            PendingRun {
                kind: "install".to_string(),
                output_path: output_path.clone(),
                cli: Some(cli.to_string()),
                source: None,
            },
        );

    state["active"] = json!({
        "runId": run_id,
        "cli": cli,
        "action": action,
        "command": script,
        "useMirror": use_mirror,
        "outputPath": output_path,
        "startedAt": now_ms(h).unwrap_or(0),
        "cancelRequested": false,
    });
    write_state(h, &state);
    h.log_info(&format!(
        "install run started (cli = {cli}, action = {action}, mirror = {use_mirror})"
    ));
    emit_and_return(h, &state)
}

/// 轮询回显：active run 读 output_path 尾部；无 active 时回放 last 的终态输出
pub(crate) fn run_output(h: &WasmHost) -> anyhow::Result<Value> {
    let state = read_state(h);
    if let Some(active) = state["active"].as_object() {
        let path = active["outputPath"].as_str().unwrap_or("");
        let output = h
            .fs_read(path)
            .map_err(|e| anyhow::anyhow!("run-output: read failed: {e}"))?
            .as_deref()
            .map(output_tail);
        return Ok(json!({
            "status": "running",
            "cli": active["cli"],
            "action": active["action"],
            "command": active["command"],
            "output": output,
        }));
    }
    if let Some(last) = state["last"].as_object() {
        let status = if last["ok"] == json!(true) {
            "ok"
        } else if last["cancelled"] == json!(true) {
            "cancelled"
        } else {
            "error"
        };
        return Ok(json!({
            "status": status,
            "cli": last["cli"],
            "action": last["action"],
            "command": last["command"],
            "output": last["output"],
        }));
    }
    Ok(json!({ "status": "idle", "output": null }))
}

/// 取消在途 run（尽力而为：进程可能已退出）；cancelRequested 标记使完成
/// 事件落为 cancelled 终态
pub(crate) fn cancel_run(h: &WasmHost) -> anyhow::Result<Value> {
    let mut state = read_state(h);
    let run_id = state["active"]["runId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("cancel-run: no active run"))?
        .to_string();
    // 已自然结束属预期内（kill 与完成事件竞态），宿主按 best-effort 返回 Ok
    h.process_kill(&run_id)
        .map_err(|e| anyhow::anyhow!("cancel-run: kill failed: {e}"))?;
    state["active"]["cancelRequested"] = json!(true);
    write_state(h, &state);
    h.log_info(&format!("install run cancel requested (run_id = {run_id})"));
    emit_and_return(h, &state)
}

/// 进程完成回灌：读输出尾部落 last 终态 → 清 active → 删除输出文件 →
/// 推送 → 自动全量重探测刷新卡片
pub(crate) fn handle_process_done(event: &ProcessDoneEvent) -> anyhow::Result<()> {
    let h = host();
    let removed = pending()
        .lock()
        .map_err(|e| anyhow::anyhow!("process-done: poisoned pending map: {e}"))?
        .remove(&event.run_id);
    let Some(entry) = removed else {
        return Ok(());
    };
    let Some(cli) = entry.cli else {
        return Ok(());
    };

    let mut state = read_state(&h);
    if !state["active"].is_object() {
        // 状态丢失（storage 异常/停用清理）但仍收到完成事件：只清 pending，落盘缺位可容忍
        h.log_warn("process-done: active run state missing; ignoring done event");
        return Ok(());
    }
    let active = state["active"].take();
    let cancelled = active["cancelRequested"] == json!(true);
    let ok = event.exit_code == Some(0) && !event.timed_out;
    let log_line =
        format!("install run finished (cli = {cli}, ok = {ok}, cancelled = {cancelled})");

    let output = h
        .fs_read(&entry.output_path)
        .map_err(|e| anyhow::anyhow!("process-done: read output failed: {e}"))?
        .as_deref()
        .map(output_tail);
    state["last"] = json!({
        "cli": cli,
        "action": active["action"],
        "command": active["command"],
        "ok": ok,
        "cancelled": cancelled,
        "exitCode": event.exit_code,
        "timedOut": event.timed_out,
        "error": if ok {
            Value::Null
        } else {
            format!("exit={:?} timed_out={}", event.exit_code, event.timed_out).into()
        },
        "output": output,
        "finishedAt": now_ms(&h).unwrap_or(0),
    });
    write_state(&h, &state);
    h.log_info(&log_line);
    emit(&h, &state);

    // 尽力清理输出文件，防 runs 目录堆积
    let _ = h.fs_delete(&entry.output_path);

    // 完成后自动全量重探测：卡片版本/安装方式随实际结果刷新（尽力而为）
    if let Err(e) = detect::spawn_all(&h) {
        h.log_warn(&format!("process-done: post-install re-detect failed: {e}"));
    }
    Ok(())
}

/// 停用兜底：终止在途 run 并落 cancelled 终态（进程可能已退出，kill 尽力而为）
pub(crate) fn abort_active(h: &WasmHost) -> anyhow::Result<()> {
    let mut state = read_state(h);
    if !state["active"].is_object() {
        return Ok(());
    }
    let Some(run_id) = state["active"]["runId"].as_str().map(|s| s.to_string()) else {
        state["active"] = json!(null);
        write_state(h, &state);
        return Ok(());
    };
    let _ = h.process_kill(&run_id);
    let mut last = state["active"].take();
    last["ok"] = json!(false);
    last["cancelled"] = json!(true);
    last["error"] = json!("deactivated while running");
    state["last"] = last;
    write_state(h, &state);
    emit(h, &state);
    Ok(())
}

/// 输出尾部截断（char boundary 安全）
fn output_tail(s: &str) -> String {
    if s.len() <= OUTPUT_TAIL_BYTES {
        return s.to_string();
    }
    let mut start = s.len() - OUTPUT_TAIL_BYTES;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    s[start..].to_string()
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

    /// 版本比较：逐段数值比较（1.18.30 > 1.18.9）、pre-release < 正式版、相等
    #[test]
    fn compare_versions_orders() {
        use Ordering::*;
        assert_eq!(compare_versions("1.18.30", "1.18.9"), Greater);
        assert_eq!(compare_versions("2.1.263", "2.2.0"), Less);
        assert_eq!(compare_versions("0.85.1", "0.85.1"), Equal);
        assert_eq!(compare_versions("1.0.0-beta", "1.0.0"), Less);
        assert_eq!(compare_versions("1.2", "1.2.0"), Equal);
        assert_eq!(compare_versions("0.153.4", "0.154.0"), Less);
    }

    /// npmrc 改写：旧 registry 行（含带空格变体）被替换、authToken 行原样保留、
    /// 无 registry 行时追加
    #[test]
    fn rewrite_registry_replaces_and_preserves() {
        let content = "registry=https://registry.npmjs.org/\n//registry.npmjs.org/:_authToken=abc123\nalways-auth=true\n";
        let (out, changed) = rewrite_registry(content, NPMMIRROR);
        assert!(changed);
        assert!(out.contains("registry=https://registry.npmmirror.com"));
        // registry= key 行只剩目标一条（authToken 注释行含 registry.npmjs.org
        // 子串属合法保留，不作子串断言）
        let registry_lines: Vec<&str> = out
            .lines()
            .filter(|l| l.trim().to_lowercase().starts_with("registry"))
            .collect();
        assert_eq!(
            registry_lines,
            vec![format!("registry={NPMMIRROR}").as_str()]
        );
        // authToken 行不含 registry= key，必须原样保留（内容不落日志原则不适用于断言）
        assert!(out.contains("//registry.npmjs.org/:_authToken=abc123"));
        assert!(out.contains("always-auth=true"));
        assert!(out.ends_with('\n'));

        // 带空格的 key 变体（key = value）
        let (out, _) = rewrite_registry("registry = https://registry.npmjs.org/\n", NPMMIRROR);
        assert_eq!(out, format!("registry={NPMMIRROR}\n"));

        // 空文件：仅追加
        let (out, changed) = rewrite_registry("", NPMJS);
        assert!(changed);
        assert_eq!(out, format!("registry={NPMJS}\n"));

        // 目标与现状一致：无改动
        let (out, changed) = rewrite_registry(&format!("registry={NPMJS}\n"), NPMJS);
        assert!(!changed);
        assert_eq!(out, format!("registry={NPMJS}\n"));
    }

    /// npmrc registry 提取：首个 registry 行的值；无则 None
    #[test]
    fn extract_registry_finds_value() {
        assert_eq!(
            extract_registry("foo=1\nregistry=https://registry.npmmirror.com\nbar=2"),
            Some("https://registry.npmmirror.com".to_string())
        );
        assert_eq!(extract_registry("always-auth=true\n"), None);
        assert_eq!(extract_registry("registry=\n"), None);
    }

    /// 输出尾部截断：小文件原样；大文件取尾部且 char boundary 安全
    #[test]
    fn output_tail_truncates() {
        assert_eq!(output_tail("hello"), "hello");
        let big = "你".repeat(OUTPUT_TAIL_BYTES); // 3 bytes/char，必然截在多字节中间
        let tail = output_tail(&big);
        assert!(tail.len() <= OUTPUT_TAIL_BYTES + 3);
        assert_eq!(tail, big[big.len() - tail.len()..]);
    }

    /// 状态默认值：updates 覆盖四家 CLI、mirror 含 speed/npmrc 两个子域
    #[test]
    fn default_state_shape() {
        let state = default_state();
        assert!(state["active"].is_null());
        assert!(state["last"].is_null());
        for cli in detect::CLI_KINDS {
            assert!(state["updates"][cli].is_null(), "{cli} 应有 updates 占位");
        }
        assert_eq!(state["mirror"]["speed"]["status"], json!("idle"));
        assert_eq!(state["mirror"]["npmrc"]["backupExists"], json!(false));
    }
}
