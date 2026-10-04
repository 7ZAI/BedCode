//! 模型列表查询（可选 URL + 手动输入互补）
//!
//! 预设的 `modelsUrl` 由用户在编辑器里填（缺省可由 baseUrl 派生，见前端
//! `defaultModelsUrl`）；本模块只做「取 URL → 宿主代发 GET → 抽模型 id」，
//! 形状解析覆盖三类常见方言（`data[].id` OpenAI/Anthropic、`models[].name`
//! Gemini（去 `models/` 前缀）、根数组），解析不出显性报错——**不做静默
//! 空列表**：空列表会被应用流程当成「这个供应商就是 0 个模型」写进目标配置，
//! 症状是目标 CLI 里看不到任何模型（2026-10-04 实测踩过）。
//!
//! key 纪律：明文只在命令在途（`apiKey` 入参或中心库现读），只进 Authorization
//! 头，不进返回载荷、不进日志；出站请求照宿主 `host-http` 的授权闸门走
//! （未授权 → 拒绝，不静默直连）。

use super::store::preset_key_of;
use super::MODELS_MAX;
use bedcode_plugin_api::host::{HostHttp, HostLog};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

// ==================== URL 与形状校验（纯函数） ====================

/// 模型查询 URL 校验：仅 http/https（挡掉 `file://` / `ftp://` 等宿主代发
/// 不该接受的 scheme），非空校验
pub(crate) fn validate_models_url(url: &str) -> Result<&str, String> {
    let u = url.trim();
    if u.is_empty() {
        return Err("models URL required".to_string());
    }
    if !(u.starts_with("https://") || u.starts_with("http://")) {
        return Err("models URL must be http(s)".to_string());
    }
    Ok(u)
}

/// 从响应体抽模型 id（纯函数，返回去重且保持原序的列表）
///
/// 覆盖三类形状：`data[].id`（OpenAI / Anthropic）、`models[].name|.id`
/// （Gemini，`models/` 前缀去掉）、根数组（裸字符串或 `{id|name}` 对象）。
pub(crate) fn extract_model_ids(body: &str) -> Result<Vec<String>, String> {
    let parsed: Value = super::jsonc::parse_jsonc(body)
        .map_err(|e| format!("model list is not valid JSON: {e}"))?;
    // 根对象：先认 data，再认 models，都不是 → 形状不认识（显性报错，不猜）
    if let Some(obj) = parsed.as_object() {
        for field in ["data", "models"] {
            if let Some(arr) = obj.get(field).and_then(|v| v.as_array()) {
                return Ok(finish(ids_of(arr)));
            }
        }
        return Err("model list shape unrecognized (expected data[] / models[])".to_string());
    }
    let arr = parsed
        .as_array()
        .ok_or_else(|| "model list shape unrecognized (expected an array)".to_string())?;
    Ok(finish(ids_of(arr)))
}

