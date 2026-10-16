//! npmrc 持久换源与自定义源管理
//!
//! 改写 `{HomeDir}/.npmrc`（改前备份、UI 一键还原）；registry 行之外的内容
//! 原样保留（含 authToken 行），文件内容不落日志。自定义源 URL 进候选表前
//! 经协议白名单 + 注入字符校验，测速与换源白名单一并纳入。

use super::state::{all_sources, custom_sources, emit_and_return, now_ms, read_state, write_state};
use super::HOME;
use bedcode_plugin_api::host::{HostFs, HostLog};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

/// 自定义源 URL 校验：http(s):// 协议白名单 + 无中间空白/换行/`=`（防 npmrc 注入），
/// 返回去除首尾空白后的 URL。注意先 trim 再查控制字符：trim 会移除首尾
/// \r\n/空格（用户粘贴常见），中间残留的控制字符才是注入面
pub(crate) fn validate_custom_source_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("custom source: empty url".to_string());
    }
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err("custom source: must start with http(s)://".to_string());
    }
    if trimmed.contains([' ', '\t', '\n', '\r', '=']) {
        return Err("custom source: url contains invalid characters".to_string());
    }
    Ok(trimmed.to_string())
}
// ==================== npmrc 持久换源 ====================
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
    let target = args
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("apply-mirror: missing target"))?;
    // URL 白名单：仅接受候选源表内地址（内置 + 用户自定义），不拼接用户输入
    let target_url = all_sources(&read_state(h))
        .iter()
        .find(|(_, url)| url == target)
        .map(|(_, url)| url.clone())
        .ok_or_else(|| anyhow::anyhow!("apply-mirror: target not in mirror whitelist"))?;
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

    let (new_content, changed) = rewrite_registry(existing.as_deref().unwrap_or(""), &target_url);
    h.fs_write(&path, &new_content)
        .map_err(|e| anyhow::anyhow!("apply-mirror: write npmrc failed: {e}"))?;

    let mut state = read_state(h);
    state["mirror"]["npmrc"] = json!({
        "backupExists": h.fs_exists(&backup).unwrap_or(false),
        "fileRegistry": target_url,
        "appliedAt": now_ms(h).unwrap_or(0),
    });
    write_state(h, &state);
    h.log_info(&format!(
        "npmrc registry rewritten (changed={changed}, target={target_url})"
    ));
    emit_and_return(h, &state)
}

/// 添加自定义源：URL 校验 + 去重；成功落库并推送（测速/换源白名单自动纳入）
pub(crate) fn add_custom_source(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let url = args
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("add-custom-source: missing url"))?;
    let url = validate_custom_source_url(url).map_err(anyhow::Error::msg)?;
    let mut state = read_state(h);
    if custom_sources(&state).iter().any(|(_, u)| *u == url) {
        // 已存在：幂等返回当前状态
        return emit_and_return(h, &state);
    }
    let mut customs = custom_sources(&state);
    let id = format!("custom-{}", customs.len() + 1);
    customs.push((id, url.clone()));
    state["mirror"]["customSources"] = json!(customs
        .iter()
        .map(|(id, url)| json!({
            "id": id,
            "url": url,
            "addedAt": now_ms(h).unwrap_or(0),
        }))
        .collect::<Vec<_>>());
    write_state(h, &state);
    h.log_info(&format!(
        "custom registry source added (url_len={})",
        url.len()
    ));
    emit_and_return(h, &state)
}

/// 删除自定义源（按 url 精确匹配）
pub(crate) fn remove_custom_source(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let url = args
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("remove-custom-source: missing url"))?;
    let mut state = read_state(h);
    let kept: Vec<Value> = state["mirror"]["customSources"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|v| v["url"].as_str() != Some(url))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    state["mirror"]["customSources"] = json!(kept);
    write_state(h, &state);
    h.log_info("custom registry source removed");
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

// ==================== Tests（纯函数单测，双平台形态覆盖） ====================

#[cfg(test)]
mod tests {
    use super::super::registry::{NPMJS, NPMMIRROR};
    use super::*;

    /// 自定义源 URL 校验：http(s):// 白名单协议、无空白/换行；其余拒绝
    #[test]
    fn custom_source_url_validation() {
        assert!(validate_custom_source_url("https://registry.example.com/").is_ok());
        assert!(validate_custom_source_url("http://mirror.local:8081/npm/").is_ok());
        // 首尾空白（粘贴常见）容忍：trim 后干净 URL 写入无注入面
        assert!(validate_custom_source_url("  https://registry.example.com/  ").is_ok());
        assert!(validate_custom_source_url("https://registry.example.com/\r").is_ok());
        assert!(validate_custom_source_url("ftp://bad.example.com").is_err());
        assert!(validate_custom_source_url("registry.example.com").is_err());
        // 中间换行/`=`：npmrc 注入面，拒绝
        assert!(validate_custom_source_url("https://evil.com/\nregistry=https://x").is_err());
        assert!(validate_custom_source_url("https://evil.com/?a=b").is_err());
        assert!(validate_custom_source_url("").is_err());
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
}
