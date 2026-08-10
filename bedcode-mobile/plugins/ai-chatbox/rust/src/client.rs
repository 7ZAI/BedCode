//! OpenAI 兼容 API 客户端
//!
//! 单一协议：`/chat/completions`（流式/非流式）+ `GET /models`。
//! 流式请求经宿主 `http_fetch`（stream:true + streamEvent + sseFormat:"openai"）
//! 后台推流，usage 随 done 事件由宿主透传；本模块只构造请求。

use bedcode_plugin_api_mobile::host::HostHttp;
use bedcode_plugin_api_mobile::WasmHost;
use serde::{Deserialize, Serialize};

/// API 格式（当前仅 OpenAI 兼容协议，字段保留供未来增量扩展）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ApiFormat {
    OpenAI,
}

/// API 提供商配置（前端 camelCase 直传）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiProvider {
    pub id: String,
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    pub api_format: ApiFormat,
    pub models: Vec<String>,
    pub active_model: String,
    /// 兼容：前端按对话临时指定模型（优先于 active_model）
    #[serde(default)]
    pub model: String,
}

/// 聊天消息（前端传 role + content）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// 获取当前使用的模型（model 字段 → active_model → 首个预设模型）
pub fn effective_model(provider: &ApiProvider) -> &str {
    if !provider.model.is_empty() {
        &provider.model
    } else if !provider.active_model.is_empty() {
        &provider.active_model
    } else {
        provider.models.first().map(|s| s.as_str()).unwrap_or("")
    }
}

/// 拼接 baseUrl 与路径（去尾部斜杠）
fn join_url(base_url: &str, path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), path)
}

/// 构造流式 chat/completions 请求（宿主 http_fetch 载荷）
fn build_chat_stream_request(
    provider: &ApiProvider,
    messages: &[ChatMessage],
    stream_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "method": "POST",
        "url": join_url(&provider.base_url, "/chat/completions"),
        "headers": {
            "Authorization": format!("Bearer {}", provider.api_key),
            "Content-Type": "application/json",
        },
        "body": serde_json::to_string(&serde_json::json!({
            "model": effective_model(provider),
            "messages": messages,
            "stream": true,
        })).expect("json serialization cannot fail"),
        "stream": true,
        "streamEvent": format!("ai-chatbox:stream:{}", stream_id),
        "sseFormat": "openai",
    })
}

/// 构造非流式 chat/completions 请求
fn build_chat_complete_request(
    provider: &ApiProvider,
    messages: &[ChatMessage],
) -> serde_json::Value {
    serde_json::json!({
        "method": "POST",
        "url": join_url(&provider.base_url, "/chat/completions"),
        "headers": {
            "Authorization": format!("Bearer {}", provider.api_key),
            "Content-Type": "application/json",
        },
        "body": serde_json::to_string(&serde_json::json!({
            "model": effective_model(provider),
            "messages": messages,
            "stream": false,
        })).expect("json serialization cannot fail"),
        "stream": false,
    })
}

/// 构造 GET /models 请求
fn build_fetch_models_request(provider: &ApiProvider) -> serde_json::Value {
    serde_json::json!({
        "method": "GET",
        "url": join_url(&provider.base_url, "/models"),
        "headers": {
            "Authorization": format!("Bearer {}", provider.api_key),
            "Content-Type": "application/json",
        },
        "stream": false,
    })
}

/// 流式对话：经宿主 http_fetch stream 推送 chunk / done(含 usage) 事件
///
/// 宿主在后台解析 SSE 并逐 chunk 推送到 `ai-chatbox:stream:{stream_id}`，
/// 本函数仅构造请求并触发，立即返回。
pub fn chat_stream(
    provider: &ApiProvider,
    messages: &[ChatMessage],
    stream_id: &str,
) -> anyhow::Result<()> {
    let host = WasmHost;
    let request = build_chat_stream_request(provider, messages, stream_id);
    host.http_fetch(&request)
        .map_err(|e| anyhow::anyhow!("http_fetch failed for streaming request: {}", e))?;
    Ok(())
}

