//! 任务状态与自动授权模式管理
//!
//! 通过插件独立数据库持久化任务历史，通过 broadcast_sync 广播变更到移动端。
//! HTTP 端点处理逻辑在此实现，由 lib.rs 的 _http_endpoint command 路由调用。

use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::Value;

// ==================== 查询辅助函数 ====================

/// 查询任务历史行 — 按 session_id 查找最新一条
fn find_task_by_session(host: &WasmHost, session_id: &str) -> Option<Value> {
    let sql = format!(
        "SELECT * FROM task_history WHERE session_id = '{}' ORDER BY created_at DESC LIMIT 1",
        session_id.replace('\'', "''")
    );
    let result = host.plugin_db_query(&sql)?;
    result.as_array()?.first().cloned()
}

/// 查询任务历史行 — 按 claude_sid 查找最新一条
fn find_task_by_claude_sid(host: &WasmHost, claude_sid: &str) -> Option<Value> {
    let sql = format!(
        "SELECT * FROM task_history WHERE claude_sid = '{}' ORDER BY created_at DESC LIMIT 1",
        claude_sid.replace('\'', "''")
    );
    let result = host.plugin_db_query(&sql)?;
    result.as_array()?.first().cloned()
}

// ==================== HTTP 端点处理 ====================

/// 处理 HTTP 端点请求
///
/// 路由：
/// - POST /task-status → update_task_status
/// - POST /session-mode → set_session_mode
/// - GET /session-mode → get_session_mode
pub fn handle_http_endpoint(host: &WasmHost, method: &str, path: &str, body: &Value, query: &Value) -> Value {
    host.log_debug(&format!("handle_http_endpoint: {} {}", method, path));

    match (method, path) {
        ("POST", "task-status") => handle_update_task_status(host, body),
        ("POST", "session-mode") => handle_set_session_mode(host, body, query),
        ("GET", "session-mode") => handle_get_session_mode(host, query),
        _ => {
            host.log_warn(&format!("Unknown HTTP endpoint: {} {}", method, path));
            error_response(404, &format!("Not found: {} {}", method, path))
        }
    }
}

