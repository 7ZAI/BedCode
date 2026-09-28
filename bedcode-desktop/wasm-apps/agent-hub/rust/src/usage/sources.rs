//! 日志来源管理（内置只读 + 自定义增删 + 每来源多目录）
//!
//! 来源 = 名称 + 若干目录（state `paths` 数组）。内置来源的默认路径只读，
//! 但允许在其上追加用户目录；自定义来源的目录与来源本身都可增删。
//! 旧状态单 `path` 字段由 `read_state` 幂等迁移为 `paths`。
//!
//! 目录路径全局唯一（同一目录只能归属一个来源）——扫描按来源名归段入库，
//! 路径重复会让同一批会话文件以两个适配器名各入一次库，统计重复。

use super::{emit_and_return, read_state, write_state, OPENCODE_ADAPTER, SESSION_ROOTS};
use crate::util::path_rejected_for_script;
use crate::{usage_sqlite, HOME};
use bedcode_plugin_api::host::HostPlatform;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

// ==================== 日志来源管理（内置只读 + 自定义增删 + 每来源多目录） ====================

/// 来源清单 + 各适配器扫描计数（state 持久化；内置条目只读）。
///
/// wire 装饰：`paths: string[]` → `paths: [{ path, removable }]`——内置默认
/// 路径不可移除（removable=false），用户追加的目录（含内置来源上追加的）
/// 可移除（removable=true）。sqlite 源是单个库文件，整行不提供增删。
pub(crate) fn list_sources(h: &WasmHost) -> anyhow::Result<Value> {
    let state = read_state(h);
    let home = HOME.get().cloned().unwrap_or_default();
    let mut out: Vec<Value> = vec![];
    if let Some(arr) = state.get("sources").and_then(|s| s.as_array()) {
        for src in arr {
            let mut s = src.clone();
            let name = s
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            if let Some(stat) = state.get("adapters").and_then(|a| a.get(name.as_str())) {
                s["scan"] = stat.clone();
            }
            if let Some(paths) = s.get("paths").and_then(|p| p.as_array()) {
                let default = builtin_default_path(&home, &name);
                s["paths"] = json!(paths
                    .iter()
                    .filter_map(|p| p.as_str())
                    .map(|p| {
                        json!({ "path": p, "removable": default.as_deref() != Some(p) })
                    })
                    .collect::<Vec<Value>>());
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
/// 返回选中绝对路径，由前端自动派生来源名后走 add-source / add-source-path 入库。
pub(crate) fn pick_source_dir(h: &WasmHost) -> anyhow::Result<Value> {
    let path = h
        .platform_pick_folder()
        .map_err(|e| anyhow::anyhow!("pick-source-dir: {e}"))?;
    Ok(json!({ "picked": !path.is_empty(), "path": path }))
}

/// 添加自定义来源：名称 + 首个目录（绝对路径或 ~/ 开头）入态，随后由前端引导扫描
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
    let home = HOME
        .get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("add-source: home unavailable"))?;
    let path = normalize_path(&raw_path, &home)?;

    let mut state = read_state(h);
    if source_named(&state, &name) {
        // 来源名已被占用（用户可见拒绝；ADR 0030 业务码）
        bedcode_plugin_api::bail_with_code!("com.bedcode.agent-hub.hub.lg.sources.error.nameTaken");
    }
    if path_registered(&state, &path) {
        // 目录已归属另一来源（用户可见拒绝；ADR 0030 业务码）
        bedcode_plugin_api::bail_with_code!("com.bedcode.agent-hub.hub.lg.sources.error.pathTaken");
    }
    // 适配器槽位 + 来源条目（首个目录即 paths[0]）
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
        arr.push(json!({ "name": name.clone(), "paths": [path], "builtin": false }));
        json!(arr)
    };
    write_state(h, &state);
    emit_and_return(h, &state)
}

/// 给既有来源（内置或自定义）追加一个目录：名称必须已登记、路径全局唯一。
/// sqlite 源是单库文件，不接受目录增删。
pub(crate) fn add_source_path(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
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
        return Err(anyhow::anyhow!("add-source-path: name and path required"));
    }
    let home = HOME
        .get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("add-source-path: home unavailable"))?;
    let path = normalize_path(&raw_path, &home)?;

    let mut state = read_state(h);
    if !source_named(&state, &name) {
        return Err(anyhow::anyhow!("add-source-path: source not found"));
    }
    if source_is_sqlite(&state, &name) {
        // sqlite 源单文件只读（用户可见拒绝；ADR 0030 业务码）
        bedcode_plugin_api::bail_with_code!("com.bedcode.agent-hub.hub.lg.sources.error.sqliteReadonly");
    }
    if path_registered(&state, &path) {
        // 目录已归属另一来源（用户可见拒绝；ADR 0030 业务码）
        bedcode_plugin_api::bail_with_code!("com.bedcode.agent-hub.hub.lg.sources.error.pathTaken");
    }
    let sources = state
        .get_mut("sources")
        .and_then(|s| s.as_array_mut())
        .ok_or_else(|| anyhow::anyhow!("add-source-path: sources state missing"))?;
    for src in sources.iter_mut() {
        if src.get("name").and_then(|n| n.as_str()) != Some(name.as_str()) {
            continue;
        }
        let mut paths = entry_paths(src);
        paths.push(path);
        src["paths"] = json!(paths);
        break;
    }
    write_state(h, &state);
    emit_and_return(h, &state)
}

