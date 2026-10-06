//! 供应商统一管理域（票据 05 / v2 中心凭据库）
//!
//! 职责：预设 CRUD（`provider_preset` 表，插件独立库）/ 反向导入（读各 CLI
//! 现有配置生成预设并把源 key 收进中心凭据库）/ 应用（写目标 CLI 原生配置，
//! 真源始终是 CLI 自己的配置）。配置写入采用**文本级 splice**（JSONC 感知，
//! 注释与未知字段逐字保留），key 纪律 = 明文只落本库、UI/状态/日志一律掩码。
//!
//! # 模块结构
//! - [`paths`]：CLI 配置文件路径（家目录相对段）
//! - [`jsonc`]：JSONC 解析 + 文本级 splice（纯函数）
//! - [`mapping`]：key 掩码与方言映射（纯函数）
//! - [`codex`]：codex 目标（TOML splice + 只读视图）
//! - [`discover`]：模型列表查询（可选 URL + 手动输入互补）
//! - [`import`]：反向导入提取（PresetDraft / presets_from_* / plan_inserts）
//! - [`merge`]：应用条目构造（合并既有 + 覆盖受控字段）
//! - [`store`]：provider_preset 表 + 域状态（导入/应用结果）
//! - [`apply`]：应用到目标 CLI（多目标 + key 四选一 + claude 桥接冲突）
//! - 本模块：命令入口（预设 CRUD / 反向导入 / get_state）

/// host-storage 键：供应商域导入/应用结果（不含 presets——presets 真源在插件库）
pub(crate) const PROVIDERS_KEY: &str = "providers";

/// 预设名/目标条目名长度上限（防误粘贴整段配置）
pub(super) const NAME_MAX: usize = 128;
/// 模型列表上限
const MODELS_MAX: usize = 256;
/// api 方言白名单（与前端 ApiStyle 同构）
const API_STYLES: [&str; 4] = ["openai", "anthropic", "gemini", "custom"];
mod apply;
mod codex;
mod discover;
mod import;
mod jsonc;
mod mapping;
mod merge;
mod paths;
mod store;

/// 命令入口面（lib.rs 路由）
pub(crate) use apply::apply_provider;
pub(crate) use discover::fetch_models;
pub(crate) use store::ensure_schema;

use super::HOME;
use crate::install::now_ms;
use crate::providers::discover::validate_models_url;
use crate::providers::import::{plan_inserts, presets_from_opencode, presets_from_pi, PresetDraft};
use crate::providers::jsonc::parse_jsonc;
use crate::providers::paths::{opencode_cfg_path, pi_auth_path, pi_models_path};
use crate::providers::store::{
    build_state, emit_and_return, list_presets, preset_key_of, preset_names, read_stored,
    write_stored,
};
use bedcode_plugin_api::host::{HostFs, HostLog, HostPluginDatabase};
use bedcode_plugin_api::sql_params;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

// ==================== 命令：预设 CRUD ====================

/// 预设入参（校验后）
struct PresetInput {
    name: String,
    base_url: String,
    api_style: String,
    models: Vec<String>,
    /// 模型查询 URL（可空；非空时必须是 http(s)）
    models_url: String,
}

fn validate_preset_payload(args: &Value) -> Result<PresetInput, String> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "name required".to_string())?;
    if name.chars().count() > NAME_MAX {
        return Err("name too long".to_string());
    }
    let base_url = args
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let api_style = args
        .get("apiStyle")
        .and_then(|v| v.as_str())
        .filter(|s| API_STYLES.contains(s))
        .unwrap_or("openai")
        .to_string();
    let models: Vec<String> = args
        .get("models")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if models.len() > MODELS_MAX {
        return Err("too many models".to_string());
    }
    // 模型查询 URL 可留空（手动输入模型列表）；非空则校验 scheme
    let models_url = match args.get("modelsUrl").and_then(|v| v.as_str()) {
        Some(u) if !u.trim().is_empty() => validate_models_url(u)
            .map_err(|e| format!("modelsUrl: {e}"))?
            .to_string(),
        _ => String::new(),
    };
    Ok(PresetInput {
        name,
        base_url,
        api_style,
        models,
        models_url,
    })
}

