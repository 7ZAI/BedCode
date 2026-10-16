//! 安装域状态（host-storage `install` 键，读-改-写 + 全量推送）
//!
//! 状态为单一真源：`{ active, last, updates, mirror: { speed, npmrc } }`，
//! 每次变更全量 emit `plugin:agent-hub:install` 推送前端。`custom_sources` /
//! `all_sources` 是镜像域的候选源读取（内置 + 自定义），供测速与换源共用。

use super::registry::MIRROR_SOURCES;
use super::INSTALL_KEY;
use crate::detect;
use bedcode_plugin_api::host::{ConfigKey, HostConfig, HostEvents, HostLog, HostStorage};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

/// 读用户自定义源 [(id, url)]（install 状态 mirror.customSources）
pub(super) fn custom_sources(state: &Value) -> Vec<(String, String)> {
    state["mirror"]["customSources"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    let id = v["id"].as_str()?.to_string();
                    let url = v["url"].as_str()?.to_string();
                    Some((id, url))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 全部候选源（内置 + 自定义）：(id, url)
pub(super) fn all_sources(state: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = MIRROR_SOURCES
        .iter()
        .map(|(id, url)| (id.to_string(), url.to_string()))
        .collect();
    out.extend(custom_sources(state));
    out
}
// ==================== 状态（读-改-写） ====================

pub(super) fn read_state(h: &WasmHost) -> Value {
    h.storage_get(INSTALL_KEY)
        .ok()
        .flatten()
        .unwrap_or_else(default_state)
}

/// 默认复合状态（纯函数，可测）
pub(super) fn default_state() -> Value {
    let updates = detect::CLI_KINDS
        .iter()
        .map(|c| (c.to_string(), json!(null)))
        .collect::<serde_json::Map<String, Value>>();
    json!({
        "active": null,
        "last": null,
        "updates": updates,
        "mirror": default_mirror(),
    })
}

pub(super) fn default_mirror() -> Value {
    json!({
        "speed": {
            "status": "idle",
            "sources": [],
            "recommend": null,
            "error": null,
            "testedAt": null,
        },
        "npmrc": { "backupExists": false, "fileRegistry": null },
        // 用户自定义源（前端可增删；测速与换源白名单一并纳入）
        "customSources": [],
    })
}

pub(super) fn write_state(h: &WasmHost, state: &Value) {
    if let Err(e) = h.storage_set(INSTALL_KEY, state) {
        h.log_warn(&format!("install: persist state failed: {e}"));
    }
}

/// 全量状态推送前端（事件名与前端订阅端一致）
pub(super) fn emit(h: &WasmHost, state: &Value) {
    h.emit_event("plugin:agent-hub:install", state);
}

/// 推送并返回状态（命令返回值与事件载荷同形）
pub(super) fn emit_and_return(h: &WasmHost, state: &Value) -> anyhow::Result<Value> {
    emit(h, state);
    Ok(json!({ "state": state }))
}

/// 宿主当前 Unix 毫秒时间戳（wasm 无系统时钟，一律经宿主获取）
pub(crate) fn now_ms(h: &WasmHost) -> Result<u64, String> {
    let s = h
        .config_get(ConfigKey::CurrentTimeMs)
        .map_err(|e| format!("time unavailable: {e}"))?
        .ok_or_else(|| "time unavailable".to_string())?;
    s.parse::<u64>()
        .map_err(|e| format!("time parse failed: {e}"))
}

// ==================== Tests（纯函数单测，双平台形态覆盖） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 状态默认值：updates 覆盖四家 CLI、mirror 含 speed/npmrc 两个子域
    #[test]
    fn default_state_shape() {
        let state = default_state();
        assert!(state["active"].is_null());
        assert!(state["last"].is_null());
        for cli in detect::CLI_KINDS {
            assert!(state["updates"][cli].is_null(), "{cli} 应有 updates 占位");
        }
        assert_eq!(state["mirror"]["speed"]["status"], json!("idle"));
        assert_eq!(state["mirror"]["npmrc"]["backupExists"], json!(false));
    }
}
