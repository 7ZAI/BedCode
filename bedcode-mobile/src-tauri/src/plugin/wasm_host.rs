//! WASM Host Utilities
//!
//! 宿主侧工具函数 — SQL 表名校验、数据库列转换、HTTP 代理执行
//! 从原 host_context.rs 迁移核心逻辑，适配 WASM Host Function 调用模式

use futures_util::StreamExt;
use regex::Regex;
use serde::Deserialize;
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

// ==================== SQL Table Name Validation ====================

/// 验证 SQL 语句中的表名是否以插件专属前缀开头
///
/// WASM 插件只能操作 `plugin_{sanitized_id}_` 前缀的表，
/// 防止插件读写宿主或其他插件的数据表
///
/// # Table Name Extraction
/// 从 SQL 中提取表名，覆盖常见 DML/DDL 语句：
/// - CREATE TABLE / INSERT INTO / UPDATE / DELETE FROM
/// - SELECT ... FROM / ALTER TABLE / DROP TABLE
///
/// # Sanitization
/// plugin_id 中的 `.` 和 `-` 替换为 `_`，确保表名前缀合法
pub fn validate_sql_table_prefix(plugin_id: &str, sql: &str) -> crate::Result<()> {
    let sanitized_id = plugin_id.replace('.', "_").replace('-', "_");
    let expected_prefix = format!("plugin_{}_", sanitized_id);

    let table_names = extract_table_names(sql);

    for table in table_names {
        if !table.starts_with(&expected_prefix) {
            return Err(crate::AppError::Plugin(format!(
                "SQL table name '{}' does not match required prefix '{}' for plugin '{}'",
                table, expected_prefix, plugin_id
            )));
        }
    }

    Ok(())
}

/// 从 SQL 语句中提取表名
///
/// 使用正则匹配常见 SQL 关键字后的表名标识符
fn extract_table_names(sql: &str) -> Vec<String> {
    let mut tables = Vec::new();

    let patterns = [
        r#"(?i)\bCREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bINSERT\s+INTO\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bUPDATE\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bDELETE\s+FROM\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bFROM\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bJOIN\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bALTER\s+TABLE\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bDROP\s+TABLE\s+(?:IF\s+EXISTS\s+)?[`"\[]?(\w+)[`"\]]?"#,
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            for cap in re.captures_iter(sql) {
                if let Some(m) = cap.get(1) {
                    let name = m.as_str().to_string();
                    if !tables.contains(&name) {
                        tables.push(name);
                    }
                }
            }
        }
    }

    tables
}

// ==================== Database Column Conversion ====================

