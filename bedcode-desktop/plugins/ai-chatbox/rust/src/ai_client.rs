//! AI API Client
//!
//! 支持 OpenAI / Anthropic / Gemini / Ollama 四种 API 格式（native 模式）
//! WASM 模式仅支持 OpenAI 格式，通过 WasmHost::http_fetch 代理 HTTP 请求

use bedcode_plugin_api::WasmHost;
use serde::{Deserialize, Serialize};

#[cfg(feature = "native")]
use bedcode_plugin_api::host::HostEvents;
#[cfg(feature = "wasm")]
use bedcode_plugin_api::host::HostHttp;

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

/// OpenAI SSE 流式响应结构（native 模式使用，WASM 模式由宿主解析 SSE）
#[cfg(feature = "native")]
#[derive(Debug, Deserialize)]
struct OpenAiSseResponse {
    choices: Vec<OpenAiSseChoice>,
}

#[cfg(feature = "native")]
#[derive(Debug, Deserialize)]
struct OpenAiSseChoice {
    delta: OpenAiSseDelta,
}

#[cfg(feature = "native")]
#[derive(Debug, Deserialize)]
struct OpenAiSseDelta {
    content: Option<String>,
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

/// Anthropic SSE 事件结构
#[cfg(feature = "native")]
#[derive(Debug, Deserialize)]
struct AnthropicDeltaEvent {
    delta: AnthropicDelta,
}

#[cfg(feature = "native")]
#[derive(Debug, Deserialize)]
struct AnthropicDelta {
    text: Option<String>,
}

/// Gemini SSE 响应结构
#[cfg(feature = "native")]
#[derive(Debug, Deserialize)]
struct GeminiSseResponse {
    candidates: Vec<GeminiCandidate>,
}

#[cfg(feature = "native")]
#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}

#[cfg(feature = "native")]
#[derive(Debug, Deserialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[cfg(feature = "native")]
#[derive(Debug, Deserialize)]
struct GeminiPart {
    text: Option<String>,
}

/// Ollama SSE 响应结构
#[cfg(feature = "native")]
#[derive(Debug, Deserialize)]
struct OllamaSseResponse {
    message: Option<OllamaMessage>,
}

#[cfg(feature = "native")]
#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: Option<String>,
}

// ==================== Shared Helpers ====================

/// 向前端 emit 流式 chunk（native 模式使用，WASM 模式由宿主直接推流）
#[cfg(feature = "native")]
fn emit_chunk(event_name: &str, chunk: &str) {
    let host = WasmHost;
    let payload = serde_json::json!({ "chunk": chunk });
    host.emit_event(event_name, &payload);
}

#[cfg(feature = "native")]
fn emit_done(event_name: &str) {
    let host = WasmHost;
    let payload = serde_json::json!({ "done": true });
    host.emit_event(event_name, &payload);
}

#[cfg(feature = "native")]
fn emit_error(event_name: &str, error: &str) {
    let host = WasmHost;
    let payload = serde_json::json!({ "error": error, "done": true });
    host.emit_event(event_name, &payload);
}

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

// ==================== Dispatch (Native) ====================

#[cfg(feature = "native")]
use futures_util::StreamExt;

/// 统一的流式聊天入口 — 根据 api_format 分发
#[cfg(feature = "native")]
pub async fn chat_stream(
    provider: &ApiProvider,
    messages: &[ChatMessage],
    stream_id: &str,
) -> anyhow::Result<()> {
    match provider.api_format {
        ApiFormat::OpenAI => chat_stream_openai(provider, messages, stream_id).await,
        ApiFormat::Anthropic => chat_stream_anthropic(provider, messages, stream_id).await,
        ApiFormat::Gemini => chat_stream_gemini(provider, messages, stream_id).await,
        ApiFormat::Ollama => chat_stream_ollama(provider, messages, stream_id).await,
    }
}

/// 统一的非流式聊天入口
#[cfg(feature = "native")]
pub async fn chat_complete(provider: &ApiProvider, messages: &[ChatMessage]) -> anyhow::Result<String> {
    match provider.api_format {
        ApiFormat::OpenAI => chat_complete_openai(provider, messages).await,
        ApiFormat::Anthropic => chat_complete_anthropic(provider, messages).await,
        ApiFormat::Gemini => chat_complete_gemini(provider, messages).await,
        ApiFormat::Ollama => chat_complete_ollama(provider, messages).await,
    }
}

// ==================== Dispatch (WASM) ====================

/// 统一的流式聊天入口 — WASM 模式仅支持 OpenAI
#[cfg(feature = "wasm")]
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
#[cfg(feature = "wasm")]
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

#[cfg(feature = "wasm")]
fn chat_stream_openai_wasm(
    provider: &ApiProvider,
    messages: &[ChatMessage],
    stream_id: &str,
) -> anyhow::Result<()> {
    let event_name = format!("ai-chatbox:stream:{}", stream_id);
    let model = effective_model(provider);
    let host = WasmHost;

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
        .map_err(|e| anyhow::anyhow!("http_fetch failed for streaming request: {}", e))?;

    // http_fetch stream:true 立即返回，宿主 spawn tokio 任务解析 SSE 并逐 chunk 推送
    Ok(())
}