/// 新建/更新预设（id 缺省 = 新建）。同名冲突返回 `nameExists`（前端提示）。
/// v2 中心凭据：载荷可选 `apiKey`——缺省 = 保留库内既有；`""` = 清空；
/// 非空 = 设置新 key。key 明文只进库（掩码函数是 UI 交界面），日志只记长度
pub(crate) fn save_preset(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let input = validate_preset_payload(args).map_err(|e| anyhow::anyhow!("save-preset: {e}"))?;
    let PresetInput {
        name,
        base_url,
        api_style,
        models,
        models_url,
    } = input;
    let models_json = serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string());
    let now = now_ms(h).unwrap_or(0);
    let id = args.get("id").and_then(|v| v.as_i64());
    // apiKey 语义：缺省（None）= 保留既有；Some("") = 清空；Some(非空) = 设置
    let api_key: Option<String> = args
        .get("apiKey")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string());

    if let Some(id) = id {
        // 更新：同名冲突只允许撞到自己
        let conflict = list_presets(h)?.iter().any(|p| {
            p.get("name").and_then(|v| v.as_str()) == Some(name.as_str())
                && p.get("id").and_then(|v| v.as_i64()) != Some(id)
        });
        if conflict {
            return Ok(json!({ "saved": false, "nameExists": true }));
        }
        let next_key = match &api_key {
            Some(k) if !k.is_empty() => k.clone(),
            Some(_) => String::new(),
            None => preset_key_of(h, id)?,
        };
        h.plugin_db_execute_params(
            "UPDATE provider_preset SET name = ?1, base_url = ?2, api_style = ?3, \
             models_json = ?4, models_url = ?5, api_key = ?6, updated_at = ?7 WHERE id = ?8",
            &sql_params![
                name,
                base_url,
                api_style,
                models_json,
                models_url,
                next_key,
                now,
                id
            ],
        )
        .map_err(|e| anyhow::anyhow!("save-preset: update failed: {e}"))?;
    } else {
        if preset_names(h)?.iter().any(|n| n == &name) {
            return Ok(json!({ "saved": false, "nameExists": true }));
        }
        let key0 = api_key.clone().unwrap_or_default();
        h.plugin_db_execute_params(
            "INSERT INTO provider_preset (name, base_url, api_style, models_json, models_url, \
             api_key, notes, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?7)",
            &sql_params![
                name,
                base_url,
                api_style,
                models_json,
                models_url,
                key0,
                now
            ],
        )
        .map_err(|e| anyhow::anyhow!("save-preset: insert failed: {e}"))?;
    }
    log_key_change(h, &name, &api_key);
    h.log_info(&format!("preset saved (name = {name}, id = {id:?})"));
    let state = build_state(h)?;
    // 回执契约：成功必须带 `saved: true`（前端按 saved/nameExists 判别成功与
    // 同名冲突；dev-shell mock 与 9e20cf8dc 前端改造均按此契约，同名冲突分支
    // 也一直按此返回）。此前成功路径只回 {state}，保存实际已落库却弹
    // 「保存失败」toast——修复 2026-10-06 反馈。state 保留供旧调用方消费。
    let mut result = emit_and_return(h, &state)?;
    result["saved"] = json!(true);
    Ok(result)
}

/// key 变更日志纪律：只记长度/清空，不记内容（删的是「存储面不落 key」红线，
/// 「日志不落明文」纪律保留）
fn log_key_change(h: &WasmHost, name: &str, api_key: &Option<String>) {
    match api_key {
        Some(k) if !k.is_empty() => {
            h.log_info(&format!(
                "preset key set (name = {name}, key_len = {})",
                k.chars().count()
            ));
        }
        Some(_) => h.log_info(&format!("preset key cleared (name = {name})")),
        None => {}
    }
}

