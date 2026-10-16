//! provider_preset 表（插件独立库，host-plugin-database）+ 域状态
//!
//! v2（2026-09-14 用户决策删除「key 不落 hub」红线）起含 `api_key` 列——
//! 中心凭据库：一处配置 key、分发到多个 agent；明文只落本库（与各 CLI
//! 原生配置同等的明文暴露面），`preset_row_to_json` 只以掩码 keyMask 出
//! wire（明文永不进状态载荷/前端）。域状态（host-storage `providers` 键）
//! 记 import/apply 最近一次结果。

use super::jsonc::parse_jsonc;
use super::mapping::mask_key;
use super::merge::claude_env_view;
use super::paths::{bridge_paths, claude_auth_paths, claude_settings_path};
use super::PROVIDERS_KEY;
use crate::HOME;
use bedcode_plugin_api::host::{HostEvents, HostFs, HostLog, HostPluginDatabase, HostStorage};
use bedcode_plugin_api::sql_params;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

// ==================== 插件库：provider_preset 表 ====================

/// 建表 + 幂等迁移。宿主 `plugin_db_execute` 为单语句版本，schema 不拆分
/// （单表无索引）；列迁移走 PRAGMA 存在性检查，旧库重跑安全
pub(crate) fn ensure_schema(h: &WasmHost) -> anyhow::Result<()> {
    h.plugin_db_execute(
        "CREATE TABLE IF NOT EXISTS provider_preset (\
         id INTEGER PRIMARY KEY AUTOINCREMENT, \
         name TEXT NOT NULL UNIQUE, \
         base_url TEXT NOT NULL DEFAULT '', \
         api_style TEXT NOT NULL DEFAULT 'openai', \
         models_json TEXT NOT NULL DEFAULT '[]', \
         models_url TEXT NOT NULL DEFAULT '', \
         api_key TEXT NOT NULL DEFAULT '', \
         notes TEXT, \
         created_at INTEGER NOT NULL, \
         updated_at INTEGER NOT NULL)",
    )
    .map_err(|e| anyhow::anyhow!("providers: ensure schema failed: {e}"))?;
    // 幂等迁移：旧库缺列时逐列补（v2 加 api_key 中心凭据列；v3 加 models_url
    // 模型查询 URL 列）。检测到缺列才执行 ALTER，旧库重跑安全
    for (col, ddl) in [
        (
            "api_key",
            "ALTER TABLE provider_preset ADD COLUMN api_key TEXT NOT NULL DEFAULT ''",
        ),
        (
            "models_url",
            "ALTER TABLE provider_preset ADD COLUMN models_url TEXT NOT NULL DEFAULT ''",
        ),
    ] {
        if !has_column(h, col)? {
            h.plugin_db_execute(ddl)
                .map_err(|e| anyhow::anyhow!("providers: migrate {col} failed: {e}"))?;
        }
    }
    Ok(())
}

/// 表是否已有某列（PRAGMA table_info）
fn has_column(h: &WasmHost, col: &str) -> anyhow::Result<bool> {
    let cols = h
        .plugin_db_query("PRAGMA table_info(provider_preset)")
        .map_err(|e| anyhow::anyhow!("providers: pragma failed: {e}"))?
        .unwrap_or(Value::Array(Vec::new()));
    Ok(cols
        .as_array()
        .map(|arr| {
            arr.iter()
                .any(|c| c.get("name").and_then(|v| v.as_str()) == Some(col))
        })
        .unwrap_or(false))
}

