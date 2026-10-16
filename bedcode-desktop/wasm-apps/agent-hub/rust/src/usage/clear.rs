//! 数据清空（票 07：全量保留 + 手动清空）
//!
//! `usage_session` 与 `parse_watermark` 在**同一事务**内清（`execute-batch`
//! 宿主实现 = unchecked_transaction + 逐条执行 + 统一 commit，任一句失败
//! 整体回滚）——两者必须同生共死：只删会话留水位 → 下轮扫描整文件跳过，
//! 那些会话永远回不来；只删水位留会话 → 重复入库。

use super::schema::ensure_schema;
use super::ADAPTERS;
use super::{auth_granted, emit_and_return, read_state, write_state};
use bedcode_plugin_api::host::{HostLog, HostPluginDatabase};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

// ==================== 数据清空（票 07：全量保留 + 手动清空） ====================
/// 清空已采集的使用统计数据（会话聚合 + 解析水位）
///
/// 保留策略：**全量保留，不自动过期**（stats 域的数据量级在会话数级别，
/// 远达不到需要自动清理的阀值；擅自过期会让历史看板出现无法解释的断层）。
/// 清理只发生在用户显式动作时，入口在统计页（两击确认）。
///
/// `usage_session` 与 `parse_watermark` 在**同一事务**内清——两者必须同生共死：
/// 只删会话而留水位，下次扫描会因「水位未变」整文件跳过，那些会话永远回不来；
/// 只删水位而留会话则变成重复入库。`execute-batch` 的宿主实现是
/// `unchecked_transaction` + 逐条执行 + 统一 commit（任一句失败整体回滚）。
///
/// 状态侧一并归零：各适配器计数清 0、`activeSessions` 清空（否则列表行会
/// 继续打「当前」标记而库里已无对应会话）。
pub(crate) fn clear_data(h: &WasmHost) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let cleared = h
        .plugin_db_execute_batch(&[
            "DELETE FROM usage_session".to_string(),
            "DELETE FROM parse_watermark".to_string(),
        ])
        .map_err(|e| anyhow::anyhow!("usage: clear data failed: {e}"))?;

    let mut state = read_state(h);
    state["authGranted"] = json!(auth_granted(h));
    // 自定义来源的适配器槽位也要归零（它们不是 ADAPTERS 成员）：先取名再借
    let custom_names: Vec<String> = state
        .get("sources")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|s| s.get("builtin").and_then(|b| b.as_bool()) != Some(true))
                .filter_map(|s| s.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let zero_stat = json!({ "files": 0, "parsed": 0, "skipped": 0, "sessions": 0, "error": null });
    if let Some(adapters) = state["adapters"].as_object_mut() {
        for name in ADAPTERS.iter().map(|s| s.to_string()).chain(custom_names) {
            adapters.insert(name, zero_stat.clone());
        }
    }
    state["activeSessions"] = json!({});
    state["error"] = json!(null);
    state["status"] = json!("idle");
    state["syncedAt"] = json!(null);
    write_state(h, &state);
    h.log_info(&format!("usage data cleared, rows = {cleared}"));
    emit_and_return(h, &state)
}
