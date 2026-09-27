//! 日志来源管理（内置只读 + 自定义增删）
//!
//! 来源清单 + 各适配器扫描计数持久化在 state（`sources` 键，旧状态缺键时
//! 幂等补齐）；自定义来源名称经合法性校验、路径经 ~ 展开 + 脚本安全拒绝。

use super::{emit_and_return, read_state, write_state};
use crate::util::path_rejected_for_script;
use crate::HOME;
use bedcode_plugin_api::host::HostPlatform;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

// ==================== 日志来源管理（内置只读 + 自定义增删） ====================

/// 来源清单 + 各适配器扫描计数（state 持久化；内置条目只读）
pub(crate) fn list_sources(h: &WasmHost) -> anyhow::Result<Value> {
    let state = read_state(h);
    let mut out: Vec<Value> = vec![];
    if let Some(arr) = state.get("sources").and_then(|s| s.as_array()) {
        for src in arr {
            let mut s = src.clone();
            if let Some(name) = s.get("name").and_then(|n| n.as_str()) {
                if let Some(stat) = state.get("adapters").and_then(|a| a.get(name)) {
                    s["scan"] = stat.clone();
                }
            }
            out.push(s);
        }
    }
    Ok(json!({ "sources": out }))
}

/// 来源名合法性：小写字母开头，字母/数字/连字符，≤ 32
fn is_valid_source_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && name.len() <= 32
}

/// 系统文件夹选择器选日志来源目录（fs:pick 权限门与选中路径授权校验都在
/// 宿主 `host-platform.pick-folder`：用户取消返回空串，授权拒绝直接 Err）。
/// 返回选中绝对路径，由前端自动派生来源名后走 add-source 入库。
pub(crate) fn pick_source_dir(h: &WasmHost) -> anyhow::Result<Value> {
    let path = h
        .platform_pick_folder()
        .map_err(|e| anyhow::anyhow!("pick-source-dir: {e}"))?;
    Ok(json!({ "picked": !path.is_empty(), "path": path }))
}

/// 添加自定义来源：名称 + 目录（绝对路径或 ~/ 开头）入态，随后由前端引导扫描
pub(crate) fn add_source(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let raw_path = args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() || raw_path.is_empty() {
        return Err(anyhow::anyhow!("add-source: name and path required"));
    }
    if !is_valid_source_name(&name) {
        return Err(anyhow::anyhow!(
            "add-source: invalid name (lowercase letters / digits / hyphen)"
        ));
    }
    // ~ 展开为绝对路径（与 builtin path 展示形态一致，便于去重）
    let path = if let Some(rest) = raw_path.strip_prefix("~/") {
        let home = HOME
            .get()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("add-source: home unavailable"))?;
        format!("{home}/{rest}")
    } else {
        raw_path
    };
    if !path.starts_with('/') {
        return Err(anyhow::anyhow!(
            "add-source: path must be absolute (or start with ~/)"
        ));
    }
    if path_rejected_for_script(&path) {
        return Err(anyhow::anyhow!(
            "add-source: path contains characters unsupported by scan scripts"
        ));
    }

    let mut state = read_state(h);
    let dup = state
        .get("sources")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter().any(|s| {
                s.get("name").and_then(|n| n.as_str()) == Some(name.as_str())
                    || s.get("path").and_then(|p| p.as_str()) == Some(path.as_str())
            })
        })
        .unwrap_or(false);
    if dup {
        return Err(anyhow::anyhow!(
            "add-source: name or path already registered"
        ));
    }
    // 适配器槽位 + 来源条目
    if let Some(adapters) = state.get_mut("adapters").and_then(|a| a.as_object_mut()) {
        adapters.insert(
            name.clone(),
            json!({ "files": 0, "parsed": 0, "skipped": 0, "sessions": 0, "error": null }),
        );
    }
    state["sources"] = {
        let mut arr = state
            .get("sources")
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default();
        arr.push(json!({ "name": name.clone(), "path": path, "builtin": false }));
        json!(arr)
    };
    write_state(h, &state);
    emit_and_return(h, &state)
}

/// 删除自定义来源（内置只读拒绝）；移出来源清单并清理适配器槽位
pub(crate) fn remove_source(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        return Err(anyhow::anyhow!("remove-source: name required"));
    }
    let mut state = read_state(h);
    let builtin = state
        .get("sources")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter().any(|s| {
                s.get("name").and_then(|n| n.as_str()) == Some(name.as_str())
                    && s.get("builtin").and_then(|b| b.as_bool()) == Some(true)
            })
        })
        .unwrap_or(false);
    if builtin {
        return Err(anyhow::anyhow!(
            "remove-source: builtin sources cannot be removed"
        ));
    }
    state["sources"] = json!(state
        .get("sources")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|s| s.get("name").and_then(|n| n.as_str()) != Some(name.as_str()))
                .cloned()
                .collect::<Vec<Value>>()
        })
        .unwrap_or_default());
    if let Some(adapters) = state.get_mut("adapters").and_then(|a| a.as_object_mut()) {
        adapters.remove(&name);
    }
    write_state(h, &state);
    emit_and_return(h, &state)
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 来源名合法性：小写字母开头 / 字母数字连字符 / 超长拒绝
    #[test]
    fn source_name_validation() {
        assert!(is_valid_source_name("opencode"));
        assert!(is_valid_source_name("my-logs2"));
        assert!(!is_valid_source_name(""));
        assert!(!is_valid_source_name("MyLog"));
        assert!(!is_valid_source_name("2logs"));
        assert!(!is_valid_source_name("logs!/x"));
        assert!(!is_valid_source_name(&"a".repeat(40)));
    }
}