/// 非流式对话（测试连接 / 短回复场景），返回回复文本
pub fn chat_complete(provider: &ApiProvider, messages: &[ChatMessage]) -> anyhow::Result<String> {
    let host = WasmHost;
    let request = build_chat_complete_request(provider, messages);
    let result = host
        .http_fetch(&request)
        .map_err(|e| anyhow::anyhow!("http_fetch failed for non-streaming request: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("http_fetch returned empty result"))?;

    let status = result.get("status").and_then(|s| s.as_u64()).unwrap_or(0);
    let body = result.get("body").and_then(|b| b.as_str()).unwrap_or("");
    if status != 200 {
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    #[derive(Debug, Deserialize)]
    struct ChatResponse {
        choices: Vec<ChatChoice>,
    }
    #[derive(Debug, Deserialize)]
    struct ChatChoice {
        message: ChatMessageResponse,
    }
    #[derive(Debug, Deserialize)]
    struct ChatMessageResponse {
        content: String,
    }

    let chat_resp: ChatResponse = serde_json::from_str(body)
        .map_err(|e| anyhow::anyhow!("failed to parse chat response: {}", e))?;
    Ok(chat_resp
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_default())
}

/// 拉取模型列表：`GET {baseUrl}/models`，解析 `data[].id`
///
/// 非 200 / 解析失败返回 Err（前端回退预设模型列表，不阻塞使用）。
pub fn fetch_models(provider: &ApiProvider) -> anyhow::Result<Vec<String>> {
    let host = WasmHost;
    let request = build_fetch_models_request(provider);
    let result = host
        .http_fetch(&request)
        .map_err(|e| anyhow::anyhow!("http_fetch failed for models request: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("http_fetch returned empty result"))?;

    let status = result.get("status").and_then(|s| s.as_u64()).unwrap_or(0);
    let body = result.get("body").and_then(|b| b.as_str()).unwrap_or("");
    if status != 200 {
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    #[derive(Debug, Deserialize)]
    struct ModelsResponse {
        data: Vec<ModelItem>,
    }
    #[derive(Debug, Deserialize)]
    struct ModelItem {
        id: String,
    }

    let resp: ModelsResponse = serde_json::from_str(body)
        .map_err(|e| anyhow::anyhow!("failed to parse models response: {}", e))?;
    Ok(resp.data.into_iter().map(|m| m.id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ApiProvider {
        ApiProvider {
            id: "p1".to_string(),
            name: "DeepSeek".to_string(),
            api_key: "sk-test".to_string(),
            base_url: "https://api.deepseek.com/v1/".to_string(),
            api_format: ApiFormat::OpenAI,
            models: vec!["deepseek-chat".to_string()],
            active_model: "deepseek-chat".to_string(),
            model: String::new(),
        }
    }

    #[test]
    fn effective_model_prefers_explicit_model() {
        let mut p = provider();
        p.model = "custom".to_string();
        assert_eq!(effective_model(&p), "custom");
        p.model = String::new();
        assert_eq!(effective_model(&p), "deepseek-chat");
        p.active_model = String::new();
        assert_eq!(effective_model(&p), "deepseek-chat");
    }

    #[test]
    fn join_url_strips_trailing_slash() {
        assert_eq!(
            join_url("https://a.com/v1/", "/chat/completions"),
            "https://a.com/v1/chat/completions"
        );
        assert_eq!(join_url("https://a.com/v1", "/models"), "https://a.com/v1/models");
    }

    #[test]
    fn stream_request_has_expected_shape() {
        let p = provider();
        let messages = vec![
            ChatMessage { role: "system".to_string(), content: "be terse".to_string() },
            ChatMessage { role: "user".to_string(), content: "hi".to_string() },
        ];
        let req = build_chat_stream_request(&p, &messages, "s1");

        assert_eq!(req["method"], "POST");
        assert_eq!(req["url"], "https://api.deepseek.com/v1/chat/completions");
        assert_eq!(req["headers"]["Authorization"], "Bearer sk-test");
        assert_eq!(req["stream"], true);
        assert_eq!(req["streamEvent"], "ai-chatbox:stream:s1");
        assert_eq!(req["sseFormat"], "openai");

        let body: serde_json::Value = serde_json::from_str(req["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["model"], "deepseek-chat");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "hi");
    }

    #[test]
    fn complete_request_is_non_streaming() {
        let req = build_chat_complete_request(&provider(), &[]);
        assert_eq!(req["url"], "https://api.deepseek.com/v1/chat/completions");
        let body: serde_json::Value = serde_json::from_str(req["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["stream"], false);
        assert_eq!(req["stream"], false);
    }

    #[test]
    fn models_request_is_get() {
        let req = build_fetch_models_request(&provider());
        assert_eq!(req["method"], "GET");
        assert_eq!(req["url"], "https://api.deepseek.com/v1/models");
        assert_eq!(req["headers"]["Authorization"], "Bearer sk-test");
    }

    #[test]
    fn custom_provider_keeps_raw_base_url() {
        let mut p = provider();
        p.base_url = "http://127.0.0.1:8080".to_string();
        let req = build_chat_complete_request(&p, &[]);
        assert_eq!(req["url"], "http://127.0.0.1:8080/chat/completions");
    }
}
