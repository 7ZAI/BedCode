//! 规范库扫描（枚举 → 逐 skill 采集 → 分发比对）
//!
//! 扫描经 host-process 平台分派枚举规范库与各目标根（`== 分段 ==` 标记；
//! 根缺失属常态，**不以 exit code 判失败**，仅超时视为失败）；回灌时
//! 逐 skill 读 SKILL.md/frontmatter + 逐文件 hash，与各目标逐文件比对得
//! 分发状态。扫描进行中（status == scanning）拒绝重入。

use super::listing::{
    combined_hash, distribution_status, group_skills, hash_of, parse_frontmatter, parse_listing,
    relativize,
};
use super::{emit_and_return, read_state, target_roots, write_state};
use super::{LIBRARY_SEG, RUN_SEQ, SCAN_TIMEOUT_MS};
use crate::install::now_ms;
use crate::util::{is_windows, sh_quote, shell_invocation};
use crate::{host, pending, PendingRun, DATA_DIR, HOME};
use bedcode_plugin_api::events::ProcessDoneEvent;
use bedcode_plugin_api::host::{HostFs, HostLog, HostProcess};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};
use std::sync::atomic::Ordering as AtomicOrdering;

// ==================== 扫描脚本（双平台分派） ====================

/// 目录列举脚本：分段标记输出各根的文件清单。unix `find -type f`；
/// Windows `dir /s /b /a:-d`（/a:-d 排除目录项，dir /s /b 会混入目录行）。
/// 根缺失属常态：错误回显走 stderr 抑制，exit code 不作判据（超时除外）。
///
/// 安全：unix 根路径经 `sh_quote` 单引号转义（import 目录是用户可控输入，
/// 未经转义直接插双引号即可被 `$()` / 反引号注入命令）；Windows 保留双引号
/// 包裹（cmd 引号内 & | < > 按字面处理），`"` 与 `%` 已在 import_local 入口
/// 拒绝（见 `path_rejected_for_script`）。
pub(super) fn scan_script(sections: &[(&str, String)], windows: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (key, root) in sections {
        if windows {
            parts.push(format!(
                "echo == {key} == & dir /s /b /a:-d \"{root}\" 2>nul"
            ));
        } else {
            parts.push(format!(
                "echo '== {key} =='\nfind {} -type f 2>/dev/null\n",
                sh_quote(root)
            ));
        }
    }
    if windows {
        parts.join(" & ")
    } else {
        parts.join("")
    }
}
// ==================== 异步流：规范库扫描 ====================

/// 触发全量扫描（枚举三个根 → 逐 skill 读内容/frontmatter/hash → 分发比对）。
/// 扫描进行中（status == scanning）拒绝重入。
pub(crate) fn scan(h: &WasmHost) -> anyhow::Result<Value> {
    let home = HOME
        .get()
        .ok_or_else(|| anyhow::anyhow!("skills: home unavailable"))?;
    let mut state = read_state(h);
    if state["status"] == json!("scanning") {
        return emit_and_return(h, &state);
    }

    let mut sections = vec![("lib", format!("{home}/{LIBRARY_SEG}"))];
    sections.extend(target_roots(home));
    let script = scan_script(&sections, is_windows());
    let (command, args_vec) = shell_invocation(script, is_windows());

    let data_dir = DATA_DIR
        .get()
        .ok_or_else(|| anyhow::anyhow!("skills: data dir unavailable"))?;
    let seq = RUN_SEQ.fetch_add(1, AtomicOrdering::Relaxed);
    let output_path = format!("{data_dir}/runs/skills-scan-{seq}.log");
    let request = json!({
        "command": command,
        "args": args_vec,
        "output_path": output_path,
        "timeout_ms": SCAN_TIMEOUT_MS,
    });
    let run_id = h
        .process_run(&request.to_string())
        .map_err(|e| anyhow::anyhow!("skills: scan spawn failed: {e}"))?;

    pending()
        .lock()
        .map_err(|e| anyhow::anyhow!("skills: poisoned pending map: {e}"))?
        .insert(
            run_id,
            PendingRun {
                kind: "skills-scan".to_string(),
                output_path,
                cli: None,
                source: None,
            },
        );

    state["status"] = json!("scanning");
    state["error"] = json!(null);
    write_state(h, &state);
    h.log_info("skills scan started");
    emit_and_return(h, &state)
}

