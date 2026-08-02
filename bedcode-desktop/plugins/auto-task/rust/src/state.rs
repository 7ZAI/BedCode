//! 任务状态与自动授权模式管理
//!
//! 通过插件独立数据库持久化任务历史，通过 broadcast_sync 广播变更到移动端。
//! HTTP 端点处理逻辑在此实现，由 lib.rs 的 _http_endpoint command 路由调用。
//!
//! SQL 一律使用参数绑定（`*_params` + `?N` 占位符），无手写转义。

use bedcode_plugin_api::events::{PluginQuestion, SyncEvent};
use bedcode_plugin_api::host::{HostBus, HostEvents, HostLog, HostPluginDatabase, HostSession};
use bedcode_plugin_api::http_response;
use bedcode_plugin_api::sql_params;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::Value;

// ==================== 查询辅助函数 ====================

/// 查询任务历史行 — 按 session_id 查找最新一条
fn find_task_by_session(host: &WasmHost, session_id: &str) -> Option<Value> {
    let result = host
        .plugin_db_query_params(
            "SELECT * FROM task_history WHERE session_id = ?1 ORDER BY created_at DESC LIMIT 1",
            &sql_params![session_id],
        )
        .ok()
        .flatten()?;
    result.as_array()?.first().cloned()
}

/// 查询任务历史行 — 按 claude_sid 查找最新一条
fn find_task_by_claude_sid(host: &WasmHost, claude_sid: &str) -> Option<Value> {
    let result = host
        .plugin_db_query_params(
            "SELECT * FROM task_history WHERE claude_sid = ?1 ORDER BY created_at DESC LIMIT 1",
            &sql_params![claude_sid],
        )
        .ok()
        .flatten()?;
    result.as_array()?.first().cloned()
}

/// 查询 session 映射 — 按 claude_sid 查找 bedcode_session_id
fn find_mapping_by_claude_sid(host: &WasmHost, claude_sid: &str) -> Option<String> {
    let result = host
        .plugin_db_query_params(
            "SELECT session_id FROM session_mapping WHERE claude_sid = ?1",
            &sql_params![claude_sid],
        )
        .ok()
        .flatten()?;
    result.as_array()?.first()?.get("session_id")?.as_str().map(|s| s.to_string())
}

/// 存储 claude_sid ↔ bedcode_session_id 映射
///
/// 使用 INSERT OR REPLACE 确保映射始终是最新的
fn upsert_session_mapping(host: &WasmHost, claude_sid: &str, session_id: &str) {
    let _ = host.plugin_db_execute_params(
        "INSERT OR REPLACE INTO session_mapping (claude_sid, session_id, created_at) \
         VALUES (?1, ?2, datetime('now'))",
        &sql_params![claude_sid, session_id],
    );
}

/// 查询 session 映射 — 按 bedcode_session_id 查找 claude_sid（反向查询）
fn find_claude_sid_by_session(host: &WasmHost, bedcode_sid: &str) -> Option<String> {
    let result = host
        .plugin_db_query_params(
            "SELECT claude_sid FROM session_mapping WHERE session_id = ?1",
            &sql_params![bedcode_sid],
        )
        .ok()
        .flatten()?;
    result.as_array()?.first()?.get("claude_sid")?.as_str().map(|s| s.to_string())
}

// ==================== 会话输入 → 任务创建 ====================

