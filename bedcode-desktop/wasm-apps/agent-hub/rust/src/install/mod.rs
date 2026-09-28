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
//! # 模块结构
//! - [`state`]：域状态（host-storage `install` 键，读-改-写 + 全量推送）
//! - [`recipe`]：安装命令 recipe 白名单（纯函数）
//! - [`version`]：宽松语义化版本比较（纯函数）
//! - [`registry`]：测速与最新版本查询（host-http 免 shell）
//! - [`mirror`]：npmrc 持久换源与自定义源管理
//! - 本模块：安装/更新执行 lifecycle（spawn → 进程回灌 → 自动重探测）

use super::{host, pending, PendingRun, DATA_DIR, HOME, INSTALL_KEY};
use crate::detect;
use crate::util::{is_windows, shell_invocation};
use bedcode_plugin_api::events::ProcessDoneEvent;
use bedcode_plugin_api::host::{HostFs, HostLog, HostProcess, HostStorage};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

mod mirror;
mod recipe;
mod registry;
mod state;
mod version;

/// 命令入口面（lib.rs 路由）
pub(crate) use mirror::{add_custom_source, apply_mirror, remove_custom_source, restore_npmrc};
pub(crate) use registry::{check_updates, speed_test};
pub(crate) use state::now_ms;

use self::recipe::build_install_script;
use self::recipe::build_uninstall_script;
use self::state::{emit, emit_and_return, read_state, write_state};

/// 安装/更新超时：npm 全局安装含完整依赖下载，15 分钟上限（host 默认 10 分钟）
const INSTALL_TIMEOUT_MS: u64 = 900_000;
/// 输出回显尾部截断：控制台只需尾部，限制 guest↔host 每次轮询的载荷
const OUTPUT_TAIL_BYTES: usize = 16 * 1024;

static RUN_SEQ: AtomicU32 = AtomicU32::new(0);