/// 将 rusqlite 行的指定列转换为 serde_json::Value
///
/// 按类型优先级尝试读取：i64 -> f64 -> String -> bool -> blob -> Null
/// rusqlite 的 FromSql 支持 i64/f64/String/bool 等，但不支持 serde_json::Value
pub fn column_to_json(row: &rusqlite::Row<'_>, col_index: usize) -> serde_json::Value {
    // 先尝试整数
    if let Ok(v) = row.get::<_, i64>(col_index) {
        // 区分整数和浮点数：如果该列实际是 REAL 类型，i64 读取可能截断
        if let Ok(fv) = row.get::<_, f64>(col_index) {
            if (fv as i64) as f64 != fv {
                return serde_json::Value::Number(
                    serde_json::Number::from_f64(fv).unwrap_or(serde_json::Number::from(0)),
                );
            }
        }
        return serde_json::Value::Number(serde_json::Number::from(v));
    }
    // 尝试浮点数
    if let Ok(v) = row.get::<_, f64>(col_index) {
        return serde_json::Value::Number(
            serde_json::Number::from_f64(v).unwrap_or(serde_json::Number::from(0)),
        );
    }
    // 尝试字符串
    if let Ok(v) = row.get::<_, String>(col_index) {
        return serde_json::Value::String(v);
    }
    // 尝试布尔值
    if let Ok(v) = row.get::<_, bool>(col_index) {
        return serde_json::Value::Bool(v);
    }
    // 尝试 blob（Vec<u8>）— 转为 hex 字符串
    if let Ok(v) = row.get::<_, Vec<u8>>(col_index) {
        use std::fmt::Write;
        let mut hex = String::with_capacity(v.len() * 2);
        for byte in &v {
            write!(hex, "{:02x}", byte).unwrap();
        }
        return serde_json::Value::String(hex);
    }
    // NULL 或无法识别的类型
    serde_json::Value::Null
}

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
/// request 格式：{ "method", "url", "headers", "body" }
/// response 格式：{ "status", "body", "headers" }
pub async fn execute_http_request(
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

    // 连接 + 总超时：插件同步 HTTP 调用（如取消上传会话）阻塞 WASM 单线程，
    // 无总超时时对端失联最长卡 30s connect + 无限响应等待，UI 全程无响应
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(PLUGIN_HTTP_CONNECT_TIMEOUT_SECS))
        .timeout(std::time::Duration::from_secs(PLUGIN_HTTP_TIMEOUT_SECS))
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
    let status = response.status().as_u16();

    let resp_headers: serde_json::Map<String, serde_json::Value> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_str().unwrap_or("").to_string())))
        .collect();

    // 响应体带上限流式读取：防止无上限响应体拷入 guest 内存 + guest serde 解析
    // 耗尽单次调用 fuel 预算（触发 trap）。超限立即中止连接并报错，
    // 引导插件改用 stream:true（宿主后台任务经事件逐 chunk 推送，不经 guest 内存）。
    let mut body_bytes = Vec::new();
    let mut body_stream = response.bytes_stream();
    while let Some(chunk) = body_stream.next().await {
        let chunk = chunk
            .map_err(|e| anyhow::anyhow!("http error: read response body failed: {}", e))?;
        if body_bytes.len() + chunk.len() > PLUGIN_HTTP_RESPONSE_BODY_LIMIT_BYTES {
            return Err(anyhow::anyhow!(
                "http error: response body exceeds {} bytes limit (use stream:true for large payloads)",
                PLUGIN_HTTP_RESPONSE_BODY_LIMIT_BYTES
            ));
        }
        body_bytes.extend_from_slice(&chunk);
    }
    let resp_body = String::from_utf8(body_bytes).map_err(|e| {
        anyhow::anyhow!("http error: response body is not UTF-8: {}", e)
    })?;

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
    app_handle: &tauri::AppHandle,
    stream_event: &str,
    plugin_id: &str,
) -> anyhow::Result<()> {
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

    let client = reqwest::Client::new();
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
    let _ = app_handle.emit(
        stream_event,
        serde_json::json!({ "done": true }),
    );

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
fn parse_and_emit_sse(
    buffer: &mut String,
    format: &str,
    app_handle: &tauri::AppHandle,
    stream_event: &str,
) -> usize {
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
                                    emitted += 1;
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
                        emitted += 1;
                    }
                }
            }
        }
    }
    emitted
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

    /// 超限响应体：立即拒绝并报错引导 stream:true，绝不把大载荷交给 guest
    /// （保证 guest 侧 serde 解析工作量有界 → 不可能耗尽 fuel 预算被 trap）
    #[tokio::test]
    async fn http_fetch_oversized_response_rejected() {
        disable_proxy_for_loopback();
        let addr =
            spawn_mock_server(vec![0u8; PLUGIN_HTTP_RESPONSE_BODY_LIMIT_BYTES + 1]).await;
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

    #[test]
    fn test_sanitize_plugin_id() {
        let sanitized = "com.example.my-plugin".replace('.', "_").replace('-', "_");
        assert_eq!(sanitized, "com_example_my_plugin");
    }

    #[test]
    fn test_validate_sql_table_prefix_valid() {
        let result = validate_sql_table_prefix(
            "com.example.my-plugin",
            "INSERT INTO plugin_com_example_my_plugin_data (id, name) VALUES (1, 'test')",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_sql_table_prefix_invalid() {
        let result = validate_sql_table_prefix(
            "com.example.my-plugin",
            "INSERT INTO sessions (id) VALUES ('abc')",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_sql_table_prefix_multiple_tables() {
        let result = validate_sql_table_prefix(
            "my-plugin",
            "SELECT * FROM plugin_my_plugin_data JOIN sessions ON sessions.id = plugin_my_plugin_data.session_id",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_sql_table_prefix_create_table() {
        let result = validate_sql_table_prefix(
            "my-plugin",
            "CREATE TABLE IF NOT EXISTS plugin_my_plugin_cache (key TEXT PRIMARY KEY, value TEXT)",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_sql_table_prefix_drop_table() {
        let result = validate_sql_table_prefix(
            "my-plugin",
            "DROP TABLE IF EXISTS plugin_my_plugin_cache",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_sql_table_prefix_alter_table() {
        let result = validate_sql_table_prefix(
            "my-plugin",
            "ALTER TABLE plugin_my_plugin_cache ADD COLUMN updated_at TEXT",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_table_names() {
        let tables = extract_table_names(
            "INSERT INTO users (id) VALUES (1); SELECT * FROM orders",
        );
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"orders".to_string()));
    }

    #[test]
    fn test_extract_table_names_quoted() {
        let tables = extract_table_names("INSERT INTO `my-table` (id) VALUES (1)");
        assert!(tables.contains(&"my".to_string()));
    }
}