/// 从来源移除一个目录（内置默认路径不可移除；自定义来源最后一条路径
/// 需整体移除来源，而不是留一个空壳来源）。
pub(crate) fn remove_source_path(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
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
        return Err(anyhow::anyhow!("remove-source-path: name and path required"));
    }
    let home = HOME
        .get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("remove-source-path: home unavailable"))?;
    let path = normalize_path(&raw_path, &home)?;

    let mut state = read_state(h);
    let sources = state
        .get_mut("sources")
        .and_then(|s| s.as_array_mut())
        .ok_or_else(|| anyhow::anyhow!("remove-source-path: sources state missing"))?;
    let mut found = false;
    for src in sources.iter_mut() {
        if src.get("name").and_then(|n| n.as_str()) != Some(name.as_str()) {
            continue;
        }
        found = true;
        let mut paths = entry_paths(src);
        if !paths.iter().any(|p| p == &path) {
            return Err(anyhow::anyhow!("remove-source-path: path not in source"));
        }
        if builtin_default_path(&home, &name).as_deref() == Some(path.as_str()) {
            // 内置默认目录不可移除（用户可见拒绝；ADR 0030 业务码）
            bedcode_plugin_api::bail_with_code!(
                "com.bedcode.agent-hub.hub.lg.sources.error.builtinProtected"
            );
        }
        if paths.len() == 1 {
            // 最后一条目录拒绝单独移除（用户可见拒绝；ADR 0030 业务码）
            bedcode_plugin_api::bail_with_code!(
                "com.bedcode.agent-hub.hub.lg.sources.error.lastPathProtected"
            );
        }
        paths.retain(|p| p != &path);
        src["paths"] = json!(paths);
        break;
    }
    if !found {
        return Err(anyhow::anyhow!("remove-source-path: source not found"));
    }
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

// ==================== 纯函数（迁移 / 校验 / 查询，命令入口共用，可测） ====================

/// 旧状态单 `path` → `paths` 数组迁移（幂等：已有非空 `paths` 的条目不动，
/// 空目录条目补空数组）；顺带剔除遗留 `path` 字段，保持单形态。
pub(super) fn normalize_sources_paths(arr: Vec<Value>) -> Value {
    let out: Vec<Value> = arr
        .into_iter()
        .map(|mut s| {
            let has_paths = s
                .get("paths")
                .and_then(|p| p.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            if !has_paths {
                let single = s.get("path").and_then(|p| p.as_str()).map(String::from);
                s["paths"] = json!(single.map(|p| vec![p]).unwrap_or_default());
            }
            if let Some(obj) = s.as_object_mut() {
                obj.remove("path");
            }
            s
        })
        .collect();
    json!(out)
}

/// 路径校验 + ~ 展开为绝对路径（add_source / add_source_path / remove_source_path 共用）。
/// 拒绝非绝对路径与脚本不安全字符（与扫描脚本双平台安全底线一致）。
fn normalize_path(raw: &str, home: &str) -> anyhow::Result<String> {
    let path = if let Some(rest) = raw.strip_prefix("~/") {
        format!("{home}/{rest}")
    } else {
        raw.to_string()
    };
    if !path.starts_with('/') {
        return Err(anyhow::anyhow!(
            "path must be absolute (or start with ~/)"
        ));
    }
    if path_rejected_for_script(&path) {
        return Err(anyhow::anyhow!(
            "path contains characters unsupported by scan scripts"
        ));
    }
    Ok(path)
}

/// 收集来源条目的全部目录（state 内 paths 数组；防御旧形态单 path 兜底）
pub(super) fn entry_paths(src: &Value) -> Vec<String> {
    if let Some(arr) = src.get("paths").and_then(|p| p.as_array()) {
        let ps: Vec<String> = arr.iter().filter_map(|p| p.as_str().map(String::from)).collect();
        if !ps.is_empty() {
            return ps;
        }
    }
    src.get("path")
        .and_then(|p| p.as_str())
        .map(|p| vec![p.to_string()])
        .unwrap_or_default()
}

/// state 的 sources 数组中是否已有同名来源
fn source_named(state: &Value, name: &str) -> bool {
    state
        .get("sources")
        .and_then(|s| s.as_array())
        .map(|arr| arr.iter().any(|s| s.get("name").and_then(|n| n.as_str()) == Some(name)))
        .unwrap_or(false)
}

/// 指定来源是否为 sqlite 单文件源（不可目录增删）
fn source_is_sqlite(state: &Value, name: &str) -> bool {
    state
        .get("sources")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter().any(|s| {
                s.get("name").and_then(|n| n.as_str()) == Some(name)
                    && s.get("kind").and_then(|k| k.as_str()) == Some("sqlite")
            })
        })
        .unwrap_or(false)
}

