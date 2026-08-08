//! HTTP 代理域 Host Functions（宿主代发请求，支持 SSE 流式推流）

use super::memory::{read_wasm_string_consume, write_result_to_out_ptr, write_wasm_string};
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext, WasmPluginState};
use crate::system::constants::plugin::{PLUGIN_HTTP_CONNECT_TIMEOUT_SECS, PLUGIN_HTTP_TIMEOUT_SECS};
use serde::Deserialize;
use std::sync::LazyLock;
use std::time::Duration;
use tauri::Emitter;

/// 非流式 HTTP 客户端（连接超时 + 总超时，全宿主复用连接池）
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(PLUGIN_HTTP_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(PLUGIN_HTTP_TIMEOUT_SECS))
        .build()
        .unwrap_or_default()
});

/// 流式 HTTP 客户端（仅连接超时，不设总超时 — SSE 长连接不应被截断）
static HTTP_STREAM_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(PLUGIN_HTTP_CONNECT_TIMEOUT_SECS))
        .build()
        .unwrap_or_default()
});

// ==================== 逻辑层（core 胶水与 Component Model 绑定共用） ====================

/// 逻辑层：发起 HTTP 请求（宿主代发，支持 SSE 流式推流）
///
/// request_json 格式：
/// ```json
/// {
///   "method": "POST",
///   "url": "https://api.example.com/v1/chat",
///   "headers": { "Authorization": "Bearer xxx", "Content-Type": "application/json" },
///   "body": "{...}",
///   "stream": true,
///   "streamEvent": "ai-chatbox:stream:xxx"
/// }
/// ```
///
/// 流式模式：宿主 spawn tokio 任务执行 HTTP 请求，逐 chunk 通过 emit_event 推送，
/// http_fetch 立即返回 stream_id
/// 非流式模式：block_on 执行，返回完整响应
pub(crate) fn http_fetch(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    request_json: &str,
) -> Result<Option<String>, String> {
    let request: serde_json::Value = serde_json::from_str(request_json)
        .map_err(|e| format!("http error: invalid request JSON: {}", e))?;

    let is_stream = request.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    if is_stream {
        // 流式模式：spawn 后台任务，立即返回 stream_id
        let stream_id = uuid::Uuid::new_v4().to_string();
        let stream_event = request
            .get("streamEvent")
            .and_then(|v| v.as_str())
            .unwrap_or(&stream_id)
            .to_string();

        // 流式推送依赖前端事件通道，无头上下文不可用
        let Some(app_handle) = host_ctx.app_handle.clone() else {
            return Err("http error: streaming requires app_handle".to_string());
        };

        let plugin_id_clone = plugin_id.to_string();
        let stream_event_clone = stream_event.clone();
        tokio::spawn(async move {
            if let Err(e) = execute_streaming_http(
                &request,
                &app_handle,
                &stream_event_clone,
                &plugin_id_clone,
            )
            .await
            {
                tracing::error!(
                    error = %e,
                    plugin_id = %plugin_id_clone,
                    stream_event = %stream_event_clone,
                    "Streaming HTTP request failed"
                );
                // 发送错误事件通知插件
                let _ = app_handle.emit(
                    &stream_event_clone,
                    serde_json::json!({ "error": e.to_string(), "done": true }),
                );
            }
        });

        let result_json = serde_json::json!({
            "streamId": stream_id,
            "streamEvent": stream_event,
        });
        serde_json::to_string(&result_json)
            .map(Some)
            .map_err(|e| format!("http error: response serialization failed: {}", e))
    } else {
        // 非流式模式：同步执行 HTTP 请求
        let response = block_on_async(execute_http_request(&request))
            .map_err(|e| format!("http error: {}", e))?;
        serde_json::to_string(&response)
            .map(Some)
            .map_err(|e| format!("http error: response serialization failed: {}", e))
    }
}

// ==================== Host Functions（core module 胶水） ====================

/// HTTP 代理：发起 HTTP 请求
///
/// 参数：(request_json_ptr, request_json_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
pub(super) fn host_http_fetch(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    req_ptr: u32,
    req_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let request_json = match read_wasm_string_consume(&mut caller, req_ptr, req_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_http_fetch: failed to read request JSON");
            return -1;
        }
    };

    match http_fetch(&host_ctx, &plugin_id, &request_json) {
        Ok(Some(json)) => match write_wasm_string(&mut caller, &json) {
            Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
            None => {
                tracing::error!(plugin_id = %plugin_id, "host_http_fetch: failed to write result to WASM memory");
                -1
            }
        },
        Ok(None) => write_result_to_out_ptr(&mut caller, out_ptr, 0, 0),
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_http_fetch: HTTP request failed");
            -1
        }
    }
}

// ==================== SSE Parsing Structures ====================

/// OpenAI SSE 流式响应结构
#[derive(Debug, Deserialize)]
struct OpenAiSseResponse {
    choices: Vec<OpenAiSseChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiSseChoice {
    delta: OpenAiSseDelta,
}

#[derive(Debug, Deserialize)]
struct OpenAiSseDelta {
    content: Option<String>,
}

// ==================== HTTP Proxy Execution ====================

/// 执行非流式 HTTP 请求
///
/// 宿主代为执行 HTTP 请求，返回完整响应
/// request 格式：{ "method", "url", "headers", "body" }
/// response 格式：{ "status", "body", "headers" }
async fn execute_http_request(
    request: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let method = request
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET");
    let url = request
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'url' in HTTP request"))?;
    let headers = request.get("headers").and_then(|v| as_string_map(v));
    let body = request.get("body").and_then(|v| v.as_str());

    let mut req_builder = HTTP_CLIENT.request(method.parse()?, url);

    if let Some(hdrs) = &headers {
        for (key, value) in hdrs {
            req_builder = req_builder.header(key.as_str(), value.as_str());
        }
    }

    if let Some(b) = body {
        req_builder = req_builder.body(b.to_string());
    }

    let response = req_builder.send().await?;
    let status = response.status().as_u16();

    let resp_headers: serde_json::Map<String, serde_json::Value> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_str().unwrap_or("").to_string())))
        .collect();

    let resp_body = response.text().await?;

    Ok(serde_json::json!({
        "status": status,
        "body": resp_body,
        "headers": resp_headers,
    }))
}