/// 扫描进程回灌：读列举输出 → 按 skill 归组 → 读 SKILL.md/frontmatter +
/// 逐文件 hash → 分发状态比对 → 持久化推送
pub(super) fn handle_scan_done(event: &ProcessDoneEvent, output_path: &str) -> anyhow::Result<()> {
    let h = host();
    let mut state = read_state(&h);

    if event.timed_out {
        state["status"] = json!("error");
        state["error"] = json!("scan timed out");
        write_state(&h, &state);
        h.log_warn("skills scan timed out");
        return emit_and_return(&h, &state).map(|_| ());
    }

    let output = h
        .fs_read(output_path)
        .map_err(|e| anyhow::anyhow!("skills: read scan output failed: {e}"))?
        .unwrap_or_default();
    let _ = h.fs_delete(output_path);

    let listing = parse_listing(&output);
    let home = HOME.get().map(|s| s.as_str()).unwrap_or("");

    // 各目标根存在性：列举出任意文件即存在（fs_exists 兜底，授权被拒时列举为空）
    let mut targets = target_roots(home);
    if let Some(obj) = state["targets"].as_object_mut() {
        for (name, root) in &mut targets {
            let found = listing
                .iter()
                .any(|(sec, p)| sec == name && relativize(root, p).is_some());
            let exists = found || h.fs_exists(root).unwrap_or(false);
            obj.insert(name.to_string(), json!({ "root": root, "exists": exists }));
        }
    }

    // 规范库归组 + 逐 skill 采集
    let lib_root = format!("{home}/{LIBRARY_SEG}");
    let mut lib_rels: Vec<String> = Vec::new();
    for (sec, path) in &listing {
        if sec == "lib" {
            if let Some(rel) = relativize(&lib_root, path) {
                lib_rels.push(rel);
            }
        }
    }
    let mut skills: Vec<Value> = Vec::new();
    for dir in group_skills(&lib_rels) {
        let rels: Vec<&String> = lib_rels
            .iter()
            .filter(|r| r.starts_with(&format!("{dir}/")))
            .collect();
        let skill_md_rel = format!("{dir}/SKILL.md");
        let skill_md_path = format!("{lib_root}/{skill_md_rel}");
        let md_content = h.fs_read(&skill_md_path).ok().flatten();
        let (name, description, allowed_tools) =
            parse_frontmatter(md_content.as_deref().unwrap_or(""));

        let mut entries: Vec<(String, Option<String>)> = Vec::new();
        let mut files: Vec<Value> = Vec::new();
        let dir_prefix_len = dir.len() + 1;
        for rel in &rels {
            let path = format!("{lib_root}/{rel}");
            let content = h.fs_read(&path).ok().flatten();
            let hash = hash_of(content.as_deref());
            let file_rel = &rel[dir_prefix_len..];
            entries.push((file_rel.to_string(), hash.clone()));
            files.push(json!({ "path": file_rel, "hash": hash }));
        }
        let hash = combined_hash(&entries);

        let mut entry = json!({
            "dir": dir,
            "path": format!("{lib_root}/{dir}"),
            "name": name.unwrap_or_else(|| dir.clone()),
            "description": description,
            "allowedTools": allowed_tools,
            "files": files,
            "hash": hash,
            "error": if md_content.is_some() { Value::Null } else { json!("SKILL.md unreadable") },
            "distribution": {},
        });
        recompute_distribution(&h, home, &mut entry);
        skills.push(entry);
    }

    state["status"] = json!("ready");
    state["error"] = json!(null);
    state["scannedAt"] = json!(now_ms(&h).unwrap_or(0));
    state["skills"] = json!(skills);
    write_state(&h, &state);
    h.log_info(&format!(
        "skills scan done (count = {})",
        state["skills"].as_array().map(|a| a.len()).unwrap_or(0)
    ));
    emit_and_return(&h, &state).map(|_| ())
}

/// 重算单个 skill 的分发状态（库侧文件 hash 与各目标逐文件比对）：
/// hash 可比 → 内容比对；binary（hash null）→ 存在性比对
pub(super) fn recompute_distribution(h: &WasmHost, home: &str, entry: &mut Value) {
    let Some(files) = entry["files"].as_array().cloned() else {
        return;
    };
    let dir = entry["dir"].as_str().unwrap_or("").to_string();
    let mut distribution = serde_json::Map::new();
    for (tname, troot) in target_roots(home) {
        let mut missing_files: Vec<String> = Vec::new();
        let mut stale_files: Vec<String> = Vec::new();
        for f in &files {
            let rel = f["path"].as_str().unwrap_or("");
            let target_path = format!("{troot}/{dir}/{rel}");
            match f["hash"].as_str() {
                Some(lib_hash) => match h.fs_read(&target_path) {
                    Ok(Some(content)) => {
                        if hash_of(Some(&content)).as_deref() != Some(lib_hash) {
                            stale_files.push(rel.to_string());
                        }
                    }
                    Ok(None) => missing_files.push(rel.to_string()),
                    // 目标为二进制而库侧可读：内容必然不同 → 落后
                    Err(_) => stale_files.push(rel.to_string()),
                },
                // 库侧 binary：仅存在性比对
                None => {
                    if !h.fs_exists(&target_path).unwrap_or(false) {
                        missing_files.push(rel.to_string());
                    }
                }
            }
        }
        let status = distribution_status(files.len(), missing_files.len(), stale_files.len());
        distribution.insert(
            tname.to_string(),
            json!({
                "status": status,
                "missingFiles": missing_files,
                "staleFiles": stale_files,
            }),
        );
    }
    entry["distribution"] = Value::Object(distribution);
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 双平台形态：unix find 分段；Windows dir /a:-d + 2>nul
    #[test]
    fn scan_script_platforms() {
        let sections = vec![("lib", "/home/u/.agents/skills".to_string())];
        let unix = scan_script(&sections, false);
        assert!(unix.contains("== lib =="));
        assert!(unix.contains("find '/home/u/.agents/skills' -type f 2>/dev/null"));

        let win = scan_script(&sections, true);
        assert!(win.contains("echo == lib =="));
        assert!(win.contains("dir /s /b /a:-d \"/home/u/.agents/skills\" 2>nul"));
    }

    /// 注入防护：import 来源路径进单引号后仅作为 find 参数；单引号转义不逃逸
    #[test]
    fn scan_script_quotes_import_root() {
        let unix = scan_script(&[("src", "/tmp/$(rm -rf ~);x".to_string())], false);
        // $() 序列整体被单引号包裹：脚本输出逐字节比对，证明序列未逃逸
        assert_eq!(
            unix,
            "echo '== src =='\nfind '/tmp/$(rm -rf ~);x' -type f 2>/dev/null\n"
        );

        let quoted = scan_script(&[("src", "/tmp/a'b".to_string())], false);
        assert!(quoted.contains("find '/tmp/a'\\''b' -type f 2>/dev/null"));
    }
}
