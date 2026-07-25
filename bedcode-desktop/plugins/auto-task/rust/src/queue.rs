//! 任务队列管理
//!
//! 每个会话维护独立的待执行任务队列，支持添加、删除、查询、清空操作。
//! 当当前任务完成时，自动从队列取出下一个任务发送给 Claude Code 执行。
//! 队列非空时自动切换到自动授权模式，清空后退出自动模式。

use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::Value;

/// 任务队列表建表 SQL
pub const TASK_QUEUE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS task_queue (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    prompt      TEXT NOT NULL,
    position    INTEGER NOT NULL,
    status      TEXT NOT NULL DEFAULT 'pending',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_task_queue_session ON task_queue(session_id, status, position);
"#;

// ==================== Queue Operations ====================

/// 添加任务到队列末尾
///
/// 返回 (task_id, position)
pub fn add_task(host: &WasmHost, session_id: &str, prompt: &str) -> (String, i64) {
    // 查询当前最大 position
    let max_pos = get_max_position(host, session_id);
    let position = max_pos + 1;

    // 生成 ID 并插入
    let id_sql = "SELECT lower(hex(randomblob(16)))";
    let id = host
        .plugin_db_query(id_sql)
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|row| row.as_array().and_then(|a| a.first().cloned()))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| {
            // fallback: 用时间戳生成
            format!("{:x}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis())
        });

    let sql = format!(
        "INSERT INTO task_queue (id, session_id, prompt, position, status, created_at, updated_at) \
         VALUES ('{}', '{}', '{}', {}, 'pending', datetime('now'), datetime('now'))",
        id.replace('\'', "''"),
        session_id.replace('\'', "''"),
        prompt.replace('\'', "''"),
        position,
    );
    host.plugin_db_execute(&sql);

    host.log_info(&format!("Task queued: id={} session_id={} position={}", id, session_id, position));

    (id, position)
}

/// 从队列删除指定任务，并重排剩余任务的 position
pub fn remove_task(host: &WasmHost, session_id: &str, task_id: &str) -> bool {
    // 先删除
    let sql = format!(
        "DELETE FROM task_queue WHERE id = '{}' AND session_id = '{}'",
        task_id.replace('\'', "''"),
        session_id.replace('\'', "''"),
    );
    let affected = host.plugin_db_execute(&sql);
    if affected == 0 {
        return false;
    }

    // 重排 position：按创建时间重新编号
    reorder_positions(host, session_id);

    host.log_info(&format!("Task removed: id={} session_id={}", task_id, session_id));
    true
}

/// 查询指定会话的待执行队列
pub fn list_queue(host: &WasmHost, session_id: &str) -> Vec<Value> {
    let sql = format!(
        "SELECT id, prompt, position, status, created_at FROM task_queue \
         WHERE session_id = '{}' AND status = 'pending' \
         ORDER BY position ASC",
        session_id.replace('\'', "''"),
    );
    host.plugin_db_query(&sql)
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

/// 清空指定会话的所有 pending 任务
pub fn clear_queue(host: &WasmHost, session_id: &str) -> i32 {
    let sql = format!(
        "DELETE FROM task_queue WHERE session_id = '{}' AND status = 'pending'",
        session_id.replace('\'', "''"),
    );
    host.plugin_db_execute(&sql)
}

/// 统计指定会话的 pending 任务数量
pub fn pending_count(host: &WasmHost, session_id: &str) -> i64 {
    let sql = format!(
        "SELECT COUNT(*) as cnt FROM task_queue WHERE session_id = '{}' AND status = 'pending'",
        session_id.replace('\'', "''"),
    );
    host.plugin_db_query(&sql)
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|row| row.get("cnt").cloned())
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
}

// ==================== Dispatch Logic ====================

