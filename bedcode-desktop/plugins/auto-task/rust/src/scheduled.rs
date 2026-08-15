//! 定时自动任务（v6，ADR 0003）
//!
//! 指定触发时刻、一次性执行的自动任务：携带会话配置与一组 prompt，
//! 触发时新建会话并依次执行。
//!
//! 状态机：
//!
//! ```text
//! pending ──(到期，session_create 成功)──▶ creating ──(Created 事件到达，prompts 入队)──▶ executed
//! pending ──(到期，session_create 失败)──▶ failed
//! pending ──(超过宽限期仍未执行，如应用关闭期间到期)──▶ missed（不补跑）
//! creating ──(应用重启，会话丢失)──▶ failed
//! missed / failed ──(用户 reset：可改触发时间)──▶ pending（重新加入调度）
//! ```
//!
//! 终态处置：missed / failed 为不可自动恢复的终态，用户可删除（清理）
//! 或 reset（改触发时间后重新调度）；executed 为执行档案，前端归入
//! 历史区段，可单条删除或一键清空（执行详情在任务记录页按来源筛选查看）。
//!
//! 触发链路与常规自动任务共用队列调度：Created 事件到达后把 prompts
//! 注入新会话队列（source='scheduled'）并开启会话自动执行开关，
//! 首轮任务由 Claude Code SessionStart 的 idle 推送驱动下发（新会话跳过上下文清理）。
//!
//! 时间基准：WASM 无系统时钟，所有时间比较使用宿主 scheduler-tick
//! 回调注入的 `now_utc`（与 SQLite datetime('now') 同格式，字符串可直接比较）。
//!
//! SQL 一律使用参数绑定（`*_params` + `?N` 占位符），无手写转义。

use bedcode_plugin_api::constants::EVENT_SESSION_MODE_CHANGED;
use bedcode_plugin_api::constants::EVENT_TASK_SCHEDULED_CHANGED;
use bedcode_plugin_api::events::SyncEvent;
use bedcode_plugin_api::host::{HostBus, HostEvents, HostLog, HostPluginDatabase, HostSession};
use bedcode_plugin_api::http_response;
use bedcode_plugin_api::sql_params;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::Value;

/// 调度器轮询间隔（秒）——宿主定时器按此周期回调 scheduler-tick
///
/// 同时承担 waiting 态延迟 clear 的到点发送（见 queue::send_due_clears，
/// 延迟窗口 CLEAR_DELAY_SECONDS = 2s），间隔取 1s 保证秒级粒度。
pub const SCHEDULER_INTERVAL_SECS: u64 = 1;

/// 到期宽限（秒）：到期后超过该时长仍未执行视为错过（应用当时未运行），
/// 标 missed 不补跑。取值远大于轮询间隔（1s），覆盖应用关闭/重启期间的
/// 调度空洞（轮询间隔缩短后宽限仍按原语义保留）
const MISSED_GRACE_SECONDS: i64 = 120;

/// 定时任务会话首轮下发宽限（秒）：TUI 型 agent（opencode）不输入 prompt 不创建
/// 会话（TUI 启动后停在输入界面，首个 prompt 提交才触发 session.created），
/// SessionStart idle 推送永不产生，队列会卡在 pending。入队超过该时长仍无任何
/// waiting/executing 项时由 scheduler-tick 兜底主动调度（handle_scheduler_tick
/// 步骤 3）。取值覆盖 opencode TUI 从 PTY 启动到输入框就绪的实测耗时（约 9s），
/// 留足余量；claude code / pi 等 agent 的 idle 秒级到达，正常路径不受影响
const FIRST_DISPATCH_GRACE_SECS: i64 = 15;

// ==================== CRUD ====================