/// 目录是否已被任一来源登记（全局唯一；同一目录归属两个来源会让同一批
/// 会话文件以两个适配器名各入一次库）
fn path_registered(state: &Value, path: &str) -> bool {
    state
        .get("sources")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter().any(|src| entry_paths(src).iter().any(|p| p == path))
        })
        .unwrap_or(false)
}

/// 内置来源的默认路径（不可移除）；自定义来源 → None（全部路径可移除）。
/// sqlite 源（opencode）也是内置单文件，默认路径即库文件。
pub(super) fn builtin_default_path(home: &str, name: &str) -> Option<String> {
    for (n, seg) in SESSION_ROOTS {
        if n == name {
            return Some(format!("{home}/{seg}"));
        }
    }
    if name == OPENCODE_ADAPTER {
        return Some(usage_sqlite::db_path(home));
    }
    None
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

    /// 旧状态单 path → paths 数组迁移：已有 paths 不动，缺 paths 用 path 兜底，
    /// 遗留 path 字段被剔除
    #[test]
    fn normalize_paths_legacy_single_to_array() {
        let legacy = json!([
            { "name": "pi", "path": "/home/u/.pi/agent/sessions", "builtin": true },
            { "name": "my-logs", "path": "/data/logs", "builtin": false },
        ]);
        let arr = legacy.as_array().cloned().unwrap();
        let out = normalize_sources_paths(arr);
        let arr = out.as_array().unwrap();
        assert_eq!(arr[0]["paths"], json!(["/home/u/.pi/agent/sessions"]));
        assert_eq!(arr[0]["path"], Value::Null, "遗留 path 字段应剔除");
        assert_eq!(arr[1]["paths"], json!(["/data/logs"]));
    }

    /// 幂等：已是 paths 数组的条目原样保留（含多目录与空目录）
    #[test]
    fn normalize_paths_idempotent() {
        let modern = json!([
            { "name": "pi", "paths": ["/home/u/.pi/agent/sessions", "/extra"], "builtin": true },
            { "name": "x", "paths": [], "builtin": false },
        ]);
        let out = normalize_sources_paths(modern.as_array().cloned().unwrap());
        let arr = out.as_array().unwrap();
        assert_eq!(arr[0]["paths"], json!(["/home/u/.pi/agent/sessions", "/extra"]));
        assert_eq!(arr[1]["paths"], json!([]));
    }

    /// 路径校验：~ 展开 / 非绝对路径拒绝 / 脚本不安全字符拒绝
    #[test]
    fn path_normalization_rules() {
        assert_eq!(
            normalize_path("~/pi/sessions", "/home/u").unwrap(),
            "/home/u/pi/sessions"
        );
        assert_eq!(normalize_path("/abs/path", "/home/u").unwrap(), "/abs/path");
        // 非绝对路径（相对 / Windows 盘符在 wasm 宿主不成立）
        assert!(normalize_path("rel/path", "/home/u").is_err());
        assert!(normalize_path("", "/home/u").is_err());
        // 双引号 / % / 控制字符进扫描脚本不安全 → 拒绝
        assert!(normalize_path("/a/\"b\"", "/home/u").is_err());
        assert!(normalize_path("/a/%b", "/home/u").is_err());
    }

    /// entry_paths：paths 数组优先；旧单 path 兜底；空 → 空列表
    #[test]
    fn entry_paths_shape() {
        assert_eq!(
            entry_paths(&json!({ "paths": ["/a", "/b"] })),
            vec!["/a".to_string(), "/b".to_string()]
        );
        assert_eq!(
            entry_paths(&json!({ "path": "/legacy" })),
            vec!["/legacy".to_string()]
        );
        assert!(entry_paths(&json!({})).is_empty());
    }

    /// 全局路径唯一：同一目录已归属任一来源时拒绝（防同一批文件双适配器入库）
    #[test]
    fn path_uniqueness_across_sources() {
        let state = json!({
            "sources": [
                { "name": "pi", "paths": ["/home/u/.pi/agent/sessions"], "builtin": true },
                { "name": "my-logs", "paths": ["/data/logs"], "builtin": false },
            ]
        });
        assert!(path_registered(&state, "/home/u/.pi/agent/sessions"));
        assert!(path_registered(&state, "/data/logs"));
        assert!(!path_registered(&state, "/home/u/.pi/other"));
    }

    /// 内置默认路径：JSONL 根与 opencode 库文件不可移除；自定义来源无默认
    #[test]
    fn builtin_default_path_rules() {
        let home = "/home/u";
        assert_eq!(
            builtin_default_path(home, "pi").as_deref(),
            Some("/home/u/.pi/agent/sessions")
        );
        assert_eq!(
            builtin_default_path(home, "claude").as_deref(),
            Some("/home/u/.claude/projects")
        );
        assert_eq!(
            builtin_default_path(home, "opencode").as_deref(),
            Some("/home/u/.local/share/opencode/opencode.db")
        );
        assert_eq!(builtin_default_path(home, "codex").as_deref(), Some("/home/u/.codex/sessions"));
        assert_eq!(builtin_default_path(home, "my-logs"), None);
    }
}
