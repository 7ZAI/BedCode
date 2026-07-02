//! AI API Client
//!
//! 通过 reqwest 调用 OpenAI 兼容 API，支持流式和非流式两种模式
//! 流式模式通过宿主 emit_event 逐 chunk 推送到前端

use crate::HOST_CONTEXT;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

/// API 提供商配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiProvider {
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

/// 聊天消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// SSE 响应结构
#[derive(Debug, Deserialize)]
struct SseResponse {
    choices: Vec<SseChoice>,
}

#[derive(Debug, Deserialize)]
struct SseChoice {
    delta: SseDelta,
}

#[derive(Debug, Deserialize)]
struct SseDelta {
    content: Option<String>,
}

/// 非流式响应结构
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

/// 非流式聊天请求
pub async fn chat_complete(provider: &ApiProvider, messages: &[ChatMessage]) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/chat/completions", provider.base_url.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": provider.model,
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

/// 流式聊天请求
///
/// 通过宿主 emit_event 逐 chunk 推送到前端，事件名格式：`ai-chatbox:stream:{stream_id}`
/// 推送完成后发送 done 事件
pub async fn chat_stream(
    provider: &ApiProvider,
    messages: &[ChatMessage],
    stream_id: &str,
) -> anyhow::Result<()> {
    // 验证插件已激活
    if HOST_CONTEXT.get().is_none() {
        return Err(anyhow::anyhow!("Plugin not activated"));
    }

    let event_name = format!("ai-chatbox:stream:{}", stream_id);
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/chat/completions", provider.base_url.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": provider.model,
            "messages": messages,
            "stream": true,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let error_payload = serde_json::json!({
            "error": format!("API error {}: {}", status, body),
            "done": true
        });
        // emit 不涉及 await，短暂获取 host 引用后立即释放
        if let Some(host) = HOST_CONTEXT.get() {
            host.emit(&event_name, &error_payload);
        }
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    // SSE 流式解析
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // 按行分割，处理完整的 SSE 事件
        while let Some(pos) = buffer.find("\n\n") {
            let event_text = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            for line in event_text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    let data = data.trim();
                    if data == "[DONE]" {
                        let done_payload = serde_json::json!({ "done": true });
                        if let Some(host) = HOST_CONTEXT.get() {
                            host.emit(&event_name, &done_payload);
                        }
                        return Ok(());
                    }

                    if let Ok(parsed) = serde_json::from_str::<SseResponse>(data) {
                        if let Some(content) = parsed.choices.first().and_then(|c| c.delta.content.as_ref()) {
                            if !content.is_empty() {
                                let chunk_payload = serde_json::json!({ "chunk": content });
                                if let Some(host) = HOST_CONTEXT.get() {
                                    host.emit(&event_name, &chunk_payload);
                                }
                            }
                        }
                    }
                    // 解析失败的行静默跳过（可能是不完整的事件或注释行）
                }
            }
        }
    }

    // 流结束但未收到 [DONE]
    let done_payload = serde_json::json!({ "done": true });
    if let Some(host) = HOST_CONTEXT.get() {
        host.emit(&event_name, &done_payload);
    }
    Ok(())
}
