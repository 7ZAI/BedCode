//! 使用统计与会话日志域（票据 06 + 增补：正在使用的项目会话；票据 07：opencode
//! SQLite 适配 + codex 预留骨架 + 数据清空）
//!
//! 数据流：`scan-usage` 以 AUTH_KEY 为闸门（fs_auth 第三层按路径弹窗，
//! 未授权时扫描会引发弹窗风暴，故整体降级为 auth-required）→ host-process
//! 枚举各家 JSONL + opencode SQLite 源同步 → 回灌解析入库 → 状态推送。
//!
//! # 模块结构
//! - [`schema`]：幂等建表（parse_watermark / usage_session / provider_preset）
//! - [`scan`]：扫描异步流（枚举脚本 → 回灌 → 并行读文件）
//! - [`ingest`]：适配器分派 + 水位/会话落库 + opencode SQLite 同步
//! - [`active`]：正在使用的项目会话（claude 配置权威 + 最新会话回退）
//! - [`stats`]：多维看板聚合（汇总 + 按天 / CLI / 模型 / 项目 / 7×24 节奏矩阵）
//! - [`sessions`]：会话列表与日志视图（事件流 + 原始行）
//! - [`sources`]：日志来源管理（内置只读 + 自定义增删）
//! - [`clear`]：数据清空（同一事务删会话 + 水位）
//! - 本模块：域状态（host-storage `usage` 键，读-改-写 + 全量推送）+ 幂等补齐
//!
//! 幂等：水位未变的文件整文件跳过；水位变更则整文件重解析并按
//! `UNIQUE(adapter, cli_session_id)` 先 UPDATE 后 INSERT。扫描进行中
//! （status == syncing）拒绝重入。**数据保留策略**：全量保留（不自动过期）
//! + 手动清空（`usage_session` 与 `parse_watermark` 在**同一事务**内清）。

mod active;
mod clear;
mod ingest;
mod scan;
mod schema;
mod sessions;
mod sources;
mod stats;

/// 命令入口面（lib.rs 路由）
pub(crate) use clear::clear_data;
pub(crate) use scan::{handle_scan_done, scan};
pub(crate) use schema::ensure_schema;
pub(crate) use sessions::{list_sessions, read_session};
pub(crate) use sources::{
    add_source, add_source_path, list_sources, pick_source_dir, remove_source, remove_source_path,
};
pub(crate) use stats::get_stats;

use super::{AUTH_KEY, HOME};
use crate::install::now_ms;
use bedcode_plugin_api::host::{HostEvents, HostLog, HostStorage};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

/// host-storage 键：使用统计域状态
pub(crate) const USAGE_KEY: &str = "usage";
/// 适配器清单（票据 06：claude / pi；票据 07 补 opencode SQLite + codex 骨架）。
/// 顺序即概览/看板的默认展示序，与 `types.ts` 的 `CliId` 一致。
pub(super) const ADAPTERS: [&str; 4] = ["claude", "codex", "opencode", "pi"];
/// (adapter, 家目录相对段)：JSONL 会话根目录
///
/// codex 官方 rollout 路径（母 spec §9 标「待实机校准」）：`~/.codex/sessions`
/// 下的 `YYYY/MM/DD/rollout-*.jsonl`，递归 `find` 覆盖三层嵌套。
pub(super) const SESSION_ROOTS: [(&str, &str); 3] = [
    ("claude", ".claude/projects"),
    ("codex", ".codex/sessions"),
    ("pi", ".pi/agent/sessions"),
];
/// opencode 的 SQLite 库（不进文件枚举，见模块头）
pub(super) const OPENCODE_ADAPTER: &str = "opencode";
/// 枚举超时：纯文件系统遍历（与 skills 扫描同量级）
pub(super) const SCAN_TIMEOUT_MS: u64 = 30_000;
/// 扫描在途无进展的判定窗口（2026-09-28 实机 bug 的兜底）
///
/// 实机症状：扫描发起后回调丢失（宿主未投递 `on_process_done` / guest 中途异常），
/// 状态永远停在 `syncing`——按钮卡「扫描中…」且禁用，`scan` 的重入闸门又
/// 拒绝再次发起，**重启应用也解不开**（status 是持久化的）。故按「距上次
/// 进展心跳多久」判定失活：超窗即落 `error`，状态复原、按钮可重试。
///
/// 窗口取枚举超时的 4 倍：回灌阶段每片（≤32 文件）刷新一次心跳，正常扫描
/// 不会触碰这条线；而一次卡死的扫描必须在窗口内被治愈，否则等于没修。
pub(super) const SCAN_STALE_MS: u64 = SCAN_TIMEOUT_MS * 4;
/// 按项目 / 按模型汇总表行数上限（看板展示面）
pub(super) const BREAKDOWN_LIMIT: usize = 20;
/// 原始 JSONL 行回显上限（与事件流同量级防御）
pub(super) const RAW_LINE_CAP: usize = 5000;
/// 会话明细列表默认分页大小
pub(crate) const PAGE_SIZE: i64 = 50;