/// 创建定时任务
///
/// trigger_at 为 UTC 时间字符串（"YYYY-MM-DD HH:MM:SS"，与 SQLite
/// datetime('now') 同格式）；前端负责把用户选择的本地时间转换为 UTC。
/// 返回新任务 ID；参数非法返回 None
pub fn create_job(
    host: &WasmHost,
    name: &str,
    config_id: &str,
    trigger_at: &str,
    prompts: &[String],
) -> Option<String> {
    if config_id.is_empty() || trigger_at.is_empty() || prompts.is_empty() {
        host.log_warn("create_job: missing config_id/trigger_at/prompts");
        return None;
    }

    let prompts_json = serde_json::to_string(prompts).unwrap_or_else(|_| "[]".to_string());

    let id = host
        .plugin_db_query("SELECT lower(hex(randomblob(16))) AS id")
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|row| {
            row.get("id")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        })
        .unwrap_or_else(|| {
            format!(
                "sched-{}-{}",
                config_id,
                trigger_at.replace([' ', ':', '-'], "")
            )
        });

    let name_param = if name.is_empty() {
        Value::Null
    } else {
        Value::String(name.to_string())
    };

    match host.plugin_db_execute_params(
        "INSERT INTO scheduled_jobs (id, name, config_id, trigger_at, prompts, status, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending', datetime('now'))",
        &sql_params![id, name_param, config_id, trigger_at, prompts_json],
    ) {
        Ok(_) => {
            host.log_info(&format!(
                "Scheduled job created: id={} config_id={} trigger_at={} prompts={}",
                id, config_id, trigger_at, prompts.len()
            ));
            Some(id)
        }
        Err(e) => {
            host.log_error(&format!("create_job: insert failed: {}", e));
            None
        }
    }
}

/// 列出全部定时任务（按触发时间升序）
pub fn list_jobs(host: &WasmHost) -> Vec<Value> {
    host.plugin_db_query(
        "SELECT id, name, config_id, trigger_at, prompts, status, session_id, created_at, executed_at, error \
         FROM scheduled_jobs ORDER BY trigger_at ASC",
    )
    .ok()
    .flatten()
    .and_then(|v| v.as_array().cloned())
    .unwrap_or_default()
}

/// 删除定时任务
///
/// 可删状态：pending（未触发）、missed（错过）、failed（创建失败/重启丢失）、
/// executed（执行档案）——后三者是不可自动恢复的终态，用户应能清理归档；
/// creating 为瞬态（由状态机自终结）。
pub fn delete_job(host: &WasmHost, job_id: &str) -> bool {
    let affected = host
        .plugin_db_execute_params(
            "DELETE FROM scheduled_jobs WHERE id = ?1 AND status IN ('pending', 'missed', 'failed', 'executed')",
            &sql_params![job_id],
        )
        .unwrap_or(-1);
    if affected > 0 {
        host.log_info(&format!("Scheduled job deleted: id={}", job_id));
        true
    } else {
        false
    }
}

/// 重置定时任务：把终态（missed / failed）任务重新加入调度
///
/// 状态回 pending、清除错误/执行/会话关联；可选更新触发时间
/// （trigger_at 为 None 时保留原值 —— 若原时间已过去，宽限期内会立即触发，
/// 超过宽限期则再次标 missed，调用方应引导用户提供新时间）。
/// 仅 missed / failed 可重置；pending 无需重置，executed 为档案，creating 为瞬态。
pub fn reset_job(host: &WasmHost, job_id: &str, trigger_at: Option<&str>) -> bool {
    let affected = host
        .plugin_db_execute_params(
            "UPDATE scheduled_jobs SET status = 'pending', \
             trigger_at = COALESCE(?2, trigger_at), \
             error = NULL, executed_at = NULL, session_id = NULL \
             WHERE id = ?1 AND status IN ('missed', 'failed')",
            &sql_params![job_id, trigger_at],
        )
        .unwrap_or(-1);
    if affected > 0 {
        host.log_info(&format!(
            "Scheduled job reset: id={} trigger_at={:?}",
            job_id, trigger_at
        ));
        true
    } else {
        false
    }
}

/// 创建定时任务并广播变更（供插件 command 与 HTTP 端点共用）
pub fn create_job_with_broadcast(
    host: &WasmHost,
    name: &str,
    config_id: &str,
    trigger_at: &str,
    prompts: &[String],
) -> Option<String> {
    let job_id = create_job(host, name, config_id, trigger_at, prompts)?;
    broadcast_scheduled_changed(host, &job_id, "pending", "create");
    Some(job_id)
}

/// 删除定时任务并广播变更（供插件 command 与 HTTP 端点共用）
pub fn delete_job_with_broadcast(host: &WasmHost, job_id: &str) -> bool {
    if delete_job(host, job_id) {
        broadcast_scheduled_changed(host, job_id, "deleted", "delete");
        true
    } else {
        false
    }
}

