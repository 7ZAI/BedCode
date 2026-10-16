//! 应用条目构造（纯函数）
//!
//! 各 CLI 目标条的「合并既有条目 + 覆盖受控字段」：pi providers /
//! opencode provider 的 models 按身份合并（既有的保留、新增的补最小定义），
//! key 仅在显式提供时覆盖（keyMode=none 保留目标既有凭据）；claude 的
//! env 块条目构造与只读视图（token 掩码）。
//!
//! 覆盖规则里有一处**刻意不覆盖**：预设 apiStyle 只到「openai 家族」这一层，
//! 目标条目已有同家族方言（pi `openai-responses` / opencode `@ai-sdk/openai`）
//! 时保持原样——预设无法表达具体方言，盲目改写会把用户已调通的端点降级成
//! 另一种（completions）而症状极隐蔽（能连上、行为不同）。跨家族
//! （anthropic / gemini）仍然照预设切换。

use super::mapping::{
    is_openai_family_api, is_openai_family_npm, mask_key, opencode_npm_of, pi_api_of,
};
use serde_json::{json, Map, Value};

// ==================== 应用条目构造（纯函数） ====================

/// pi providers 条目：合并既有条目（保留用户模型定义等未知字段），
/// 覆盖 name/baseUrl；api 按家族规则覆盖（同家族不降级）；models 按 id
/// 合并（既有的保留，新增的补最小定义）
pub(crate) fn merge_pi_entry(
    existing: Option<&Value>,
    name: &str,
    base_url: &str,
    api_style: &str,
    models: &[String],
) -> Value {
    let mut entry = match existing {
        Some(v) if v.is_object() => v.as_object().unwrap().clone(),
        _ => Map::new(),
    };
    if !name.is_empty() {
        entry.insert("name".to_string(), json!(name));
    }
    entry.insert("baseUrl".to_string(), json!(base_url));
    if !keep_existing_pi_api(&entry, api_style) {
        entry.insert("api".to_string(), json!(pi_api_of(api_style)));
    }
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

/// 预设 openai 家族 + 既有条目同家族方言 → 保留既有 `api`
fn keep_existing_pi_api(entry: &Map<String, Value>, api_style: &str) -> bool {
    api_style == "openai"
        && entry
            .get("api")
            .and_then(|v| v.as_str())
            .is_some_and(is_openai_family_api)
}

/// pi auth.json 条目
pub(crate) fn pi_auth_entry(key: &str) -> Value {
    json!({ "type": "api_key", "key": key })
}

/// opencode provider 条目：合并既有条目；`name` 覆盖、npm 按家族规则覆盖
/// （同 openai 家族不降级）；options.baseURL 覆盖、apiKey 仅在有 key 时覆盖
/// （keyMode=none 时保留用户既有 key）；models 按 key 合并
pub(crate) fn merge_opencode_entry(
    existing: Option<&Value>,
    name: &str,
    base_url: &str,
    api_style: &str,
    models: &[String],
    key: Option<&str>,
) -> Value {
    let mut entry = match existing {
        Some(v) if v.is_object() => v.as_object().unwrap().clone(),
        _ => Map::new(),
    };
    if !name.is_empty() {
        entry.insert("name".to_string(), json!(name));
    }
    if !keep_existing_opencode_npm(&entry, api_style) {
        entry.insert("npm".to_string(), json!(opencode_npm_of(api_style)));
    }
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

/// 预设 openai 家族 + 既有条目同家族包 → 保留既有 `npm`
fn keep_existing_opencode_npm(entry: &Map<String, Value>, api_style: &str) -> bool {
    api_style == "openai"
        && entry
            .get("npm")
            .and_then(|v| v.as_str())
            .is_some_and(is_openai_family_npm)
}

/// 条目合并后的模型数（写前自检：0 个模型的条目对 pi / opencode 不可用）
pub(crate) fn merged_model_count(entry: &Value) -> usize {
    match entry.get("models") {
        Some(Value::Array(a)) => a.len(),
        // opencode 的 models 是对象（键即 id），pi 的是数组
        Some(Value::Object(o)) => o.len(),
        _ => 0,
    }
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
            "新名字",
            "https://new/v1",
            "anthropic",
            &["glm-5.2".to_string(), "claude-sonnet-4".to_string()],
        );
        assert_eq!(merged["baseUrl"], "https://new/v1");
        assert_eq!(merged["api"], "anthropic-messages");
        assert_eq!(merged["name"], "新名字");
        let models = merged["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        // 既有定义保留（reasoning/contextWindow 未丢）
        assert_eq!(models[0]["contextWindow"], 1048576);
        assert_eq!(models[1]["id"], "claude-sonnet-4");
        assert_eq!(models[1]["name"], "claude-sonnet-4");

        // 无既有条目 → 最小条目
        let fresh = merge_pi_entry(None, "u", "https://u/v1", "openai", &["m1".to_string()]);
        assert_eq!(fresh["api"], "openai-completions");
        assert_eq!(fresh["models"][0]["id"], "m1");

        // 预设无名字（空白）→ 不写 name 键（不拿空串覆盖用户既有展示名）
        let unnamed = merge_pi_entry(None, "", "https://u/v1", "openai", &["m1".to_string()]);
        assert!(
            unnamed.get("name").is_none(),
            "blank name must not overwrite"
        );
    }

    /// 方言保护：预设只到「openai 家族」层，跨家族照预设切、同家族保留既有方言。
    ///
    /// 反例（回归锁）：目标条目已是 `openai-responses`，预设 apiStyle=openai 时
    /// 若改写成 `openai-completions`，用户已调通的 responses 端点被静默降级。
    #[test]
    fn merge_pi_entry_never_downgrades_openai_dialect() {
        let responses =
            parse_jsonc(r#"{ "api": "openai-responses", "models": [ { "id": "m1" } ] }"#).unwrap();
        let kept = merge_pi_entry(
            Some(&responses),
            "u",
            "https://new/v1",
            "openai",
            &["m2".to_string()],
        );
        assert_eq!(
            kept["api"], "openai-responses",
            "same openai family must keep the existing dialect"
        );

        // 跨家族：预设要 anthropic 就切（受控字段仍由预设说了算）
        let switched = merge_pi_entry(Some(&responses), "u", "https://b/v1", "anthropic", &[]);
        assert_eq!(switched["api"], "anthropic-messages");

        // 无既有条目 → 落预设默认方言
        let fresh = merge_pi_entry(None, "u", "https://b/v1", "openai", &[]);
        assert_eq!(fresh["api"], "openai-completions");
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
            "GMI 新名字",
            "https://new/v1",
            "openai",
            &["openai/gpt-5".to_string(), "openai/gpt-5.5".to_string()],
            None,
        );
        assert_eq!(merged["options"]["baseURL"], "https://new/v1");
        assert_eq!(merged["name"], "GMI 新名字");
        assert_eq!(merged["npm"], "@ai-sdk/openai-compatible");
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
            "GMI",
            "https://new/v1",
            "openai",
            &[],
            Some("fresh"),
        );
        assert_eq!(merged["options"]["apiKey"], "fresh");
    }

    /// opencode 方言保护：既有 `@ai-sdk/openai`（responses 包）不被预设降级成
    /// openai-compatible；跨家族（anthropic）照预设切
    #[test]
    fn merge_opencode_entry_never_downgrades_openai_package() {
        let responses = parse_jsonc(
            r#"{ "npm": "@ai-sdk/openai", "options": { "baseURL": "https://old/v1" } }"#,
        )
        .unwrap();
        let kept = merge_opencode_entry(
            Some(&responses),
            "u",
            "https://new/v1",
            "openai",
            &["m1".to_string()],
            None,
        );
        assert_eq!(kept["npm"], "@ai-sdk/openai");
        let switched = merge_opencode_entry(
            Some(&responses),
            "u",
            "https://new/v1",
            "anthropic",
            &[],
            None,
        );
        assert_eq!(switched["npm"], "@ai-sdk/anthropic");
    }

    /// 合并后模型数（写前自检用）：pi 数组 / opencode 对象 / 缺失均计入
    #[test]
    fn merged_model_count_counts_both_shapes() {
        assert_eq!(merged_model_count(&json!({})), 0);
        assert_eq!(merged_model_count(&json!({ "models": [] })), 0);
        assert_eq!(merged_model_count(&json!({ "models": [{ "id": "a" }] })), 1);
        assert_eq!(
            merged_model_count(&json!({ "models": { "a": {}, "b": {} } })),
            2
        );
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