// ==================== 状态（读-改-写） ====================

/// 内置来源（(adapter, 家目录相对段)）→ 家目录绝对路径条目（只读）
///
/// opencode 是 SQLite 源而非 JSONL 目录，条目带 `kind: "sqlite"` 与展开后的
/// 库文件路径（前端据此提示「需要 sqlite3」而不是把它当可添加/移除的目录）。
/// 每个条目带 `paths` 数组（首个元素 = 内置默认路径，只读；用户可在其上追加
/// 更多目录）。
fn builtin_sources() -> Vec<Value> {
    let home = HOME.get().cloned().unwrap_or_default();
    let mut out: Vec<Value> = SESSION_ROOTS
        .iter()
        .map(|(name, seg)| {
            json!({ "name": name, "paths": [format!("{home}/{seg}")], "kind": "jsonl", "builtin": true })
        })
        .collect();
    out.push(json!({
        "name": OPENCODE_ADAPTER,
        "paths": [crate::usage_sqlite::db_path(&home)],
        "kind": "sqlite",
        "builtin": true,
    }));
    out
}

pub(super) fn default_state() -> Value {
    let mut adapters = serde_json::Map::new();
    for name in ADAPTERS {
        adapters.insert(
            name.to_string(),
            json!({ "files": 0, "parsed": 0, "skipped": 0, "sessions": 0, "error": null }),
        );
    }
    json!({
        "status": "idle",
        "error": null,
        "syncedAt": null,
        // 扫描在途心跳（epoch ms）：发起时与回灌每片刷新，stale 守卫据此判失活
        "scanStartedAt": null,
        "authGranted": false,
        "adapters": Value::Object(adapters),
        // 日志来源清单（内置只读 + 自定义增删）；旧状态无此键由 read_state 补齐
        "sources": json!(builtin_sources()),
        // 正在使用的项目会话（扫描时计算：claude 配置权威 / 其余最新会话）
        "activeSessions": json!({}),
    })
}

pub(super) fn read_state(h: &WasmHost) -> Value {
    let mut state = h
        .storage_get(USAGE_KEY)
        .ok()
        .flatten()
        .filter(|s| s.get("adapters").is_some())
        .unwrap_or_else(default_state);
    // 票 06 旧状态无 sources：增量注入内置来源（新能力对旧状态兼容）
    if !state.get("sources").is_some() {
        state["sources"] = json!(builtin_sources());
    }
    // 票 07 旧状态的两条来源缺 codex/opencode 条目：按名补齐（不重建整表，
    // 否则会丢用户已添加的自定义来源）
    if let Some(arr) = state["sources"].as_array().cloned() {
        let mut merged = arr;
        for want in builtin_sources() {
            let name = want.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let has = merged
                .iter()
                .any(|s| s.get("name").and_then(|n| n.as_str()) == Some(name));
            if !has {
                merged.push(want);
            }
        }
        state["sources"] = json!(merged);
    }
    // 多目录（票 XX）：旧状态单 path → paths 数组幂等迁移（内置/自定义统一）
    if let Some(arr) = state["sources"].as_array().cloned() {
        state["sources"] = sources::normalize_sources_paths(arr);
    }
    // 票 07 旧状态的 adapters 只有 claude/pi：补齐新槽位（保留已有计数）
    if let Some(adapters) = state["adapters"].as_object_mut() {
        for name in ADAPTERS {
            if !adapters.contains_key(name) {
                adapters.insert(
                    name.to_string(),
                    json!({ "files": 0, "parsed": 0, "skipped": 0, "sessions": 0, "error": null }),
                );
            }
        }
    }
    // 票 06 增补前旧状态无 activeSessions：默认空映射（下次扫描触发计算）
    if !state.get("activeSessions").is_some() {
        state["activeSessions"] = json!({});
    }
    // home 每次以运行时值为准（前端项目路径 ~ 折叠用）
    state["home"] = json!(HOME.get().cloned().unwrap_or_default());
    state
}

