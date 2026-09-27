//! 反向导入提取（纯函数）
//!
//! 读各 CLI 现有配置（pi `models.json` providers + `auth.json`、opencode
//! `opencode.json` `provider.*`；claude 只读展示不生成预设）生成 [`PresetDraft`]
//! 并把源 key 明文收进中心凭据库（掩码随行供导入结果回显）；`plan_inserts`
//! 做同名去重计划（pi 与 opencode 常有同名 provider，如 sensenova）。
//!
//! key 纪律：`PresetDraft.key` 是明文（仅导入入库用，永不进 wire/日志）；
//! `Debug` 实现经 `mask_key` 掩码化，任何 `{{:?}}` 输出不泄明文。

use super::mapping::{map_opencode_npm, map_pi_api, mask_key};
use serde_json::Value;
use std::fmt;

// ==================== 反向导入提取（纯函数） ====================

/// 导入草稿（写库前的中间形态；v2 起携带源 key 明文——导入直接入中心凭据库；
/// 掩码随改名走，同名去重改名的预设其掩码仍能对上）
#[derive(PartialEq)]
pub(crate) struct PresetDraft {
    pub name: String,
    pub base_url: String,
    pub api_style: String,
    pub models: Vec<String>,
    pub notes: String,
    /// 源 key 明文（无 key 为空串；仅导入入库用，永不进 wire/日志）
    pub key: String,
    /// 源 key 掩码（无 key 可直拷为 "—"；导入结果回显用）
    pub key_mask: String,
}

/// Debug 掩码化草稿 key：任何 `{:?}` 输出（测试断言/误打的调试日志）不泄明文
impl fmt::Debug for PresetDraft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PresetDraft")
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .field("api_style", &self.api_style)
            .field("models", &self.models)
            .field("notes", &self.notes)
            .field("key", &mask_key(&self.key))
            .field("key_mask", &self.key_mask)
            .finish()
    }
}