/// POST /task-status — 接收 Claude Code hook 推送的任务状态
fn handle_update_task_status(host: &WasmHost, body: &Value) -> Value {
    host.log_debug(&format!("task-status body: {}", body));

    let session_id = body.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    let status = body.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let reason = body.get("reason").and_then(|v| v.as_str());
    let questions = body.get("questions");
    let bedcode_session_id = body.get("bedcode_session_id").and_then(|v| v.as_str());

    host.log_debug(&format!(
        "task-status parsed: session_id={}, status={}, reason={:?}, has_questions={}, bedcode_sid={:?}",
        session_id, status, reason, questions.is_some(), bedcode_session_id
    ));

    if session_id.is_empty() {
        host.log_warn("task-status rejected: empty session_id");
        return error_response(400, "Missing session_id");
    }

    // 验证 status 值
    let valid_statuses = ["idle", "in_progress", "asking", "completed", "interrupted"];
    if !valid_statuses.contains(&status) {
        host.log_warn(&format!("task-status invalid status: '{}' for session_id={}", status, session_id));
        return error_response(400, &format!("Invalid task status: {}. Must be one of: {}", status, valid_statuses.join(", ")));
    }

    // 解析 bedcode_session_id
    let resolved_session_id = bedcode_session_id.filter(|s| !s.is_empty()).unwrap_or(session_id);

    // 查找已有任务记录
    let existing = find_task_by_claude_sid(host, session_id)
        .or_else(|| find_task_by_session(host, resolved_session_id));

    if let Some(row) = existing {
        // 更新已有记录
        let task_id = row.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let mut sql_parts = vec![
            format!("status = '{}'", status.replace('\'', "''")),
            "updated_at = datetime('now')".to_string(),
        ];

        if let Some(r) = reason {
            sql_parts.push(format!("exit_reason = '{}'", r.replace('\'', "''")));
        }
        if let Some(q) = questions {
            let q_str = serde_json::to_string(q).unwrap_or_default().replace('\'', "''");
            sql_parts.push(format!("questions = '{}'", q_str));
        }

        // 更新 session_id 映射
        if let Some(bedcode_sid) = bedcode_session_id.filter(|s| !s.is_empty()) {
            sql_parts.push(format!("session_id = '{}'", bedcode_sid.replace('\'', "''")));
            sql_parts.push(format!("claude_sid = '{}'", session_id.replace('\'', "''")));
        }

        // 状态转换时更新时间戳
        match status {
            "in_progress" => sql_parts.push("started_at = datetime('now')".to_string()),
            "completed" | "interrupted" | "failed" => sql_parts.push("completed_at = datetime('now')".to_string()),
            _ => {}
        }

        let sql = format!(
            "UPDATE task_history SET {} WHERE id = '{}'",
            sql_parts.join(", "),
            task_id.replace('\'', "''")
        );
        let affected = host.plugin_db_execute(&sql);
        host.log_debug(&format!("UPDATE task_history: affected={}", affected));
    } else {
        // 创建新记录
        let questions_str = questions
            .map(|q| serde_json::to_string(q).unwrap_or_default().replace('\'', "''"))
            .unwrap_or_default();
        let reason_str = reason
            .map(|r| r.replace('\'', "''"))
            .unwrap_or_default();

        let sql = format!(
            "INSERT INTO task_history (id, name, status, session_id, claude_sid, exit_reason, questions, created_at, updated_at) \
             VALUES (lower(hex(randomblob(16))), '', '{}', '{}', '{}', '{}', '{}', datetime('now'), datetime('now'))",
            status.replace('\'', "''"),
            resolved_session_id.replace('\'', "''"),
            session_id.replace('\'', "''"),
            reason_str,
            questions_str,
        );
        let affected = host.plugin_db_execute(&sql);
        host.log_debug(&format!("INSERT task_history: affected={}", affected));
    }

    // 广播状态变更到移动端
    let mut broadcast_payload = serde_json::json!({
        "type": "TaskStatusChanged",
        "session_id": resolved_session_id,
        "task_status": status,
    });
    if let Some(r) = reason {
        broadcast_payload["task_reason"] = serde_json::Value::String(r.to_string());
    }
    if let Some(q) = questions {
        broadcast_payload["task_questions"] = q.clone();
    }
    host.broadcast_sync(&broadcast_payload);

    // 通过消息总线通知其他插件任务状态变更
    host.bus_publish("task:status-changed", &serde_json::json!({
        "session_id": resolved_session_id,
        "task_status": status,
    }));

    host.log_info(&format!("Task status updated: claude_sid={} bedcode_sid={} status={}", session_id, resolved_session_id, status));

    // 任务终态时检查队列，尝试调度下一个任务
    // idle 不触发：仅表示"无任务运行"，SessionStart 时推送 idle，此时不应出队
    if matches!(status, "completed" | "interrupted") {
        crate::queue::try_dispatch_next(&host, resolved_session_id);
    }

    ok_response()
}