pub(super) fn write_state(h: &WasmHost, state: &Value) {
    if let Err(e) = h.storage_set(USAGE_KEY, state) {
        h.log_warn(&format!("usage: persist state failed: {e}"));
    }
}

/// 全量状态推送前端（命令返回值与事件载荷同形）
pub(super) fn emit_and_return(h: &WasmHost, state: &Value) -> anyhow::Result<Value> {
    h.emit_event("plugin:agent-hub:usage", state);
    Ok(json!({ "state": state }))
}

/// 授权标记是否为 granted（activate 批量申请 / request-auth 命令写入）
pub(super) fn auth_granted(h: &WasmHost) -> bool {
    h.storage_get(AUTH_KEY)
        .ok()
        .flatten()
        .map(|v| v == json!("granted"))
        .unwrap_or(false)
}

/// 当前域状态（前端挂载首拉；状态本身已在推送/落库前以 authGranted 实时化）
pub(crate) fn get_state(h: &WasmHost) -> anyhow::Result<Value> {
    let mut state = read_state(h);
    state["authGranted"] = json!(auth_granted(h));
    // 失活扫描在**读侧**治愈：这是「按一下重新打开面板就能重试」的那条路
    heal_stale_scan(h, &mut state);
    Ok(json!({ "state": state }))
}

// ==================== 扫描失活守卫（2026-09-28） ====================

/// 扫描是否已失活（纯函数，可测）：`status == syncing` 且距上次心跳超窗
///
/// **无心跳时间戳的 syncing 一律判失活**：新代码每次进入 syncing 都会写
/// `scanStartedAt`，所以「有 syncing 却无心跳」只可能来自旧版本落库或
/// 写盘前崩溃——那正是实机卡死的形态，放它继续卡着没有意义。
pub(super) fn scan_is_stale(state: &Value, now_ms: u64) -> bool {
    if state["status"] != json!("syncing") {
        return false;
    }
    match state.get("scanStartedAt").and_then(|v| v.as_u64()) {
        None => true,
        Some(started) => now_ms.saturating_sub(started) > SCAN_STALE_MS,
    }
}