pub(crate) fn delete_preset(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let id = args
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("delete-preset: missing id"))?;
    h.plugin_db_execute_params(
        "DELETE FROM provider_preset WHERE id = ?1",
        &sql_params![id],
    )
    .map_err(|e| anyhow::anyhow!("delete-preset: failed: {e}"))?;
    h.log_info(&format!("preset deleted (id = {id})"));
    let state = build_state(h)?;
    emit_and_return(h, &state)
}
// ==================== 命令：反向导入 ====================
// ==================== 命令：反向导入 ====================

/// 反向导入（同步命令）：pi（models.json + auth.json）与 opencode
/// （opencode.json）各生成预设并把源 key 一并收进中心凭据库；同名去重见
/// `plan_inserts`；claude 只读展示不生成预设。导入结果只回掩码 keys
pub(crate) fn import_providers(h: &WasmHost) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let home = HOME
        .get()
        .ok_or_else(|| anyhow::anyhow!("import: home unavailable"))?;

    let mut drafts: Vec<PresetDraft> = Vec::new();

    // pi
    match h.fs_read(&pi_models_path(home)) {
        Ok(Some(text)) => match parse_jsonc(&text) {
            Ok(models) => {
                let auth = h
                    .fs_read(&pi_auth_path(home))
                    .ok()
                    .flatten()
                    .and_then(|t| parse_jsonc(&t).ok())
                    .unwrap_or_else(|| json!({}));
                drafts.extend(presets_from_pi(&models, &auth));
            }
            Err(e) => h.log_warn(&format!("import: pi models.json parse failed: {e}")),
        },
        Ok(None) => {}
        Err(e) => h.log_warn(&format!("import: read pi models.json failed: {e}")),
    }

    // opencode
    match h.fs_read(&opencode_cfg_path(home)) {
        Ok(Some(text)) => match parse_jsonc(&text) {
            Ok(cfg) => drafts.extend(presets_from_opencode(&cfg)),
            Err(e) => h.log_warn(&format!("import: opencode.json parse failed: {e}")),
        },
        Ok(None) => {}
        Err(e) => h.log_warn(&format!("import: read opencode.json failed: {e}")),
    }

    let (to_create, skipped) = plan_inserts(&preset_names(h)?, drafts);
    let now = now_ms(h).unwrap_or(0);
    let mut created: Vec<String> = Vec::new();
    let mut keys: serde_json::Map<String, Value> = serde_json::Map::new();
    for d in to_create {
        let models_json = serde_json::to_string(&d.models).unwrap_or_else(|_| "[]".to_string());
        h.plugin_db_execute_params(
            "INSERT INTO provider_preset (name, base_url, api_style, models_json, api_key, notes, \
             created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            &sql_params![
                d.name,
                d.base_url,
                d.api_style,
                models_json,
                d.key,
                d.notes,
                now
            ],
        )
        .map_err(|e| anyhow::anyhow!("import: insert preset failed: {e}"))?;
        // 掩码以最终（可能被去重改名的）预设名为键，前端按 preset.name 取用
        keys.insert(d.name.clone(), Value::String(d.key_mask));
        created.push(d.name);
    }
    // 注意：auth.json 无对应条目/解析失败时掩码为 "—"（视为无 key 可直拷）

    let import_last = json!({
        "ok": true,
        "created": created,
        "skipped": skipped,
        "keys": Value::Object(keys),
        "error": Value::Null,
        "at": now,
    });
    let (_, apply_last) = read_stored(h);
    write_stored(h, &import_last, &apply_last);
    h.log_info(&format!(
        "providers imported (created = {}, skipped = {})",
        created.len(),
        skipped.len()
    ));
    let state = build_state(h)?;
    let mut result = emit_and_return(h, &state)?;
    result["created"] = json!(created);
    result["skipped"] = json!(skipped);
    result["keys"] = json!(import_last["keys"]);
    Ok(result)
}
// ==================== 命令入口 ====================
// ==================== 命令入口 ====================

pub(crate) fn get_state(h: &WasmHost) -> anyhow::Result<Value> {
    let state = build_state(h)?;
    Ok(json!({ "state": state }))
}