/// 尝试调度下一个任务
///
/// 当 task-status 变为终态时调用，检查队列并自动执行下一个任务。
/// 执行前先发送 /clear 清屏，再发送 prompt。
/// 队列非空时自动切换到自动授权模式，清空后退出。
pub fn try_dispatch_next(host: &WasmHost, session_id: &str) {
    host.log_debug(&format!("try_dispatch_next: session_id={}", session_id));

    // 检查队列是否有 pending 任务
    let queue = list_queue(host, session_id);
    if queue.is_empty() {
        host.log_debug(&format!("try_dispatch_next: no pending tasks for session_id={}", session_id));

        // 队列空，退出自动模式
        ensure_auto_mode_off(host, session_id);
        return;
    }

    // 取队首任务
    let first = match queue.first() {
        Some(t) => t,
        None => return,
    };
    let task_id = first.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let prompt = first.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

    if prompt.is_empty() {
        host.log_warn(&format!("try_dispatch_next: empty prompt for task_id={}", task_id));
        return;
    }

    // 确认会话仍在运行
    let session = host.session_get(session_id);
    if session.is_none() {
        host.log_warn(&format!("try_dispatch_next: session {} not found or not running", session_id));
        return;
    }

    // 更新队列项状态为 executing
    let sql = format!(
        "UPDATE task_queue SET status = 'executing', updated_at = datetime('now') WHERE id = '{}'",
        task_id.replace('\'', "''"),
    );
    host.plugin_db_execute(&sql);

    // 合并 /clear 和 prompt 为一次发送，避免 /clear 未处理完 prompt 就到达
    let combined = format!("/clear\n\n{}", prompt);
    host.terminal_send(session_id, &combined);
    host.log_info(&format!(
        "try_dispatch_next: dispatched task_id={} prompt_len={} session_id={}",
        task_id, prompt.len(), session_id
    ));

    // 更新 task_history 状态为 in_progress（子查询定位最新记录，SQLite 不支持 UPDATE ... ORDER BY）
    let sql = format!(
        "UPDATE task_history SET status = 'in_progress', updated_at = datetime('now') \
         WHERE id = (SELECT id FROM task_history WHERE session_id = '{}' ORDER BY created_at DESC LIMIT 1)",
        session_id.replace('\'', "''"),
    );
    host.plugin_db_execute(&sql);

    // 标记队列项为 done
    let sql = format!(
        "UPDATE task_queue SET status = 'done', updated_at = datetime('now') WHERE id = '{}'",
        task_id.replace('\'', "''"),
    );
    host.plugin_db_execute(&sql);

    // 计算剩余数量（刚出队一个，所以是队列长度 - 1）
    let remaining = (queue.len() as i64).saturating_sub(1);
    broadcast_queue_changed(host, session_id, remaining, "dequeue");

    // 如果还有剩余任务，确保自动模式开启
    if remaining > 0 {
        ensure_auto_mode_on(host, session_id);
    } else {
        ensure_auto_mode_off(host, session_id);
    }
}

// ==================== HTTP Endpoint Handler ====================

/// 处理队列相关的 HTTP 端点请求
///
/// 路由：
/// - POST task-queue/add → 添加任务
/// - DELETE task-queue/remove → 删除任务
/// - GET task-queue/list → 查询队列
/// - POST task-queue/clear → 清空队列
pub fn handle_queue_http(host: &WasmHost, method: &str, path: &str, body: &Value, query: &Value) -> Value {
    host.log_debug(&format!("handle_queue_http: {} {}", method, path));

    match (method, path) {
        ("POST", "add") => handle_add(host, body, query),
        ("DELETE", "remove") => handle_remove(host, body, query),
        ("GET", "list") => handle_list(host, query),
        ("POST", "clear") => handle_clear(host, body, query),
        _ => {
            host.log_warn(&format!("Unknown queue endpoint: {} {}", method, path));
            error_response(404, &format!("Not found: {} {}", method, path))
        }
    }
}

// ==================== HTTP Handler Implementations ====================

/// POST task-queue/add
fn handle_add(host: &WasmHost, body: &Value, _query: &Value) -> Value {
    let session_id = body.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    let prompt = body.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

    if session_id.is_empty() {
        return error_response(400, "Missing session_id");
    }
    if prompt.is_empty() {
        return error_response(400, "Missing prompt");
    }

    // 如果是第一个 pending 任务，自动开启自动模式
    let count_before = pending_count(host, session_id);

    let (task_id, position) = add_task(host, session_id, prompt);

    // 队列从空变为非空，自动开启自动模式
    if count_before == 0 {
        ensure_auto_mode_on(host, session_id);
    }

    // 广播队列变更
    let count_after = pending_count(host, session_id);
    broadcast_queue_changed(host, session_id, count_after, "add");

    ok_response_with_data(serde_json::json!({
        "task_id": task_id,
        "position": position,
    }))
}

/// DELETE task-queue/remove
fn handle_remove(host: &WasmHost, body: &Value, _query: &Value) -> Value {
    let session_id = body.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    let task_id = body.get("task_id").and_then(|v| v.as_str()).unwrap_or("");

    if session_id.is_empty() {
        return error_response(400, "Missing session_id");
    }
    if task_id.is_empty() {
        return error_response(400, "Missing task_id");
    }

    let removed = remove_task(host, session_id, task_id);
    if !removed {
        return error_response(404, "Task not found");
    }

    // 删除后无 pending 任务，退出自动模式
    let remaining = pending_count(host, session_id);
    if remaining == 0 {
        ensure_auto_mode_off(host, session_id);
    }

    broadcast_queue_changed(host, session_id, remaining, "remove");

    ok_response()
}

/// GET task-queue/list
fn handle_list(host: &WasmHost, query: &Value) -> Value {
    let session_id = query.get("session_id").and_then(|v| v.as_str()).unwrap_or("");

    if session_id.is_empty() {
        return error_response(400, "Missing session_id");
    }

    let tasks = list_queue(host, session_id);
    let queue_count = tasks.len() as i64;

    ok_response_with_data(serde_json::json!({
        "session_id": session_id,
        "tasks": tasks,
        "queue_count": queue_count,
    }))
}