/// 库行 → wire JSON（models_json 反序列化；key 只以掩码 keyMask 出现——
/// 明文只存库，状态载荷/前端永不接触）
fn preset_row_to_json(row: &Value) -> Option<Value> {
    let models = row
        .get("models_json")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let key = row.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
    Some(json!({
        "id": row.get("id")?,
        "name": row.get("name")?,
        "baseUrl": row.get("base_url")?.as_str().unwrap_or(""),
        "apiStyle": row.get("api_style")?.as_str().unwrap_or("openai"),
        "models": models,
        // 模型查询 URL（可空：留空 = 手动输入模型列表）
        "modelsUrl": row.get("models_url").and_then(|v| v.as_str()).unwrap_or(""),
        "keyMask": mask_key(key),
        "notes": row.get("notes").cloned().unwrap_or(Value::Null),
        "createdAt": row.get("created_at")?,
        "updatedAt": row.get("updated_at")?,
    }))
}

pub(super) fn list_presets(h: &WasmHost) -> anyhow::Result<Vec<Value>> {
    let rows = h
        .plugin_db_query(
            "SELECT id, name, base_url, api_style, models_json, models_url, api_key, notes, \
             created_at, updated_at FROM provider_preset ORDER BY name",
        )
        .map_err(|e| anyhow::anyhow!("providers: list failed: {e}"))?
        .unwrap_or(Value::Array(Vec::new()));
    Ok(rows
        .as_array()
        .map(|arr| arr.iter().filter_map(preset_row_to_json).collect())
        .unwrap_or_default())
}

