//! 本地目录导入（pick-folder + 授权 → 枚举 → fs_copy）
//!
//! 无 path 时先 pick-folder（同步对话框）+ 一次批量授权；目标同名 skill
//! 已存在且未确认覆盖时返回 exists 名单。确认后 spawn 枚举进程（异步），
//! 回灌时逐文件 fs_copy 入规范库（保留相对路径，字节级复制支持二进制）。

use super::listing::{basename, parse_listing, relativize};
use super::scan::{scan, scan_script};
use super::{emit_and_return, library_root, read_state, write_state};
use super::{RUN_SEQ, SCAN_TIMEOUT_MS};
use crate::install::now_ms;
use crate::util::{is_windows, path_rejected_for_script, shell_invocation};
use crate::{host, pending, PendingRun, DATA_DIR};
use bedcode_plugin_api::events::ProcessDoneEvent;
use bedcode_plugin_api::host::{HostFs, HostLog, HostPlatform, HostProcess};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};
use std::sync::atomic::Ordering as AtomicOrdering;

// ==================== 异步流：本地目录导入 ====================

/// 本地目录导入：无 path 时先 pick-folder（同步对话框）+ 一次批量授权；
/// 目标同名 skill 已存在且未确认覆盖时返回 exists 名单。确认后 spawn 枚举
/// 进程（异步），回灌时逐文件 fs_copy 入规范库。
pub(crate) fn import_local(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let lib_root = library_root()?;
    let path = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => {
            let picked = h
                .platform_pick_folder()
                .map_err(|e| anyhow::anyhow!("import: pick folder failed: {e}"))?;
            if picked.is_empty() {
                return Ok(json!({ "picked": false }));
            }
            picked
        }
    };
    // 规范化分隔符（Windows 反斜杠路径）后取 basename 作为入库目录名
    let normalized = path.replace('\\', "/");
    if path_rejected_for_script(&path) {
        return Err(anyhow::anyhow!(
            "import: path contains characters unsupported by scan scripts"
        ));
    }
    let Some(name) = basename(&normalized) else {
        return Err(anyhow::anyhow!(
            "import: cannot derive skill name from {path}"
        ));
    };
    if name.contains("..") {
        return Err(anyhow::anyhow!("import: invalid skill name {name}"));
    }

    // 所选目录通常不在批量授权列：一次弹窗授权（记住前缀后幂等）
    let granted = h
        .fs_request_auth(std::slice::from_ref(&path))
        .map_err(|e| anyhow::anyhow!("import: auth failed: {e}"))?;
    if !granted {
        return Ok(json!({ "picked": true, "auth": false, "path": path, "name": name }));
    }

    let exists = h
        .fs_exists(&format!("{lib_root}/{name}"))
        .map_err(|e| anyhow::anyhow!("import: exists check failed: {e}"))?;
    let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
    if exists && !force {
        return Ok(
            json!({ "picked": true, "auth": true, "path": path, "name": name, "exists": true }),
        );
    }

    start_import(h, &path, &name)
}

/// spawn 导入枚举进程（`== src ==` 单分段列举源目录全量文件）
fn start_import(h: &WasmHost, source: &str, name: &str) -> anyhow::Result<Value> {
    let script = scan_script(&[("src", source.to_string())], is_windows());
    let (command, args_vec) = shell_invocation(script, is_windows());
    let data_dir = DATA_DIR
        .get()
        .ok_or_else(|| anyhow::anyhow!("import: data dir unavailable"))?;
    let seq = RUN_SEQ.fetch_add(1, AtomicOrdering::Relaxed);
    let output_path = format!("{data_dir}/runs/skills-import-{seq}.log");
    let request = json!({
        "command": command,
        "args": args_vec,
        "output_path": output_path,
        "timeout_ms": SCAN_TIMEOUT_MS,
    });
    let run_id = h
        .process_run(&request.to_string())
        .map_err(|e| anyhow::anyhow!("import: spawn failed: {e}"))?;
    pending()
        .lock()
        .map_err(|e| anyhow::anyhow!("skills: poisoned pending map: {e}"))?
        .insert(
            run_id,
            PendingRun {
                kind: "skills-import".to_string(),
                output_path,
                cli: None,
                source: Some(format!("{source}\n{name}")),
            },
        );

    let mut state = read_state(h);
    state["importing"] = json!(true);
    write_state(h, &state);
    h.log_info(&format!("skills import started (name = {name})"));
    emit_and_return(h, &state)
}

/// 导入进程回灌：解析源目录文件清单 → 逐文件 fs_copy 入规范库（保留相对
/// 路径）→ 结果落状态 → 触发重扫描
pub(crate) fn handle_import_done(
    event: &ProcessDoneEvent,
    output_path: &str,
    source: &str,
) -> anyhow::Result<()> {
    let h = host();
    let mut state = read_state(&h);
    state["importing"] = json!(false);

    let (src_dir, name) = source
        .split_once('\n')
        .ok_or_else(|| anyhow::anyhow!("skills: import pending entry malformed"))?;

    let fail = |state: &mut Value, error: String| -> anyhow::Result<()> {
        state["import"]["last"] = json!({
            "ok": false, "name": name, "fileCount": 0, "error": error, "at": now_ms(&h).unwrap_or(0),
        });
        write_state(&h, state);
        h.log_warn(&format!("skills import failed (name = {name})"));
        emit_and_return(&h, state).map(|_| ())
    };

    if event.timed_out {
        return fail(&mut state, "import timed out".to_string());
    }
    let output = h
        .fs_read(output_path)
        .map_err(|e| anyhow::anyhow!("skills: read import output failed: {e}"))?
        .unwrap_or_default();
    let _ = h.fs_delete(output_path);

    let src_normalized = src_dir.replace('\\', "/");
    let mut rels: Vec<String> = Vec::new();
    for (sec, path) in parse_listing(&output) {
        if sec != "src" {
            continue;
        }
        if let Some(rel) = relativize(&src_normalized, &path) {
            rels.push(rel);
        }
    }
    if !rels.iter().any(|r| r == "SKILL.md") {
        return fail(&mut state, "selected directory has no SKILL.md".to_string());
    }

    let lib_root = library_root()?;
    let mut copied = 0u32;
    let mut errors: Vec<String> = Vec::new();
    for rel in &rels {
        // fs_copy 字节级复制（支持二进制），目标父目录自动创建
        match h.fs_copy(
            &format!("{src_dir}/{rel}"),
            &format!("{lib_root}/{name}/{rel}"),
        ) {
            Ok(()) => copied += 1,
            Err(e) => errors.push(format!("{rel}: {e}")),
        }
    }
    let ok = errors.is_empty();
    state["import"]["last"] = json!({
        "ok": ok,
        "name": name,
        "fileCount": copied,
        "error": if ok { Value::Null } else { json!(errors.join("; ")) },
        "at": now_ms(&h).unwrap_or(0),
    });
    write_state(&h, &state);
    h.log_info(&format!(
        "skills import done (name = {name}, copied = {copied}, errors = {})",
        errors.len()
    ));
    emit_and_return(&h, &state)?;

    // 导入内容需重扫描刷新库列表与分发状态（尽力而为）
    if let Err(e) = scan(&h) {
        h.log_warn(&format!("skills: post-import rescan failed: {e}"));
    }
    Ok(())
}