/// POST /session-mode — 设置会话自动授权模式
fn handle_set_session_mode(host: &WasmHost, body: &Value, query: &Value) -> Value {
    host.log_debug(&format!("session-mode POST body: {}, query: {}", body, query));

    let session_id = body.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    let auto_approve = body.get("auto_approve").and_then(|v| v.as_bool()).unwrap_or(false);
    let token = body.get("token").and_then(|v| v.as_str()).unwrap_or("");

    host.log_debug(&format!(
        "session-mode POST parsed: session_id={}, auto_approve={}, has_token={}",
        session_id, auto_approve, !token.is_empty()
    ));

    if session_id.is_empty() {
        host.log_warn("session-mode POST rejected: empty session_id");
        return error_response(400, "Missing session_id");
    }

    // 双认证：plugin token 或 query 中的 token
    let token_valid = validate_token(host, token) || validate_token_from_query(host, query);
    if !token_valid {
        host.log_warn(&format!("session-mode POST auth failed: session_id={}, body_token_len={}, query_token={:?}",
            session_id, token.len(), query.get("token").and_then(|v| v.as_str()).map(|t| t.len())));
        return error_response(403, "Invalid plugin token or JWT authentication");
    }

    // 更新任务历史表中的 auto_approve 字段
    let sql = format!(
        "UPDATE task_history SET auto_approve = {}, updated_at = datetime('now') WHERE session_id = '{}' ORDER BY created_at DESC LIMIT 1",
        if auto_approve { 1 } else { 0 },
        session_id.replace('\'', "''")
    );
    host.plugin_db_execute(&sql);

    // 广播模式变更到移动端
    host.broadcast_sync(&serde_json::json!({
        "type": "SessionModeChanged",
        "session_id": session_id,
        "auto_approve": auto_approve,
    }));
    host.log_debug(&format!("broadcast_sync: SessionModeChanged for session_id={}", session_id));

    // 通过消息总线通知其他插件会话模式变更
    host.bus_publish("session:mode-changed", &serde_json::json!({
        "session_id": session_id,
        "auto_approve": auto_approve,
    }));

    host.log_info(&format!("Session mode set: session_id={}, auto_approve={}", session_id, auto_approve));
    ok_response()
}

/// GET /session-mode — 查询会话自动授权模式
fn handle_get_session_mode(host: &WasmHost, query: &Value) -> Value {
    host.log_debug(&format!("session-mode GET query: {}", query));

    let session_id = query.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    let token = query.get("token").and_then(|v| v.as_str()).unwrap_or("");

    host.log_debug(&format!(
        "session-mode GET parsed: session_id={}, has_token={}",
        session_id, !token.is_empty()
    ));

    if session_id.is_empty() {
        host.log_warn("session-mode GET rejected: empty session_id");
        return error_response(400, "Missing session_id");
    }

    // 验证 plugin token
    if !validate_token(host, token) {
        host.log_warn(&format!("session-mode GET auth failed: session_id={}, token_len={}", session_id, token.len()));
        return error_response(403, "Invalid plugin token");
    }

    // 解析 Claude Code session_id → BedCode PTY session_id
    let resolved_id = resolve_session_id(host, session_id);
    host.log_debug(&format!("session-mode GET resolved: claude_sid={} → resolved_sid={}", session_id, resolved_id));

    // 从任务历史表查询 auto_approve
    let auto_approve = find_task_by_session(host, &resolved_id)
        .and_then(|row| row.get("auto_approve").cloned())
        .and_then(|v| v.as_i64())
        .map(|v| v != 0)
        .unwrap_or(false);

    host.log_debug(&format!("Session mode queried: claude_sid={} resolved_sid={} auto_approve={}", session_id, resolved_id, auto_approve));

    ok_response_with_data(serde_json::json!({
        "session_id": session_id,
        "auto_approve": auto_approve,
    }))
}

// ==================== 辅助函数 ====================

/// 验证 plugin token
fn validate_token(host: &WasmHost, token: &str) -> bool {
    if token.is_empty() {
        host.log_debug("validate_token: empty token provided");
        return false;
    }
    let config_token = host.config_get("plugin.token").unwrap_or_default();
    if config_token.is_empty() {
        host.log_warn("validate_token: no plugin.token configured in host config");
        return false;
    }
    let valid = token == config_token;
    if !valid {
        host.log_debug(&format!("validate_token: mismatch (input_len={}, config_len={})", token.len(), config_token.len()));
    }
    valid
}

/// 从 query 参数中验证 token
fn validate_token_from_query(host: &WasmHost, query: &Value) -> bool {
    let token = query.get("token").and_then(|v| v.as_str()).unwrap_or("");
    validate_token(host, token)
}

