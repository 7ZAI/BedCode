//! 薄透传 HTTP 客户端
//!
//! 协议请求构建已全部前移到前端 TS 适配层（ADR-0010）：供应商差异
//! （OpenAI / Anthropic / Gemini 请求形状、SSE 解析）收敛在插件 `src/adapters/`，
//! 本模块不再持有任何方言知识，只对前端传来的 http_fetch 载荷做最小校验后
//! 透传宿主；流式统一走 raw 模式（`sseFormat` 为空），SSE 语义由前端解析。

use bedcode_plugin_api::host::HostHttp;
use bedcode_plugin_api::WasmHost;

/// 校验 http_fetch 载荷（url / method 齐备；流式请求另要求 streamEvent）
///
/// `expect_stream` 为 true 时要求 `streamEvent`（流事件通道），
/// 非流式请求（chat-complete / fetch-models）不要求。
pub fn validate_request(request: &serde_json::Value, expect_stream: bool) -> anyhow::Result<()> {
    let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
    if method.is_empty() {
        return Err(anyhow::anyhow!("http request missing method"));
    }
    let url = request.get("url").and_then(|v| v.as_str()).unwrap_or("");
    if url.is_empty() {
        return Err(anyhow::anyhow!("http request missing url"));
    }
    if expect_stream {
        let stream_event = request
            .get("streamEvent")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if stream_event.is_empty() {
            return Err(anyhow::anyhow!("http stream request missing streamEvent"));
        }
    }
    Ok(())
}

/// 流式对话：透传前端构建的 raw 模式请求，返回宿主结果（{ streamId, streamEvent }）
///
/// 前端监听 `ai-chatbox:stream:{stream_id}` 接收逐 chunk 原始字节并自解析 SSE。
pub fn chat_stream(
    stream_id: &str,
    request: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    validate_request(request, true)?;
    // 流事件通道必须与命令声明的 streamId 一致（前端监听同名事件，不一致则消息丢失）；
    // 精确匹配而非后缀匹配——后缀会误放行 `ai-chatbox:stream:xs1` 与 `s1` 这类不相关组合
    let expected = format!("ai-chatbox:stream:{}", stream_id);
    let stream_event = request
        .get("streamEvent")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if stream_event != expected {
        return Err(anyhow::anyhow!(
            "chat_stream: streamEvent mismatch: {} vs {}",
            stream_event,
            expected
        ));
    }

    let host = WasmHost;
    host.http_fetch(request)
        .map_err(|e| anyhow::anyhow!("http_fetch failed for streaming request: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("http_fetch returned empty result"))
}

/// 非流式对话（测试连接）：透传请求，原样返回宿主响应（status/body/headers），
/// 回复文本解析由前端 adapter 完成
pub fn chat_complete(request: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    validate_request(request, false)?;
    let host = WasmHost;
    host.http_fetch(request)
        .map_err(|e| anyhow::anyhow!("http_fetch failed for non-streaming request: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("http_fetch returned empty result"))
}

/// 拉取模型列表：透传请求，原样返回宿主响应（data[].id 解析由前端完成）
pub fn fetch_models(request: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    validate_request(request, false)?;
    let host = WasmHost;
    host.http_fetch(request)
        .map_err(|e| anyhow::anyhow!("http_fetch failed for models request: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("http_fetch returned empty result"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validate_request_requires_method_and_url() {
        assert!(validate_request(&json!({}), false).is_err());
        assert!(validate_request(&json!({ "url": "https://a.com" }), false).is_err());
        assert!(validate_request(&json!({ "method": "GET" }), false).is_err());
        assert!(validate_request(
            &json!({ "method": "GET", "url": "https://a.com/models" }),
            false
        )
        .is_ok());
    }

    #[test]
    fn validate_request_stream_requires_stream_event() {
        assert!(validate_request(&json!({ "method": "POST", "url": "https://a.com" }), true)
            .is_err());
        assert!(validate_request(
            &json!({
                "method": "POST",
                "url": "https://a.com/chat/completions",
                "streamEvent": "ai-chatbox:stream:s1",
            }),
            true
        )
        .is_ok());
    }

    #[test]
    fn chat_stream_rejects_mismatched_stream_event() {
        // 校验在 http_fetch 之前完成，不触网即可断言错误路径
        let request = json!({
            "method": "POST",
            "url": "https://a.com/chat/completions",
            "streamEvent": "ai-chatbox:stream:other",
        });
        let err = chat_stream("s1", &request).unwrap_err();
        assert!(
            err.to_string().contains("streamEvent mismatch"),
            "error should mention mismatch, got: {}",
            err
        );
    }

    #[test]
    fn chat_stream_rejects_suffix_collision() {
        // 后缀匹配会误放行 `ai-chatbox:stream:xs1` 与 `s1`（前缀不同），精确匹配必须拒绝
        let request = json!({
            "method": "POST",
            "url": "https://a.com/chat/completions",
            "streamEvent": "ai-chatbox:stream:xs1",
        });
        let err = chat_stream("s1", &request).unwrap_err();
        assert!(
            err.to_string().contains("streamEvent mismatch"),
            "suffix collision should be rejected, got: {}",
            err
        );
    }
}
