//! 应用条目构造（纯函数）
//!
//! 各 CLI 目标条的「合并既有条目 + 覆盖受控字段」：pi providers /
//! opencode provider 的 models 按身份合并（既有的保留、新增的补最小定义），
//! key 仅在显式提供时覆盖（keyMode=none 保留目标既有凭据）；claude 的
//! env 块条目构造与只读视图（token 掩码）。

use super::mapping::{mask_key, opencode_npm_of, pi_api_of};
use serde_json::{json, Map, Value};

// ==================== 应用条目构造（纯函数） ====================

/// pi providers 条目：合并既有条目（保留用户模型定义等未知字段），
/// 覆盖 name/baseUrl/api；models 按 id 合并（既有的保留，新增的补最小定义）
pub(crate) fn merge_pi_entry(
    existing: Option<&Value>,
    base_url: &str,
    api_style: &str,
    models: &[String],
) -> Value {
    let mut entry = match existing {
        Some(v) if v.is_object() => v.as_object().unwrap().clone(),
        _ => Map::new(),
    };
    entry.insert("baseUrl".to_string(), json!(base_url));
    entry.insert("api".to_string(), json!(pi_api_of(api_style)));
    let mut merged: Vec<Value> = entry
        .get("models")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for id in models {
        if !merged
            .iter()
            .any(|m| m.get("id").and_then(|v| v.as_str()) == Some(id.as_str()))
        {
            merged.push(json!({ "id": id, "name": id }));
        }
    }
    if !merged.is_empty() {
        entry.insert("models".to_string(), Value::Array(merged));
    }
    Value::Object(entry)
}

/// pi auth.json 条目
pub(crate) fn pi_auth_entry(key: &str) -> Value {
    json!({ "type": "api_key", "key": key })
}

/// opencode provider 条目：合并既有条目；npm 由 apiStyle 决定；options.baseURL
/// 覆盖、apiKey 仅在有 key 时覆盖（keyMode=none 时保留用户既有 key）；
/// models 按 key 合并
pub(crate) fn merge_opencode_entry(
    existing: Option<&Value>,
    base_url: &str,
    api_style: &str,
    models: &[String],
    key: Option<&str>,
) -> Value {
    let mut entry = match existing {
        Some(v) if v.is_object() => v.as_object().unwrap().clone(),
        _ => Map::new(),
    };
    entry.insert("npm".to_string(), json!(opencode_npm_of(api_style)));
    let mut options = entry
        .get("options")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    options.insert("baseURL".to_string(), json!(base_url));
    if let Some(k) = key {
        options.insert("apiKey".to_string(), json!(k));
    }
    entry.insert("options".to_string(), Value::Object(options));
    let mut merged = entry
        .get("models")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    for id in models {
        merged
            .entry(id.clone())
            .or_insert_with(|| json!({ "name": id }));
    }
    if !merged.is_empty() {
        entry.insert("models".to_string(), Value::Object(merged));
    }
    Value::Object(entry)
}

/// claude settings.json `env` 块条目：key 未提供时不生成 AUTH_TOKEN 键
/// （保留用户既有 token）；MODEL 取预设首个模型
pub(crate) fn claude_env_entries(
    base_url: &str,
    key: Option<&str>,
    model: Option<&str>,
) -> Vec<(&'static str, Value)> {
    let mut entries = vec![("ANTHROPIC_BASE_URL", json!(base_url))];
    if let Some(k) = key {
        entries.push(("ANTHROPIC_AUTH_TOKEN", json!(k)));
    }
    if let Some(m) = model {
        entries.push(("ANTHROPIC_MODEL", json!(m)));
    }
    entries
}

