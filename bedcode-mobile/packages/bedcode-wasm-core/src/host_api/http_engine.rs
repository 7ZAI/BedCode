//! 插件 HTTP 执行引擎（票 17 批次 2 自宿主 `plugin/wasm_host.rs` HTTP 段迁入）
//!
//! 宿主代为执行插件 HTTP 请求（reqwest，移动 rustls 形态）。宿主依赖仅两处，
//! 均经 [`HostEnginePorts`] 注入（egress 安全闸门 D5 / 全局 token C4）：
//! - 跳转重校验策略：`ports.egress_redirect_policy()`
//! - jwtAuth 代注 token：`ports.global_token()`（token 不落插件）

use futures_util::StreamExt;
use serde::Deserialize;
use std::sync::Arc;

use crate::host_api::ports::HostEnginePorts;
use tauri::Emitter;

/// 非流式 `http_fetch` 响应体上限（字节）
///
/// 响应体会拷入 guest 线性内存并由插件 serde 解析（guest 指令，消耗单次调用
/// fuel 预算）；无上限响应体可能耗尽 fuel 触发 trap。大载荷必须走
/// `stream:true` 流式模式（宿主后台任务经事件逐 chunk 推送，不经 guest 内存）。
/// 32MB 对目录列举/元数据绰绰有余（guest 解析约几 G 指令，远低于 FUEL_PER_CALL）。
const PLUGIN_HTTP_RESPONSE_BODY_LIMIT_BYTES: usize = 32 * 1024 * 1024;

/// 非流式 `http_fetch` 连接超时（秒，与桌面端常量对齐）
const PLUGIN_HTTP_CONNECT_TIMEOUT_SECS: u64 = 10;
/// 非流式 `http_fetch` 总超时（秒，与桌面端常量对齐）
///
/// 插件同步 HTTP 调用（如取消上传会话）阻塞 WASM 单线程执行，
/// 对端失联时必须有界返回，否则前端表现为「取消无反应」
const PLUGIN_HTTP_TIMEOUT_SECS: u64 = 120;

// ==================== SSE Parsing Structures ====================

/// OpenAI SSE 流式响应结构
#[derive(Debug, Deserialize)]
struct OpenAiSseResponse {
    choices: Vec<OpenAiSseChoice>,
    /// 流末尾的用量信息（部分供应商在最后一个 chunk 携带，缺失时为 None）
    usage: Option<serde_json::Value>,
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
/// request 格式：{ "method", "url", "headers", "body", "jwtAuth"? }
/// response 格式：{ "status", "body", "headers" }
///
/// `jwtAuth: true`（票 13，默认 false，向后兼容）＝宿主代注
/// `Authorization: Bearer <global token>`——token 不出宿主（C4，对齐
/// host-websocket `jwt-auth` 首消息代发先例）；开关置位但 token 为空 →
/// 显性 Err（fail-visible，禁静默降级为匿名请求——桌面端必 401，晚失败不如早失败）。
pub async fn execute_http_request(request: &serde_json::Value, ports: &Arc<dyn HostEnginePorts>) -> anyhow::Result<serde_json::Value> {
    let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
    let url = request
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'url' in HTTP request"))?;
    let headers = request.get("headers").and_then(|v| as_string_map(v));
    let body = request.get("body").and_then(|v| v.as_str());
    let jwt_auth = request.get("jwtAuth").and_then(|v| v.as_bool()).unwrap_or(false);

    // 连接 + 总超时：插件同步 HTTP 调用（如取消上传会话）阻塞 WASM 单线程，
    // 无总超时时对端失联最长卡 30s connect + 无限响应等待，UI 全程无响应
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(PLUGIN_HTTP_CONNECT_TIMEOUT_SECS))
        .timeout(std::time::Duration::from_secs(PLUGIN_HTTP_TIMEOUT_SECS))
        // 跳转重校验：check_egress 只校验首跳，302 → 内网/云元数据须过同源/
        // 桌面目标/私网链白名单（egress 引擎，宿主端口投影）
        .redirect(ports.egress_redirect_policy())
        .build()?;
    let mut req_builder = client.request(method.parse()?, url);

    if let Some(hdrs) = &headers {
        for (key, value) in hdrs {
            req_builder = req_builder.header(key.as_str(), value.as_str());
        }
    }