/// 重置定时任务并广播变更（供插件 command 与 HTTP 端点共用）
pub fn reset_job_with_broadcast(host: &WasmHost, job_id: &str, trigger_at: Option<&str>) -> bool {
    if reset_job(host, job_id, trigger_at) {
        broadcast_scheduled_changed(host, job_id, "pending", "reset");
        true
    } else {
        false
    }
}

// ==================== 调度触发 ====================

/// 定时器到点回调：处理到期任务
///
/// 0. 超过宽限时长的 creating 任务标 failed（宿主异步创建失败时
///    Created 事件不会到达，看门狗兑底）
/// 1. 超过宽限期的 pending 任务标 missed（应用关闭期间错过，不补跑）
/// 2. 宽限期内的到期任务：session_create 排队宿主异步创建（返回预生成
///    会话 ID），置 creating 等待 Created 事件入队
pub fn handle_scheduler_tick(host: &WasmHost, now_utc: &str) {
    // 0. creating 卡死兑底：会话创建在宿主异步执行（wasm 调用栈内同步创建会
    //    死锁，见宿主 host_session_create），创建失败时 Created 事件不会到达；
    //    超过宽限时长仍为 creating 视为创建失败
    let stuck_sql = format!(
        "UPDATE scheduled_jobs SET status = 'failed', executed_at = datetime('now'), \
         error = 'Session creation timed out' \
         WHERE status = 'creating' AND trigger_at <= datetime(?1, '-{} seconds')",
        MISSED_GRACE_SECONDS
    );
    let stuck_count = host
        .plugin_db_execute_params(&stuck_sql, &sql_params![now_utc])
        .unwrap_or(-1);
    if stuck_count > 0 {
        host.log_warn(&format!(
            "scheduler-tick: {} job(s) marked failed (session creation timed out)",
            stuck_count
        ));
        broadcast_scheduled_changed(host, "", "failed", "failed");
    }

    // 1. 错过判定：trigger_at <= now - 宽限期 且仍 pending → missed
    let missed_sql = format!(
        "UPDATE scheduled_jobs SET status = 'missed', executed_at = ?1, \
         error = 'Trigger time passed while app was not running' \
         WHERE status = 'pending' AND trigger_at <= datetime(?2, '-{} seconds')",
        MISSED_GRACE_SECONDS
    );
    let missed_count = host
        .plugin_db_execute_params(&missed_sql, &sql_params![now_utc, now_utc])
        .unwrap_or(-1);
    if missed_count > 0 {
        host.log_warn(&format!(
            "scheduler-tick: {} job(s) marked missed (trigger time passed)",
            missed_count
        ));
        broadcast_scheduled_changed(host, "", "missed", "missed");
    }

    // 2. 到期且仍在宽限期内：触发执行
    let due_jobs = host
        .plugin_db_query_params(
            "SELECT id, config_id, prompts FROM scheduled_jobs \
             WHERE status = 'pending' AND trigger_at <= ?1 \
             AND trigger_at > datetime(?1, ?2)",
            &sql_params![now_utc, format!("-{} seconds", MISSED_GRACE_SECONDS)],
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    for job in due_jobs {
        let job_id = job
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let config_id = job
            .get("config_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if job_id.is_empty() || config_id.is_empty() {
            continue;
        }

        host.log_info(&format!(
            "scheduler-tick: triggering job_id={} config_id={}",
            job_id, config_id
        ));

        // 创建会话（核心 SessionManager::create_session，v6 host function）。
        // 宿主异步创建并立即返回预生成 session_id：成功时 PTY 随后启动，
        // Created 生命周期事件随后到达（事件分发在 wasm 调用返回后，锁已释放）
        match host.session_create(&config_id) {
            Ok(session_id) => {
                // 置 creating 并记录 session_id：Created 事件的匹配键。
                // 先于事件到达更新（session_create 返回时事件尚未分发），不会丢失匹配
                let _ = host.plugin_db_execute_params(
                    "UPDATE scheduled_jobs SET status = 'creating', session_id = ?1 WHERE id = ?2",
                    &sql_params![session_id, job_id],
                );
                host.log_info(&format!(
                    "scheduler-tick: session create queued for job_id={}: session_id={}",
                    job_id, session_id
                ));
                broadcast_scheduled_changed(host, &job_id, "creating", "trigger");
            }
            Err(e) => {
                let _ = host.plugin_db_execute_params(
                    "UPDATE scheduled_jobs SET status = 'failed', executed_at = datetime('now'), error = ?1 \
                     WHERE id = ?2",
                    &sql_params![e.to_string(), job_id],
                );
                host.log_error(&format!(
                    "scheduler-tick: session_create failed for job_id={}: {}",
                    job_id, e
                ));
                broadcast_scheduled_changed(host, &job_id, "failed", "failed");
            }
        }
    }

    // 3. 定时任务会话首轮下发兜底：opencode 等 TUI 型 agent 不输入 prompt 不创建
    //    会话（opencode TUI 启动后停在输入界面，会话由首个 prompt 提交触发），
    //    session.created → idle 推送永不产生，handle_session_created 有意等待的
    //    首轮调度信号缺失，队列会永久卡在 pending。入队超过宽限期仍无任何
    //    waiting/executing 项时主动 try_dispatch_next：对无 clear 命令的 agent
    //    （opencode）会直接下发 prompt（输入即创建会话并执行）。
    //    claude code / pi 等 agent 的 SessionStart idle 秒级到达，正常路径早已
    //    完成首轮下发，此兜底不会触发（幂等：有 waiting/executing 即跳过）。
    let stale_sessions = host
        .plugin_db_query_params(
            &format!(
                "SELECT DISTINCT q.session_id AS session_id FROM task_queue q \
                 JOIN scheduled_jobs j ON j.session_id = q.session_id AND j.status = 'executed' \
                 WHERE q.status = 'pending' \
                   AND q.created_at <= datetime(?1, '-{} seconds') \
                   AND NOT EXISTS ( \
                       SELECT 1 FROM task_queue q2 \
                       WHERE q2.session_id = q.session_id AND q2.status IN ('waiting', 'executing') \
                   )",
                FIRST_DISPATCH_GRACE_SECS
            ),
            &sql_params![now_utc],
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    for row in stale_sessions {
        let session_id = row
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if session_id.is_empty() {
            continue;
        }

        // 会话已不存在（进程退出 / 应用重启后的残留队列，如 opencode TUI 未
        // 创建会话即被杀）：继续每 tick（1s）重试只会无限刷日志且永远失败。
        // 一次性取消全部 pending 项并广播（复用 check_waiting_timeouts 的
        // cancel 语义：移动端预设据此落 interrupted），cancelled 不再命中
        // 本查询，后续 tick 静默跳过
        if host.session_get(&session_id).ok().flatten().is_none() {
            let pending_ids: Vec<String> = host
                .plugin_db_query_params(
                    "SELECT id FROM task_queue WHERE session_id = ?1 AND status = 'pending'",
                    &sql_params![session_id],
                )
                .ok()
                .flatten()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|row| {
                    row.get("id").and_then(|v| v.as_str()).map(|s| s.to_string())
                })
                .collect();
            for task_id in &pending_ids {
                let _ = host.plugin_db_execute_params(
                    "UPDATE task_queue SET status = 'cancelled', updated_at = datetime('now') \
                     WHERE id = ?1",
                    &sql_params![task_id],
                );
                let remaining = crate::queue::pending_count(host, &session_id);
                crate::queue::broadcast_queue_changed(
                    host,
                    &session_id,
                    remaining,
                    "cancel",
                    Some(task_id),
                    Some("cancelled"),
                );
            }
            if !pending_ids.is_empty() {
                host.log_warn(&format!(
                    "scheduler-tick: session {} gone, cancelled {} stale pending task(s)",
                    session_id,
                    pending_ids.len()
                ));
            }
            continue;
        }

        host.log_info(&format!(
            "scheduler-tick: first dispatch fallback for scheduled session_id={} (no idle signal)",
            session_id
        ));
        crate::queue::try_dispatch_next(host, &session_id);
    }
}

/// Created 生命周期事件回调：为 creating 态任务注入 prompts
///
/// 以 session_id 精确匹配（同 config 多个定时任务并发时不混淆）。
/// prompts 入队后不主动调度：新会话由 Claude Code SessionStart 的
/// idle 推送驱动首轮下发（等待 agent CLI 就绪，见 queue::on_session_idle）
pub fn handle_session_created(host: &WasmHost, session_id: &str, config_id: &str) {
    let job = host
        .plugin_db_query_params(
            "SELECT id, prompts FROM scheduled_jobs WHERE status = 'creating' AND session_id = ?1",
            &sql_params![session_id],
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()));

    let Some(job) = job else {
        // 非定时任务创建的会话（用户手动创建等），无需处理
        return;
    };

    let job_id = job
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let prompts: Vec<String> = job
        .get("prompts")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default();

    if prompts.is_empty() {
        host.log_warn(&format!(
            "handle_session_created: job_id={} has no prompts, marking failed",
            job_id
        ));
        let _ = host.plugin_db_execute_params(
            "UPDATE scheduled_jobs SET status = 'failed', executed_at = datetime('now'), \
             error = 'Empty prompts list' WHERE id = ?1",
            &sql_params![job_id],
        );
        broadcast_scheduled_changed(host, &job_id, "failed", "failed");
        return;
    }

    // prompts 依次入队（source='scheduled'），队列调度链复用常规自动任务路径
    for prompt in &prompts {
        crate::queue::add_task_with_source(host, session_id, prompt, "scheduled");
    }

    // 开启会话自动执行：定时任务语义为无人值守自动执行，入队任务必须能自动调度。
    // 不能调用 set_auto_mode —— 其副作用会立即 try_dispatch_next，而此刻 agent CLI
    // 尚未就绪（SessionStart idle 未到达），terminal_send 的输入会丢失或导致重复下发；
    // 首轮下发由 SessionStart 的 idle 推送驱动（on_session_idle → try_dispatch_next）。
    // auto_answer 保持用户设置（默认关，可在会话弹窗手动开启自动应答权限请求）。
    let (_, auto_answer) = crate::state::session_flags(host, session_id);
    crate::state::set_session_flags(host, session_id, Some(true), None);
    host.broadcast_sync(&SyncEvent::SessionModeChanged {
        session_id: session_id.to_string(),
        auto_approve: auto_answer,
    });
    let _ = host.bus_publish(
        EVENT_SESSION_MODE_CHANGED,
        &serde_json::json!({
            "session_id": session_id,
            "auto_approve": auto_answer,
            "auto_execute": true,
        }),
    );
    host.emit_event(
        EVENT_SESSION_MODE_CHANGED,
        &serde_json::json!({
            "session_id": session_id,
            "autoApprove": auto_answer,
            "auto_answer": auto_answer,
            "autoExecute": true,
            "auto_execute": true,
        }),
    );

    let _ = host.plugin_db_execute_params(
        "UPDATE scheduled_jobs SET status = 'executed', executed_at = datetime('now'), error = NULL \
         WHERE id = ?1",
        &sql_params![job_id],
    );

    host.log_info(&format!(
        "handle_session_created: job_id={} enqueued {} prompt(s) to session_id={} config_id={}",
        job_id,
        prompts.len(),
        session_id,
        config_id
    ));

    let remaining = crate::queue::pending_count(host, session_id);
    crate::queue::broadcast_queue_changed(host, session_id, remaining, "add", None, None);
    broadcast_scheduled_changed(host, &job_id, "executed", "trigger");
}