/// 失活扫描落 `error` 并推前端（返回是否发生了治愈）
///
/// fail-visible：状态绝不允许无限期停在 `syncing`——那会让「扫描中…」成为
/// 不可撤销的假状态（用户既看不到失败也点不动重试）。
pub(super) fn heal_stale_scan(h: &WasmHost, state: &mut Value) -> bool {
    if !scan_is_stale(state, now_ms(h).unwrap_or(0)) {
        return false;
    }
    state["status"] = json!("error");
    state["error"] = json!("scan-interrupted");
    state["scanStartedAt"] = json!(null);
    write_state(h, state);
    h.log_warn("usage: scan abandoned (no progress), state reset to error");
    let _ = emit_and_return(h, state);
    true
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::sources::normalize_sources_paths;

    /// 状态默认形状：四内置适配器槽位 + 四内置来源齐备（自定义来源待添加）
    #[test]
    fn default_state_shape() {
        let s = default_state();
        assert_eq!(s["status"], "idle");
        for name in ["claude", "codex", "opencode", "pi"] {
            assert!(s["adapters"][name].is_object(), "缺少适配器槽位 {name}");
        }
        let sources = s["sources"].as_array().expect("sources array");
        assert_eq!(sources.len(), 4, "三家 JSONL 目录 + opencode SQLite 库");
        assert!(sources.iter().all(|x| x["builtin"] == json!(true)));
        // opencode 是 SQLite 源：条目带 kind=sqlite，paths 是展开后的库文件
        let oc = sources
            .iter()
            .find(|x| x["name"] == json!("opencode"))
            .expect("opencode source");
        assert_eq!(oc["kind"], json!("sqlite"));
        assert!(oc["paths"][0].as_str().unwrap_or("").ends_with("opencode.db"));
        // 三家 JSONL 源显式标 kind（前端据此区分「可添加/移除的目录」）
        let jsonl: Vec<&Value> = sources
            .iter()
            .filter(|x| x["kind"] == json!("jsonl"))
            .collect();
        assert_eq!(jsonl.len(), 3);
        // 内置源每条都带 paths 数组（首个元素为默认路径）
        assert!(sources.iter().all(|x| x["paths"].as_array().map(|a| a.len() == 1).unwrap_or(false)));
        // 正在使用的项目会话：默认空映射（扫描收尾重算）
        assert!(s["activeSessions"]
            .as_object()
            .map(|o| o.is_empty())
            .unwrap_or(false));
    }

    /// 旧状态升级（票 06 的库/存储）必须补齐新槽位与新来源，且**不丢**
    /// 用户已添加的自定义来源
    #[test]
    fn legacy_state_backfills_without_losing_custom_sources() {
        // 票 06 形状：adapters 只有 claude/pi，sources 只有两内置条目
        let legacy = json!({
            "status": "ok",
            "adapters": {
                "claude": { "files": 2, "parsed": 2, "skipped": 0, "sessions": 4, "error": null },
                "pi": { "files": 1, "parsed": 1, "skipped": 0, "sessions": 1, "error": null },
            },
            "sources": [
                { "name": "claude", "path": "/home/u/.claude/projects", "kind": "jsonl", "builtin": true },
                { "name": "pi", "path": "/home/u/.pi/agent/sessions", "kind": "jsonl", "builtin": true },
                { "name": "my-logs", "path": "/data/logs", "kind": "jsonl", "builtin": false },
            ],
        });
        // 直接验证 read_state 的补齐规则（不依赖 host：形状变换写成本地 helper）
        let mut merged = legacy["sources"].as_array().cloned().expect("sources");
        for want in json!([
            { "name": "codex", "paths": ["/home/u/.codex/sessions"], "kind": "jsonl", "builtin": true },
            { "name": "opencode", "paths": ["/home/u/.local/share/opencode/opencode.db"], "kind": "sqlite", "builtin": true },
        ])
        .as_array()
        .cloned()
        .expect("builtins")
        {
            let name = want["name"].as_str().unwrap_or("");
            if !merged.iter().any(|s| s["name"].as_str() == Some(name)) {
                merged.push(want);
            }
        }
        // 旧单 path 条目迁移为 paths 数组（read_state 的幂等归一）
        let merged = normalize_sources_paths(merged);
        let names: Vec<&str> = merged.as_array().unwrap()
            .iter().map(|s| s["name"].as_str().unwrap()).collect();
        // 五条：四内置 + 用户自定义；自定义来源**未丢**
        assert_eq!(names.len(), 5);
        assert!(names.contains(&"my-logs"));
        assert!(names.contains(&"codex"));
        assert!(names.contains(&"opencode"));
        // 迁移后的每条都带非空 paths 数组且无遗留 path 字段
        for s in merged.as_array().unwrap() {
            assert!(s["paths"].as_array().map(|a| !a.is_empty()).unwrap_or(false));
            assert!(s.get("path").is_none());
        }
    }

    /// 扫描失活判定（2026-09-28 实机卡死回归锁）
    ///
    /// 契约：`syncing` + 心跳超窗 → 失活（落 error，按钮复原可重试）；
    /// 心跳在窗内 → 存活（回灌慢 ≠ 卡死）；**无心跳的 syncing → 直接判失活**
    /// （旧版本落库 / 崩溃现场的形态）；非 syncing 状态永不判失活。
    #[test]
    fn scan_is_stale_heals_only_abandoned_runs() {
        let now = 1_000_000_000u64;

        // 正例：syncing 且心跳过期 → 失活
        let stuck = json!({ "status": "syncing", "scanStartedAt": now - SCAN_STALE_MS - 1 });
        assert!(scan_is_stale(&stuck, now));

        // 反例守门：心跳在窗内 → 存活（回灌 852 文件耗时数分钟属正常）
        let live = json!({ "status": "syncing", "scanStartedAt": now - 5_000 });
        assert!(!scan_is_stale(&live, now));

        // 边界：恰好等于窗口不算失活（严格大于才判定）
        let edge = json!({ "status": "syncing", "scanStartedAt": now - SCAN_STALE_MS });
        assert!(!scan_is_stale(&edge, now));

        // 实机形态：syncing 却无心跳（旧版本落库）→ 立即判失活
        assert!(scan_is_stale(&json!({ "status": "syncing" }), now));
        assert!(scan_is_stale(&json!({ "status": "syncing", "scanStartedAt": null }), now));

        // 非在途状态永不判失活（否则每次读状态都会误伤一次成功扫描）
        for status in ["idle", "ok", "error", "auth-required"] {
            let s = json!({ "status": status });
            assert!(!scan_is_stale(&s, now), "{status} 不该被判失活");
            let s_old = json!({ "status": status, "scanStartedAt": 0 });
            assert!(!scan_is_stale(&s_old, now), "{status}（旧心跳）不该被判失活");
        }

        // 时钟回拨（now < started）不得下溢成「超窗」
        let future = json!({ "status": "syncing", "scanStartedAt": now + 10_000 });
        assert!(!scan_is_stale(&future, now));
    }
}
