//! AI API Client (Mobile)
//!
//! WASM 模式仅支持 OpenAI 格式，通过 WasmHost::http_fetch 代理 HTTP 请求

use bedcode_plugin_api_mobile::WasmHost;
use serde::{Deserialize, Serialize};

/// API 格式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ApiFormat {
    OpenAI,
    Anthropic,
    Gemini,
    Ollama,
}

/// API 提供商配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiProvider {
    pub id: String,
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    pub api_format: ApiFormat,
    pub models: Vec<String>,
    pub active_model: String,
    /// 兼容：前端传来的 provider 可能有 model 字段（由 useAiChat 构造）
    #[serde(default)]
    pub model: String,
}

/// 聊天消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// OpenAI 非流式响应结构
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

// ==================== Shared Helpers ====================

/// 获取当前使用的模型（优先使用 model 字段，其次 active_model）
fn effective_model(provider: &ApiProvider) -> &str {
    if !provider.model.is_empty() {
        &provider.model
    } else if !provider.active_model.is_empty() {
        &provider.active_model
    } else {
        provider.models.first().map(|s| s.as_str()).unwrap_or("")
    }
}

// ==================== WASM Mode Only ====================

/// 统一的流式聊天入口 — WASM 模式仅支持 OpenAI
pub fn chat_stream(
    provider: &ApiProvider,
    messages: &[ChatMessage],
    stream_id: &str,
) -> anyhow::Result<()> {
    match provider.api_format {
        ApiFormat::OpenAI => chat_stream_openai_wasm(provider, messages, stream_id),
        _ => Err(anyhow::anyhow!(
            "{} format is not supported in WASM mode, only OpenAI is available",
            format!("{:?}", provider.api_format)
        )),
    }
}

/// 统一的非流式聊天入口 — WASM 模式仅支持 OpenAI
pub fn chat_complete(provider: &ApiProvider, messages: &[ChatMessage]) -> anyhow::Result<String> {
    match provider.api_format {
        ApiFormat::OpenAI => chat_complete_openai_wasm(provider, messages),
        _ => Err(anyhow::anyhow!(
            "{} format is not supported in WASM mode, only OpenAI is available",
            format!("{:?}", provider.api_format)
        )),
    }
}

// ==================== OpenAI (WASM) ====================

fn chat_stream_openai_wasm(
    provider: &ApiProvider,
    messages: &[ChatMessage],
    stream_id: &str,
) -> anyhow::Result<()> {
    let event_name = format!("ai-chatbox:stream:{}", stream_id);
    let model = effective_model(provider);
    let host = WasmHost::new("com.bedcode.ai-chatbox");

    let request = serde_json::json!({
        "method": "POST",
        "url": format!("{}/chat/completions", provider.base_url.trim_end_matches('/')),
        "headers": {
            "Authorization": format!("Bearer {}", provider.api_key),
            "Content-Type": "application/json",
        },
        "body": serde_json::to_string(&serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": true,
        }))?,
        "stream": true,
        "streamEvent": event_name,
        "sseFormat": "openai",
    });

    host.http_fetch(&request)
        .ok_or_else(|| anyhow::anyhow!("http_fetch failed for streaming request"))?;

    Ok(())
}

fn chat_complete_openai_wasm(
    provider: &ApiProvider,
    messages: &[ChatMessage],
) -> anyhow::Result<String> {
    let model = effective_model(provider);
    let host = WasmHost::new("com.bedcode.ai-chatbox");

    let request = serde_json::json!({
        "method": "POST",
        "url": format!("{}/chat/completions", provider.base_url.trim_end_matches('/')),
        "headers": {
            "Authorization": format!("Bearer {}", provider.api_key),
            "Content-Type": "application/json",
        },
        "body": serde_json::to_string(&serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": false,
        }))?,
        "stream": false,
    });

    let result = host.http_fetch(&request)
        .ok_or_else(|| anyhow::anyhow!("http_fetch failed for non-streaming request"))?;

    let status = result.get("status").and_then(|s| s.as_u64()).unwrap_or(0);
    if status != 200 {
        let body = result.get("body").and_then(|b| b.as_str()).unwrap_or("");
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    let body_str = result.get("body").and_then(|b| b.as_str()).unwrap_or("");
    let chat_resp: ChatResponse = serde_json::from_str(body_str)?;
    Ok(chat_resp.choices.first()
        .map(|c| c.message.content.clone())
        .unwrap_or_default())
}