#[cfg(feature = "wasm")]
fn chat_complete_openai_wasm(
    provider: &ApiProvider,
    messages: &[ChatMessage],
) -> anyhow::Result<String> {
    let model = effective_model(provider);
    let host = WasmHost;

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
        .map_err(|e| anyhow::anyhow!("http_fetch failed for non-streaming request: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("http_fetch returned empty result"))?;

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

// ==================== OpenAI (Native) ====================

#[cfg(feature = "native")]
async fn chat_stream_openai(
    provider: &ApiProvider,
    messages: &[ChatMessage],
    stream_id: &str,
) -> anyhow::Result<()> {
    let event_name = format!("ai-chatbox:stream:{}", stream_id);
    let client = reqwest::Client::new();
    let model = effective_model(provider);

    let response = client
        .post(format!("{}/chat/completions", provider.base_url.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": true,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        emit_error(&event_name, &format!("API error {}: {}", status, body));
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find("\n\n") {
            let event_text = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            for line in event_text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    let data = data.trim();
                    if data == "[DONE]" {
                        emit_done(&event_name);
                        return Ok(());
                    }
                    if let Ok(parsed) = serde_json::from_str::<OpenAiSseResponse>(data) {
                        if let Some(content) = parsed.choices.first().and_then(|c| c.delta.content.as_ref()) {
                            if !content.is_empty() {
                                emit_chunk(&event_name, content);
                            }
                        }
                    }
                }
            }
        }
    }

    emit_done(&event_name);
    Ok(())
}

#[cfg(feature = "native")]
async fn chat_complete_openai(
    provider: &ApiProvider,
    messages: &[ChatMessage],
) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let model = effective_model(provider);

    let response = client
        .post(format!("{}/chat/completions", provider.base_url.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": false,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    let chat_resp: ChatResponse = response.json().await?;
    Ok(chat_resp.choices.first()
        .map(|c| c.message.content.clone())
        .unwrap_or_default())
}

// ==================== Anthropic (Native) ====================

/// 将 ChatMessage 转为 Anthropic 格式（system 单独提取）
#[cfg(feature = "native")]
fn split_anthropic_messages(messages: &[ChatMessage]) -> (String, Vec<serde_json::Value>) {
    let mut system_prompt = String::new();
    let mut chat_msgs = Vec::new();

    for msg in messages {
        if msg.role == "system" {
            system_prompt = msg.content.clone();
        } else {
            chat_msgs.push(serde_json::json!({
                "role": msg.role,
                "content": msg.content,
            }));
        }
    }

    (system_prompt, chat_msgs)
}

#[cfg(feature = "native")]
async fn chat_stream_anthropic(
    provider: &ApiProvider,
    messages: &[ChatMessage],
    stream_id: &str,
) -> anyhow::Result<()> {
    let event_name = format!("ai-chatbox:stream:{}", stream_id);
    let client = reqwest::Client::new();
    let model = effective_model(provider);
    let (system_prompt, chat_msgs) = split_anthropic_messages(messages);

    let mut body = serde_json::json!({
        "model": model,
        "messages": chat_msgs,
        "max_tokens": 8192,
        "stream": true,
    });
    if !system_prompt.is_empty() {
        body["system"] = serde_json::json!(system_prompt);
    }

    let response = client
        .post(format!("{}/v1/messages", provider.base_url.trim_end_matches('/')))
        .header("x-api-key", &provider.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        emit_error(&event_name, &format!("API error {}: {}", status, body));
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut current_event_type = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find("\n\n") {
            let event_text = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            for line in event_text.lines() {
                if let Some(evt_type) = line.strip_prefix("event: ") {
                    current_event_type = evt_type.trim().to_string();
                } else if let Some(data) = line.strip_prefix("data: ") {
                    let data = data.trim();

                    match current_event_type.as_str() {
                        "content_block_delta" => {
                            if let Ok(parsed) = serde_json::from_str::<AnthropicDeltaEvent>(data) {
                                if let Some(text) = parsed.delta.text {
                                    if !text.is_empty() {
                                        emit_chunk(&event_name, &text);
                                    }
                                }
                            }
                        }
                        "message_stop" => {
                            emit_done(&event_name);
                            return Ok(());
                        }
                        "message_delta" => {
                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                                if parsed["delta"]["stop_reason"].as_str() == Some("end_turn") {
                                    emit_done(&event_name);
                                    return Ok(());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    emit_done(&event_name);
    Ok(())
}

#[cfg(feature = "native")]
async fn chat_complete_anthropic(
    provider: &ApiProvider,
    messages: &[ChatMessage],
) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let model = effective_model(provider);
    let (system_prompt, chat_msgs) = split_anthropic_messages(messages);

    let mut body = serde_json::json!({
        "model": model,
        "messages": chat_msgs,
        "max_tokens": 8192,
    });
    if !system_prompt.is_empty() {
        body["system"] = serde_json::json!(system_prompt);
    }

    let response = client
        .post(format!("{}/v1/messages", provider.base_url.trim_end_matches('/')))
        .header("x-api-key", &provider.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    #[derive(Debug, Deserialize)]
    struct AnthropicResponse {
        content: Vec<AnthropicContentBlock>,
    }
    #[derive(Debug, Deserialize)]
    struct AnthropicContentBlock {
        text: Option<String>,
    }

    let resp: AnthropicResponse = response.json().await?;
    let text = resp.content.iter()
        .filter_map(|b| b.text.clone())
        .collect::<Vec<_>>()
        .join("");
    Ok(text)
}

// ==================== Gemini (Native) ====================

/// 将 ChatMessage 转为 Gemini 格式（contents 数组 + systemInstruction）
#[cfg(feature = "native")]
fn build_gemini_body(messages: &[ChatMessage], _model: &str) -> serde_json::Value {
    let mut system_instruction = None;
    let mut contents = Vec::new();

    for msg in messages {
        if msg.role == "system" {
            system_instruction = Some(serde_json::json!({
                "parts": [{ "text": msg.content }]
            }));
        } else {
            // Gemini 用 "user" / "model" 而非 "assistant"
            let role = if msg.role == "assistant" { "model" } else { "user" };
            contents.push(serde_json::json!({
                "role": role,
                "parts": [{ "text": msg.content }]
            }));
        }
    }

    let mut body = serde_json::json!({
        "contents": contents,
        "generationConfig": {},
    });
    if let Some(si) = system_instruction {
        body["systemInstruction"] = si;
    }
    body
}

#[cfg(feature = "native")]
async fn chat_stream_gemini(
    provider: &ApiProvider,
    messages: &[ChatMessage],
    stream_id: &str,
) -> anyhow::Result<()> {
    let event_name = format!("ai-chatbox:stream:{}", stream_id);
    let client = reqwest::Client::new();
    let model = effective_model(provider);
    let body = build_gemini_body(messages, model);

    let url = format!(
        "{}/models/{}:streamGenerateContent?alt=sse&key={}",
        provider.base_url.trim_end_matches('/'),
        model,
        provider.api_key
    );

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        emit_error(&event_name, &format!("API error {}: {}", status, body));
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find("\n\n") {
            let event_text = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            for line in event_text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    let data = data.trim();
                    if let Ok(parsed) = serde_json::from_str::<GeminiSseResponse>(data) {
                        if let Some(text) = parsed.candidates.first()
                            .and_then(|c| c.content.parts.first())
                            .and_then(|p| p.text.as_ref())
                        {
                            if !text.is_empty() {
                                emit_chunk(&event_name, text);
                            }
                        }
                    }
                }
            }
        }
    }

    emit_done(&event_name);
    Ok(())
}

#[cfg(feature = "native")]
async fn chat_complete_gemini(
    provider: &ApiProvider,
    messages: &[ChatMessage],
) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let model = effective_model(provider);
    let body = build_gemini_body(messages, model);

    let url = format!(
        "{}/models/{}:generateContent?key={}",
        provider.base_url.trim_end_matches('/'),
        model,
        provider.api_key
    );

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    #[derive(Debug, Deserialize)]
    struct GeminiResponse {
        candidates: Vec<GeminiCandidate>,
    }

    let resp: GeminiResponse = response.json().await?;
    let text = resp.candidates.first()
        .and_then(|c| c.content.parts.first())
        .and_then(|p| p.text.clone())
        .unwrap_or_default();
    Ok(text)
}

// ==================== Ollama (Native) ====================

#[cfg(feature = "native")]
async fn chat_stream_ollama(
    provider: &ApiProvider,
    messages: &[ChatMessage],
    stream_id: &str,
) -> anyhow::Result<()> {
    let event_name = format!("ai-chatbox:stream:{}", stream_id);
    let client = reqwest::Client::new();
    let model = effective_model(provider);

    let response = client
        .post(format!("{}/api/chat", provider.base_url.trim_end_matches('/')))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": true,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        emit_error(&event_name, &format!("API error {}: {}", status, body));
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    // Ollama 每行一个 JSON 对象，非 SSE 格式
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            if let Ok(parsed) = serde_json::from_str::<OllamaSseResponse>(&line) {
                if let Some(msg) = &parsed.message {
                    if let Some(text) = &msg.content {
                        if !text.is_empty() {
                            emit_chunk(&event_name, text);
                        }
                    }
                }
            }
        }
    }

    emit_done(&event_name);
    Ok(())
}

#[cfg(feature = "native")]
async fn chat_complete_ollama(
    provider: &ApiProvider,
    messages: &[ChatMessage],
) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let model = effective_model(provider);

    let response = client
        .post(format!("{}/api/chat", provider.base_url.trim_end_matches('/')))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": false,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    let resp: OllamaSseResponse = response.json().await?;
    Ok(resp.message.and_then(|m| m.content).unwrap_or_default())
}
