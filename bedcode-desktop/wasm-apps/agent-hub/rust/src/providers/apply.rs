//! 应用到目标 CLI（同步命令，一次可写多个 CLI）
//!
//! key 来源四选一：stored 中心库已存（默认）/ inline 现场输入 / source 内存
//! 直拷（现读源配置，不落存储/日志）/ none 不带 key（pi 的 auth.json 与
//! 既有 apiKey 均保留）。claude 桥接冲突时阻止（force = 用户确认后仅写
//! env 块，桥接文件永不触碰）。真源始终是 CLI 自己的配置文件。
//!
//! # 多目标
//! `targets`（数组，顺序即写入顺序）逐个写，**互不影响**：单目标失败只记在
//! 该目标的结果里，其余目标照写；前端按 `results` 逐目标呈现。兼容旧的单数
//! `target` 入参。
//!
//! # codex 目标的差异（见 [`super::codex`]）
//! codex 不吃 key 值也不存模型清单：只写 `env_key = "<变量名>"` +
//! 顶层 `model` / `model_provider`（单值）。因此 codex 目标**不参与 key 四选一**
//! （纯 codex 应用时完全跳过 key 解析），env 变量名走独立的 `codex.envKey` 入参。
//!
//! # 写前自检（fail-visible）
//! pi / opencode 的条目合并后若**一个模型都没有**，拒绝写入并回
//! `reason = "noModels"`——写入等于「供应商配好了」的假象，目标 CLI 里却
//! 看不到任何模型（2026-10-04 实测：InkStone 预设模型列表为空，应用回执
//! `applied: true`，pi `/model` 里 0 个模型）。claude 不受此约束（env 块
//! 只改端点与 token）。

use super::codex;
use super::jsonc::{ensure_container, parse_jsonc, upsert_entry};
use super::merge::{
    claude_env_entries, merge_opencode_entry, merge_pi_entry, merged_model_count, pi_auth_entry,
};
use super::paths::{
    bridge_paths, claude_auth_paths, claude_settings_path, codex_config_path, opencode_cfg_path,
    pi_auth_path, pi_models_path,
};
use super::store::{
    build_state, emit_and_return, ensure_schema, list_presets, preset_key_of, read_stored,
    write_stored,
};
use super::NAME_MAX;
use crate::install::now_ms;
use crate::HOME;
use bedcode_plugin_api::host::{HostFs, HostLog};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

/// 应用目标白名单（codex 见 [`super::codex`]：登记 provider + 设为当前模型）
const TARGETS: [&str; 4] = ["claude", "pi", "opencode", "codex"];

// ==================== 预设载入与入参校验 ====================

/// 目标条目名合法性（作为 JSON 对象键写入各 CLI 配置）
fn validate_target_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.chars().count() > NAME_MAX {
        return Err("target name empty or too long".to_string());
    }
    Ok(())
}

/// 目标列表：白名单 + 去重保序 + 非空。兼容旧单数 `target`
fn parse_targets(args: &Value) -> Result<Vec<String>, String> {
    let mut raw: Vec<String> = Vec::new();
    if let Some(arr) = args.get("targets").and_then(|v| v.as_array()) {
        for v in arr {
            if let Some(s) = v.as_str() {
                raw.push(s.trim().to_string());
            }
        }
    }
    if raw.is_empty() {
        // 旧调用方（单目标）：`target: "pi"`
        if let Some(s) = args.get("target").and_then(|v| v.as_str()) {
            raw.push(s.trim().to_string());
        }
    }
    let mut out: Vec<String> = Vec::new();
    for t in raw {
        if !TARGETS.contains(&t.as_str()) {
            return Err(format!("unsupported target {t}"));
        }
        if !out.contains(&t) {
            out.push(t);
        }
    }
    if out.is_empty() {
        return Err("no target selected".to_string());
    }
    Ok(out)
}

/// 预设快照（一次应用内的只读视图）
struct Preset {
    name: String,
    base_url: String,
    api_style: String,
    models: Vec<String>,
}