/// pi 配置 → 预设草稿（掩码内嵌）。models.json providers.<key>：
/// name/baseUrl/api/models[].id；auth.json 同名条目的 key 只取掩码
pub(crate) fn presets_from_pi(models: &Value, auth: &Value) -> Vec<PresetDraft> {
    let mut drafts = Vec::new();
    let Some(providers) = models.get("providers").and_then(|v| v.as_object()) else {
        return drafts;
    };
    for (key, p) in providers {
        let base_url = p
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let api_style = p
            .get("api")
            .and_then(|v| v.as_str())
            .map(map_pi_api)
            .unwrap_or("openai")
            .to_string();
        let models: Vec<String> = p
            .get("models")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        // v2：源 key 明文收进中心凭据库（掩码随行供导入结果回显）
        let raw_key = auth
            .get(key)
            .and_then(|p| p.get("key"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        drafts.push(PresetDraft {
            name: key.clone(),
            base_url,
            api_style,
            models,
            notes: format!("pi:{key}"),
            key: raw_key.clone(),
            key_mask: mask_key(&raw_key),
        });
    }
    drafts
}

/// opencode 配置 → 预设草稿（掩码内嵌）。
/// provider.<key>：options.baseURL / npm / models 对象键
pub(crate) fn presets_from_opencode(cfg: &Value) -> Vec<PresetDraft> {
    let mut drafts = Vec::new();
    let Some(providers) = cfg.get("provider").and_then(|v| v.as_object()) else {
        return drafts;
    };
    for (key, p) in providers {
        let base_url = p
            .get("options")
            .and_then(|o| o.get("baseURL"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let api_style = p
            .get("npm")
            .and_then(|v| v.as_str())
            .map(map_opencode_npm)
            .unwrap_or("openai")
            .to_string();
        let models: Vec<String> = p
            .get("models")
            .and_then(|v| v.as_object())
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        // v2：源 key 明文收进中心凭据库（掩码随行供导入结果回显）
        let raw_key = p
            .get("options")
            .and_then(|o| o.get("apiKey"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        drafts.push(PresetDraft {
            name: key.clone(),
            base_url,
            api_style,
            models,
            notes: format!("opencode:{key}"),
            key: raw_key.clone(),
            key_mask: mask_key(&raw_key),
        });
    }
    drafts
}

/// 同名去重计划：已存在同名 → 试 `{name}-{source}`；仍存在 → 跳过
/// （pi 与 opencode 常有同名 provider，如 sensenova）
pub(crate) fn plan_inserts(
    existing: &[String],
    drafts: Vec<PresetDraft>,
) -> (Vec<PresetDraft>, Vec<String>) {
    let mut taken: Vec<String> = existing.to_vec();
    let mut to_create = Vec::new();
    let mut skipped = Vec::new();
    for d in drafts {
        let source = d.notes.split(':').next().unwrap_or("import").to_string();
        let name = if taken.iter().any(|n| n == &d.name) {
            let alt = format!("{}-{}", d.name, source);
            if taken.iter().any(|n| n == &alt) {
                skipped.push(d.name);
                continue;
            }
            PresetDraft { name: alt, ..d }
        } else {
            d
        };
        taken.push(name.name.clone());
        to_create.push(name);
    }
    (to_create, skipped)
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::super::jsonc::parse_jsonc;
    use super::*;
    /// pi 提取：真实形态（sensenova/amd）→ 草稿字段 + 掩码；key 本体不进草稿
    #[test]
    fn presets_from_pi_extracts() {
        let models = parse_jsonc(
            r#"{
  "providers": {
    "sensenova": {
      "name": "商汤日日新 SenseNova",
      "baseUrl": "https://token.sensenova.cn/v1",
      "api": "openai-completions",
      "models": [ { "id": "glm-5.2" }, { "id": "sensenova-u1-fast" } ]
    },
    "amd": {
      "name": "AMD Radeon Developer Cloud",
      "baseUrl": "https://developer.amd.com.cn/radeon/api/v1",
      // provider 级 compat 注释
      "api": "openai-completions",
      "models": [ { "id": "DeepSeek-V4-Flash" }, { "id": "Qwen3.8-Flash-Next" } ]
    }
  }
}"#,
        )
        .unwrap();
        let auth = parse_jsonc(
            r#"{
  "sensenova": { "type": "api_key", "key": "sensenova-raw-key-000111222333444" },
  "amd": { "type": "api_key", "key": "amd-raw-key-000111222333444555666777888" }
}"#,
        )
        .unwrap();
        let drafts = presets_from_pi(&models, &auth);
        assert_eq!(drafts.len(), 2);
        let sen = drafts.iter().find(|d| d.name == "sensenova").unwrap();
        assert_eq!(sen.base_url, "https://token.sensenova.cn/v1");
        assert_eq!(sen.api_style, "openai");
        assert_eq!(sen.models, vec!["glm-5.2", "sensenova-u1-fast"]);
        assert_eq!(sen.notes, "pi:sensenova");
        let amd = drafts.iter().find(|d| d.name == "amd").unwrap();
        assert_eq!(amd.api_style, "openai");
        // 掩码回显：前 3 字符 + 长度，raw key 不出现（掩码内嵌草稿，随改名走）
        assert_eq!(sen.key_mask, "sen…(33)");
        // v2：源 key 明文收进中心凭据库（draft.key 承载；Debug 掩码化）
        assert_eq!(sen.key, "sensenova-raw-key-000111222333444");
        let amd = drafts.iter().find(|d| d.name == "amd").unwrap();
        assert_eq!(amd.key, "amd-raw-key-000111222333444555666777888");
        let all = format!("{drafts:?}");
        assert!(
            !all.contains("sensenova-raw-key"),
            "raw key must not leak into drafts Debug"
        );
    }

    /// opencode 提取：provider.* → 草稿 + 掩码；无 apiKey 的条目掩码为 "—"
    #[test]
    fn presets_from_opencode_extracts() {
        let cfg = parse_jsonc(
            r#"{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "gmi": {
      "name": "GMI",
      "npm": "@ai-sdk/openai-compatible",
      "options": { "apiKey": "gmi-raw-key-000111222333444555666777888999", "baseURL": "https://api.gmi-serving.com/v1" },
      "models": { "MiniMaxAI/MiniMax-M3": { "name": "MiniMax M3" }, "openai/gpt-5": {} }
    },
    "anthropic-direct": {
      "npm": "@ai-sdk/anthropic",
      "options": { "baseURL": "https://api.anthropic.com/v1" }
    }
  }
}"#,
        )
        .unwrap();
        let drafts = presets_from_opencode(&cfg);
        assert_eq!(drafts.len(), 2);
        let gmi = drafts.iter().find(|d| d.name == "gmi").unwrap();
        assert_eq!(gmi.base_url, "https://api.gmi-serving.com/v1");
        assert_eq!(gmi.api_style, "openai");
        assert!(gmi.models.contains(&"MiniMaxAI/MiniMax-M3".to_string()));
        assert_eq!(gmi.notes, "opencode:gmi");
        let ant = drafts
            .iter()
            .find(|d| d.name == "anthropic-direct")
            .unwrap();
        assert_eq!(ant.api_style, "anthropic");
        assert_eq!(gmi.key_mask, "gmi…(42)");
        assert_eq!(ant.key_mask, "—");
        // v2：无 apiKey 的条目 key 为空串；有 key 的进中心凭据库
        assert_eq!(gmi.key, "gmi-raw-key-000111222333444555666777888999");
        assert_eq!(ant.key, "");
        let all = format!("{drafts:?}");
        assert!(!all.contains("gmi-raw-key"), "Debug 必须掩码草稿 key");
    }

    /// 同名去重：pi 先建 sensenova，opencode 的同名 → `sensenova-opencode`；
    /// 掩码内嵌草稿，改名后仍随预设走（前端按最终名取掩码）
    #[test]
    fn plan_inserts_dedupes_cross_source() {
        let existing = vec!["sensenova".to_string()];
        let drafts = vec![
            PresetDraft {
                name: "sensenova".to_string(),
                base_url: "u".to_string(),
                api_style: "openai".to_string(),
                models: vec![],
                notes: "opencode:sensenova".to_string(),
                key: String::new(),
                key_mask: "gmi…(42)".to_string(),
            },
            PresetDraft {
                name: "gmi".to_string(),
                base_url: "u".to_string(),
                api_style: "openai".to_string(),
                models: vec![],
                notes: "opencode:gmi".to_string(),
                key: String::new(),
                key_mask: "—".to_string(),
            },
        ];
        let (create, skipped) = plan_inserts(&existing, drafts);
        assert_eq!(create.len(), 2);
        assert_eq!(create[0].name, "sensenova-opencode");
        assert_eq!(create[0].key_mask, "gmi…(42)", "mask must survive rename");
        assert_eq!(create[1].name, "gmi");
        assert!(skipped.is_empty());

        // 两名都占 → 跳过
        let existing = vec!["sensenova".to_string(), "sensenova-opencode".to_string()];
        let drafts = vec![PresetDraft {
            name: "sensenova".to_string(),
            base_url: "u".to_string(),
            api_style: "openai".to_string(),
            models: vec![],
            notes: "opencode:sensenova".to_string(),
            key: String::new(),
            key_mask: "—".to_string(),
        }];
        let (create, skipped) = plan_inserts(&existing, drafts);
        assert!(create.is_empty());
        assert_eq!(skipped, vec!["sensenova".to_string()]);
    }
}
