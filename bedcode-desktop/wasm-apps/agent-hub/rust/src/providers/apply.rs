//! 应用到目标 CLI（同步命令）
//!
//! key 来源四选一：stored 中心库已存（默认）/ inline 现场输入 / source 内存
//! 直拷（现读源配置，不落存储/日志）/ none 不带 key（pi 的 auth.json 与
//! 既有 apiKey 均保留）。claude 桥接冲突时阻止（force = 用户确认后仅写
//! env 块，桥接文件永不触碰）。真源始终是 CLI 自己的配置文件。

use super::jsonc::{ensure_container, parse_jsonc, upsert_entry};
use super::merge::{claude_env_entries, merge_opencode_entry, merge_pi_entry, pi_auth_entry};
use super::paths::{
    bridge_paths, claude_auth_paths, claude_settings_path, opencode_cfg_path, pi_auth_path,
    pi_models_path,
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

// ==================== 命令：应用到目标 CLI ====================

/// 目标条目名合法性（作为 JSON 对象键写入各 CLI 配置）
fn validate_target_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.chars().count() > NAME_MAX {
        return Err("target name empty or too long".to_string());
    }
    Ok(())
}

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

/// 应用预设到目标 CLI（同步命令）。key 来源四选一：stored 中心库已存（默认）/
/// inline 现场输入 / source 内存直拷（现读源配置）/ none 不带 key（pi 的
/// auth.json 与既有 apiKey 均保留）。claude 桥接冲突时阻止（force = 用户
/// 确认，仅写 env 块）
pub(crate) fn apply_provider(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let id = args
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("apply: missing preset id"))?;
    let target = args
        .get("target")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if !matches!(target.as_str(), "claude" | "pi" | "opencode") {
        // codex config.toml 官方格式未校准（spec 开放问题 3），v1 不开放
        return Err(anyhow::anyhow!("apply: unsupported target {target}"));
    }
    let preset = list_presets(h)?
        .into_iter()
        .find(|p| p.get("id").and_then(|v| v.as_i64()) == Some(id))
        .ok_or_else(|| anyhow::anyhow!("apply: preset {id} not found"))?;
    let name = preset
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let base_url = preset
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let api_style = preset
        .get("apiStyle")
        .and_then(|v| v.as_str())
        .unwrap_or("openai")
        .to_string();
    let models: Vec<String> = preset
        .get("models")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let target_name = args
        .get("targetName")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| name.clone());
    validate_target_name(&target_name)
        .map_err(|e| anyhow::anyhow!("apply: invalid target name: {e}"))?;

    let key_mode = args
        .get("key")
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("none")
        .to_string();
    let home = HOME
        .get()
        .ok_or_else(|| anyhow::anyhow!("apply: home unavailable"))?;
    // claude 目标（或 key 源为 claude）一次应用会读写最多 5 个文件（source_key
    // 直读 settings、settings 读+写、两桥接文件存在性检查）：先批量授权整组
    // （一次弹窗列出全部，已授权路径宿主静默跳过），把「同一业务预见多个文件
    // 访问」收进一次授权，不再让每次 fs_read / fs_write 各自弹一次框。
    // 拒绝不阻断流程：后续读写的显性失败文案照常给出（fail-visible）。
    let key_source_claude = matches!(key_mode.as_str(), "source")
        && args
            .get("key")
            .and_then(|k| k.get("cli"))
            .and_then(|v| v.as_str())
            == Some("claude");
    if target.as_str() == "claude" || key_source_claude {
        let _ = h.fs_request_auth(&claude_auth_paths(&home)).unwrap_or(false);
    }
    let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
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

    let mut files: Vec<String> = Vec::new();

    match target.as_str() {
        "claude" => {
            let bridges = bridge_paths(home);
            let found: Vec<String> = bridges
                .iter()
                .filter(|p| h.fs_exists(p).unwrap_or(false))
                .map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
                .collect();
            if !found.is_empty() && !force {
                // 桥接冲突：阻止写入（桥接文件永不触碰），由用户确认后再写 env
                h.log_warn("apply: claude bridge detected; write blocked");
                return Ok(json!({ "applied": false, "bridgeConflict": true, "bridges": found }));
            }
            let path = claude_settings_path(home);
            let text = h
                .fs_read(&path)
                .map_err(|e| anyhow::anyhow!("apply: read settings.json failed: {e}"))?
                .unwrap_or_else(|| "{}".to_string());
            let mut t = ensure_container(&text, "env")
                .map_err(|e| anyhow::anyhow!("apply: claude env container: {e}"))?;
            for (k, v) in claude_env_entries(
                &base_url,
                key.as_deref(),
                models.first().map(|s| s.as_str()),
            ) {
                t = upsert_entry(&t, Some("env"), k, &v.to_string())
                    .map_err(|e| anyhow::anyhow!("apply: claude env upsert {k}: {e}"))?;
            }
            h.fs_write(&path, &t)
                .map_err(|e| anyhow::anyhow!("apply: write settings.json failed: {e}"))?;
            files.push("settings.json".to_string());
        }
        "pi" => {
            let path = pi_models_path(home);
            let text = h
                .fs_read(&path)
                .map_err(|e| anyhow::anyhow!("apply: read models.json failed: {e}"))?
                .unwrap_or_else(|| "{\n  \"providers\": {}\n}".to_string());
            let t = ensure_container(&text, "providers")
                .map_err(|e| anyhow::anyhow!("apply: pi providers container: {e}"))?;
            // 合并既有条目（保留用户模型定义）， pretty 序列化与 pi 既有排版一致
            let existing = parse_jsonc(&t).ok().and_then(|v| {
                v.get("providers")
                    .and_then(|p| p.get(&target_name))
                    .cloned()
            });
            let entry = merge_pi_entry(existing.as_ref(), &base_url, &api_style, &models);
            let pretty = serde_json::to_string_pretty(&entry)
                .map_err(|e| anyhow::anyhow!("apply: serialize pi entry: {e}"))?;
            let t = upsert_entry(&t, Some("providers"), &target_name, &pretty)
                .map_err(|e| anyhow::anyhow!("apply: pi providers upsert: {e}"))?;
            h.fs_write(&path, &t)
                .map_err(|e| anyhow::anyhow!("apply: write models.json failed: {e}"))?;
            files.push("models.json".to_string());
            // key 条目：无 key 时不动 auth.json（保留既有凭据）
            if let Some(k) = &key {
                let auth_path = pi_auth_path(home);
                let auth_text = h
                    .fs_read(&auth_path)
                    .map_err(|e| anyhow::anyhow!("apply: read auth.json failed: {e}"))?
                    .unwrap_or_else(|| "{}".to_string());
                let auth_entry = serde_json::to_string(&pi_auth_entry(k))
                    .map_err(|e| anyhow::anyhow!("apply: serialize auth entry: {e}"))?;
                let t = upsert_entry(&auth_text, None, &target_name, &auth_entry)
                    .map_err(|e| anyhow::anyhow!("apply: auth.json upsert: {e}"))?;
                h.fs_write(&auth_path, &t)
                    .map_err(|e| anyhow::anyhow!("apply: write auth.json failed: {e}"))?;
                files.push("auth.json".to_string());
            }
        }
        "opencode" => {
            let path = opencode_cfg_path(home);
            let text = h
                .fs_read(&path)
                .map_err(|e| anyhow::anyhow!("apply: read opencode.json failed: {e}"))?
                .unwrap_or_else(|| "{}".to_string());
            let t = ensure_container(&text, "provider")
                .map_err(|e| anyhow::anyhow!("apply: opencode provider container: {e}"))?;
            let existing = parse_jsonc(&t)
                .ok()
                .and_then(|v| v.get("provider").and_then(|p| p.get(&target_name)).cloned());
            let entry = merge_opencode_entry(
                existing.as_ref(),
                &base_url,
                &api_style,
                &models,
                key.as_deref(),
            );
            let pretty = serde_json::to_string_pretty(&entry)
                .map_err(|e| anyhow::anyhow!("apply: serialize opencode entry: {e}"))?;
            let t = upsert_entry(&t, Some("provider"), &target_name, &pretty)
                .map_err(|e| anyhow::anyhow!("apply: opencode provider upsert: {e}"))?;
            h.fs_write(&path, &t)
                .map_err(|e| anyhow::anyhow!("apply: write opencode.json failed: {e}"))?;
            files.push("opencode.json".to_string());
        }
        _ => unreachable!("target validated above"),
    }

    let apply_last = json!({
        "ok": true,
        "preset": name,
        "target": target,
        "files": files,
        "keyMode": key_mode,
        "keyLen": key.as_ref().map(|k| k.chars().count()),
        "error": Value::Null,
        "at": now_ms(h).unwrap_or(0),
    });
    let (import_last, _) = read_stored(h);
    write_stored(h, &import_last, &apply_last);
    // 日志纪律：key 只记长度（spec §6）
    h.log_info(&format!(
        "provider applied (preset = {name}, target = {target}, key_len = {:?})",
        apply_last["keyLen"]
    ));
    let state = build_state(h)?;
    let mut result = emit_and_return(h, &state)?;
    result["applied"] = json!(true);
    result["files"] = json!(files);
    Ok(result)
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;
    /// 端到端拼装（纯函数链）：pi 目标写入内容包含 key（目标配置的职责），
    /// 而返回给前端的状态载荷不含 key
    #[test]
    fn apply_pipeline_key_placement() {
        let key = "pi-inline-key-0123456789";
        let text = "{\n  \"providers\": {}\n}";
        let t = ensure_container(text, "providers").unwrap();
        let entry = merge_pi_entry(None, "https://u/v1", "openai", &["m1".to_string()]);
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
}
