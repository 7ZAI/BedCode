//! HTTP 代理域宿主实现（宿主代发请求，支持 SSE 流式推流）

use crate::wasm_core::manager::runtime::WasmHostContext;
use crate::wasm_core::runtime_util::block_on_async;
use crate::wasm_core::permission::PERMISSION_NETWORK_HTTP;
use crate::system::constants::{
    PLUGIN_HTTP_CONNECT_TIMEOUT_SECS, PLUGIN_HTTP_RESPONSE_BODY_LIMIT_BYTES, PLUGIN_HTTP_TIMEOUT_SECS,
};
use futures_util::StreamExt;
use std::sync::LazyLock;
use std::time::Duration;
use tauri::Emitter;

/// 非流式 HTTP 客户端（连接超时 + 总超时，全宿主复用连接池）
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(PLUGIN_HTTP_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(PLUGIN_HTTP_TIMEOUT_SECS))
        .redirect(redirect_policy())
        .build()
        .unwrap_or_default()
});

/// 流式 HTTP 客户端（仅连接超时，不设总超时 — SSE 长连接不应被截断）
static HTTP_STREAM_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(PLUGIN_HTTP_CONNECT_TIMEOUT_SECS))
        .redirect(redirect_policy())
        .build()
        .unwrap_or_default()
});

/// 判断目标地址是否为私网/回环/链路本地地址（动态判定，无硬编码网段）
///
/// 系统代理（如 Clash）只应代理外网：局域网文件服务（对端共享目录）请求若
/// 走代理，会被劫持到本地代理端口（127.0.0.1:10808），对端服务器收不到请求。
/// 标准库 `Ipv4Addr::is_private()` 即 RFC1918（10/8、172.16/12、192.168/16），
/// 配合 loopback/link-local，覆盖内网传输场景的全部直连目标。
fn is_private_target(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().and_then(|h| h.parse::<std::net::IpAddr>().ok()))
        .map(|ip| match ip {
            std::net::IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
            std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unicast_link_local(),
        })
        .unwrap_or(false)
}

/// 跳转裁决（纯函数，供 `redirect_policy` 与单测共用）：
///
/// reqwest 默认跟随 10 跳且不重校验目标——公网插件 API 302 到内网/云元数据
/// 在无系统代理环境（直连）下即 SSRF。规则：
/// - 公网目标：跟随；
/// - 私网目标：仅当链上前序 URL 全为私网时跟随（局域网文件服务站内跳转）；
///   其余（公网 → 私网）Stop，调用方拿到 3xx 自行处理。
fn redirect_decision(next_url: &str, previous: &[&str]) -> bool {
    if !is_private_target(next_url) {
        return true;
    }
    !previous.iter().any(|p| !is_private_target(p))
}

/// reqwest 跳转策略：`redirect_decision` 的适配层（同步裁决，见其文档）
fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let prev: Vec<&str> = attempt.previous().iter().map(|u| u.as_str()).collect();
        if redirect_decision(attempt.url().as_str(), &prev) {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

/// 直连客户端（禁系统代理）：私网目标（局域网文件服务）专用，
/// 配置与对应默认 client 一致（超时/响应上限语义不变）
static HTTP_DIRECT_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(PLUGIN_HTTP_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(PLUGIN_HTTP_TIMEOUT_SECS))
        .redirect(redirect_policy())
        .build()
        .unwrap_or_default()
});

/// 流式直连客户端（禁系统代理，仅连接超时）
static HTTP_DIRECT_STREAM_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(PLUGIN_HTTP_CONNECT_TIMEOUT_SECS))
        .redirect(redirect_policy())
        .build()
        .unwrap_or_default()
});

/// 按目标地址选择客户端：私网直连，其余走系统代理（外网插件 API 不受影响）
fn client_for(url: &str) -> &'static reqwest::Client {
    if is_private_target(url) {
        &HTTP_DIRECT_CLIENT
    } else {
        &HTTP_CLIENT
    }
}

/// 流式客户端选择（同上）
fn stream_client_for(url: &str) -> &'static reqwest::Client {
    if is_private_target(url) {
        &HTTP_DIRECT_STREAM_CLIENT
    } else {
        &HTTP_STREAM_CLIENT
    }
}

