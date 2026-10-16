//! key 掩码与方言映射（纯函数，多处共用）
//!
//! `mask_key` 是 key 与 UI 的唯一交界面（前 3 字符 + 长度；删红线后保留的
//! 「不落明文于 UI/日志」层）；`map_*` / `*_of` 是 pi models.json `api` 字段
//! 与 opencode `npm` 字段 ↔ 预设 apiStyle 的双向映射（custom 回落各自默认）。

// ==================== key 掩码（纯函数） ====================

/// UI 掩码：前 3 字符 + 长度（spec §4.4）；短 key（≤6）不泄前缀只泄长度
pub(crate) fn mask_key(key: &str) -> String {
    let n = key.chars().count();
    if n == 0 {
        return "—".to_string();
    }
    if n <= 6 {
        return format!("•••({n})");
    }
    let prefix: String = key.chars().take(3).collect();
    format!("{prefix}…({n})")
}
// ==================== 方言映射（纯函数） ====================

/// pi models.json `api` 字段 → 预设 apiStyle
pub(crate) fn map_pi_api(api: &str) -> &'static str {
    match api {
        "openai-completions" | "openai-responses" => "openai",
        "anthropic-messages" => "anthropic",
        "google-generative-ai" | "google-vertex" => "gemini",
        _ => "custom",
    }
}

/// pi `api` 是否属 openai 家族（completions / responses 两种方言）
///
/// 用途：应用时**不把用户目标配置里已有的 openai 方言降级**——预设只表达
/// 「openai 家族」这一层（见 [`crate::providers::merge`]），具体方言由目标
/// CLI 既有配置说了算，否则「重新应用一次」就会把 responses 端点改坏。
pub(crate) fn is_openai_family_api(api: &str) -> bool {
    matches!(api, "openai-completions" | "openai-responses")
}

/// opencode `npm` 是否属 openai 家族（compatible / openai 两种包）
pub(crate) fn is_openai_family_npm(npm: &str) -> bool {
    matches!(npm, "@ai-sdk/openai-compatible" | "@ai-sdk/openai")
}

/// pi apiStyle → models.json `api` 字段（custom 回落 openai-completions）
pub(crate) fn pi_api_of(style: &str) -> &'static str {
    match style {
        "anthropic" => "anthropic-messages",
        "gemini" => "google-generative-ai",
        _ => "openai-completions",
    }
}

/// opencode `npm` 字段 → 预设 apiStyle
pub(crate) fn map_opencode_npm(npm: &str) -> &'static str {
    match npm {
        "@ai-sdk/openai-compatible" | "@ai-sdk/openai" => "openai",
        "@ai-sdk/anthropic" => "anthropic",
        "@ai-sdk/google" => "gemini",
        _ => "custom",
    }
}

/// opencode apiStyle → `npm` 字段（custom 回落 openai-compatible）
pub(crate) fn opencode_npm_of(style: &str) -> &'static str {
    match style {
        "anthropic" => "@ai-sdk/anthropic",
        "gemini" => "@ai-sdk/google",
        _ => "@ai-sdk/openai-compatible",
    }
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 掩码形态：前 3 字符 + 长度；短 key 不泄前缀；空 key 占位
    #[test]
    fn mask_formats() {
        assert_eq!(mask_key("sensenova-key-0123456789"), "sen…(24)");
        assert_eq!(mask_key("abcdef"), "•••(6)");
        assert_eq!(mask_key("abc"), "•••(3)");
        assert_eq!(mask_key(""), "—");
    }

    /// 掩码不得包含第 4 字符起的任何内容（防长度 ≤3 之外的泄漏）
    #[test]
    fn mask_never_leaks_tail() {
        let key = "sk-1234567890-abcdef";
        let m = mask_key(key);
        assert!(!m.contains(&key[3..]));
        assert_eq!(m, "sk-…(20)");
    }

    #[test]
    fn api_style_mappings() {
        assert_eq!(map_pi_api("openai-completions"), "openai");
        assert_eq!(map_pi_api("openai-responses"), "openai");
        assert_eq!(map_pi_api("anthropic-messages"), "anthropic");
        assert_eq!(map_pi_api("google-generative-ai"), "gemini");
        assert_eq!(map_pi_api("mistral-conversations"), "custom");
        assert_eq!(pi_api_of("openai"), "openai-completions");
        assert_eq!(pi_api_of("anthropic"), "anthropic-messages");
        assert_eq!(pi_api_of("gemini"), "google-generative-ai");
        assert_eq!(pi_api_of("custom"), "openai-completions");

        assert_eq!(map_opencode_npm("@ai-sdk/openai-compatible"), "openai");
        assert_eq!(map_opencode_npm("@ai-sdk/anthropic"), "anthropic");
        assert_eq!(map_opencode_npm("@ai-sdk/google"), "gemini");
        assert_eq!(map_opencode_npm("@ai-sdk/azure"), "custom");
        assert_eq!(opencode_npm_of("anthropic"), "@ai-sdk/anthropic");
        assert_eq!(opencode_npm_of("custom"), "@ai-sdk/openai-compatible");
    }

    /// 家族判定：只有 openai 家族互认，跨家族（responses ↔ anthropic）不认
    #[test]
    fn openai_family_membership() {
        assert!(is_openai_family_api("openai-completions"));
        assert!(is_openai_family_api("openai-responses"));
        assert!(!is_openai_family_api("anthropic-messages"));
        assert!(!is_openai_family_api("google-generative-ai"));
        assert!(!is_openai_family_api(""));

        assert!(is_openai_family_npm("@ai-sdk/openai-compatible"));
        assert!(is_openai_family_npm("@ai-sdk/openai"));
        assert!(!is_openai_family_npm("@ai-sdk/anthropic"));
        assert!(!is_openai_family_npm("@ai-sdk/azure"));
    }
}