/// 执行流式 HTTP 请求
///
/// 宿主 spawn tokio 任务执行 HTTP 请求，逐 chunk 通过 emit_event 推送到前端
/// 插件通过监听 streamEvent 事件接收流式数据
///
/// 当请求中包含 `sseFormat` 字段时，宿主解析 SSE 事件并提取 content delta 后 emit，
/// 否则 emit 原始 chunk 数据
async fn execute_streaming_http(
    request: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    stream_event: &str,
    plugin_id: &str,
) -> anyhow::Result<()> {
    use futures_util::StreamExt;

    let method = request
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("POST");
    let url = request
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'url' in streaming HTTP request"))?;
    let headers = request.get("headers").and_then(|v| as_string_map(v));
    let body = request.get("body").and_then(|v| v.as_str());
    let sse_format = request
        .get("sseFormat")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut req_builder = HTTP_STREAM_CLIENT.request(method.parse()?, url);

    if let Some(hdrs) = &headers {
        for (key, value) in hdrs {
            req_builder = req_builder.header(key.as_str(), value.as_str());
        }
    }

    if let Some(b) = body {
        req_builder = req_builder.body(b.to_string());
    }

    let response = req_builder.send().await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_body = response.text().await.unwrap_or_default();
        // 非 2xx 响应通过事件通知前端，而非 bail（因为 tokio::spawn 中的 Err 只记录日志）
        let _ = app_handle.emit(
            stream_event,
            serde_json::json!({
                "error": format!("API error {}: {}", status, error_body),
                "done": true,
            }),
        );
        return Ok(());
    }

    if sse_format.is_empty() {
        // 原始模式：逐 chunk emit 原始字节
        let mut stream = response.bytes_stream();
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    let chunk_str = String::from_utf8_lossy(&chunk).to_string();
                    let _ = app_handle.emit(
                        stream_event,
                        serde_json::json!({
                            "chunk": chunk_str,
                            "done": false,
                        }),
                    );
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        plugin_id = %plugin_id,
                        stream_event = %stream_event,
                        "Streaming HTTP chunk read error"
                    );
                    break;
                }
            }
        }
    } else {
        // SSE 解析模式：缓冲并按格式解析 SSE 事件，提取 content delta 后 emit
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                    parse_and_emit_sse(&mut buffer, sse_format, app_handle, stream_event);
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        plugin_id = %plugin_id,
                        stream_event = %stream_event,
                        "Streaming HTTP chunk read error"
                    );
                    break;
                }
            }
        }
    }

    // 发送完成事件
    let _ = app_handle.emit(
        stream_event,
        serde_json::json!({ "done": true }),
    );

    Ok(())
}

/// 解析 SSE 事件并提取 content delta 推送到前端
///
/// SSE 规范允许 `\n\n`、`\r\n\r\n`、`\r\r` 三种事件分隔符，
/// 取缓冲区中最先出现的分隔符切分（部分服务端使用 CRLF 行尾）；
/// 根据 format 解析 data 行中的 JSON，
/// 提取文本增量后以 `{ chunk, done: false }` 格式 emit
fn parse_and_emit_sse(
    buffer: &mut String,
    format: &str,
    app_handle: &tauri::AppHandle,
    stream_event: &str,
) {
    loop {
        // 查找最先出现的事件分隔符：(位置, 分隔符字节长度)
        let separator = [
            buffer.find("\r\n\r\n").map(|p| (p, 4)),
            buffer.find("\n\n").map(|p| (p, 2)),
            buffer.find("\r\r").map(|p| (p, 2)),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|(pos, _)| *pos);

        let Some((pos, sep_len)) = separator else {
            break;
        };

        let event_text = buffer[..pos].to_string();
        buffer.drain(..pos + sep_len);

        for line in event_text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                let data = data.trim();
                if data == "[DONE]" {
                    let _ = app_handle.emit(stream_event, serde_json::json!({ "done": true }));
                    return;
                }

                match format {
                    "openai" => {
                        if let Ok(parsed) = serde_json::from_str::<OpenAiSseResponse>(data) {
                            if let Some(content) = parsed
                                .choices
                                .first()
                                .and_then(|c| c.delta.content.as_ref())
                            {
                                if !content.is_empty() {
                                    let _ = app_handle.emit(
                                        stream_event,
                                        serde_json::json!({ "chunk": content, "done": false }),
                                    );
                                }
                            }
                        }
                    }
                    _ => {
                        // 未知格式：emit 原始 data
                        let _ = app_handle.emit(
                            stream_event,
                            serde_json::json!({ "chunk": data, "done": false }),
                        );
                    }
                }
            }
        }
    }
}

/// 将 serde_json::Value 转换为 HashMap<String, String>
fn as_string_map(value: &serde_json::Value) -> Option<std::collections::HashMap<String, String>> {
    let obj = value.as_object()?;
    let mut map = std::collections::HashMap::new();
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            map.insert(k.clone(), s.to_string());
        }
    }
    Some(map)
}