/// claude settings.json env 只读视图（掩码）
pub(crate) fn claude_env_view(settings: &Value) -> Value {
    let env = settings.get("env");
    let val = |name: &str| env.and_then(|e| e.get(name)).and_then(|v| v.as_str());
    json!({
        "baseUrl": val("ANTHROPIC_BASE_URL"),
        "model": val("ANTHROPIC_MODEL"),
        "authTokenMask": val("ANTHROPIC_AUTH_TOKEN").map(mask_key),
    })
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::super::jsonc::parse_jsonc;
    use super::*;
    /// pi 条目合并：既有模型的完整定义保留，新增 id 补最小定义，baseUrl/api 覆盖
    #[test]
    fn merge_pi_entry_preserves_user_models() {
        let existing = parse_jsonc(
            r#"{
  "name": "商汤日日新 SenseNova",
  "baseUrl": "https://old/v1",
  "api": "openai-completions",
  "models": [ { "id": "glm-5.2", "reasoning": true, "contextWindow": 1048576 } ]
}"#,
        )
        .unwrap();
        let merged = merge_pi_entry(
            Some(&existing),
            "https://new/v1",
            "anthropic",
            &["glm-5.2".to_string(), "claude-sonnet-4".to_string()],
        );
        assert_eq!(merged["baseUrl"], "https://new/v1");
        assert_eq!(merged["api"], "anthropic-messages");
        let models = merged["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        // 既有定义保留（reasoning/contextWindow 未丢）
        assert_eq!(models[0]["contextWindow"], 1048576);
        assert_eq!(models[1]["id"], "claude-sonnet-4");
        assert_eq!(models[1]["name"], "claude-sonnet-4");

        // 无既有条目 → 最小条目
        let fresh = merge_pi_entry(None, "https://u/v1", "openai", &["m1".to_string()]);
        assert_eq!(fresh["api"], "openai-completions");
        assert_eq!(fresh["models"][0]["id"], "m1");
    }

    /// opencode 条目合并：keyMode=none 时保留既有 apiKey；models 按 key 合并
    #[test]
    fn merge_opencode_entry_preserves_key_when_none() {
        let existing = parse_jsonc(
            r#"{
  "name": "GMI",
  "npm": "@ai-sdk/openai-compatible",
  "options": { "apiKey": "existing-key", "baseURL": "https://old/v1", "setCacheKey": true },
  "models": { "openai/gpt-5": { "name": "GPT-5" } }
}"#,
        )
        .unwrap();
        let merged = merge_opencode_entry(
            Some(&existing),
            "https://new/v1",
            "openai",
            &["openai/gpt-5".to_string(), "openai/gpt-5.5".to_string()],
            None,
        );
        assert_eq!(merged["options"]["baseURL"], "https://new/v1");
        assert_eq!(
            merged["options"]["apiKey"], "existing-key",
            "no-key apply must keep existing"
        );
        assert_eq!(merged["options"]["setCacheKey"], true);
        assert_eq!(merged["models"]["openai/gpt-5"]["name"], "GPT-5");
        assert_eq!(merged["models"]["openai/gpt-5.5"]["name"], "openai/gpt-5.5");

        // 带 key 覆盖
        let merged = merge_opencode_entry(
            Some(&existing),
            "https://new/v1",
            "openai",
            &[],
            Some("fresh"),
        );
        assert_eq!(merged["options"]["apiKey"], "fresh");
    }

    /// claude env 条目：key 缺省不生成 AUTH_TOKEN（保留既有 token）；MODEL 取首模型
    #[test]
    fn claude_env_entries_shape() {
        let entries = claude_env_entries("https://b/v1", Some("tok"), Some("m1"));
        assert_eq!(entries[0].0, "ANTHROPIC_BASE_URL");
        assert_eq!(entries[1].0, "ANTHROPIC_AUTH_TOKEN");
        assert_eq!(entries[2].0, "ANTHROPIC_MODEL");
        let entries = claude_env_entries("https://b/v1", None, None);
        assert_eq!(entries.len(), 1);
    }

    /// claude env 只读视图：token 掩码、base/model 原样（非敏感）
    #[test]
    fn claude_env_view_masks_token() {
        let settings = parse_jsonc(
            r#"{
  "model": "haiku",
  "env": {
    "ANTHROPIC_BASE_URL": "https://bridge.local/v1",
    "ANTHROPIC_AUTH_TOKEN": "bridge-token-0123456789",
    "ANTHROPIC_MODEL": "sonnet",
    "OTHER_VAR": "keep"
  }
}"#,
        )
        .unwrap();
        let view = claude_env_view(&settings);
        assert_eq!(view["baseUrl"], "https://bridge.local/v1");
        assert_eq!(view["model"], "sonnet");
        assert_eq!(view["authTokenMask"], "bri…(23)");
        // 视图不含 token 明文
        assert!(!view.to_string().contains("bridge-token-0123456789"));
    }
}