/// 启动恢复：重启前处于 creating 态的任务标记 failed
///
/// 应用退出会销毁全部 PTY 会话，creating 态等待的 Created 事件
/// 永远不会到达（新进程的新会话不属于该任务），直接终结避免卡死
pub fn recover_creating_jobs(host: &WasmHost) {
    let affected = host
        .plugin_db_execute_params(
            "UPDATE scheduled_jobs SET status = 'failed', executed_at = datetime('now'), \
             error = 'App restarted before session was ready' \
             WHERE status = 'creating'",
            &[],
        )
        .unwrap_or(-1);
    if affected > 0 {
        host.log_warn(&format!(
            "recover_creating_jobs: {} job(s) marked failed (session lost on restart)",
            affected
        ));
        broadcast_scheduled_changed(host, "", "failed", "failed");
    }
}

// ==================== HTTP 端点 ====================

/// 处理定时任务 HTTP 端点（路由前缀 scheduled-jobs/，仿 task-queue/*）
///
/// - POST scheduled-jobs/create → 创建
/// - GET scheduled-jobs/list → 列表
/// - DELETE scheduled-jobs/remove → 删除（pending / missed / failed）
/// - POST scheduled-jobs/reset → 重置（missed / failed 重新加入调度，可改触发时间）
pub fn handle_scheduled_http(
    host: &WasmHost,
    method: &str,
    path: &str,
    body: &Value,
    query: &Value,
) -> Value {
    host.log_debug(&format!("handle_scheduled_http: {} {}", method, path));

    match (method, path) {
        ("POST", "create") => handle_create(host, body),
        ("GET", "list") => {
            http_response::ok_with_data(serde_json::json!({ "jobs": list_jobs(host) }))
        }
        ("DELETE", "remove") => handle_remove(host, body, query),
        ("POST", "reset") => handle_reset(host, body),
        _ => {
            host.log_warn(&format!("Unknown scheduled endpoint: {} {}", method, path));
            http_response::error(404, &format!("Not found: {} {}", method, path))
        }
    }
}

