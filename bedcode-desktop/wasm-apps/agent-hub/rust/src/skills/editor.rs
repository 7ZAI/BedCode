//! 读 / 保存 / 分发命令
//!
//! 保存前重读做冲突检测（baseContent 与磁盘现状比对；WIT 无 mtime 原语，
//! 内容比对严格更强）；分发 = 库侧逐文件 fs_copy 到目标（自动建父目录、
//! 字节级复制支持二进制），逐文件容错并重算分发状态。

use super::listing::{combined_hash, hash_of, parse_frontmatter};
use super::scan::recompute_distribution;
use super::{emit_and_return, library_root, read_state, target_roots, write_state, TARGET_SEGS};
use crate::HOME;
use bedcode_plugin_api::host::{HostFs, HostLog};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

// ==================== 命令：读 / 保存 / 分发 ====================

/// 读取 skill 的 SKILL.md 内容（编辑器装载）
pub(crate) fn read_skill(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let dir = args
        .get("dir")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("read-skill: missing dir"))?;
    // dir 为库根下单段目录名：拒绝路径分隔符，防拼接逃逸
    if dir.is_empty() || dir.contains('/') || dir.contains('\\') || dir.contains("..") {
        return Err(anyhow::anyhow!("read-skill: invalid dir"));
    }
    let lib_root = library_root()?;
    let path = format!("{lib_root}/{dir}/SKILL.md");
    let content = h
        .fs_read(&path)
        .map_err(|e| anyhow::anyhow!("read-skill: read failed: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("read-skill: SKILL.md missing for {dir}"))?;
    let (name, description, allowed_tools) = parse_frontmatter(&content);
    Ok(json!({
        "dir": dir,
        "name": name.unwrap_or_else(|| dir.to_string()),
        "description": description,
        "allowedTools": allowed_tools,
        "content": content,
    }))
}

/// 保存 SKILL.md：保存前重读做冲突检测（baseContent 与磁盘现状比对）；
/// 冲突时返回磁盘内容（force = 用户确认覆盖）。成功后重算该 skill 的
/// frontmatter/hash/分发状态并推送。
pub(crate) fn save_skill(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let dir = args
        .get("dir")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("save-skill: missing dir"))?;
    if dir.is_empty() || dir.contains('/') || dir.contains('\\') || dir.contains("..") {
        return Err(anyhow::anyhow!("save-skill: invalid dir"));
    }
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("save-skill: missing content"))?;
    let base = args
        .get("baseContent")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

    let lib_root = library_root()?;
    let path = format!("{lib_root}/{dir}/SKILL.md");
    let current = h
        .fs_read(&path)
        .map_err(|e| anyhow::anyhow!("save-skill: read failed: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("save-skill: SKILL.md missing for {dir}"))?;

    if !force && current != base {
        h.log_warn(&format!("save-skill: conflict detected (dir = {dir})"));
        return Ok(json!({ "saved": false, "conflict": true, "current": current }));
    }

    h.fs_write(&path, content)
        .map_err(|e| anyhow::anyhow!("save-skill: write failed: {e}"))?;

    // 重算该 skill 条目（frontmatter 可能随编辑变化）+ 分发状态
    let home = HOME.get().map(|s| s.as_str()).unwrap_or("").to_string();
    let mut state = read_state(h);
    if let Some(skills) = state["skills"].as_array_mut() {
        if let Some(entry) = skills.iter_mut().find(|s| s["dir"] == json!(dir)) {
            let (name, description, allowed_tools) = parse_frontmatter(content);
            entry["name"] = json!(name.unwrap_or_else(|| dir.to_string()));
            entry["description"] = json!(description);
            entry["allowedTools"] = json!(allowed_tools);
            let files = entry["files"].as_array().cloned().unwrap_or_default();
            let mut entries: Vec<(String, Option<String>)> = Vec::new();
            let mut new_files: Vec<Value> = Vec::new();
            for f in &files {
                let rel = f["path"].as_str().unwrap_or("").to_string();
                let hash = if rel == "SKILL.md" {
                    hash_of(Some(content))
                } else {
                    let c = h.fs_read(&format!("{lib_root}/{dir}/{rel}")).ok().flatten();
                    hash_of(c.as_deref())
                };
                entries.push((rel.clone(), hash.clone()));
                new_files.push(json!({ "path": rel, "hash": hash }));
            }
            entry["files"] = json!(new_files);
            entry["hash"] = json!(combined_hash(&entries));
            recompute_distribution(h, &home, entry);
        }
    }
    write_state(h, &state);
    h.log_info(&format!("save-skill: saved (dir = {dir})"));
    emit_and_return(h, &state)
}

/// 分发（重新分发同路径）：库侧逐文件 fs_copy 到目标（fs_copy 自动建父目录，
/// 字节级复制支持二进制）。逐文件容错：失败的文件记录在 errors，成功后重算
/// 分发状态。targets 缺省 = 全部白名单目标。
pub(crate) fn distribute(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let dir = args
        .get("dir")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("distribute: missing dir"))?;
    if dir.is_empty() || dir.contains('/') || dir.contains('\\') || dir.contains("..") {
        return Err(anyhow::anyhow!("distribute: invalid dir"));
    }
    let wanted: Vec<String> = match args.get("targets").and_then(|v| v.as_array()) {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        None => TARGET_SEGS.iter().map(|(n, _)| n.to_string()).collect(),
    };
    for t in &wanted {
        if !TARGET_SEGS.iter().any(|(n, _)| n == t) {
            return Err(anyhow::anyhow!("distribute: unknown target {t}"));
        }
    }

    let home = HOME.get().map(|s| s.as_str()).unwrap_or("").to_string();
    let lib_root = library_root()?;
    let mut state = read_state(h);
    let Some(entry) = state["skills"]
        .as_array()
        .and_then(|skills| skills.iter().find(|s| s["dir"] == json!(dir)))
        .cloned()
    else {
        return Err(anyhow::anyhow!(
            "distribute: unknown skill {dir}（请先扫描）"
        ));
    };
    let files = entry["files"].as_array().cloned().unwrap_or_default();

    let mut copied = 0u32;
    let mut errors: Vec<String> = Vec::new();
    for (tname, troot) in target_roots(&home) {
        if !wanted.contains(&tname.to_string()) {
            continue;
        }
        for f in &files {
            let rel = f["path"].as_str().unwrap_or("");
            let src = format!("{lib_root}/{dir}/{rel}");
            let dst = format!("{troot}/{dir}/{rel}");
            match h.fs_copy(&src, &dst) {
                Ok(()) => copied += 1,
                Err(e) => errors.push(format!("{tname}/{rel}: {e}")),
            }
        }
    }

    // 重算该 skill 分发状态（复制后库/目标内容一致的部分转为 distributed）
    if let Some(skills) = state["skills"].as_array_mut() {
        if let Some(entry) = skills.iter_mut().find(|s| s["dir"] == json!(dir)) {
            recompute_distribution(h, &home, entry);
        }
    }
    write_state(h, &state);
    h.log_info(&format!(
        "distribute done (dir = {dir}, copied = {copied}, errors = {})",
        errors.len()
    ));
    let mut result = emit_and_return(h, &state)?;
    result["copied"] = json!(copied);
    result["errors"] = json!(errors);
    Ok(result)
}