/// 发起 HTTP 请求（宿主代发，支持 SSE 流式推流）
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
    // 权限仲裁：未声明 network:http 的插件（WASM 路径）在宿主侧直接拒绝。
    // 前端 TS 路径已 fast-fail，此处是 Rust 端最终仲裁（安全边界，AGENTS.md §8）。
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_NETWORK_HTTP, "host_http_fetch") {
        return Err(format!(
            "http error: permission denied: plugin '{}' does not declare network:http",
            plugin_id
        ));
    }
    let request: serde_json::Value =
        serde_json::from_str(request_json).map_err(|e| format!("http error: invalid request JSON: {}", e))?;

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
        crate::system::error_boundary::spawn_with_error_boundary("streaming_http", async move {
            if let Err(e) = execute_streaming_http(&request, &app_handle, &stream_event_clone, &plugin_id_clone).await {
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
        let response = block_on_async(execute_http_request(&request)).map_err(|e| format!("http error: {}", e))?;
        serde_json::to_string(&response)
            .map(Some)
            .map_err(|e| format!("http error: response serialization failed: {}", e))
    }
}

// ==================== Streaming Execution ====================

/// 执行非流式 HTTP 请求
///
/// 宿主代为执行 HTTP 请求，返回完整响应
/// request 格式：{ "method", "url", "headers", "body" }
/// response 格式：{ "status", "body", "headers" }
async fn execute_http_request(request: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
    let url = request
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'url' in HTTP request"))?;
    let headers = request.get("headers").and_then(as_string_map);
    let body = request.get("body").and_then(|v| v.as_str());

    let mut req_builder = client_for(url).request(method.parse()?, url);

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
        .map(|(k, v)| {
            (
                k.to_string(),
                serde_json::Value::String(v.to_str().unwrap_or("").to_string()),
            )
        })
        .collect();

    // 响应体带上限流式读取：防止无上限响应体拷入 guest 内存 + guest serde 解析
    // 耗尽单次调用 fuel 预算（触发 trap 污染 Store）。超限立即中止连接并报错，
    // 引导插件改用 stream:true（宿主后台任务经事件逐 chunk 推送，不经 guest 内存）。
    let mut body_bytes = Vec::new();
    let mut body_stream = response.bytes_stream();
    while let Some(chunk) = body_stream.next().await {
        let chunk = chunk.map_err(|e| anyhow::anyhow!("http error: read response body failed: {}", e))?;
        if body_bytes.len() + chunk.len() > PLUGIN_HTTP_RESPONSE_BODY_LIMIT_BYTES {
            return Err(anyhow::anyhow!(
                "http error: response body exceeds {} bytes limit (use stream:true for large payloads)",
                PLUGIN_HTTP_RESPONSE_BODY_LIMIT_BYTES
            ));
        }
        body_bytes.extend_from_slice(&chunk);
    }
    let resp_body =
        String::from_utf8(body_bytes).map_err(|e| anyhow::anyhow!("http error: response body is not UTF-8: {}", e))?;

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
/// `sseFormat` 不再有供应商语义（票据 02 宿主零业务语义）：空串 = raw 模式
/// （逐网络 chunk 透传原始字节，消费侧自行切分）；非空 = 通用 SSE 模式（按事件
/// 分隔符切分、透传 data 行原文）。OpenAI/Anthropic 等格式解析全部由插件消费侧自管。
async fn execute_streaming_http(
    request: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    stream_event: &str,
    plugin_id: &str,
) -> anyhow::Result<()> {
    let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("POST");
    let url = request
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'url' in streaming HTTP request"))?;
    let headers = request.get("headers").and_then(as_string_map);
    let body = request.get("body").and_then(|v| v.as_str());
    let sse_format = request.get("sseFormat").and_then(|v| v.as_str()).unwrap_or("");

    let mut req_builder = stream_client_for(url).request(method.parse()?, url);

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
        tracing::warn!(status, stream_event, "Streaming HTTP non-2xx response");
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

    tracing::debug!(
        status = response.status().as_u16(),
        sse_format = %sse_format,
        stream_event,
        "Streaming HTTP connected"
    );

    let mut emitted_events: usize = 0;
    if sse_format.is_empty() {
        // 原始模式：逐 chunk emit 原始字节（消费侧自行切分 SSE 事件）
        let mut stream = response.bytes_stream();
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    let chunk_str = String::from_utf8_lossy(&chunk).to_string();
                    emitted_events += 1;
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
        // 通用 SSE 模式：按事件分隔符切分、透传 data 行原文（票据 02 宿主零业务语义）。
        // 不做任何供应商格式解析（OpenAI/Anthropic 等语义由插件消费侧自管）。
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                    for data in extract_sse_data_lines(&mut buffer) {
                        emitted_events += 1;
                        let _ = app_handle.emit(stream_event, serde_json::json!({ "chunk": data, "done": false }));
                    }
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
    let _ = app_handle.emit(stream_event, serde_json::json!({ "done": true }));

    tracing::debug!(
        emitted_events,
        plugin_id = %plugin_id,
        stream_event,
        "Streaming HTTP finished"
    );

    Ok(())
}

/// 通用 SSE 事件切分与 data 提取（宿主零业务语义，票据 02）
///
/// SSE 规范允许 `\n\n`、`\r\n\r\n`、`\r\r` 三种事件分隔符，取缓冲区中最先出现的
/// 切分（部分服务端使用 CRLF 行尾）；每条完整事件抽取 `data:` 行原文透传，
/// 不做任何供应商格式解析（chunk 内容 JSON 语义由插件消费侧自管）。
/// 跨 chunk 缓冲：未闭合的半截事件留在缓冲区，下次追加后补齐。
/// 返回本次提取的 data 行内容列表（无完整事件时为空）。
fn extract_sse_data_lines(buffer: &mut String) -> Vec<String> {
    let mut out = Vec::new();
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
                if !data.is_empty() {
                    out.push(data.to_string());
                }
            }
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// 禁用系统代理对 loopback 的干扰（Windows 全局代理可能拦截测试请求）
    fn disable_proxy_for_loopback() {
        std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
    }

    /// 极简 mock HTTP 服务器：返回固定 body，响应后关闭连接
    async fn spawn_mock_server(body: Vec<u8>) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                // 读完请求头即可响应（忽略 body）
                let _ = sock.read(&mut buf).await;
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(&body).await;
            }
        });
        addr
    }

    /// 正常小响应体：完整返回，不受上限影响
    #[tokio::test]
    async fn http_fetch_small_response_ok() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(b"{\"ok\":true}".to_vec()).await;
        let resp = execute_http_request(&json!({
            "method": "GET",
            "url": format!("http://{}/small", addr),
        }))
        .await
        .expect("small response must succeed");
        assert_eq!(resp["status"], 200);
        assert_eq!(resp["body"], "{\"ok\":true}");
    }

    /// 未声明 network:http 的插件调用 host-http.fetch：宿主侧直接拒绝（Rust 端最终仲裁）。
    /// 错误消息含明确原因，与请求类错误可区分（审计 H1 修复）。
    #[test]
    fn http_fetch_permission_denied_rejected() {
        let ctx = super::super::tests::build_host_ctx();
        // 未授予任何权限（含 network:http）
        let err = http_fetch(&ctx, "p1", r#"{"url":"http://127.0.0.1:1/x"}"#)
            .expect_err("unpermissioned fetch must be rejected");
        assert!(
            err.contains("permission denied") && err.contains("network:http"),
            "error should state permission reason, got: {}",
            err
        );
    }

    /// 声明 network:http 后放行：请求进入执行阶段（此处以缺 url 的请求 JSON 验证
    /// 错误从「权限拒绝」变为「请求错误」，证明权限检查通过且未碰网络）。
    #[test]
    fn http_fetch_permission_granted_passes() {
        let ctx = super::super::tests::build_host_ctx();
        super::super::tests::grant_permissions(&ctx, "p1", &[PERMISSION_NETWORK_HTTP]);
        // 权限已放行 → 错误是请求解析/执行类，不再是 permission denied
        let err = http_fetch(&ctx, "p1", r#"{"method":"GET"}"#).expect_err("missing url is a request error");
        assert!(
            err.contains("Missing 'url'") || err.contains("http error"),
            "after permission, error should be request-level, got: {}",
            err
        );
        assert!(
            !err.contains("permission denied"),
            "permissioned plugin should not hit permission denial, got: {}",
            err
        );
    }

    /// 非法请求 JSON：权限放行后仍是解析错误（错误分类保持：权限拒绝 ≠ 请求错误）
    #[test]
    fn http_fetch_invalid_json_is_request_error_not_permission() {
        let ctx = super::super::tests::build_host_ctx();
        super::super::tests::grant_permissions(&ctx, "p1", &[PERMISSION_NETWORK_HTTP]);
        let err = http_fetch(&ctx, "p1", "not-json").expect_err("invalid JSON is a request error");
        assert!(
            err.contains("invalid request JSON"),
            "error should be parse-level, got: {}",
            err
        );
    }

    /// 超限响应体：立即拒绝并报错引导 stream:true，绝不把大载荷交给 guest
    /// （保证 guest 侧 serde 解析工作量有界 → 不可能耗尽 fuel 预算被 trap）
    #[tokio::test]
    async fn http_fetch_oversized_response_rejected() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(vec![0u8; PLUGIN_HTTP_RESPONSE_BODY_LIMIT_BYTES + 1]).await;
        let err = execute_http_request(&json!({
            "method": "GET",
            "url": format!("http://{}/big", addr),
        }))
        .await
        .expect_err("oversized response must be rejected");
        assert!(
            err.to_string().contains("exceeds"),
            "error should mention size limit, got: {}",
            err
        );
        assert!(
            err.to_string().contains("stream:true"),
            "error should guide to streaming mode, got: {}",
            err
        );
    }

    /// 通用 SSE 事件切分（票据 02 宿主零业务语义）：三种分隔符都能切分并提取 data 行原文
    #[test]
    fn sse_extract_supports_all_separators() {
        let mut buf = "data: a\n\ndata: b\r\n\r\ndata: c\r\r".to_string();
        let events = extract_sse_data_lines(&mut buf);
        assert_eq!(events, ["a", "b", "c"]);
        assert!(buf.is_empty());
    }

    /// 跨 chunk 缓冲：半截事件留在缓冲区，下次追加后补齐
    #[test]
    fn sse_extract_buffers_across_chunks() {
        let mut buf = "data: hel".to_string();
        assert!(extract_sse_data_lines(&mut buf).is_empty());
        buf.push_str("lo\n\n");
        assert_eq!(extract_sse_data_lines(&mut buf), ["hello"]);
        assert!(buf.is_empty());
    }

    /// 无 data 行的事件（注释/仅 event 字段）：忽略，不产出
    #[test]
    fn sse_extract_ignores_events_without_data() {
        let mut buf = ": keep-alive\n\ndata: ok\n\n".to_string();
        assert_eq!(extract_sse_data_lines(&mut buf), ["ok"]);
    }

    /// 供应商标记（如 [DONE]）按 data 原文透传，宿主不做任何供应商语义解析（票据 02）
    #[test]
    fn sse_extract_passes_through_vendor_markers_raw() {
        let mut buf = "data: [DONE]\n\n".to_string();
        assert_eq!(extract_sse_data_lines(&mut buf), ["[DONE]"]);
    }

    /// 私网目标判定：局域网/回环/链路本地 → 直连（不走系统代理）
    ///
    /// 文件服务对端通常是局域网 IP（RFC1918），标准库 is_private 动态判定，
    /// 不硬编码网段；域名（外网 API）→ false 走系统代理
    #[test]
    fn is_private_target_classifies_correctly() {
        // RFC1918：10/8、172.16/12、192.168/16
        assert!(is_private_target(
            "http://10.60.74.97:43145/com.bedcode.file-transfer/files/list"
        ));
        assert!(is_private_target("http://192.168.1.5:8080/"));
        assert!(is_private_target("http://172.16.0.1/"));
        // loopback 与链路本地
        assert!(is_private_target("http://127.0.0.1:5173/"));
        assert!(is_private_target("http://169.254.1.1/"));
        // 外网域名/IP → 走代理
        assert!(!is_private_target("https://api.example.com/v1/chat"));
        assert!(!is_private_target("http://8.8.8.8/"));
        // 无 host 的畸形 URL → false（默认走代理，行为保守）
        assert!(!is_private_target("not a url"));
    }

    /// 跳转裁决：公网→私网阻断（无系统代理环境直连即 SSRF）；私网→私网放行
    #[test]
    fn redirect_decision_blocks_public_to_private() {
        // 公网跳转：跟随
        assert!(redirect_decision(
            "https://cdn.example.com/file",
            &["https://api.github.com/x"],
        ));
        // 外网 302 → 内网 / 回环 / 云元数据：Stop
        assert!(!redirect_decision(
            "http://192.168.1.5:8080/x",
            &["https://api.example.com/y"],
        ));
        assert!(!redirect_decision(
            "http://127.0.0.1:8000/meta",
            &["https://api.example.com/y"],
        ));
        assert!(!redirect_decision(
            "http://169.254.169.254/latest/meta-data",
            &["https://api.example.com/y"],
        ));
        // 私网→私网（局域网文件服务站内跳转）：跟随
        assert!(redirect_decision(
            "http://192.168.1.9/x",
            &["http://192.168.1.5:8080/a"],
        ));
        // 混合链（私网前序 + 公网）跳私网：Stop
        assert!(!redirect_decision(
            "http://192.168.1.9/x",
            &["http://192.168.1.5:8080/a", "https://api.example.com/y"],
        ));
    }
}