// ==================== 安装/更新执行 ====================
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
        // 并发安装/更新（用户可见拒绝；ADR 0030 业务码，前端经插件 i18n 展示）
        bedcode_plugin_api::bail_with_code!("com.bedcode.agent-hub.hub.inst.error.busy");
    }

    // method/installed 取自探测状态（不信任前端传参）；node 环境缺失时前端
    // 降级为展示命令，这里兜底拒绝
    let detection = h
        .storage_get(super::STATE_KEY)
        .map_err(|e| anyhow::anyhow!("install: read detection failed: {e}"))?
        .ok_or_else(|| {
            // 探测未就绪（用户可见拒绝；ADR 0030 业务码）
            anyhow::anyhow!(bedcode_plugin_api::user_facing_string(
                "com.bedcode.agent-hub.hub.inst.error.detectionPending",
                serde_json::Value::Null,
            ))
        })?;
    let node_ok = detection["env"]["node"].is_string();
    if !node_ok {
        // 无 node 环境（用户可见拒绝；ADR 0030 业务码）
        bedcode_plugin_api::bail_with_code!("com.bedcode.agent-hub.hub.inst.error.nodeMissing");
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
    let (command, args_vec) = shell_invocation(script.clone(), is_windows());
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

/// 触发卸载：recipe 白名单构造 → host-process 平台分派执行 → 状态落
/// storage 推送前端（与安装共用同一 run 管线：active 互斥、输出回显、取消、
/// 完成后自动重探测刷新卡片）。卸载是破坏性动作：只接受白名单 cli + 固定
/// 模板，method 取自探测状态（不信任前端传参）；npm-global 依赖 node。
pub(crate) fn uninstall(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let cli = args
        .get("cli")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("uninstall: missing cli"))?;
    if !detect::CLI_KINDS.contains(&cli) {
        return Err(anyhow::anyhow!("uninstall: unknown cli: {cli}"));
    }

    let mut state = read_state(h);
    if state["active"].is_object() {
        // 并发卸载（用户可见拒绝；ADR 0030 业务码）
        bedcode_plugin_api::bail_with_code!("com.bedcode.agent-hub.hub.inst.error.busy");
    }

    // method 取自探测状态（不信任前端传参）；npm-global 卸载依赖 node 环境
    let detection = h
        .storage_get(super::STATE_KEY)
        .map_err(|e| anyhow::anyhow!("uninstall: read detection failed: {e}"))?
        .ok_or_else(|| {
            // 探测未就绪（用户可见拒绝；ADR 0030 业务码）
            anyhow::anyhow!(bedcode_plugin_api::user_facing_string(
                "com.bedcode.agent-hub.hub.inst.error.detectionPending",
                serde_json::Value::Null,
            ))
        })?;
    let method = detection["clis"][cli]["method"]
        .as_str()
        .unwrap_or("unknown");
    if method == "npm-global" && !detection["env"]["node"].is_string() {
        // 无 node 环境（用户可见拒绝；ADR 0030 业务码）
        bedcode_plugin_api::bail_with_code!("com.bedcode.agent-hub.hub.inst.error.nodeMissing");
    }
    let script = build_uninstall_script(cli, method, is_windows()).map_err(anyhow::Error::msg)?;

    let data_dir = DATA_DIR
        .get()
        .ok_or_else(|| anyhow::anyhow!("uninstall: data dir unavailable"))?;
    let seq = RUN_SEQ.fetch_add(1, AtomicOrdering::Relaxed);
    let output_path = format!("{data_dir}/runs/uninstall-{cli}-{seq}.log");
    let (command, args_vec) = shell_invocation(script.clone(), is_windows());
    let request = json!({
        "command": command,
        "args": args_vec,
        "output_path": output_path,
        "timeout_ms": INSTALL_TIMEOUT_MS,
    });
    let run_id = h
        .process_run(&request.to_string())
        .map_err(|e| anyhow::anyhow!("uninstall: spawn failed: {e}"))?;

    pending()
        .lock()
        .map_err(|e| anyhow::anyhow!("uninstall: poisoned pending map: {e}"))?
        .insert(
            run_id.clone(),
            PendingRun {
                kind: "uninstall".to_string(),
                output_path: output_path.clone(),
                cli: Some(cli.to_string()),
                source: None,
            },
        );

    state["active"] = json!({
        "runId": run_id,
        "cli": cli,
        "action": "uninstall",
        "command": script,
        "useMirror": false,
        "outputPath": output_path,
        "startedAt": now_ms(h).unwrap_or(0),
        "cancelRequested": false,
    });
    write_state(h, &state);
    h.log_info(&format!("uninstall run started (cli = {cli})"));
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
/// 推送 → 自动全量重探测刷新卡片（install / uninstall 共用）
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
    let kind = entry.kind;

    let mut state = read_state(&h);
    if !state["active"].is_object() {
        // 状态丢失（storage 异常/停用清理）但仍收到完成事件：只清 pending，落盘缺位可容忍
        h.log_warn("process-done: active run state missing; ignoring done event");
        return Ok(());
    }
    let active = state["active"].take();
    let cancelled = active["cancelRequested"] == json!(true);
    let ok = event.exit_code == Some(0) && !event.timed_out;
    let log_line = format!("{kind} run finished (cli = {cli}, ok = {ok}, cancelled = {cancelled})");

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

    /// 输出尾部截断：小文件原样；大文件取尾部且 char boundary 安全
    #[test]
    fn output_tail_truncates() {
        assert_eq!(output_tail("hello"), "hello");
        let big = "你".repeat(OUTPUT_TAIL_BYTES); // 3 bytes/char，必然截在多字节中间
        let tail = output_tail(&big);
        assert!(tail.len() <= OUTPUT_TAIL_BYTES + 3);
        assert_eq!(tail, big[big.len() - tail.len()..]);
    }
}