    // 票 13：宿主代注 JWT（token 不出宿主，C4）；置位但无 token → 显性拒绝
    if let Some(token) = resolve_jwt_auth_header(jwt_auth, ports.global_token())? {
        req_builder = req_builder.bearer_auth(token);
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
    // 耗尽单次调用 fuel 预算（触发 trap）。超限立即中止连接并报错，
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
/// 当请求中包含 `sseFormat` 字段时，宿主解析 SSE 事件并提取 content delta 后 emit，
/// 否则 emit 原始 chunk 数据
pub async fn execute_streaming_http(
    request: &serde_json::Value,
    ports: &Arc<dyn HostEnginePorts>,
    app_handle: &tauri::AppHandle,
    stream_event: &str,
    plugin_id: &str,
) -> anyhow::Result<()> {
    let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("POST");
    let url = request
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'url' in streaming HTTP request"))?;
    let headers = request.get("headers").and_then(|v| as_string_map(v));
    let body = request.get("body").and_then(|v| v.as_str());
    let sse_format = request.get("sseFormat").and_then(|v| v.as_str()).unwrap_or("");

    let client = reqwest::Client::builder()
        // 跳转重校验（与 execute_http_request 同策略，见 egress 引擎端口投影）
        .redirect(ports.egress_redirect_policy())
        .build()?;
    let mut req_builder = client.request(method.parse()?, url);

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
        // 原始模式：逐 chunk emit 原始字节
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
        // SSE 解析模式：缓冲并按格式解析 SSE 事件，提取 content delta 后 emit
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                    let events = parse_and_emit_sse(&mut buffer, sse_format, app_handle, stream_event);
                    emitted_events += events;
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

/// 解析 SSE 事件并提取 content delta 推送到前端
///
/// 按 `\n\n` 分割 SSE 事件，根据 format 解析 data 行中的 JSON，
/// 提取文本增量后以 `{ chunk, done: false }` 格式 emit
fn parse_and_emit_sse(buffer: &mut String, format: &str, app_handle: &tauri::AppHandle, stream_event: &str) -> usize {
    let mut last_usage: Option<serde_json::Value> = None;
    let mut emitted = 0usize;
    while let Some(pos) = buffer.find("\n\n") {
        let event_text = buffer[..pos].to_string();
        buffer.drain(..pos + 2);

        for line in event_text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                let data = data.trim();
                if data == "[DONE]" {
                    // done 事件携带最后一次出现的 usage（无则省略，向后兼容）
                    let mut payload = serde_json::Map::new();
                    payload.insert("done".to_string(), serde_json::Value::Bool(true));
                    if let Some(usage) = last_usage.take() {
                        payload.insert("usage".to_string(), usage);
                    }
                    let _ = app_handle.emit(stream_event, serde_json::Value::Object(payload));
                    emitted += 1;
                    return emitted;
                }

                match format {
                    "openai" => {
                        if let Ok(parsed) = serde_json::from_str::<OpenAiSseResponse>(data) {
                            if parsed.usage.is_some() {
                                last_usage = parsed.usage.clone();
                            }
                            if let Some(content) = parsed.choices.first().and_then(|c| c.delta.content.as_ref()) {
                                if !content.is_empty() {
                                    let _ = app_handle
                                        .emit(stream_event, serde_json::json!({ "chunk": content, "done": false }));
                                    emitted += 1;
                                }
                            }
                        }
                    }
                    _ => {
                        // 未知格式：emit 原始 data
                        let _ = app_handle.emit(stream_event, serde_json::json!({ "chunk": data, "done": false }));
                        emitted += 1;
                    }
                }
            }
        }
    }
    emitted
}

/// jwtAuth 头注入裁决（票 13）：
/// - 开关关 → `None`（不注入；请求方自带 Authorization 时不动它）
/// - 开关开 + token 非空 → `Some(token)`（宿主代注 Bearer，token 不出宿主）
/// - 开关开 + token 空 → 显性 Err（fail-visible：禁静默降级为匿名请求）
///
/// 抽为纯函数：`global_token()` 经端口投影（token 是进程级宿主状态），
/// 决策分支在此无竞争锁死。
fn resolve_jwt_auth_header(jwt_auth: bool, token: String) -> anyhow::Result<Option<String>> {
    if !jwt_auth {
        return Ok(None);
    }
    if token.is_empty() {
        anyhow::bail!("jwtAuth requested but no global token available (not authenticated)");
    }
    Ok(Some(token))
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

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_api::ports::UnimplementedPorts;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// 禁用系统代理对 loopback 的干扰（Windows 全局代理可能拦截测试请求）
    fn disable_proxy_for_loopback() {
        std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
    }

    fn ports() -> Arc<dyn HostEnginePorts> {
        // 占位端口：redirect_policy 返回 none（直连）、global_token 空——
        // 供 mock server 往返与 jwtAuth 裁决分支断言；egress 引擎行为测试
        // 与 jwtAuth 端到端 Bearer 注入断言在宿主侧（egress 是宿主安全闸门、
        // token 真源在宿主 state，真源测试随引擎留宿主）
        Arc::new(UnimplementedPorts)
    }

    /// 极简 mock HTTP 服务器：返回固定 body，响应后关闭连接
    async fn spawn_mock_server(body: Vec<u8>) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
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
        let resp = execute_http_request(
            &json!({
                "method": "GET",
                "url": format!("http://{}/small", addr),
            }),
            &ports(),
        )
        .await
        .expect("small response must succeed");
        assert_eq!(resp["status"], 200);
        assert_eq!(resp["body"], "{\"ok\":true}");
    }

