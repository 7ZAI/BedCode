//! 任务状态与自动授权模式管理
//!
//! 通过插件独立数据库持久化任务历史，通过 broadcast_sync 广播变更到移动端。
//! HTTP 端点处理逻辑在此实现，由 lib.rs 的 _http_endpoint command 路由调用。
//!
//! SQL 一律使用参数绑定（`*_params` + `?N` 占位符），无手写转义。

use bedcode_plugin_api::constants::{EVENT_SESSION_MODE_CHANGED, EVENT_TASK_STATUS_CHANGED};
use bedcode_plugin_api::events::{PluginQuestion, SyncEvent};
use bedcode_plugin_api::host::{HostBus, HostEvents, HostLog, HostPluginDatabase, HostSession};
use bedcode_plugin_api::http_response;
use bedcode_plugin_api::sql_params;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::Value;
use std::collections::HashMap;

use crate::agent;

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
    result
        .as_array()?
        .first()?
        .get("session_id")?
        .as_str()
        .map(|s| s.to_string())
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
    result
        .as_array()?
        .first()?
        .get("claude_sid")?
        .as_str()
        .map(|s| s.to_string())
}

// ==================== 会话输入 → 任务创建 ====================

/// 查询会话启动命令（config_id → session_config_list 匹配 command）
fn session_command(host: &WasmHost, session_id: &str) -> Option<String> {
    // 1. session_get 获取 config_id（SessionInfo 序列化为 camelCase）
    let config_id = host
        .session_get(session_id)
        .ok()
        .flatten()
        .and_then(|info| {
            info.get("configId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })?;

    // 2. session_config_list 查找对应配置的启动命令
    host.session_config_list()
        .ok()
        .flatten()
        .and_then(|configs| {
            configs
                .as_array()
                .and_then(|arr| {
                    arr.iter()
                        .find(|c| c.get("id").and_then(|v| v.as_str()) == Some(config_id.as_str()))
                })
                .and_then(|c| {
                    c.get("command")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
        })
}

/// 检测会话的执行 agent（CLI 级，见 agent::detect_agent）
///
/// 会话不存在或配置无命令时返回 "unknown"
pub fn session_agent(host: &WasmHost, session_id: &str) -> &'static str {
    session_command(host, session_id)
        .map(|cmd| agent::detect_agent(&cmd))
        .unwrap_or("unknown")
}

/// 判断会话当前是否有进行中的任务
///
/// 以 task_history 最新一条记录为准：状态为终态（completed/interrupted）、
/// idle 或无记录时视为无当前任务，可以创建新任务。
pub fn has_active_task(host: &WasmHost, session_id: &str) -> bool {
    find_task_by_session(host, session_id)
        .and_then(|row| {
            row.get("status")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .map(|status| !matches!(status.as_str(), "completed" | "interrupted" | "idle"))
        .unwrap_or(false)
}

/// 判断会话是否存在终态任务记录（completed/interrupted）
///
/// 队列调度的上下文判断依据：无终态记录 = 全新会话，首个任务无需
/// 上下文清理（clear）；有终态记录 = 会话已有上下文，执行前先清理
pub fn has_terminal_task(host: &WasmHost, session_id: &str) -> bool {
    host.plugin_db_query_params(
        "SELECT COUNT(*) AS cnt FROM task_history WHERE session_id = ?1 AND status IN ('completed', 'interrupted')",
        &sql_params![session_id],
    )
    .ok()
    .flatten()
    .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
    .and_then(|row| row.get("cnt").and_then(|v| v.as_i64()))
    .unwrap_or(0)
        > 0
}

/// 写入任务行（队列出队 / 定时触发调度共用）
///
/// 出队时由插件直接创建任务记录（description = 任务发起输入），
/// 不再依赖输入行重建（见 ADR-0004：`/clear` 与 prompt 拆行提交的时序
/// 竞争会导致任务内容错误）。auto_approve 由调度方决定（队列任务恒为 true）。
pub fn create_task_from_dispatch(
    host: &WasmHost,
    session_id: &str,
    prompt: &str,
    agent: &str,
    source: &str,
) {
    insert_task_row(host, session_id, prompt, agent, source);

    // 调度触发的任务同样广播状态变更（移动端/UI 需要感知任务开始）
    host.broadcast_sync(&SyncEvent::TaskStatusChanged {
        session_id: session_id.to_string(),
        task_status: "in_progress".to_string(),
        task_reason: Some(format!("Dispatched from {}", source)),
        task_questions: None,
    });
    let _ = host.bus_publish(
        EVENT_TASK_STATUS_CHANGED,
        &serde_json::json!({
            "session_id": session_id,
            "task_status": "in_progress",
        }),
    );
    host.emit_event(
        EVENT_TASK_STATUS_CHANGED,
        &serde_json::json!({
            "session_id": session_id,
            "taskStatus": "in_progress",
            "taskReason": format!("Dispatched from {}", source),
        }),
    );
}

/// 插入任务行的内部实现
///
/// auto_approve 取会话当前的 auto_answer 开关（自动应答）：开启则新任务行
/// 标记为可自动应答（hook 据此自动回答提问），关闭则标记手动，随会话设置同步。
fn insert_task_row(host: &WasmHost, session_id: &str, input: &str, agent: &str, source: &str) {
    let claude_sid = find_claude_sid_by_session(host, session_id);
    let (_, auto_answer) = session_flags(host, session_id);

    let sql = "INSERT INTO task_history \
               (id, description, status, agent, source, session_id, claude_sid, auto_approve, event_time, started_at, created_at, updated_at) \
               VALUES (lower(hex(randomblob(16))), ?1, 'in_progress', ?2, ?3, ?4, ?5, ?6, strftime('%Y-%m-%d %H:%M:%f', 'now'), datetime('now'), datetime('now'), datetime('now'))";
    // claude_sid 映射缺失时绑定 NULL（SessionStart 之前就提交输入等边缘场景）
    let claude_sid_param = claude_sid
        .as_ref()
        .map(|s| serde_json::Value::String(s.clone()))
        .unwrap_or(serde_json::Value::Null);
    match host.plugin_db_execute_params(
        sql,
        &sql_params![
            input,
            agent,
            source,
            session_id,
            claude_sid_param,
            auto_answer
        ],
    ) {
        Ok(affected) => {
            host.log_info(&format!(
            "Task row inserted: session_id={} agent={} source={} len={} auto_answer={} affected={}",
            session_id, agent, source, input.len(), auto_answer, affected
        ))
        }
        Err(e) => host.log_error(&format!(
            "Failed to insert task row: session_id={} source={} err={}",
            session_id, source, e
        )),
    }
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
    let agent_name = session_agent(host, session_id);
    insert_task_row(host, session_id, input, agent_name, "user");

    // 广播状态变更到移动端 + 消息总线通知其他插件
    host.broadcast_sync(&SyncEvent::TaskStatusChanged {
        session_id: session_id.to_string(),
        task_status: "in_progress".to_string(),
        task_reason: Some("User submitted input".to_string()),
        task_questions: None,
    });
    let _ = host.bus_publish(
        EVENT_TASK_STATUS_CHANGED,
        &serde_json::json!({
            "session_id": session_id,
            "task_status": "in_progress",
        }),
    );
    // 通知前端 UI 实时刷新（事件名与前端 context.events.on 监听一致）
    host.emit_event(
        EVENT_TASK_STATUS_CHANGED,
        &serde_json::json!({
            "session_id": session_id,
            "taskStatus": "in_progress",
            "taskReason": "User submitted input",
        }),
    );
}

// ==================== HTTP 端点处理 ====================

/// 处理 HTTP 端点请求
///
/// 路由：
/// - POST /task-status → update_task_status
/// - GET /task-status → get_task_status
/// - POST /session-mode → set_session_mode
/// - GET /session-mode → get_session_mode
/// - GET /session-settings → get_session_settings (auto_execute + auto_answer)
/// - GET /task-history/current → get current task for session
pub fn handle_http_endpoint(
    host: &WasmHost,
    method: &str,
    path: &str,
    body: &Value,
    query: &Value,
) -> Value {
    host.log_debug(&format!("handle_http_endpoint: {} {}", method, path));

    match (method, path) {
        ("POST", "task-status") => handle_update_task_status(host, body),
        ("GET", "task-status") => handle_get_task_status(host, query),
        ("POST", "session-mode") => handle_set_session_mode(host, body, query),
        ("GET", "session-mode") => handle_get_session_mode(host, query),
        ("GET", "session-settings") => handle_get_session_settings_http(host, query),
        ("GET", "task-history/current") => handle_get_current_task(host, query),
        _ => {
            host.log_warn(&format!("Unknown HTTP endpoint: {} {}", method, path));
            http_response::error(404, &format!("Not found: {} {}", method, path))
        }
    }
}

/// POST /task-status — 接收 Claude Code hook 推送的任务状态
fn handle_update_task_status(host: &WasmHost, body: &Value) -> Value {
    host.log_debug(&format!("task-status body: {}", body));

    let session_id = body
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let status = body.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let reason = body.get("reason").and_then(|v| v.as_str());
    let questions = body.get("questions");
    let bedcode_session_id = body.get("bedcode_session_id").and_then(|v| v.as_str());
    // 事件发生时刻（脚本 UTC 时间戳，固定宽度字符串，字典序可比）：
    // 宿主仅在 event_time >= 行内已应用事件时应用，拒绝迟到的旧事件覆盖新状态
    let event_time = body.get("event_time").and_then(|v| v.as_str());

    host.log_debug(&format!(
        "task-status parsed: session_id={}, status={}, reason={:?}, has_questions={}, bedcode_sid={:?}, event_time={:?}",
        session_id, status, reason, questions.is_some(), bedcode_session_id, event_time
    ));

    if session_id.is_empty() {
        host.log_warn("task-status rejected: empty session_id");
        return http_response::error(400, "Missing session_id");
    }

    // 验证 status 值
    let valid_statuses = ["idle", "in_progress", "asking", "completed", "interrupted"];
    if !valid_statuses.contains(&status) {
        host.log_warn(&format!(
            "task-status invalid status: '{}' for session_id={}",
            status, session_id
        ));
        return http_response::error(
            400,
            &format!(
                "Invalid task status: {}. Must be one of: {}",
                status,
                valid_statuses.join(", ")
            ),
        );
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
            // idle（新会话就绪，如 /clear 后重建）不受终态保护拦截：
            // 仍需触发 on_session_idle 驱动 waiting 态任务调度，
            // 否则任务永远卡在 waiting 直到超时取消
            if status == "idle" {
                host.log_info(&format!(
                    "task-status: session_id={} terminal row exists ('{}'), idle triggers on_session_idle",
                    session_id, current_status
                ));
                if let Some(bedcode_sid) = bedcode_session_id.filter(|s| !s.is_empty()) {
                    upsert_session_mapping(host, session_id, bedcode_sid);
                }
                crate::queue::on_session_idle(host, &resolved_session_id);
                return http_response::ok();
            }
            host.log_info(&format!(
                "task-status: session_id={} skip, current '{}' is terminal, new '{}' is not",
                session_id, current_status, status
            ));
            return http_response::ok();
        }

        // 时序保护：拒绝迟到的旧事件（event_time 早于行内已应用事件）。
        // HTTP 推送可能被阻塞乱序到达（Stop 的 GET+POST 竞态、resume 后旧会话迟到推送等），
        // 旧状态覆盖最新状态会导致任务状态回跳、队列调度错乱。
        // 无 event_time（旧版脚本）或行无 event_time（迁移前数据）时不比较，保持兼容。
        let row_event_time = row.get("event_time").and_then(|v| v.as_str()).unwrap_or("");
        let incoming_event_time = event_time.unwrap_or("");
        if !incoming_event_time.is_empty()
            && !row_event_time.is_empty()
            && incoming_event_time < row_event_time
        {
            host.log_info(&format!(
                "task-status: session_id={} skip, stale event_time '{}' < row '{}' (current status '{}')",
                session_id, incoming_event_time, row_event_time, current_status
            ));
            return http_response::ok();
        }

        // 更新已有记录：动态子句与绑定参数同步组装（占位符 ?N 按序编号）
        let task_id = row
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut clauses: Vec<String> = Vec::new();
        let mut params: Vec<Value> = Vec::new();

        // 追加一个绑定参数，返回对应占位符（?1、?2 …）
        let push_param = |params: &mut Vec<Value>, v: Value| -> String {
            params.push(v);
            format!("?{}", params.len())
        };

        clauses.push(format!(
            "status = {}",
            push_param(&mut params, Value::String(status.to_string()))
        ));
        if let Some(r) = reason {
            clauses.push(format!(
                "exit_reason = {}",
                push_param(&mut params, Value::String(r.to_string()))
            ));
        }
        if let Some(q) = questions {
            clauses.push(format!(
                "questions = {}",
                push_param(&mut params, q.clone())
            ));
        }

        // 更新 session_id 映射
        if let Some(bedcode_sid) = bedcode_session_id.filter(|s| !s.is_empty()) {
            clauses.push(format!(
                "session_id = {}",
                push_param(&mut params, Value::String(bedcode_sid.to_string()))
            ));
            clauses.push(format!(
                "claude_sid = {}",
                push_param(&mut params, Value::String(session_id.to_string()))
            ));
        }

        // 推进时序保护基线（event_time 与脚本 payload 同格式，字符串字典序比较）
        if !incoming_event_time.is_empty() {
            clauses.push(format!(
                "event_time = {}",
                push_param(&mut params, Value::String(incoming_event_time.to_string()))
            ));
        }

        // 状态转换时更新时间戳（SQL 函数，无绑定参数）
        match status {
            "in_progress" => clauses.push("started_at = datetime('now')".to_string()),
            "completed" | "interrupted" | "failed" => {
                clauses.push("completed_at = datetime('now')".to_string())
            }
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
            host.log_info(&format!(
                "Session mapping stored: claude_sid={} → bedcode_sid={}",
                session_id, bedcode_sid
            ));
        }

        // 新会话就绪信号（SessionStart → idle）：驱动队列 waiting 态调度。
        // 上下文清理（/clear）后 Claude Code 重建会话，此处收到新会话的 idle
        // 推送，即 ADR-0004 约定的"新会话就绪"时机，可安全下发排队任务。
        crate::queue::on_session_idle(host, &resolved_session_id);

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
    let _ = host.bus_publish(
        EVENT_TASK_STATUS_CHANGED,
        &serde_json::json!({
            "session_id": resolved_session_id,
            "task_status": status,
        }),
    );
    // 通知前端 UI 实时刷新（事件名与前端 context.events.on 监听一致）
    host.emit_event(
        EVENT_TASK_STATUS_CHANGED,
        &serde_json::json!({
            "session_id": resolved_session_id,
            "taskStatus": status,
            "taskReason": reason,
        }),
    );

    host.log_info(&format!(
        "Task status updated: claude_sid={} bedcode_sid={} status={}",
        session_id, resolved_session_id, status
    ));

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

    let session_id = body
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let auto_approve = body
        .get("auto_approve")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if session_id.is_empty() {
        host.log_warn("session-mode POST rejected: empty session_id");
        return http_response::error(400, "Missing session_id");
    }

    // 认证由网关中间件统一处理（JWT 或本地放行），此处不重复校验

    // 解析 Claude Code session_id → BedCode PTY session_id（与 GET 及 task-status 一致）：
    // hook/移动端可能携带 claude_sid，不经解析会把开关写到错误的 session_settings 行，
    // 导致前端按 bedcode sid 读取时开关状态不生效
    let resolved_id = resolve_session_id(host, session_id);
    host.log_debug(&format!(
        "session-mode POST resolved: claude_sid={} → resolved_sid={}",
        session_id, resolved_id
    ));

    // 写入会话设置表（auto_answer 开关，兼容旧 hook 只传 auto_approve 的语义）
    set_session_flags(host, &resolved_id, None, Some(auto_approve));

    // 当前 auto_execute 值（本端点不改动它，事件载荷带上供前端开关保持显示一致）
    let (auto_execute, _) = session_flags(host, &resolved_id);

    // 广播模式变更到移动端
    host.broadcast_sync(&SyncEvent::SessionModeChanged {
        session_id: resolved_id.to_string(),
        auto_approve,
    });
    host.log_debug(&format!(
        "broadcast_sync: SessionModeChanged for session_id={}",
        resolved_id
    ));

    // 通过消息总线通知其他插件会话模式变更
    let _ = host.bus_publish(
        EVENT_SESSION_MODE_CHANGED,
        &serde_json::json!({
            "session_id": resolved_id,
            "auto_approve": auto_approve,
            "auto_execute": auto_execute,
        }),
    );
    // 通知前端 UI（事件名与前端 context.events.on 监听一致，载荷用前端约定的 camelCase，
    // 与 set_auto_mode 的 emit 保持同构，前端 onModeChanged 才能正确同步两个开关）
    host.emit_event(
        EVENT_SESSION_MODE_CHANGED,
        &serde_json::json!({
            "session_id": resolved_id,
            "autoApprove": auto_approve,
            "auto_answer": auto_approve,
            "autoExecute": auto_execute,
            "auto_execute": auto_execute,
        }),
    );

    host.log_info(&format!(
        "Session mode set: session_id={} resolved_sid={} auto_approve={}",
        session_id, resolved_id, auto_approve
    ));
    http_response::ok()
}

/// GET /task-status — 查询当前任务状态
///
/// 供终止 hook（Stop/SubagentStop/SessionEnd）查询当前状态，避免盲目覆盖终态
fn handle_get_task_status(host: &WasmHost, query: &Value) -> Value {
    let session_id = query
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if session_id.is_empty() {
        host.log_warn("task-status GET rejected: empty session_id");
        return http_response::error(400, "Missing session_id");
    }

    let resolved_id = resolve_session_id(host, session_id);
    let task_status = find_task_by_session(host, &resolved_id)
        .and_then(|row| {
            row.get("status")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "idle".to_string());

    http_response::ok_with_data(serde_json::json!({
        "session_id": session_id,
        "task_status": task_status,
    }))
}

/// GET /session-mode — 查询会话自动授权模式
fn handle_get_session_mode(host: &WasmHost, query: &Value) -> Value {
    host.log_debug(&format!("session-mode GET query: {}", query));

    let session_id = query
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if session_id.is_empty() {
        host.log_warn("session-mode GET rejected: empty session_id");
        return http_response::error(400, "Missing session_id");
    }

    // 认证由网关中间件统一处理（JWT 或本地放行），此处不重复校验

    // 解析 Claude Code session_id → BedCode PTY session_id
    let resolved_id = resolve_session_id(host, session_id);
    host.log_debug(&format!(
        "session-mode GET resolved: claude_sid={} → resolved_sid={}",
        session_id, resolved_id
    ));

    // 从会话设置表读取 auto_answer（自动应答开关）；旧数据回退读取 task_history.auto_approve
    let (_, auto_approve) = session_flags(host, &resolved_id);

    host.log_debug(&format!(
        "Session mode queried: claude_sid={} resolved_sid={} auto_approve={}",
        session_id, resolved_id, auto_approve
    ));

    http_response::ok_with_data(serde_json::json!({
        "session_id": session_id,
        "auto_approve": auto_approve,
    }))
}

/// GET /session-settings — 查询会话设置（auto_execute + auto_answer）
fn handle_get_session_settings_http(host: &WasmHost, query: &Value) -> Value {
    let session_id = query
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if session_id.is_empty() {
        return http_response::error(400, "Missing session_id");
    }

    let resolved_id = resolve_session_id(host, session_id);
    let (auto_execute, auto_answer) = session_flags(host, &resolved_id);

    http_response::ok_with_data(serde_json::json!({
        "session_id": session_id,
        "auto_execute": auto_execute,
        "auto_answer": auto_answer,
    }))
}

/// GET /task-history/current — 查询会话当前任务（最新一条历史记录）
fn handle_get_current_task(host: &WasmHost, query: &Value) -> Value {
    let session_id = query
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if session_id.is_empty() {
        return http_response::error(400, "Missing session_id");
    }

    let resolved_id = resolve_session_id(host, session_id);
    let task = find_task_by_session(host, &resolved_id);

    http_response::ok_with_data(serde_json::json!({
        "session_id": session_id,
        "task": task,
    }))
}

// ==================== 辅助函数 ====================

/// 解析 Claude Code session_id → BedCode PTY session_id
///
/// 优先从 session_mapping 表查找映射，fallback 到 task_history 表
fn resolve_session_id(host: &WasmHost, claude_session_id: &str) -> String {
    // 优先查 session_mapping 表
    if let Some(mapped) = find_mapping_by_claude_sid(host, claude_session_id) {
        host.log_debug(&format!(
            "resolve_session_id: found in session_mapping: {} → {}",
            claude_session_id, mapped
        ));
        return mapped;
    }

    // fallback: 从 task_history 查找
    find_task_by_claude_sid(host, claude_session_id)
        .and_then(|row| {
            row.get("session_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            host.log_debug(&format!(
                "resolve_session_id: no mapping found, using claude_session_id as-is: {}",
                claude_session_id
            ));
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

/// 任务历史查询筛选条件（字段均为可选，空/None 不参与过滤）
pub struct TaskHistoryFilter {
    pub session_id: Option<String>,
    pub status: Option<String>,
    pub agent: Option<String>,
    pub source: Option<String>,
    /// created_at 下界（ISO 时间字符串，含）
    pub since: Option<String>,
    /// created_at 上界（ISO 时间字符串，含）
    pub until: Option<String>,
    /// 分页大小，默认 100，上限 500
    pub limit: i64,
    /// 分页偏移
    pub offset: i64,
}

impl Default for TaskHistoryFilter {
    fn default() -> Self {
        Self {
            session_id: None,
            status: None,
            agent: None,
            source: None,
            since: None,
            until: None,
            limit: 100,
            offset: 0,
        }
    }
}

impl TaskHistoryFilter {
    /// 组装 WHERE 子句与绑定参数（全部参数绑定，无手写转义）
    fn build_where(&self) -> (String, Vec<Value>) {
        let mut clauses: Vec<String> = Vec::new();
        let mut params: Vec<Value> = Vec::new();
        let add =
            |clauses: &mut Vec<String>, params: &mut Vec<Value>, col: &str, v: &Option<String>| {
                if let Some(val) = v {
                    if !val.is_empty() {
                        params.push(Value::String(val.clone()));
                        clauses.push(format!("{} = ?{}", col, params.len()));
                    }
                }
            };
        add(&mut clauses, &mut params, "session_id", &self.session_id);
        add(&mut clauses, &mut params, "status", &self.status);
        add(&mut clauses, &mut params, "agent", &self.agent);
        add(&mut clauses, &mut params, "source", &self.source);
        if let Some(since) = &self.since {
            if !since.is_empty() {
                params.push(Value::String(since.clone()));
                clauses.push(format!("created_at >= ?{}", params.len()));
            }
        }
        if let Some(until) = &self.until {
            if !until.is_empty() {
                params.push(Value::String(until.clone()));
                clauses.push(format!("created_at <= ?{}", params.len()));
            }
        }
        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", clauses.join(" AND "))
        };
        (where_sql, params)
    }
}

/// 查询任务历史记录（供插件内部 command 使用）
///
/// 支持 session_id/status/agent/source/时间范围筛选与分页，
/// 返回字段含 agent、source（P1 起开始填充）
pub fn list_task_history(host: &WasmHost, filter: &TaskHistoryFilter) -> anyhow::Result<Value> {
    let (where_sql, mut params) = filter.build_where();

    // 分页：limit 上限 500，防止前端误传大值拖垮查询
    let limit = filter.limit.clamp(1, 500);
    let offset = filter.offset.max(0);
    params.push(Value::Number(limit.into()));
    let limit_ph = format!("?{}", params.len());
    params.push(Value::Number(offset.into()));
    let offset_ph = format!("?{}", params.len());

    let sql = format!(
        "SELECT id, description, status, agent, source, session_id, claude_sid, working_dir, \
         auto_approve, exit_reason, created_at, started_at, completed_at, input_tokens, output_tokens \
         FROM task_history{} ORDER BY created_at DESC LIMIT {} OFFSET {}",
        where_sql, limit_ph, offset_ph
    );
    let rows = host
        .plugin_db_query_params(&sql, &params)
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    // 同条件统计总数，供前端分页展示
    let count_sql = format!("SELECT COUNT(*) AS cnt FROM task_history{}", where_sql);
    let count_params: Vec<Value> = params[..params.len() - 2].to_vec();
    let total = host
        .plugin_db_query_params(&count_sql, &count_params)
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|row| row.get("cnt").and_then(|v| v.as_i64()))
        .unwrap_or(0);

    Ok(serde_json::json!({
        "tasks": rows,
        "total": total,
        "limit": limit,
        "offset": offset,
    }))
}

/// 任务历史统计聚合（同筛选条件）
///
/// 返回：总数、各状态数、成功率（终态中 completed 占比）、
/// 平均耗时（秒，有 started_at + completed_at 的终态任务）
pub fn task_history_stats(host: &WasmHost, filter: &TaskHistoryFilter) -> anyhow::Result<Value> {
    let (where_sql, params) = filter.build_where();

    let rows = host
        .plugin_db_query_params(
            &format!(
                "SELECT status, COUNT(*) AS cnt, \
                 AVG(CASE WHEN status IN ('completed', 'interrupted') AND started_at IS NOT NULL AND completed_at IS NOT NULL \
                     THEN (julianday(completed_at) - julianday(started_at)) * 86400.0 END) AS avg_duration \
                 FROM task_history{} GROUP BY status",
                where_sql
            ),
            &params,
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    let mut by_status = serde_json::Map::new();
    let mut total: i64 = 0;
    let mut completed: i64 = 0;
    let mut terminal: i64 = 0;
    let mut duration_sum: f64 = 0.0;
    let mut duration_count: i64 = 0;
    for row in &rows {
        let status = row
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let cnt = row.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0);
        by_status.insert(status.clone(), Value::Number(cnt.into()));
        total += cnt;
        if status == "completed" {
            completed += cnt;
        }
        if status == "completed" || status == "interrupted" {
            terminal += cnt;
        }
        if let Some(avg) = row.get("avg_duration").and_then(|v| v.as_f64()) {
            duration_sum += avg * cnt as f64;
            duration_count += cnt;
        }
    }

    Ok(serde_json::json!({
        "total": total,
        "by_status": by_status,
        "completed": completed,
        "terminal": terminal,
        "success_rate": if terminal > 0 { completed as f64 / terminal as f64 } else { 0.0 },
        "avg_duration_seconds": if duration_count > 0 { duration_sum / duration_count as f64 } else { 0.0 },
    }))
}

/// 列出运行中的会话（含最新任务摘要与待执行队列数）
///
/// 供前端「当前任务」Tab 展示活动任务，并提供「选择运行中会话创建任务」的下拉选项。
/// 过滤条件：会话状态为 Running / Starting / WaitingInput（存活会话），
/// 已停止 / 停止中 / 错误 / 空闲会话不参与（无法向其投递任务）。
/// 结果按活跃度排序：有活动任务 > 有待执行队列 > 其余，同档按名称排序。
pub fn list_running_sessions(host: &WasmHost) -> Vec<Value> {
    // config_id → working_dir 映射（会话配置列表含路径信息，供前端展示会话标签）
    let config_working_dirs: HashMap<String, String> = host
        .session_config_list()
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|c| {
            let id = c.get("id")?.as_str()?.to_string();
            let wd = c
                .get("workingDir")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some((id, wd))
        })
        .collect();

    let sessions = host
        .session_list()
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    let mut result: Vec<Value> = sessions
        .iter()
        .filter_map(|s| {
            // SessionInfo 序列化为 camelCase（见 session_event.rs）
            let status = s.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if !matches!(status, "Running" | "Starting" | "WaitingInput") {
                return None;
            }
            let session_id = s
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if session_id.is_empty() {
                return None;
            }
            let name = s
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let config_id = s
                .get("configId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let working_dir = config_working_dirs
                .get(&config_id)
                .cloned()
                .unwrap_or_default();

            // 最新任务摘要（无记录时视为 idle）
            let task = find_task_by_session(host, &session_id);
            let task_status = task
                .as_ref()
                .and_then(|r| r.get("status"))
                .and_then(|v| v.as_str())
                .unwrap_or("idle")
                .to_string();
            let description = task
                .as_ref()
                .and_then(|r| r.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let started_at = task
                .as_ref()
                .and_then(|r| r.get("started_at"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let queue_count = crate::queue::pending_count(host, &session_id);

            Some(serde_json::json!({
                "session_id": session_id,
                "name": name,
                "config_id": config_id,
                "working_dir": working_dir,
                "status": status,
                "task_status": task_status,
                "description": description,
                "started_at": started_at,
                "agent": session_agent(host, &session_id),
                "queue_count": queue_count,
            }))
        })
        .collect();

    // 活跃度排序：活动任务 > 有待执行队列 > 其余
    let activity = |v: &Value| -> u8 {
        let ts = v.get("task_status").and_then(|x| x.as_str()).unwrap_or("");
        let qc = v.get("queue_count").and_then(|x| x.as_i64()).unwrap_or(0);
        if matches!(ts, "in_progress" | "asking") {
            2
        } else if qc > 0 {
            1
        } else {
            0
        }
    };
    result.sort_by(|a, b| {
        activity(b).cmp(&activity(a)).then_with(|| {
            let na = a
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_lowercase();
            let nb = b
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_lowercase();
            na.cmp(&nb)
        })
    });

    result
}

/// 设置会话级开关（供插件内部 command 使用）
///
/// auto_execute（自动执行）与 auto_answer（自动应答）为两个独立开关，
/// 任一为 None 时保持当前值。auto_execute 由关闭切换为开启且会话空闲时
/// 立即调度队列（try_dispatch_next 内部以 auto_execute 为门，安全幂等）。
pub fn set_auto_mode(
    host: &WasmHost,
    session_id: &str,
    auto_execute: Option<bool>,
    auto_answer: Option<bool>,
) -> anyhow::Result<Value> {
    host.log_debug(&format!(
        "set_auto_mode: session_id={}, auto_execute={:?}, auto_answer={:?}",
        session_id, auto_execute, auto_answer
    ));

    let (prev_execute, prev_answer) = session_flags(host, session_id);
    let new_execute = auto_execute.unwrap_or(prev_execute);
    let new_answer = auto_answer.unwrap_or(prev_answer);

    set_session_flags(host, session_id, Some(new_execute), Some(new_answer));

    // 通知前端模式变更（事件载荷同时携带两个开关 + 兼容旧字段 autoApprove）
    host.emit_event(
        EVENT_SESSION_MODE_CHANGED,
        &serde_json::json!({
            "session_id": session_id,
            "autoApprove": new_answer,
            "auto_answer": new_answer,
            "autoExecute": new_execute,
            "auto_execute": new_execute,
        }),
    );
    host.log_debug(&format!(
        "emit_event: session:mode-changed for session_id={} (execute={}, answer={})",
        session_id, new_execute, new_answer
    ));

    // 广播到移动端（类型化事件仅含 auto_approve 字段，保持线协议兼容）
    host.broadcast_sync(&SyncEvent::SessionModeChanged {
        session_id: session_id.to_string(),
        auto_approve: new_answer,
    });

    // 通过消息总线通知其他插件会话模式变更
    let _ = host.bus_publish(
        EVENT_SESSION_MODE_CHANGED,
        &serde_json::json!({
            "session_id": session_id,
            "auto_approve": new_answer,
            "auto_execute": new_execute,
        }),
    );

    // 自动执行刚开启且会话空闲 → 立即调度队列中已积累的任务
    if new_execute && !prev_execute && !has_active_task(host, session_id) {
        crate::queue::try_dispatch_next(host, session_id);
    }

    Ok(serde_json::json!({
        "success": true,
        "auto_execute": new_execute,
        "auto_answer": new_answer,
    }))
}

// ==================== 会话级开关（session_settings 表） ====================

/// 读取会话的 auto_execute（自动执行）与 auto_answer（自动应答）开关
///
/// session_settings 表为唯一事实来源；旧会话无该表记录时，
/// auto_answer 回退读取 task_history 最新行的 auto_approve（旧版语义），
/// auto_execute 默认关闭（旧版自动执行行为由手动开关取代）。
pub fn session_flags(host: &WasmHost, session_id: &str) -> (bool, bool) {
    let row = host
        .plugin_db_query_params(
            "SELECT auto_execute, auto_answer FROM session_settings WHERE session_id = ?1",
            &sql_params![session_id],
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()));

    match row {
        Some(r) => (
            r.get("auto_execute").and_then(|v| v.as_i64()).unwrap_or(0) != 0,
            r.get("auto_answer").and_then(|v| v.as_i64()).unwrap_or(0) != 0,
        ),
        None => {
            let legacy_answer = find_task_by_session(host, session_id)
                .and_then(|row| row.get("auto_approve").and_then(|v| v.as_i64()))
                .unwrap_or(0)
                != 0;
            (false, legacy_answer)
        }
    }
}

/// 会话是否开启自动执行（任务入队后自动调度）
pub fn auto_execute_on(host: &WasmHost, session_id: &str) -> bool {
    session_flags(host, session_id).0
}

/// 写入会话开关（部分更新：None 字段保持当前值）
///
/// 同时同步最新任务行的 auto_approve 字段，兼容旧 hook/移动端对
/// task_history.auto_approve 的读取路径。
pub fn set_session_flags(
    host: &WasmHost,
    session_id: &str,
    auto_execute: Option<bool>,
    auto_answer: Option<bool>,
) {
    let (cur_execute, cur_answer) = session_flags(host, session_id);
    let new_execute = auto_execute.unwrap_or(cur_execute);
    let new_answer = auto_answer.unwrap_or(cur_answer);

    // INSERT OR REPLACE：按 session_id 覆盖，与 session_mapping 的 upsert 模式一致
    let _ = host.plugin_db_execute_params(
        "INSERT OR REPLACE INTO session_settings (session_id, auto_execute, auto_answer, updated_at) \
         VALUES (?1, ?2, ?3, datetime('now'))",
        &sql_params![session_id, new_execute, new_answer],
    );

    // 同步最新任务行的 auto_approve（子查询定位最新记录，SQLite 不支持 UPDATE ... ORDER BY）
    let _ = host.plugin_db_execute_params(
        "UPDATE task_history SET auto_approve = ?1, updated_at = datetime('now') \
         WHERE id = (SELECT id FROM task_history WHERE session_id = ?2 ORDER BY created_at DESC LIMIT 1)",
        &sql_params![new_answer, session_id],
    );

    host.log_debug(&format!(
        "session flags set: session_id={} auto_execute={} auto_answer={}",
        session_id, new_execute, new_answer
    ));
}

/// 查询会话开关（供前端弹窗初始化开关状态）
pub fn get_session_settings(host: &WasmHost, session_id: &str) -> anyhow::Result<Value> {
    let (auto_execute, auto_answer) = session_flags(host, session_id);
    Ok(serde_json::json!({
        "session_id": session_id,
        "auto_execute": auto_execute,
        "auto_answer": auto_answer,
    }))
}