/// POST scheduled-jobs/create
///
/// body: { name?, config_id, trigger_at, prompts: [string] }
/// trigger_at 必须是 UTC "YYYY-MM-DD HH:MM:SS" 格式（前端转换）
fn handle_create(host: &WasmHost, body: &Value) -> Value {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let config_id = body.get("config_id").and_then(|v| v.as_str()).unwrap_or("");
    let trigger_at = body
        .get("trigger_at")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let prompts: Vec<String> = body
        .get("prompts")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    if config_id.is_empty() {
        return http_response::error(400, "Missing config_id");
    }
    if trigger_at.is_empty() {
        return http_response::error(400, "Missing trigger_at");
    }
    if prompts.is_empty() {
        return http_response::error(400, "Missing prompts");
    }

    match create_job_with_broadcast(host, name, config_id, trigger_at, &prompts) {
        Some(job_id) => http_response::ok_with_data(serde_json::json!({ "job_id": job_id })),
        None => http_response::error(500, "Failed to create scheduled job"),
    }
}

/// DELETE scheduled-jobs/remove（body 或 query 携带 job_id）
fn handle_remove(host: &WasmHost, body: &Value, query: &Value) -> Value {
    let job_id = body
        .get("job_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| query.get("job_id").and_then(|v| v.as_str()))
        .unwrap_or("");

    if job_id.is_empty() {
        return http_response::error(400, "Missing job_id");
    }

    if delete_job_with_broadcast(host, job_id) {
        http_response::ok()
    } else {
        http_response::error(
            404,
            "Job not found or not deletable (only pending/missed/failed/executed jobs can be deleted)",
        )
    }
}

/// POST scheduled-jobs/reset — 重置 missed / failed 任务
///
/// body: { job_id, trigger_at? } — trigger_at 可选，缺省保留原触发时间
fn handle_reset(host: &WasmHost, body: &Value) -> Value {
    let job_id = body.get("job_id").and_then(|v| v.as_str()).unwrap_or("");
    let trigger_at = body.get("trigger_at").and_then(|v| v.as_str());

    if job_id.is_empty() {
        return http_response::error(400, "Missing job_id");
    }

    let trigger_param = trigger_at.filter(|s| !s.is_empty());
    if reset_job_with_broadcast(host, job_id, trigger_param) {
        http_response::ok_with_data(serde_json::json!({ "job_id": job_id, "status": "pending" }))
    } else {
        http_response::error(
            404,
            "Job not found or not resettable (only missed/failed jobs can be reset)",
        )
    }
}

// ==================== 事件广播 ====================

/// 广播定时任务变更（三通道：broadcast_sync + bus + emit_event，仿现有模式）
fn broadcast_scheduled_changed(host: &WasmHost, job_id: &str, status: &str, action: &str) {
    host.broadcast_sync(&SyncEvent::TaskScheduledChanged {
        job_id: job_id.to_string(),
        status: status.to_string(),
        action: action.to_string(),
    });

    let _ = host.bus_publish(
        EVENT_TASK_SCHEDULED_CHANGED,
        &serde_json::json!({
            "job_id": job_id,
            "status": status,
            "action": action,
        }),
    );
    host.emit_event(
        EVENT_TASK_SCHEDULED_CHANGED,
        &serde_json::json!({
            "job_id": job_id,
            "status": status,
            "action": action,
        }),
    );
}