/// POST task-queue/clear
fn handle_clear(host: &WasmHost, body: &Value, _query: &Value) -> Value {
    let session_id = body.get("session_id").and_then(|v| v.as_str()).unwrap_or("");

    if session_id.is_empty() {
        return error_response(400, "Missing session_id");
    }

    clear_queue(host, session_id);

    // 清空后退出自动模式
    ensure_auto_mode_off(host, session_id);

    broadcast_queue_changed(host, session_id, 0, "clear");

    ok_response()
}

// ==================== Internal Helpers ====================

/// 查询指定会话的最大 position
fn get_max_position(host: &WasmHost, session_id: &str) -> i64 {
    let sql = format!(
        "SELECT MAX(position) as max_pos FROM task_queue WHERE session_id = '{}' AND status = 'pending'",
        session_id.replace('\'', "''"),
    );
    host.plugin_db_query(&sql)
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|row| row.get("max_pos").cloned())
        .and_then(|v| v.as_i64())
        .unwrap_or(-1)
}

/// 重排指定会话的 pending 任务 position（填补删除后的空缺）
fn reorder_positions(host: &WasmHost, session_id: &str) {
    let sql = format!(
        "SELECT id FROM task_queue WHERE session_id = '{}' AND status = 'pending' ORDER BY position ASC, created_at ASC",
        session_id.replace('\'', "''"),
    );
    let tasks = host.plugin_db_query(&sql)
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    for (idx, task) in tasks.iter().enumerate() {
        let id = task.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let update_sql = format!(
            "UPDATE task_queue SET position = {}, updated_at = datetime('now') WHERE id = '{}'",
            idx,
            id.replace('\'', "''"),
        );
        host.plugin_db_execute(&update_sql);
    }
}

/// 确保自动模式开启
fn ensure_auto_mode_on(host: &WasmHost, session_id: &str) {
    // 先尝试更新已有记录
    // 子查询定位最新记录，SQLite 不支持 UPDATE ... ORDER BY
    let sql = format!(
        "UPDATE task_history SET auto_approve = 1, updated_at = datetime('now') \
         WHERE id = (SELECT id FROM task_history WHERE session_id = '{}' ORDER BY created_at DESC LIMIT 1)",
        session_id.replace('\'', "''"),
    );
    let affected = host.plugin_db_execute(&sql);

    // 如果该 session 还没有 task_history 记录，插入一条语义合理的记录
    // 队列调度时任务确实在执行中，所以 status='in_progress' 而非 idle
    if affected == 0 {
        let insert_sql = format!(
            "INSERT INTO task_history (id, name, status, session_id, auto_approve, started_at, created_at, updated_at) \
             VALUES (lower(hex(randomblob(16))), 'Auto Task', 'in_progress', '{}', 1, datetime('now'), datetime('now'), datetime('now'))",
            session_id.replace('\'', "''"),
        );
        host.plugin_db_execute(&insert_sql);
    }

    host.broadcast_sync(&serde_json::json!({
        "type": "SessionModeChanged",
        "session_id": session_id,
        "auto_approve": true,
    }));

    host.bus_publish("session:mode-changed", &serde_json::json!({
        "session_id": session_id,
        "auto_approve": true,
    }));

    host.log_debug(&format!("Auto mode ON for session_id={}", session_id));
}

/// 确保自动模式关闭
fn ensure_auto_mode_off(host: &WasmHost, session_id: &str) {
    // 子查询定位最新记录，SQLite 不支持 UPDATE ... ORDER BY
    let sql = format!(
        "UPDATE task_history SET auto_approve = 0, updated_at = datetime('now') \
         WHERE id = (SELECT id FROM task_history WHERE session_id = '{}' ORDER BY created_at DESC LIMIT 1)",
        session_id.replace('\'', "''"),
    );
    host.plugin_db_execute(&sql);

    host.broadcast_sync(&serde_json::json!({
        "type": "SessionModeChanged",
        "session_id": session_id,
        "auto_approve": false,
    }));

    host.bus_publish("session:mode-changed", &serde_json::json!({
        "session_id": session_id,
        "auto_approve": false,
    }));

    host.log_debug(&format!("Auto mode OFF for session_id={}", session_id));
}

/// 广播队列变更事件
fn broadcast_queue_changed(host: &WasmHost, session_id: &str, queue_count: i64, action: &str) {
    host.broadcast_sync(&serde_json::json!({
        "type": "TaskQueueChanged",
        "session_id": session_id,
        "queue_count": queue_count,
        "action": action,
    }));

    host.bus_publish("task:queue-changed", &serde_json::json!({
        "session_id": session_id,
        "queue_count": queue_count,
        "action": action,
    }));
}

// ==================== HTTP Response Helpers ====================

fn ok_response() -> Value {
    serde_json::json!({
        "status": 200,
        "body": { "code": 0, "message": "ok" }
    })
}

fn ok_response_with_data(data: Value) -> Value {
    serde_json::json!({
        "status": 200,
        "body": { "code": 0, "message": "ok", "data": data }
    })
}

fn error_response(status: u16, message: &str) -> Value {
    serde_json::json!({
        "status": status,
        "body": { "code": status as i32, "message": message }
    })
}