/// 判断会话启动命令是否为 Claude Code
///
/// 两步查询：session_get 取 SessionInfo.config_id（camelCase 序列化），
/// 再在 session_config_list 中匹配对应配置的 command。
/// 命令如 `claude` / `claude.exe` 或完整路径均视为 Claude 会话。
pub fn session_command_is_claude(host: &WasmHost, session_id: &str) -> bool {
    // 1. session_get 获取 config_id（SessionInfo 序列化为 camelCase）
    let config_id = match host.session_get(session_id) {
        Ok(Some(info)) => info
            .get("configId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        _ => None,
    };
    let config_id = match config_id {
        Some(id) => id,
        None => return false,
    };

    // 2. session_config_list 查找对应配置的启动命令
    let command = match host.session_config_list() {
        Ok(Some(configs)) => configs
            .as_array()
            .and_then(|arr| {
                arr.iter().find(|c| {
                    c.get("id").and_then(|v| v.as_str()) == Some(config_id.as_str())
                })
            })
            .and_then(|c| c.get("command").and_then(|v| v.as_str()).map(|s| s.to_string())),
        _ => None,
    };

    command
        .map(|c| c.to_lowercase().contains("claude"))
        .unwrap_or(false)
}

/// 判断会话当前是否有进行中的任务
///
/// 以 task_history 最新一条记录为准：状态为终态（completed/interrupted）、
/// idle 或无记录时视为无当前任务，可以创建新任务。
pub fn has_active_task(host: &WasmHost, session_id: &str) -> bool {
    find_task_by_session(host, session_id)
        .and_then(|row| row.get("status").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .map(|status| !matches!(status.as_str(), "completed" | "interrupted" | "idle"))
        .unwrap_or(false)
}

/// 从提交的输入行创建任务记录（宿主 on_input_submitted 会话扩展调用）
///
/// 仅 Claude 会话且无当前任务时调用。输入作为任务内容写入 description 字段，
/// 状态置为 in_progress（输入已提交执行）。写表职责从 Claude Code 输入 hook
/// 移交到宿主侧，避免 hook 与宿主双重写表。
///
/// 同时反向查 session_mapping 写入 claude_sid：这样任务行同时带 claude_sid
/// 和 bedcode session_id 双键，后续 hook 的状态推送（只带 claude_sid 或
/// 只带 bedcode_session_id）都能命中该行。
pub fn create_task_from_input(host: &WasmHost, session_id: &str, input: &str) {
    let claude_sid = find_claude_sid_by_session(host, session_id);

    let sql = "INSERT INTO task_history (id, description, status, session_id, claude_sid, started_at, created_at, updated_at) \
               VALUES (lower(hex(randomblob(16))), ?1, 'in_progress', ?2, ?3, datetime('now'), datetime('now'), datetime('now'))";
    // claude_sid 映射缺失时绑定 NULL（SessionStart 之前就提交输入等边缘场景）
    let claude_sid_param = claude_sid
        .as_ref()
        .map(|s| serde_json::Value::String(s.clone()))
        .unwrap_or(serde_json::Value::Null);
    match host.plugin_db_execute_params(sql, &sql_params![input, session_id, claude_sid_param]) {
        Ok(affected) => host.log_info(&format!(
            "Task created from input: session_id={} claude_sid={:?} len={} affected={}",
            session_id, claude_sid, input.len(), affected
        )),
        Err(e) => host.log_error(&format!(
            "Failed to create task from input: session_id={} err={}",
            session_id, e
        )),
    }

    // 广播状态变更到移动端 + 消息总线通知其他插件
    host.broadcast_sync(&SyncEvent::TaskStatusChanged {
        session_id: session_id.to_string(),
        task_status: "in_progress".to_string(),
        task_reason: Some("User submitted input".to_string()),
        task_questions: None,
    });
    let _ = host.bus_publish("task:status-changed", &serde_json::json!({
        "session_id": session_id,
        "task_status": "in_progress",
    }));
}

// ==================== HTTP 端点处理 ====================

/// 处理 HTTP 端点请求
///
/// 路由：
/// - POST /task-status → update_task_status
/// - GET /task-status → get_task_status
/// - POST /session-mode → set_session_mode
/// - GET /session-mode → get_session_mode
pub fn handle_http_endpoint(host: &WasmHost, method: &str, path: &str, body: &Value, query: &Value) -> Value {
    host.log_debug(&format!("handle_http_endpoint: {} {}", method, path));

    match (method, path) {
        ("POST", "task-status") => handle_update_task_status(host, body),
        ("GET", "task-status") => handle_get_task_status(host, query),
        ("POST", "session-mode") => handle_set_session_mode(host, body, query),
        ("GET", "session-mode") => handle_get_session_mode(host, query),
        _ => {
            host.log_warn(&format!("Unknown HTTP endpoint: {} {}", method, path));
            http_response::error(404, &format!("Not found: {} {}", method, path))
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
        return http_response::error(400, "Missing session_id");
    }

    // 验证 status 值
    let valid_statuses = ["idle", "in_progress", "asking", "completed", "interrupted"];
    if !valid_statuses.contains(&status) {
        host.log_warn(&format!("task-status invalid status: '{}' for session_id={}", status, session_id));
        return http_response::error(400, &format!("Invalid task status: {}. Must be one of: {}", status, valid_statuses.join(", ")));
    }

    // 解析 bedcode_session_id：显式携带优先；否则经 session_mapping 表把 claude_sid 解析成 bedcode sid
    // （宿主 on_input_submitted 创建的行以 bedcode sid 作为 session_id 键控，
    //   仅带 claude_sid 的推送必须经映射解析才能命中）
    let resolved_session_id = bedcode_session_id
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| resolve_session_id(host, session_id));

    // 查找已有任务记录：优先按 claude_sid（宿主建行时已反向写入），
    // 兜底按解析后的 bedcode sid（兼容建行时映射缺失的旧数据）
    let existing = find_task_by_claude_sid(host, session_id)
        .or_else(|| find_task_by_session(host, &resolved_session_id));

    if let Some(row) = existing {
        // 终态保护：completed / interrupted 不应被后续事件降级
        // 防止 Stop(completed) 后 SessionEnd(interrupted) 覆盖正常完成状态
        let current_status = row.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let is_current_terminal = matches!(current_status, "completed" | "interrupted");
        let is_new_terminal = matches!(status, "completed" | "interrupted");
        if is_current_terminal && !is_new_terminal {
            host.log_info(&format!(
                "task-status: session_id={} skip, current '{}' is terminal, new '{}' is not",
                session_id, current_status, status
            ));
            return http_response::ok();
        }

        // 更新已有记录：动态子句与绑定参数同步组装（占位符 ?N 按序编号）
        let task_id = row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let mut clauses: Vec<String> = Vec::new();
        let mut params: Vec<Value> = Vec::new();

        // 追加一个绑定参数，返回对应占位符（?1、?2 …）
        let push_param = |params: &mut Vec<Value>, v: Value| -> String {
            params.push(v);
            format!("?{}", params.len())
        };

        clauses.push(format!("status = {}", push_param(&mut params, Value::String(status.to_string()))));
        if let Some(r) = reason {
            clauses.push(format!("exit_reason = {}", push_param(&mut params, Value::String(r.to_string()))));
        }
        if let Some(q) = questions {
            clauses.push(format!("questions = {}", push_param(&mut params, q.clone())));
        }

        // 更新 session_id 映射
        if let Some(bedcode_sid) = bedcode_session_id.filter(|s| !s.is_empty()) {
            clauses.push(format!("session_id = {}", push_param(&mut params, Value::String(bedcode_sid.to_string()))));
            clauses.push(format!("claude_sid = {}", push_param(&mut params, Value::String(session_id.to_string()))));
        }

        // 状态转换时更新时间戳（SQL 函数，无绑定参数）
        match status {
            "in_progress" => clauses.push("started_at = datetime('now')".to_string()),
            "completed" | "interrupted" | "failed" => clauses.push("completed_at = datetime('now')".to_string()),
            _ => {}
        }
        clauses.push("updated_at = datetime('now')".to_string());

        // WHERE id 占位符
        params.push(Value::String(task_id));
        let sql = format!(
            "UPDATE task_history SET {} WHERE id = ?{}",
            clauses.join(", "),
            params.len()
        );

        match host.plugin_db_execute_params(&sql, &params) {
            Ok(affected) => host.log_debug(&format!("UPDATE task_history: affected={}", affected)),
            Err(e) => host.log_error(&format!("UPDATE task_history failed: {}", e)),
        }
    } else if status == "idle" {
        // idle 状态不创建任务记录，只存储 session 映射
        // SessionStart 时还没有任务，映射关系在 session_mapping 表中维护
        if let Some(bedcode_sid) = bedcode_session_id.filter(|s| !s.is_empty()) {
            upsert_session_mapping(host, session_id, bedcode_sid);
            host.log_info(&format!("Session mapping stored: claude_sid={} → bedcode_sid={}", session_id, bedcode_sid));
        }
        // idle 状态无需广播任务变更，直接返回
        return http_response::ok();
    } else {
        // 无已有记录：任务行创建已移交宿主 on_input_submitted 会话扩展，
        // Claude Code 输入 hook 不再负责写表，此处仅记录日志避免静默忽略
        host.log_debug(&format!(
            "task-status: session_id={} has no task row, skip write (host session extension owns creation)",
            session_id
        ));
    }

    // 广播状态变更到移动端（类型化 SyncEvent，serde 表示即线协议）
    host.broadcast_sync(&SyncEvent::TaskStatusChanged {
        session_id: resolved_session_id.to_string(),
        task_status: status.to_string(),
        task_reason: reason.map(|s| s.to_string()),
        // hook 脚本推送的 questions 载荷反序列化为类型化 PluginQuestion
        task_questions: questions
            .and_then(|q| serde_json::from_value::<Vec<PluginQuestion>>(q.clone()).ok()),
    });

    // 通过消息总线通知其他插件任务状态变更
    let _ = host.bus_publish("task:status-changed", &serde_json::json!({
        "session_id": resolved_session_id,
        "task_status": status,
    }));

    host.log_info(&format!("Task status updated: claude_sid={} bedcode_sid={} status={}", session_id, resolved_session_id, status));

    // 任务终态时检查队列，尝试调度下一个任务
    // idle 不触发：仅表示"无任务运行"，SessionStart 时推送 idle，此时不应出队
    if matches!(status, "completed" | "interrupted") {
        crate::queue::try_dispatch_next(host, &resolved_session_id);
    }

    http_response::ok()
}

/// POST /session-mode — 设置会话自动授权模式
fn handle_set_session_mode(host: &WasmHost, body: &Value, _query: &Value) -> Value {
    host.log_debug(&format!("session-mode POST body: {}", body));

    let session_id = body.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    let auto_approve = body.get("auto_approve").and_then(|v| v.as_bool()).unwrap_or(false);

    if session_id.is_empty() {
        host.log_warn("session-mode POST rejected: empty session_id");
        return http_response::error(400, "Missing session_id");
    }

    // 认证由网关中间件统一处理（JWT 或本地放行），此处不重复校验

    // 更新任务历史表中的 auto_approve 字段（子查询定位最新记录，SQLite 不支持 UPDATE ... ORDER BY）
    let sql = "UPDATE task_history SET auto_approve = ?1, updated_at = datetime('now') \
        WHERE id = (SELECT id FROM task_history WHERE session_id = ?2 ORDER BY created_at DESC LIMIT 1)";
    let _ = host.plugin_db_execute_params(sql, &sql_params![auto_approve, session_id]);

    // 广播模式变更到移动端
    host.broadcast_sync(&SyncEvent::SessionModeChanged {
        session_id: session_id.to_string(),
        auto_approve,
    });
    host.log_debug(&format!("broadcast_sync: SessionModeChanged for session_id={}", session_id));

    // 通过消息总线通知其他插件会话模式变更
    let _ = host.bus_publish("session:mode-changed", &serde_json::json!({
        "session_id": session_id,
        "auto_approve": auto_approve,
    }));

    host.log_info(&format!("Session mode set: session_id={}, auto_approve={}", session_id, auto_approve));
    http_response::ok()
}

/// GET /task-status — 查询当前任务状态
///
/// 供终止 hook（Stop/SubagentStop/SessionEnd）查询当前状态，避免盲目覆盖终态
fn handle_get_task_status(host: &WasmHost, query: &Value) -> Value {
    let session_id = query.get("session_id").and_then(|v| v.as_str()).unwrap_or("");

    if session_id.is_empty() {
        host.log_warn("task-status GET rejected: empty session_id");
        return http_response::error(400, "Missing session_id");
    }

    let resolved_id = resolve_session_id(host, session_id);
    let task_status = find_task_by_session(host, &resolved_id)
        .and_then(|row| row.get("status").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| "idle".to_string());

    http_response::ok_with_data(serde_json::json!({
        "session_id": session_id,
        "task_status": task_status,
    }))
}

/// GET /session-mode — 查询会话自动授权模式
fn handle_get_session_mode(host: &WasmHost, query: &Value) -> Value {
    host.log_debug(&format!("session-mode GET query: {}", query));

    let session_id = query.get("session_id").and_then(|v| v.as_str()).unwrap_or("");

    if session_id.is_empty() {
        host.log_warn("session-mode GET rejected: empty session_id");
        return http_response::error(400, "Missing session_id");
    }

    // 认证由网关中间件统一处理（JWT 或本地放行），此处不重复校验

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

    http_response::ok_with_data(serde_json::json!({
        "session_id": session_id,
        "auto_approve": auto_approve,
    }))
}

// ==================== 辅助函数 ====================

/// 解析 Claude Code session_id → BedCode PTY session_id
///
/// 优先从 session_mapping 表查找映射，fallback 到 task_history 表
fn resolve_session_id(host: &WasmHost, claude_session_id: &str) -> String {
    // 优先查 session_mapping 表
    if let Some(mapped) = find_mapping_by_claude_sid(host, claude_session_id) {
        host.log_debug(&format!("resolve_session_id: found in session_mapping: {} → {}", claude_session_id, mapped));
        return mapped;
    }

    // fallback: 从 task_history 查找
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
    let (sql, params): (&str, Vec<Value>) = if session_id.is_empty() {
        (
            "SELECT id, description, status, session_id, auto_approve, exit_reason, created_at, started_at, completed_at FROM task_history ORDER BY created_at DESC LIMIT 100",
            vec![],
        )
    } else {
        (
            "SELECT id, description, status, session_id, auto_approve, exit_reason, created_at, started_at, completed_at FROM task_history WHERE session_id = ?1 ORDER BY created_at DESC LIMIT 100",
            sql_params![session_id],
        )
    };
    let rows = host.plugin_db_query_params(sql, &params)
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    Ok(serde_json::json!({ "tasks": rows }))
}

/// 设置自动授权模式（供插件内部 command 使用）
pub fn set_auto_mode(host: &WasmHost, session_id: &str, auto_approve: bool) -> anyhow::Result<Value> {
    host.log_debug(&format!("set_auto_mode: session_id={}, auto_approve={}", session_id, auto_approve));

    // 子查询定位最新记录，SQLite 不支持 UPDATE ... ORDER BY
    let sql = "UPDATE task_history SET auto_approve = ?1, updated_at = datetime('now') \
        WHERE id = (SELECT id FROM task_history WHERE session_id = ?2 ORDER BY created_at DESC LIMIT 1)";
    let _ = host.plugin_db_execute_params(sql, &sql_params![auto_approve, session_id]);

    // 通知前端模式变更
    host.emit_event("session:modeChanged", &serde_json::json!({
        "session_id": session_id,
        "autoApprove": auto_approve,
    }));
    host.log_debug(&format!("emit_event: session:modeChanged for session_id={}", session_id));

    // 广播到移动端
    host.broadcast_sync(&SyncEvent::SessionModeChanged {
        session_id: session_id.to_string(),
        auto_approve,
    });
    host.log_debug(&format!("broadcast_sync: SessionModeChanged for session_id={}", session_id));

    // 通过消息总线通知其他插件会话模式变更
    let _ = host.bus_publish("session:mode-changed", &serde_json::json!({
        "session_id": session_id,
        "auto_approve": auto_approve,
    }));

    Ok(serde_json::json!({ "success": true }))
}