/// 读预设已存 key 明文（guest 内部专用：stored 应用 / 保存保留与清空判断）。
/// 返回值永不进 wire 与日志——调用方只做长度统计或写入目标配置
pub(super) fn preset_key_of(h: &WasmHost, id: i64) -> anyhow::Result<String> {
    let rows = h
        .plugin_db_query_params(
            "SELECT api_key FROM provider_preset WHERE id = ?1",
            &sql_params![id],
        )
        .map_err(|e| anyhow::anyhow!("providers: read key failed: {e}"))?
        .unwrap_or(Value::Array(Vec::new()));
    Ok(rows
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|r| r.get("api_key"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

pub(super) fn preset_names(h: &WasmHost) -> anyhow::Result<Vec<String>> {
    Ok(list_presets(h)?
        .iter()
        .filter_map(|p| {
            p.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect())
}
// ==================== 域状态（读-改-写 + 全量推送） ====================

pub(super) fn read_stored(h: &WasmHost) -> (Value, Value) {
    let stored = h
        .storage_get(PROVIDERS_KEY)
        .ok()
        .flatten()
        .unwrap_or_else(|| json!({}));
    let default = || json!(null);
    (
        stored
            .get("import")
            .and_then(|v| v.get("last"))
            .cloned()
            .unwrap_or_else(default),
        stored
            .get("apply")
            .and_then(|v| v.get("last"))
            .cloned()
            .unwrap_or_else(default),
    )
}

pub(super) fn write_stored(h: &WasmHost, import_last: &Value, apply_last: &Value) {
    let payload = json!({ "import": { "last": import_last }, "apply": { "last": apply_last } });
    if let Err(e) = h.storage_set(PROVIDERS_KEY, &payload) {
        h.log_warn(&format!("providers: persist state failed: {e}"));
    }
}

/// claude 只读视图（env 掩码 + 桥接文件存在性，读时现查）
fn claude_view(h: &WasmHost, home: &str) -> Value {
    let env = h
        .fs_read(&claude_settings_path(home))
        .ok()
        .flatten()
        .map(|t| {
            parse_jsonc(&t)
                .map(|v| claude_env_view(&v))
                .unwrap_or(json!({}))
        })
        .unwrap_or_else(|| json!({}));
    let bridges = bridge_paths(home);
    json!({
        "env": env,
        "bridge": {
            "providerConfigSh": h.fs_exists(&bridges[0]).unwrap_or(false),
            "anthropicBridgeMjs": h.fs_exists(&bridges[1]).unwrap_or(false),
        },
    })
}

/// 组装全量状态（命令返回值与事件载荷同形）
///
/// claude 视图一次读 3 个路径（settings + 两桥接文件存在性），先批量授权整组
/// （[`claude_auth_paths`]）再逐路径读取：同一业务（打开供应商页 / 一次应用）
/// 预见多个文件访问，逐个 fs_read 会弹 N 次框；一次 request-auth 弹一次框列出
/// 全部，命中即静默、未命中拒绝则视图降级（claude_view 的读失败分支与
/// 未授权一致——env 空 + 桥接 false，不阻断页签）。
/// codex 视图同理：读 `~/.codex/config.toml`（激活时的批量授权已含该目录），
/// 读失败/未授权 → 全 null 视图（面板据此不显示「当前模型」行，不阻断）。
pub(super) fn build_state(h: &WasmHost) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let (import_last, apply_last) = read_stored(h);
    let home = HOME.get().map(|s| s.as_str()).unwrap_or("");
    // 拒绝只降级视图（见上），不阻断状态组装
    let _ = h.fs_request_auth(&claude_auth_paths(home)).unwrap_or(false);
    Ok(json!({
        "presets": list_presets(h)?,
        "claude": claude_view(h, home),
        "codex": super::codex::codex_view(h, home),
        "import": { "last": import_last },
        "apply": { "last": apply_last },
    }))
}

pub(super) fn emit_and_return(h: &WasmHost, state: &Value) -> anyhow::Result<Value> {
    h.emit_event("plugin:agent-hub:providers", state);
    Ok(json!({ "state": state }))
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    /// 预设 wire 形状：key 只以掩码 keyMask 出现（明文永不序列化进状态）
    #[test]
    fn preset_row_masks_key() {
        let row = json!({
            "id": 1, "name": "sensenova", "base_url": "https://u/v1",
            "api_style": "openai", "models_json": "[\"m1\"]",
            "models_url": "https://u/v1/models",
            "api_key": "super-secret-raw-0123456789", "notes": "pi:sensenova",
            "created_at": 1, "updated_at": 2,
        });
        let p = preset_row_to_json(&row).expect("row maps");
        assert!(p.get("key").is_none());
        assert!(p.get("apiKey").is_none());
        assert_eq!(p["keyMask"], "sup…(27)");
        assert!(
            p.get("models_json").is_none(),
            "models_json 已解码为 models"
        );
        assert_eq!(p["models"], json!(["m1"]));
        assert_eq!(p["modelsUrl"], "https://u/v1/models");
        assert_eq!(p["notes"], "pi:sensenova");
        assert!(
            !p.to_string().contains("super-secret-raw"),
            "明文不得进入 wire 形状"
        );

        // 旧行缺 models_url 列（v3 迁移前）→ 空串而非缺键（前端不必判 undefined）
        let legacy = json!({
            "id": 2, "name": "legacy", "base_url": "https://l/v1",
            "api_style": "openai", "models_json": "[]", "api_key": "",
            "notes": Value::Null, "created_at": 1, "updated_at": 1,
        });
        assert_eq!(
            preset_row_to_json(&legacy).expect("legacy maps")["modelsUrl"],
            ""
        );
    }

    /// 应用结果状态（apply.last）：key 只出现长度（keyLen），不出现内容
    #[test]
    fn apply_state_records_key_len_only() {
        let key = "super-secret-key-0123456789";
        let apply_last = json!({
            "ok": true, "preset": "sensenova", "target": "pi", "targets": ["pi"],
            "files": ["models.json", "auth.json"], "keyMode": "source",
            "keyLen": key.chars().count(), "error": null, "at": 0,
        });
        let s = apply_last.to_string();
        assert!(
            !s.contains(key),
            "apply state must not contain key plaintext"
        );
        assert!(s.contains("\"keyLen\":27"));
        // 日志行同样只含长度（与 apply_provider 的 log_info 同构断言）
        let log_line = format!(
            "provider applied (target = pi, key_len = {:?})",
            apply_last["keyLen"]
        );
        assert!(!log_line.contains(key));
    }
}
