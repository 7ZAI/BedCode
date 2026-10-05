//! 使用统计与会话日志域（票据 06 + 增补：正在使用的项目会话；票据 07：opencode
//! SQLite 适配 + codex 预留骨架 + 数据清空）
//!
//! 数据流：`scan-usage` 发起时先对**本次扫描的实际来源根目录**批量授权
//! （一次 `fs_request_auth` 弹窗列全部，已授权路径宿主静默跳过——同一业务
//! 预见多个文件访问用批量授权代替逐个弹窗；拒绝才降级 auth-required）→
//! host-process 枚举各家 JSONL + opencode SQLite 源同步 → 回灌解析入库 →
//! 状态推送。
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

// ==================== Tests ====================

// 用例按功能拆至 `tests/`（本内联模块的子模块路径由 rustc
// 自动解析到该目录；模块树 `usage::usage::tests::<文件>` 与内联形态等价，私有项可见性不受影响）。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::sources::normalize_sources_paths;
    mod default_state_shape;
}