/// 解析 Claude Code session_id → BedCode PTY session_id
///
/// 从任务历史表查找 claude_sid → session_id 映射
fn resolve_session_id(host: &WasmHost, claude_session_id: &str) -> String {
    find_task_by_claude_sid(host, claude_session_id)
        .and_then(|row| row.get("session_id").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            host.log_debug(&format!("resolve_session_id: no mapping found, using claude_session_id as-is: {}", claude_session_id));
            claude_session_id.to_string()
        })
}

/// 获取任务状态（供插件内部 command 使用）
pub fn get_task_status(host: &WasmHost, session_id: &str) -> anyhow::Result<Value> {
    let task = find_task_by_session(host, session_id);
    Ok(serde_json::json!({
        "session_id": session_id,
        "task_status": task.and_then(|row| row.get("status").cloned()),
    }))
}

/// 查询任务历史记录（供插件内部 command 使用）
///
/// 可选 session_id 过滤，无则返回所有会话记录
pub fn list_task_history(host: &WasmHost, session_id: &str) -> anyhow::Result<Value> {
    let sql = if session_id.is_empty() {
        "SELECT id, name, status, session_id, auto_approve, exit_reason, created_at, started_at, completed_at FROM task_history ORDER BY created_at DESC LIMIT 100".to_string()
    } else {
        format!(
            "SELECT id, name, status, session_id, auto_approve, exit_reason, created_at, started_at, completed_at FROM task_history WHERE session_id = '{}' ORDER BY created_at DESC LIMIT 100",
            session_id.replace('\'', "''")
        )
    };
    let rows = host.plugin_db_query(&sql)
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    Ok(serde_json::json!({ "tasks": rows }))
}

/// 设置自动授权模式（供插件内部 command 使用）
pub fn set_auto_mode(host: &WasmHost, session_id: &str, auto_approve: bool) -> anyhow::Result<Value> {
    host.log_debug(&format!("set_auto_mode: session_id={}, auto_approve={}", session_id, auto_approve));

    let sql = format!(
        "UPDATE task_history SET auto_approve = {}, updated_at = datetime('now') WHERE session_id = '{}' ORDER BY created_at DESC LIMIT 1",
        if auto_approve { 1 } else { 0 },
        session_id.replace('\'', "''")
    );
    host.plugin_db_execute(&sql);

    // 通知前端模式变更
    host.emit_event("session:modeChanged", &serde_json::json!({
        "session_id": session_id,
        "autoApprove": auto_approve,
    }));
    host.log_debug(&format!("emit_event: session:modeChanged for session_id={}", session_id));

    // 广播到移动端
    host.broadcast_sync(&serde_json::json!({
        "type": "SessionModeChanged",
        "session_id": session_id,
        "auto_approve": auto_approve,
    }));
    host.log_debug(&format!("broadcast_sync: SessionModeChanged for session_id={}", session_id));

    // 通过消息总线通知其他插件会话模式变更
    host.bus_publish("session:mode-changed", &serde_json::json!({
        "session_id": session_id,
        "auto_approve": auto_approve,
    }));

    Ok(serde_json::json!({ "success": true }))
}

// ==================== HTTP 响应构造 ====================

/// 构造成功响应（插件 HTTP 端点格式：{ status, body }）
fn ok_response() -> Value {
    serde_json::json!({
        "status": 200,
        "body": { "code": 0, "message": "ok" }
    })
}

/// 构造成功响应（带 data）
fn ok_response_with_data(data: Value) -> Value {
    serde_json::json!({
        "status": 200,
        "body": { "code": 0, "message": "ok", "data": data }
    })
}

/// 构造错误响应
fn error_response(status: u16, message: &str) -> Value {
    serde_json::json!({
        "status": status,
        "body": { "code": status as i32, "message": message }
    })
}