impl Preset {
    fn load(h: &WasmHost, id: i64) -> anyhow::Result<Self> {
        let preset = list_presets(h)?
            .into_iter()
            .find(|p| p.get("id").and_then(|v| v.as_i64()) == Some(id))
            .ok_or_else(|| anyhow::anyhow!("apply: preset {id} not found"))?;
        let models: Vec<String> = preset
            .get("models")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|m| m.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            name: preset
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            base_url: preset
                .get("baseUrl")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            api_style: preset
                .get("apiStyle")
                .and_then(|v| v.as_str())
                .unwrap_or("openai")
                .to_string(),
            models,
        })
    }
}

// ==================== key 解析 ====================

/// 从源 CLI 配置内存直拷 key（应用时现读，不落存储/日志）
fn source_key(h: &WasmHost, cli: &str, provider: &str) -> anyhow::Result<String> {
    let home = HOME
        .get()
        .ok_or_else(|| anyhow::anyhow!("apply: home unavailable"))?;
    let (path, extract) = match cli {
        "pi" => (pi_auth_path(home), Extract::PiAuth),
        "opencode" => (opencode_cfg_path(home), Extract::Opencode),
        "claude" => (claude_settings_path(home), Extract::ClaudeEnv),
        other => return Err(anyhow::anyhow!("apply: unknown key source cli {other}")),
    };
    let text = h
        .fs_read(&path)
        .map_err(|e| anyhow::anyhow!("apply: read source config failed: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("apply: source config missing ({cli})"))?;
    let cfg =
        parse_jsonc(&text).map_err(|e| anyhow::anyhow!("apply: source config invalid: {e}"))?;
    let key = match extract {
        Extract::PiAuth => cfg
            .get(provider)
            .and_then(|p| p.get("key"))
            .and_then(|v| v.as_str()),
        Extract::Opencode => cfg
            .get("provider")
            .and_then(|p| p.get(provider))
            .and_then(|p| p.get("options"))
            .and_then(|o| o.get("apiKey"))
            .and_then(|v| v.as_str()),
        Extract::ClaudeEnv => cfg
            .get("env")
            .and_then(|e| e.get("ANTHROPIC_AUTH_TOKEN"))
            .and_then(|v| v.as_str()),
    }
    .filter(|s| !s.is_empty())
    .ok_or_else(|| anyhow::anyhow!("apply: no key for {cli}:{provider}"))?;
    Ok(key.to_string())
}

enum Extract {
    PiAuth,
    Opencode,
    ClaudeEnv,
}

/// key 四选一解析（与旧语义一致；stored 现读库内明文，明文不进 wire/日志）
fn resolve_key(h: &WasmHost, args: &Value, id: i64) -> anyhow::Result<(String, Option<String>)> {
    let key_mode = args
        .get("key")
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("none")
        .to_string();
    let key: Option<String> = match key_mode.as_str() {
        "inline" => Some(
            args.get("key")
                .and_then(|k| k.get("value"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("apply: inline key empty"))?
                .to_string(),
        ),
        "stored" => {
            let k = preset_key_of(h, id)?;
            if k.is_empty() {
                return Err(anyhow::anyhow!(
                    "apply: preset has no stored key (save one first)"
                ));
            }
            Some(k)
        }
        "source" => {
            let cli = args
                .get("key")
                .and_then(|k| k.get("cli"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let provider = args
                .get("key")
                .and_then(|k| k.get("provider"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Some(source_key(h, cli, provider)?)
        }
        _ => None,
    };
    Ok((key_mode, key))
}

// ==================== 单目标写入 ====================

/// 单目标写入结局
enum TargetWrite {
    /// 已写入，附写入文件名
    Applied(Vec<String>),
    /// claude 桥接冲突，被阻止（未写任何文件）
    BridgeConflict(Vec<String>),
}

/// 失败分类（前端按 reason 呈现文案；error 只进日志，界面不携带原文）
const REASON_NO_MODELS: &str = "noModels";
const REASON_WRITE_FAILED: &str = "writeFailed";
const REASON_BRIDGE: &str = "bridgeConflict";
// codex 专属分类码由 [`super::codex::plan_apply`] 返回（unsupportedDialect /
// invalidEnvKey），与前端 i18n 一一对应

/// 写单个目标 CLI 的配置文件（配置写入 = 文本级 splice，注释逐字保留）
fn write_target(
    h: &WasmHost,
    target: &str,
    preset: &Preset,
    target_name: &str,
    key: Option<&str>,
    force: bool,
    codex_env_key: &str,
) -> Result<TargetWrite, (String, String)> {
    let home = HOME.get().ok_or_else(|| {
        (
            REASON_WRITE_FAILED.to_string(),
            "home unavailable".to_string(),
        )
    })?;
    let outcome = match target {
        "claude" => write_claude(h, preset, key, force, home),
        "pi" => write_pi(h, preset, target_name, key, home),
        "opencode" => write_opencode(h, preset, target_name, key, home),
        "codex" => write_codex(h, preset, target_name, codex_env_key, home),
        other => Err((
            REASON_WRITE_FAILED.to_string(),
            format!("unsupported target {other}"),
        )),
    };
    // 统一补 apply: 上下文（各写入函数只报步骤级原因）
    outcome.map_err(|(reason, msg)| (reason, format!("apply: {msg}")))
}

/// claude：只改 settings.json 的 env 块；桥接文件在用时阻止（force 后仅写 env）
fn write_claude(
    h: &WasmHost,
    preset: &Preset,
    key: Option<&str>,
    force: bool,
    home: &str,
) -> Result<TargetWrite, (String, String)> {
    let bridges = bridge_paths(home);
    let found: Vec<String> = bridges
        .iter()
        .filter(|p| h.fs_exists(p).unwrap_or(false))
        .map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
        .collect();
    if !found.is_empty() && !force {
        // 桥接冲突：阻止写入（桥接文件永不触碰），由用户确认后再写 env
        h.log_warn("apply: claude bridge detected; write blocked");
        return Ok(TargetWrite::BridgeConflict(found));
    }
    let path = claude_settings_path(home);
    let text = read_or_default(h, &path, "settings.json")
        .map_err(|e| (REASON_WRITE_FAILED.to_string(), e))?;
    let mut t = ensure_container(&text, "env").map_err(|e| {
        (
            REASON_WRITE_FAILED.to_string(),
            format!("claude env container: {e}"),
        )
    })?;
    for (k, v) in claude_env_entries(
        &preset.base_url,
        key,
        preset.models.first().map(|s| s.as_str()),
    ) {
        t = upsert_entry(&t, Some("env"), k, &v.to_string()).map_err(|e| {
            (
                REASON_WRITE_FAILED.to_string(),
                format!("claude env upsert {k}: {e}"),
            )
        })?;
    }
    h.fs_write(&path, &t).map_err(|e| {
        (
            REASON_WRITE_FAILED.to_string(),
            format!("write settings.json failed: {e}"),
        )
    })?;
    Ok(TargetWrite::Applied(vec!["settings.json".to_string()]))
}

/// pi：models.json 写 providers 条目（有 key 时另写 auth.json）
fn write_pi(
    h: &WasmHost,
    preset: &Preset,
    target_name: &str,
    key: Option<&str>,
    home: &str,
) -> Result<TargetWrite, (String, String)> {
    let path = pi_models_path(home);
    let text = read_or_default(h, &path, "models.json")
        .map_err(|e| (REASON_WRITE_FAILED.to_string(), e))?;
    let t = ensure_container(&text, "providers").map_err(|e| {
        (
            REASON_WRITE_FAILED.to_string(),
            format!("pi providers container: {e}"),
        )
    })?;
    // 合并既有条目（保留用户模型定义）， pretty 序列化与 pi 既有排版一致
    let existing = parse_jsonc(&t)
        .ok()
        .and_then(|v| v.get("providers").and_then(|p| p.get(target_name)).cloned());
    let entry = merge_pi_entry(
        existing.as_ref(),
        &preset.name,
        &preset.base_url,
        &preset.api_style,
        &preset.models,
    );
    if merged_model_count(&entry) == 0 {
        // fail-visible：写入会让 pi `/model` 里出现一个 0 模型的供应商
        return Err((
            REASON_NO_MODELS.to_string(),
            format!("preset {target_name} has no models"),
        ));
    }
    let pretty = serde_json::to_string_pretty(&entry).map_err(|e| {
        (
            REASON_WRITE_FAILED.to_string(),
            format!("serialize pi entry: {e}"),
        )
    })?;
    let t = upsert_entry(&t, Some("providers"), target_name, &pretty).map_err(|e| {
        (
            REASON_WRITE_FAILED.to_string(),
            format!("pi providers upsert: {e}"),
        )
    })?;
    h.fs_write(&path, &t).map_err(|e| {
        (
            REASON_WRITE_FAILED.to_string(),
            format!("write models.json failed: {e}"),
        )
    })?;
    let mut files = vec!["models.json".to_string()];
    // key 条目：无 key 时不动 auth.json（保留既有凭据）
    if let Some(k) = key {
        let auth_path = pi_auth_path(home);
        let auth_text = read_or_default(h, &auth_path, "auth.json")
            .map_err(|e| (REASON_WRITE_FAILED.to_string(), e))?;
        let auth_entry = serde_json::to_string(&pi_auth_entry(k)).map_err(|e| {
            (
                REASON_WRITE_FAILED.to_string(),
                format!("serialize auth entry: {e}"),
            )
        })?;
        let t = upsert_entry(&auth_text, None, target_name, &auth_entry).map_err(|e| {
            (
                REASON_WRITE_FAILED.to_string(),
                format!("auth.json upsert: {e}"),
            )
        })?;
        h.fs_write(&auth_path, &t).map_err(|e| {
            (
                REASON_WRITE_FAILED.to_string(),
                format!("write auth.json failed: {e}"),
            )
        })?;
        files.push("auth.json".to_string());
    }
    Ok(TargetWrite::Applied(files))
}

/// opencode：opencode.json 写 provider 条目（options.apiKey 内联）
fn write_opencode(
    h: &WasmHost,
    preset: &Preset,
    target_name: &str,
    key: Option<&str>,
    home: &str,
) -> Result<TargetWrite, (String, String)> {
    let path = opencode_cfg_path(home);
    let text = read_or_default(h, &path, "opencode.json")
        .map_err(|e| (REASON_WRITE_FAILED.to_string(), e))?;
    let t = ensure_container(&text, "provider").map_err(|e| {
        (
            REASON_WRITE_FAILED.to_string(),
            format!("opencode provider container: {e}"),
        )
    })?;
    let existing = parse_jsonc(&t)
        .ok()
        .and_then(|v| v.get("provider").and_then(|p| p.get(target_name)).cloned());
    let entry = merge_opencode_entry(
        existing.as_ref(),
        &preset.name,
        &preset.base_url,
        &preset.api_style,
        &preset.models,
        key,
    );
    if merged_model_count(&entry) == 0 {
        return Err((
            REASON_NO_MODELS.to_string(),
            format!("preset {target_name} has no models"),
        ));
    }
    let pretty = serde_json::to_string_pretty(&entry).map_err(|e| {
        (
            REASON_WRITE_FAILED.to_string(),
            format!("serialize opencode entry: {e}"),
        )
    })?;
    let t = upsert_entry(&t, Some("provider"), target_name, &pretty).map_err(|e| {
        (
            REASON_WRITE_FAILED.to_string(),
            format!("opencode provider upsert: {e}"),
        )
    })?;
    h.fs_write(&path, &t).map_err(|e| {
        (
            REASON_WRITE_FAILED.to_string(),
            format!("write opencode.json failed: {e}"),
        )
    })?;
    Ok(TargetWrite::Applied(vec!["opencode.json".to_string()]))
}

/// codex：登记 `[model_providers.<name>]` + 把顶层 `model` / `model_provider`
/// 指向它（codex 无模型清单，只能指向单个模型 = 预设首个模型）
///
/// 破坏性：会切走用户当前的 `model` / `model_provider`（codex 没有「只登记不切换」
/// 的形态）。面板在写入前展示「当前 → 将切换为」由用户确认。
fn write_codex(
    h: &WasmHost,
    preset: &Preset,
    target_name: &str,
    env_key: &str,
    home: &str,
) -> Result<TargetWrite, (String, String)> {
    // 计划（三道守卫在纯函数侧，写入方只负责文本与 fs）
    let plan = codex::plan_apply(
        target_name,
        &preset.name,
        &preset.base_url,
        &preset.api_style,
        &preset.models,
        env_key,
    )
    .map_err(|(reason, msg)| (reason.to_string(), msg))?;

    let path = codex_config_path(home);
    let text = read_or_default_raw(h, &path, "config.toml")
        .map_err(|e| (REASON_WRITE_FAILED.to_string(), e))?;
    let entries = plan.entries();
    let entry_refs: Vec<(&str, &str)> = entries.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let mut next = codex::upsert_table(&text, &plan.table, &entry_refs);
    next = codex::set_top_level(&next, "model", &codex::toml_quote(&plan.model));
    next = codex::set_top_level(
        &next,
        "model_provider",
        &codex::toml_quote(&plan.target_name),
    );
    h.fs_write(&path, &next).map_err(|e| {
        (
            REASON_WRITE_FAILED.to_string(),
            format!("write config.toml failed: {e}"),
        )
    })?;
    Ok(TargetWrite::Applied(vec!["config.toml".to_string()]))
}

/// 读配置文件：不存在时给空串（codex 的空配置骨架就是空文件；`{}` 是 JSON 骨架，
/// 对 TOML 无意义）
fn read_or_default_raw(h: &WasmHost, path: &str, label: &str) -> Result<String, String> {
    let text = h
        .fs_read(path)
        .map_err(|e| format!("read {label} failed: {e}"))?;
    Ok(text.unwrap_or_default())
}

/// 读配置文件：不存在（或为空）时给空对象骨架（首次应用不要求文件已存在）
fn read_or_default(h: &WasmHost, path: &str, label: &str) -> Result<String, String> {
    let text = h
        .fs_read(path)
        .map_err(|e| format!("read {label} failed: {e}"))?;
    Ok(match text {
        Some(t) if !t.trim().is_empty() => t,
        _ => "{}".to_string(),
    })
}

// ==================== 命令入口 ====================

/// 应用预设到目标 CLI（同步命令，可多目标）。key 来源四选一：stored 中心库已存
/// （默认）/ inline 现场输入 / source 内存直拷 / none 不带 key（pi 的 auth.json
/// 与既有 apiKey 均保留）。claude 桥接冲突时阻止（force = 用户确认，仅写 env）
///
/// 返回：`{ applied, targets, results[], files[], bridgeConflict, bridges, error }`
/// —— `results` 逐目标给 `ok / files / reason`，一个目标失败不牵连其余目标。
pub(crate) fn apply_provider(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let id = args
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("apply: missing preset id"))?;
    let preset = Preset::load(h, id)?;
    let targets = parse_targets(args).map_err(|e| anyhow::anyhow!("apply: {e}"))?;

    let target_name = args
        .get("targetName")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| preset.name.clone());
    validate_target_name(&target_name)
        .map_err(|e| anyhow::anyhow!("apply: invalid target name: {e}"))?;

    let key_source_claude = matches!(
        args.get("key")
            .and_then(|k| k.get("kind"))
            .and_then(|v| v.as_str()),
        Some("source")
    ) && args
        .get("key")
        .and_then(|k| k.get("cli"))
        .and_then(|v| v.as_str())
        == Some("claude");
    if targets.iter().any(|t| t == "claude") || key_source_claude {
        authorize_claude_group(h)?;
    }
    let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
    // codex 不吃 key 值（只写 env_key 变量名）：纯 codex 应用时完全跳过 key 解析，
    // 否则「默认 inline 但未填」会把一次本可成功的 codex 应用判失败
    let codex_env_key = args
        .get("codex")
        .and_then(|c| c.get("envKey"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let key_targets = targets.iter().any(|t| *t != "codex");
    let (key_mode, key) = if key_targets {
        resolve_key(h, args, id)?
    } else {
        ("none".to_string(), None)
    };

    let mut results: Vec<Value> = Vec::new();
    let mut all_files: Vec<String> = Vec::new();
    let mut all_bridges: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for target in &targets {
        match write_target(
            h,
            target,
            &preset,
            &target_name,
            key.as_deref(),
            force,
            &codex_env_key,
        ) {
            Ok(TargetWrite::Applied(files)) => {
                for f in &files {
                    if !all_files.contains(f) {
                        all_files.push(f.clone());
                    }
                }
                results.push(json!({
                    "target": target, "ok": true, "files": files,
                    "reason": Value::Null, "error": Value::Null,
                }));
            }
            Ok(TargetWrite::BridgeConflict(bridges)) => {
                for b in &bridges {
                    if !all_bridges.contains(b) {
                        all_bridges.push(b.clone());
                    }
                }
                results.push(json!({
                    "target": target, "ok": false, "files": Value::Array(Vec::new()),
                    "reason": REASON_BRIDGE, "bridges": bridges, "error": Value::Null,
                }));
                h.log_warn(&format!(
                    "provider apply blocked by claude bridge (preset = {}, target = {})",
                    preset.name, target
                ));
            }
            Err((reason, msg)) => {
                // 失败原文只进日志（票 04 / ADR 0030：界面给友好文案，不携带原文）
                h.log_error(&format!("provider apply failed (target = {target}): {msg}"));
                errors.push(msg.clone());
                results.push(json!({
                    "target": target, "ok": false, "files": Value::Array(Vec::new()),
                    "reason": reason, "error": msg,
                }));
            }
        }
    }

    let applied = errors.is_empty() && all_bridges.is_empty();
    let apply_last = json!({
        "ok": applied,
        "preset": preset.name,
        "targets": targets,
        // 旧字段保留（单目标视图）：首个目标
        "target": targets.first().map_or(Value::Null, |s| json!(s)),
        "results": results,
        "files": all_files,
        "keyMode": key_mode,
        "keyLen": key.as_ref().map(|k| k.chars().count()),
        "error": if applied { Value::Null } else { Value::String(errors.join("; ")) },
        "at": now_ms(h).unwrap_or(0),
    });
    let (import_last, _) = read_stored(h);
    write_stored(h, &import_last, &apply_last);
    // 日志纪律：key 只记长度（spec §6）
    h.log_info(&format!(
        "provider applied (preset = {}, targets = {:?}, ok = {applied}, key_len = {:?})",
        preset.name, targets, apply_last["keyLen"]
    ));
    let state = build_state(h)?;
    let mut result = emit_and_return(h, &state)?;
    result["applied"] = json!(applied);
    result["targets"] = json!(targets);
    result["results"] = json!(results);
    result["files"] = json!(all_files);
    result["bridgeConflict"] = json!(!all_bridges.is_empty());
    result["bridges"] = json!(all_bridges);
    result["error"] = apply_last["error"].clone();
    Ok(result)
}

/// claude 相关文件批量授权（一次弹窗列出全部；拒绝不阻断，读写失败各自显性）
fn authorize_claude_group(h: &WasmHost) -> anyhow::Result<()> {
    let home = HOME
        .get()
        .ok_or_else(|| anyhow::anyhow!("apply: home unavailable"))?;
    // 一次应用最多读 5 个文件（source_key 直读 settings、settings 读+写、
    // 两桥接文件存在性检查）：先批量授权整组，把「同一业务预见多个文件访问」
    // 收进一次授权，不再让每次 fs_read / fs_write 各自弹一次框
    let _ = h.fs_request_auth(&claude_auth_paths(home)).unwrap_or(false);
    Ok(())
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 目标列表：多目标去重保序 + 白名单；旧单数 `target` 仍可用
    #[test]
    fn parse_targets_accepts_multi_and_legacy_single() {
        assert_eq!(
            parse_targets(&json!({ "targets": ["pi", "opencode", "pi"] })).unwrap(),
            vec!["pi", "opencode"]
        );
        assert_eq!(
            parse_targets(&json!({ "target": "claude" })).unwrap(),
            vec!["claude"]
        );
        // codex 已开放（登记 provider + 设为当前模型）
        assert_eq!(
            parse_targets(&json!({ "targets": ["pi", "codex"] })).unwrap(),
            vec!["pi", "codex"]
        );
        // 空字符串与空白不构成目标
        assert!(parse_targets(&json!({ "targets": ["  "] })).is_err());
        assert!(parse_targets(&json!({})).is_err());
    }

    /// 白名单：白名单外（如拼错的 cli 名）一律拒绝
    #[test]
    fn parse_targets_rejects_unsupported() {
        let err = parse_targets(&json!({ "targets": ["pi", "cdoex"] })).unwrap_err();
        assert!(err.contains("cdoex"), "got: {err}");
        assert!(parse_targets(&json!({ "targets": ["gemini"] })).is_err());
    }

    /// 目标条目名：空 / 超长拒绝
    #[test]
    fn target_name_guard() {
        assert!(validate_target_name("InkStone").is_ok());
        assert!(validate_target_name("").is_err());
        assert!(validate_target_name(&"x".repeat(NAME_MAX + 1)).is_err());
        assert!(validate_target_name(&"x".repeat(NAME_MAX)).is_ok());
    }

    /// 端到端拼装（纯函数链）：pi 目标写入内容包含 key（目标配置的职责），
    /// 而返回给前端的状态载荷不含 key
    #[test]
    fn apply_pipeline_key_placement() {
        let key = "pi-inline-key-0123456789";
        let text = "{\n  \"providers\": {}\n}";
        let t = ensure_container(text, "providers").unwrap();
        let entry = merge_pi_entry(None, "u", "https://u/v1", "openai", &["m1".to_string()]);
        let pretty = serde_json::to_string_pretty(&entry).unwrap();
        let t = upsert_entry(&t, Some("providers"), "sensenova", &pretty).unwrap();
        assert!(parse_jsonc(&t).is_ok());
        assert!(!t.contains(key), "models.json entry itself carries no key");

        let auth_text = "{}";
        let auth_entry = serde_json::to_string(&pi_auth_entry(key)).unwrap();
        let out = upsert_entry(auth_text, None, "sensenova", &auth_entry).unwrap();
        assert!(out.contains(key), "auth.json is the designated key carrier");
        assert!(parse_jsonc(&out).is_ok());
    }

    /// fail-visible 的判定依据（写前自检的纯函数侧）：预设空模型 + 目标无既有
    /// 条目 → 合并结果 0 模型，必须被拒；反之有模型则放行
    #[test]
    fn empty_models_would_produce_model_less_entry() {
        let preset = Preset {
            name: "InkStone".to_string(),
            base_url: "https://b/v1".to_string(),
            api_style: "openai".to_string(),
            models: Vec::new(),
        };
        let fresh = merge_pi_entry(
            None,
            &preset.name,
            &preset.base_url,
            &preset.api_style,
            &preset.models,
        );
        assert_eq!(
            merged_model_count(&fresh),
            0,
            "must be rejected before write"
        );

        let existing =
            parse_jsonc(r#"{ "baseUrl": "https://old/v1", "models": [ { "id": "glm-5.2" } ] }"#)
                .unwrap();
        let merged = merge_pi_entry(
            Some(&existing),
            &preset.name,
            &preset.base_url,
            &preset.api_style,
            &preset.models,
        );
        assert_eq!(
            merged_model_count(&merged),
            1,
            "existing entry keeps its models → apply stays allowed"
        );
    }
}