/// 数组 → id 列表（`id` / `name` / 裸字符串；Gemini 的 `models/x` 去前缀）
fn ids_of(arr: &[Value]) -> Vec<String> {
    arr.iter()
        .filter_map(|item| match item {
            Value::String(s) => Some(s.clone()),
            Value::Object(o) => o
                .get("id")
                .or_else(|| o.get("name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            _ => None,
        })
        .map(|s| strip_gemini_prefix(s.trim()))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Gemini 形状的 `models/gemini-2.0-flash` → `gemini-2.0-flash`
fn strip_gemini_prefix(id: &str) -> String {
    id.strip_prefix("models/").unwrap_or(id).to_string()
}

/// 去重（保持原序） + 上限截断（[`MODELS_MAX`]）
fn finish(ids: Vec<String>) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for id in ids {
        if !seen.contains(&id) {
            seen.push(id);
        }
    }
    seen.truncate(MODELS_MAX);
    seen
}

// ==================== 命令：查询模型列表 ====================

/// 查询模型列表：`{ url, presetId?, apiKey? }`
///
/// - `url`：用户填的模型查询 URL（必填，http/https）
/// - `presetId`：给了就用中心库已存 key（guest 现读，明文不进 wire）
/// - `apiKey`：现场输入的 key（缺省优先用 presetId 的已存 key；两者皆无 → 不带
///   Authorization，部分网关的 /models 允许匿名）
///
/// 返回 `{ models, url, count }`（模型 id 数组，不含任何凭据）
pub(crate) fn fetch_models(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let url = validate_models_url(args.get("url").and_then(|v| v.as_str()).unwrap_or(""))
        .map_err(|e| anyhow::anyhow!("fetch-models: {e}"))?
        .to_string();
    let key = match args.get("apiKey").and_then(|v| v.as_str()) {
        Some(k) if !k.trim().is_empty() => k.trim().to_string(),
        _ => match args.get("presetId").and_then(|v| v.as_i64()) {
            Some(id) => preset_key_of(h, id)?,
            None => String::new(),
        },
    };

    let mut headers = serde_json::Map::new();
    headers.insert("Accept".to_string(), json!("application/json"));
    if !key.is_empty() {
        headers.insert("Authorization".to_string(), json!(format!("Bearer {key}")));
    }
    let request = json!({ "method": "GET", "url": url, "headers": Value::Object(headers) });
    let response = h
        .http_fetch(&request)
        .map_err(|e| anyhow::anyhow!("fetch-models: request failed: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("fetch-models: empty response"))?;
    let status = response.get("status").and_then(|v| v.as_u64()).unwrap_or(0);
    let body = response.get("body").and_then(|v| v.as_str()).unwrap_or("");
    if !(200..300).contains(&status) {
        // 响应体可能含服务端诊断原文，不回传给前端（只进日志，界面给通用文案）
        h.log_warn(&format!("fetch-models: endpoint returned status {status}"));
        return Err(anyhow::anyhow!("fetch-models: endpoint status {status}"));
    }
    let models = extract_model_ids(body).map_err(|e| anyhow::anyhow!("fetch-models: {e}"))?;
    if models.is_empty() {
        return Err(anyhow::anyhow!("fetch-models: no models found"));
    }
    h.log_info(&format!(
        "models fetched (count = {}, key_len = {:?})",
        models.len(),
        if key.is_empty() {
            None
        } else {
            Some(key.chars().count())
        }
    ));
    Ok(json!({ "models": models, "url": url, "count": models.len() }))
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// URL 校验：仅 http/https 放行（`file://` 之类显性拒绝）
    #[test]
    fn models_url_scheme_guard() {
        assert!(validate_models_url("https://a.example.com/v1/models").is_ok());
        assert!(validate_models_url("http://127.0.0.1:8000/v1/models").is_ok());
        assert!(validate_models_url("  https://a/v1/models  ").is_ok());
        assert!(validate_models_url("").is_err());
        assert!(validate_models_url("file:///etc/passwd").is_err());
        assert!(validate_models_url("ftp://a/v1/models").is_err());
        assert!(validate_models_url("//a/v1/models").is_err());
    }

    /// OpenAI / Anthropic 形状：`data[].id`，去重保序
    #[test]
    fn extracts_openai_data_ids() {
        let body = r#"{"object":"list","data":[{"id":"b"},{"id":"a"},{"id":"b"}]}"#;
        assert_eq!(extract_model_ids(body).unwrap(), vec!["b", "a"]);
    }

    /// Gemini 形状：`models[].name` 且带 `models/` 前缀 → 去前缀
    #[test]
    fn extracts_gemini_model_names() {
        let body =
            r#"{"models":[{"name":"models/gemini-2.0-flash"},{"name":"models/gemini-2.5-pro"}]}"#;
        assert_eq!(
            extract_model_ids(body).unwrap(),
            vec!["gemini-2.0-flash", "gemini-2.5-pro"]
        );
    }

    /// 根数组（裸字符串 / `{id}` 对象混排）
    #[test]
    fn extracts_bare_array_ids() {
        let body = r#"["m1", {"id": "m2"}, {"name": "m3"}, 42, "", {"foo": 1}]"#;
        assert_eq!(extract_model_ids(body).unwrap(), vec!["m1", "m2", "m3"]);
    }

    /// 形状不认识 / 非法 JSON → 显性报错（**不**返回空列表：空列表会被应用
    /// 流程当成「该供应商就是 0 个模型」写进目标配置）
    #[test]
    fn rejects_unknown_shape_and_broken_json() {
        assert!(extract_model_ids(r#"{"object":"list"}"#).is_err());
        assert!(extract_model_ids(r#"{"result":{"models":["m"]}}"#).is_err());
        assert!(extract_model_ids("not json").is_err());
        // 形状对但列表为空：解析成功，由调用方按「没查到模型」显性报错
        assert_eq!(
            extract_model_ids(r#"{"data":[]}"#).unwrap(),
            Vec::<String>::new()
        );
    }

    /// 上限截断：超出 MODELS_MAX 只留前 N 个（顺序稳定）
    #[test]
    fn truncates_to_models_max() {
        let ids: Vec<String> = (0..MODELS_MAX + 50).map(|i| format!("m{i}")).collect();
        let arr = Value::Array(ids.iter().map(|s| json!({ "id": s })).collect());
        let out = extract_model_ids(&arr.to_string()).unwrap();
        assert_eq!(out.len(), MODELS_MAX);
        assert_eq!(out[0], "m0");
        assert_eq!(out[MODELS_MAX - 1], format!("m{}", MODELS_MAX - 1));
    }
}