    /// 超限响应体：立即拒绝并报错引导 stream:true，绝不把大载荷交给 guest
    /// （保证 guest 侧 serde 解析工作量有界 → 不可能耗尽 fuel 预算被 trap）
    #[tokio::test]
    async fn http_fetch_oversized_response_rejected() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(vec![0u8; PLUGIN_HTTP_RESPONSE_BODY_LIMIT_BYTES + 1]).await;
        let err = execute_http_request(
            &json!({
                "method": "GET",
                "url": format!("http://{}/big", addr),
            }),
            &ports(),
        )
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

    /// jwtAuth 开关关闭：不注入 Authorization 头（捕获服务器断言无 Bearer）
    #[tokio::test]
    async fn http_fetch_without_jwt_auth_sends_no_authorization() {
        disable_proxy_for_loopback();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let _ = tx.send(String::from_utf8_lossy(&buf[..n]).to_string());
                let head = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
                let _ = sock.write_all(head.as_bytes()).await;
            }
        });
        let resp = execute_http_request(
            &json!({
                "method": "GET",
                "url": format!("http://{}/api/sessions", addr),
                "jwtAuth": false,
            }),
            &ports(),
        )
        .await
        .expect("plain request must succeed");
        assert_eq!(resp["status"], 200);
        let raw = rx.await.expect("server must capture request").to_lowercase();
        assert!(!raw.contains("authorization:"), "jwtAuth=false 不得注入 Authorization 头");
    }

    // ==================== jwtAuth（票 13） ====================

    /// 纯函数三向裁决（无全局静态竞争）：
    /// 关 = 不注入；开 + 有 token = 注入该 token；开 + 空 token = 显性 Err
    #[test]
    fn resolve_jwt_auth_header_three_way() {
        assert_eq!(resolve_jwt_auth_header(false, "any".to_string()).unwrap(), None);
        assert_eq!(
            resolve_jwt_auth_header(true, "tok-13".to_string()).unwrap(),
            Some("tok-13".to_string())
        );
        let err = resolve_jwt_auth_header(true, String::new()).expect_err("empty token must fail visibly");
        assert!(err.to_string().contains("no global token"), "got: {err}");
    }

    /// jwtAuth=true + 端口 token 空 → 显性 Err（fail-visible：禁静默匿名请求）
    #[tokio::test]
    async fn http_fetch_jwt_auth_without_token_fails_visibly() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(b"{}".to_vec()).await;
        let err = execute_http_request(
            &json!({
                "method": "GET",
                "url": format!("http://{}/api/sessions", addr),
                "jwtAuth": true,
            }),
            &ports(), // UnimplementedPorts.global_token() = ""
        )
        .await
        .expect_err("jwtAuth with empty token must fail visibly");
        assert!(err.to_string().contains("no global token"), "got: {err}");
    }
}
