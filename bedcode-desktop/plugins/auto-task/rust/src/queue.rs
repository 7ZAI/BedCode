//! 任务队列管理与调度状态机
//!
//! 每个会话维护独立的待执行任务队列，支持添加、删除、查询、清空操作。
//!
//! 调度状态机（ADR-0004）：

//! ```text
//! pending ──(会话已有上下文，需先 clear，延迟 2s 发送)──▶ waiting ──(SessionStart idle 到达)──▶ executing ──▶ done
//! pending ──(全新会话，跳过 clear，直接下发)────────────────▶ executing ──▶ done
//! waiting ──(超时 60s，重试一次 clear 后仍无响应)──▶ cancelled
//! ```
//!
//! - waiting：clear 已计划（延迟 CLEAR_DELAY_SECONDS 由 scheduler-tick 发送，见
//!   send_due_clears）或已发送，等待 Claude Code 重建会话后的 idle 推送（见 state.rs idle 分支）
//! - 出队时由插件直接写任务行（description = prompt，source='queue'），
//!   不再依赖输入行重建，避免 /clear 与 prompt 拆行提交的时序竞争
//!
//! SQL 一律使用参数绑定（`*_params` + `?N` 占位符），无手写转义。

use bedcode_plugin_api::constants::EVENT_TASK_QUEUE_CHANGED;
use bedcode_plugin_api::events::SyncEvent;
use bedcode_plugin_api::host::{
    HostBus, HostEvents, HostLog, HostPluginDatabase, HostSession, HostStorage, HostTerminal,
};
use bedcode_plugin_api::http_response;
use bedcode_plugin_api::sql_params;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::Value;

use crate::agent;

/// waiting 态超时阈值（秒）：clear 发出后超过该时长仍未收到新会话 idle，触发重试
const WAITING_TIMEOUT_SECONDS: i64 = 60;
/// waiting 态最大重试次数（首次 clear + 1 次重试）
const MAX_DISPATCH_ATTEMPTS: i64 = 2;
/// clear 命令发送延迟（秒）
///
/// 任务终态到达后终端 UI 的输出渲染有一定延迟（WebSocket 输出缓冲合并），
/// 立即发送 clear 会给人"上一任务还没执行完就被清屏"的错觉。
/// 延迟 CLEAR_DELAY_SECONDS 再发送，给终端留出渲染输出的时间窗口。
const CLEAR_DELAY_SECONDS: i64 = 2;

/// 自动任务投递输入的提交符（按宿主平台动态选择）
///
/// 投递输入必须以提交符结尾，agent（Claude Code）才会把它当作指令执行：
/// - Windows（ConPTY）：Enter 键产生的字节是 `\r`（CR），Claude Code 只把 `\r` 识别为
///   提交，`\n`（LF）仅是换行内容 —— 发 `\n` 会导致 prompt 被"输入"但任务永不开始执行
/// - Linux / macOS：`\n`（LF）为传统终端提交符（Unix pty 对 `\r` 经 ICRNL 同样兼容）
///
/// 平台由前端在插件激活时通过 `@tauri-apps/plugin-os` 读取并调用
/// `auto-task.set-platform` 上报到插件存储；未上报 / 未知平台回退 `\r`
/// （Windows 必需，Unix 兼容，两端安全）。
fn input_submit_char(host: &WasmHost) -> &'static str {
    let platform = host
        .storage_get("platform")
        .ok()
        .flatten()
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    match platform.as_deref() {
        Some("windows") => "\r",
        Some("linux") | Some("macos") => "\n",
        _ => "\r",
    }
}

/// 任务队列表建表 SQL（按语句拆分）
///
/// 宿主 `plugin_db_execute` 为 rusqlite 单语句版本（后续语句被静默忽略），
/// 多语句 schema 必须拆分，否则 CREATE INDEX 永远不会执行
pub const TASK_QUEUE_SCHEMA: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS task_queue (
    id                TEXT PRIMARY KEY,
    session_id        TEXT NOT NULL,
    prompt            TEXT NOT NULL,
    position          INTEGER NOT NULL,
    status            TEXT NOT NULL DEFAULT 'pending',
    dispatch_attempts INTEGER NOT NULL DEFAULT 0,
    source            TEXT NOT NULL DEFAULT 'queue',
    clear_due_at      TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
)"#,
    "CREATE INDEX IF NOT EXISTS idx_task_queue_session ON task_queue(session_id, status, position)",
];

// ==================== Queue Operations ====================

/// 添加任务到队列末尾（来源 queue：用户在 UI 手动添加）
///
/// 返回 (task_id, position)
pub fn add_task(host: &WasmHost, session_id: &str, prompt: &str) -> (String, i64) {
    add_task_with_source(host, session_id, prompt, "queue")
}

/// 添加任务到队列末尾（带来源标记：queue / scheduled）
///
/// source 随调度写入 task_history.source，区分手动队列任务与定时任务。
/// 返回 (task_id, position)
pub fn add_task_with_source(
    host: &WasmHost,
    session_id: &str,
    prompt: &str,
    source: &str,
) -> (String, i64) {
    // 查询当前最大 position
    let max_pos = get_max_position(host, session_id);
    let position = max_pos + 1;

    // 生成 ID 并插入
    // 宿主 plugin_db_query 返回的是对象行数组（[{"col": value}]），按列名取值；
    // 不要写成行内数组（row.as_array()），否则解析失败会落入下方回退分支
    let id_sql = "SELECT lower(hex(randomblob(16))) AS id";
    let id = host
        .plugin_db_query(id_sql)
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|row| {
            row.get("id")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        })
        .unwrap_or_else(|| {
            // wasm32-unknown-unknown 无系统时钟，SystemTime::now() 会 panic（unreachable trap）；
            // 回退用会话+位置组合，天然唯一且无时间依赖
            format!("fallback-{}-{}", session_id, position)
        });

    let _ = host.plugin_db_execute_params(
        "INSERT INTO task_queue (id, session_id, prompt, position, status, source, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, 'pending', ?5, datetime('now'), datetime('now'))",
        &sql_params![id, session_id, prompt, position, source],
    );

    host.log_info(&format!(
        "Task queued: id={} session_id={} position={}",
        id, session_id, position
    ));

    (id, position)
}

/// 从队列删除指定任务，并重排剩余任务的 position
pub fn remove_task(host: &WasmHost, session_id: &str, task_id: &str) -> bool {
    // 先删除
    let affected = host
        .plugin_db_execute_params(
            "DELETE FROM task_queue WHERE id = ?1 AND session_id = ?2",
            &sql_params![task_id, session_id],
        )
        .unwrap_or(-1);
    if affected == 0 {
        return false;
    }

    // 重排 position：按创建时间重新编号
    reorder_positions(host, session_id);

    host.log_info(&format!(
        "Task removed: id={} session_id={}",
        task_id, session_id
    ));
    true
}

/// 查询指定会话的待执行队列
pub fn list_queue(host: &WasmHost, session_id: &str) -> Vec<Value> {
    host.plugin_db_query_params(
        "SELECT id, prompt, position, status, source, created_at FROM task_queue \
         WHERE session_id = ?1 AND status = 'pending' \
         ORDER BY position ASC",
        &sql_params![session_id],
    )
    .ok()
    .flatten()
    .and_then(|v| v.as_array().cloned())
    .unwrap_or_default()
}

/// 清空指定会话的所有 pending 任务
pub fn clear_queue(host: &WasmHost, session_id: &str) -> i32 {
    host.plugin_db_execute_params(
        "DELETE FROM task_queue WHERE session_id = ?1 AND status = 'pending'",
        &sql_params![session_id],
    )
    .unwrap_or(-1)
}

/// 编辑待执行任务的 prompt 内容（仅 pending 状态可改）
///
/// 返回是否找到并更新成功。已出队/已执行的任务不可编辑。
pub fn update_task(host: &WasmHost, session_id: &str, task_id: &str, prompt: &str) -> bool {
    let affected = host
        .plugin_db_execute_params(
            "UPDATE task_queue SET prompt = ?1, updated_at = datetime('now') \
             WHERE id = ?2 AND session_id = ?3 AND status = 'pending'",
            &sql_params![prompt, task_id, session_id],
        )
        .unwrap_or(-1);
    if affected > 0 {
        host.log_info(&format!(
            "Task updated: id={} session_id={}",
            task_id, session_id
        ));
        true
    } else {
        false
    }
}

/// 按给定顺序重排待执行任务的 position
///
/// ordered_ids 必须是该会话全部 pending 任务的 id 集合（顺序可任意），
/// 数量与 id 集合不一致时拒绝执行，避免与并发修改产生数据不一致。
pub fn reorder_queue(host: &WasmHost, session_id: &str, ordered_ids: &[String]) -> bool {
    // 取当前全部 pending 任务
    let tasks = list_queue(host, session_id);
    if tasks.len() != ordered_ids.len() {
        host.log_warn(&format!(
            "reorder_queue: id count mismatch session_id={} current={} given={}",
            session_id,
            tasks.len(),
            ordered_ids.len()
        ));
        return false;
    }

    // 校验 id 集合一致（顺序不限，逐一出列检查）
    let mut remaining_ids: Vec<&str> = tasks
        .iter()
        .filter_map(|t| t.get("id").and_then(|v| v.as_str()))
        .collect();
    for id in ordered_ids {
        match remaining_ids.iter().position(|c| c == id) {
            Some(idx) => {
                remaining_ids.remove(idx);
            }
            None => {
                host.log_warn(&format!("reorder_queue: unknown task id={}", id));
                return false;
            }
        }
    }
    if !remaining_ids.is_empty() {
        host.log_warn("reorder_queue: ordered_ids missing some pending tasks");
        return false;
    }

    // 按新顺序重写 position（每行独立更新，失败即中止并记录日志，不做静默忽略）
    for (idx, id) in ordered_ids.iter().enumerate() {
        let affected = host
            .plugin_db_execute_params(
                "UPDATE task_queue SET position = ?1, updated_at = datetime('now') \
                 WHERE id = ?2 AND session_id = ?3",
                &sql_params![idx as i64, id, session_id],
            )
            .unwrap_or(-1);
        if affected <= 0 {
            host.log_warn(&format!(
                "reorder_queue: failed to set position for id={}",
                id
            ));
            return false;
        }
    }

    host.log_info(&format!(
        "reorder_queue: session_id={} reordered {} tasks",
        session_id,
        ordered_ids.len()
    ));
    true
}

/// 统计指定会话的 pending 任务数量
pub fn pending_count(host: &WasmHost, session_id: &str) -> i64 {
    host.plugin_db_query_params(
        "SELECT COUNT(*) as cnt FROM task_queue WHERE session_id = ?1 AND status = 'pending'",
        &sql_params![session_id],
    )
    .ok()
    .flatten()
    .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
    .and_then(|row| row.get("cnt").cloned())
    .and_then(|v| v.as_i64())
    .unwrap_or(0)
}

// ==================== Dispatch Logic ====================

/// 尝试调度下一个任务
///
/// 触发时机：任务终态推送（completed/interrupted）、队列从空变非空。
/// 调度策略（ADR-0004 上下文清理语义）：
/// - 会话无终态任务记录（全新会话）或 agent 无清理命令 → 跳过 clear 直接下发
/// - 会话已有上下文 → 置 waiting + 登记延迟 clear（CLEAR_DELAY_SECONDS 后由
///   send_due_clears 实际发送），等新会话 idle 推送后再下发（见 on_session_idle）
pub fn try_dispatch_next(host: &WasmHost, session_id: &str) {
    host.log_debug(&format!("try_dispatch_next: session_id={}", session_id));

    // 自动执行关闭时不调度：仅入队等待，用户开启 auto_execute 后统一执行
    // （手动模式用于先添加多个任务再一起执行；开启瞬间由 set_auto_mode 触发本入口）
    if !crate::state::auto_execute_on(host, session_id) {
        host.log_debug(&format!(
            "try_dispatch_next: auto_execute off, hold dispatch for session_id={}",
            session_id
        ));
        return;
    }

    // 终态到达：上一轮下发的 executing 项归档为 done
    // （状态机把 done 延后到任务真正完成时，使队列视图能反映执行中的任务）
    let _ = host.plugin_db_execute_params(
        "UPDATE task_queue SET status = 'done', updated_at = datetime('now') \
         WHERE session_id = ?1 AND status = 'executing'",
        &sql_params![session_id],
    );

    // 先处理超时的 waiting 项（重试或取消），避免卡住后续调度
    check_waiting_timeouts(host, session_id);

    // 仍有 waiting 项（clear 已发、新会话尚未就绪）时不重复调度
    if find_waiting_task(host, session_id).is_some() {
        host.log_debug(&format!(
            "try_dispatch_next: session_id={} has waiting task, hold dispatch",
            session_id
        ));
        return;
    }

    let queue = list_queue(host, session_id);
    if queue.is_empty() {
        host.log_debug(&format!(
            "try_dispatch_next: no pending tasks for session_id={}",
            session_id
        ));
        return;
    }

    let first = match queue.first() {
        Some(t) => t,
        None => return,
    };
    let task_id = first
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let prompt = first
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let source = first
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("queue")
        .to_string();

    if prompt.is_empty() {
        host.log_warn(&format!(
            "try_dispatch_next: empty prompt for task_id={}",
            task_id
        ));
        return;
    }

    // 确认会话仍在运行
    if host.session_get(session_id).ok().flatten().is_none() {
        host.log_warn(&format!(
            "try_dispatch_next: session {} not found or not running",
            session_id
        ));
        return;
    }

    let agent_name = crate::state::session_agent(host, session_id);
    let clear_command = agent::clear_command_for(agent_name);

    // 全新会话（无终态任务记录）或 agent 未适配清理命令 → 跳过 clear 直接下发
    if clear_command.is_none() || !crate::state::has_terminal_task(host, session_id) {
        dispatch_task(host, session_id, &task_id, &prompt, agent_name, &source);
        return;
    }

    // 有上下文：置 waiting 并登记延迟发送时间（clear_due_at = now + 2s）。
    // 实际发送由 scheduler-tick 的 send_due_clears 在到点后执行：
    // 立即发送会让终端 UI 来不及渲染上一任务的输出，产生"任务未完成就被清屏"的错觉
    let clear_due = format!("datetime('now', '+{} seconds')", CLEAR_DELAY_SECONDS);
    let _ = host.plugin_db_execute_params(
        &format!(
            "UPDATE task_queue SET status = 'waiting', dispatch_attempts = 1, \
             clear_due_at = {}, updated_at = datetime('now') WHERE id = ?1",
            clear_due
        ),
        &sql_params![task_id],
    );
    host.log_info(&format!(
        "try_dispatch_next: task_id={} entering waiting, clear scheduled in {}s for session_id={}",
        task_id, CLEAR_DELAY_SECONDS, session_id
    ));
}

/// 新会话就绪回调（SessionStart → idle 推送时由 state.rs 调用）
///
/// 有 waiting 项且 clear 已实际发送（clear_due_at 已置空）→ 立即下发；
/// clear 仍在延迟窗口内 → 等 send_due_clears 到点发送后，Claude 重建会话的
/// 下一次 idle 再下发。无 waiting 项但有 pending 项 → 走常规调度
/// （定时任务新建会话入队后首次就绪走此路径）。
/// 队列无任务时不做任何事（避免普通用户会话每次 SessionStart 都广播自动模式变更）。
pub fn on_session_idle(host: &WasmHost, session_id: &str) {
    // 超时检查：若 waiting 已超时，先重试/取消再决定是否下发
    check_waiting_timeouts(host, session_id);

    let has_work =
        !list_queue(host, session_id).is_empty() || find_waiting_task(host, session_id).is_some();
    if !has_work {
        return;
    }

    if let Some(waiting) = find_waiting_task(host, session_id) {
        let task_id = waiting
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let prompt = waiting
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let source = waiting
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("queue")
            .to_string();
        if task_id.is_empty() || prompt.is_empty() {
            host.log_warn(&format!(
                "on_session_idle: malformed waiting task for session_id={}",
                session_id
            ));
            return;
        }

        // clear 仍在延迟窗口内（尚未实际发送）时不下发：
        // 等 send_due_clears 到点发送 clear、Claude 重建会话后的 idle 再调度，
        // 否则会在上一任务上下文未清理的情况下直接投递 prompt
        let clear_pending = waiting
            .get("clear_due_at")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        if clear_pending {
            host.log_debug(&format!(
                "on_session_idle: task_id={} clear still pending, hold dispatch",
                task_id
            ));
            return;
        }

        let agent_name = crate::state::session_agent(host, session_id);
        dispatch_task(host, session_id, &task_id, &prompt, agent_name, &source);
    } else {
        // 有 pending 但无 waiting：首次调度（如定时任务新建会话后的首轮出队）
        try_dispatch_next(host, session_id);
    }
}

/// 下发任务：置 executing → 写任务行（source 随队列项）→ 发送 prompt
///
/// 顺序约束：先写任务行再 terminal_send —— 输入监听（on_input_submitted）
/// 依赖 has_active_task 跳过插件自身投递的输入行，任务行必须先落库
fn dispatch_task(
    host: &WasmHost,
    session_id: &str,
    task_id: &str,
    prompt: &str,
    agent_name: &str,
    source: &str,
) {
    let _ = host.plugin_db_execute_params(
        "UPDATE task_queue SET status = 'executing', updated_at = datetime('now') WHERE id = ?1",
        &sql_params![task_id],
    );

    // 出队直接写任务行（description=prompt、source 随队列项），不再依赖输入行重建
    crate::state::create_task_from_dispatch(host, session_id, prompt, agent_name, source);

    // 投递输入必须以提交符结尾（按宿主平台动态选择，见 input_submit_char）：
    // PTY 写入原样透传（宿主不会自动补提交符，见 SessionManager::write_input）。
    // Windows ConPTY 下 Claude Code 只把 \r 识别为提交，\n 仅是换行内容；
    // Linux 下 \n 为传统提交符。prompt 统一去尾部空白后拼提交符，避免重复换行。
    // 行重建（input_line.rs）对 \r 与 \n 均视为提交，插件自身的输入监听跳过逻辑不受影响。
    let input_line = format!("{}{}", prompt.trim_end(), input_submit_char(host));
    if let Err(e) = host.terminal_send(session_id, &input_line) {
        host.log_error(&format!(
            "dispatch_task: terminal_send failed: task_id={} err={}",
            task_id, e
        ));
        // 发送失败：任务行已写入，标为中断避免假 in_progress 悬挂
        mark_latest_task_interrupted(host, session_id, "terminal_send failed on dispatch");
        let _ = host.plugin_db_execute_params(
            "UPDATE task_queue SET status = 'done', updated_at = datetime('now') WHERE id = ?1",
            &sql_params![task_id],
        );
        return;
    }

    host.log_info(&format!(
        "dispatch_task: dispatched task_id={} prompt_len={} session_id={}",
        task_id,
        prompt.len(),
        session_id
    ));

    // 队列项执行中：任务终态推送到达后由 try_dispatch_next 置 done 并继续出队。
    // executing 状态用于区分"已下发未完成"与"已完成"，避免重复下发。
    let remaining = pending_count(host, session_id);
    broadcast_queue_changed(host, session_id, remaining, "dequeue");
}

/// 将指定会话最新任务行标为 interrupted（调度失败兑底）
fn mark_latest_task_interrupted(host: &WasmHost, session_id: &str, reason: &str) {
    let _ = host.plugin_db_execute_params(
        "UPDATE task_history SET status = 'interrupted', exit_reason = ?1, completed_at = datetime('now'), updated_at = datetime('now') \
         WHERE id = (SELECT id FROM task_history WHERE session_id = ?2 ORDER BY created_at DESC LIMIT 1)",
        &sql_params![reason, session_id],
    );
}

/// 查询会话的 waiting 态队列项（最多一项）
fn find_waiting_task(host: &WasmHost, session_id: &str) -> Option<Value> {
    let result = host
        .plugin_db_query_params(
            "SELECT id, prompt, source, dispatch_attempts, clear_due_at FROM task_queue \
             WHERE session_id = ?1 AND status = 'waiting' \
             ORDER BY position ASC LIMIT 1",
            &sql_params![session_id],
        )
        .ok()
        .flatten()?;
    result.as_array()?.first().cloned()
}

/// 到点发送等待中的延迟 clear 命令（scheduler-tick 周期调用）
///
/// try_dispatch_next 置 waiting 时只登记 clear_due_at（now + 2s，见
/// CLEAR_DELAY_SECONDS），实际写入终端的 clear 由本函数在到点后发送：
/// 给终端 UI 留出渲染上一任务输出的时间，避免"任务未执行完就被清屏"的错觉。
/// 发送成功把 clear_due_at 置空（on_session_idle 据此放行下发）并重置
/// updated_at（超时计时从 clear 实际发送时刻起算）；失败回退 pending。
pub fn send_due_clears(host: &WasmHost, now_utc: &str) {
    let due = host
        .plugin_db_query_params(
            "SELECT id, session_id FROM task_queue \
             WHERE status = 'waiting' AND clear_due_at IS NOT NULL AND clear_due_at <= ?1",
            &sql_params![now_utc],
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    for row in due {
        let task_id = row
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let session_id = row
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if task_id.is_empty() || session_id.is_empty() {
            continue;
        }

        let clear_command =
            agent::clear_command_for(crate::state::session_agent(host, &session_id))
                .unwrap_or("/clear");
        if let Err(e) = host.terminal_send(
            &session_id,
            &format!("{}{}", clear_command, input_submit_char(host)),
        ) {
            host.log_error(&format!(
                "send_due_clears: terminal_send clear failed: task_id={} err={}",
                task_id, e
            ));
            // clear 发送失败：回退 pending，下次终态触发时重试调度
            let _ = host.plugin_db_execute_params(
                "UPDATE task_queue SET status = 'pending', clear_due_at = NULL, updated_at = datetime('now') WHERE id = ?1",
                &sql_params![task_id],
            );
            continue;
        }
        // 发送成功：清空 clear_due_at 标记已发送，重置 updated_at 使超时计时
        // 从 clear 实际发送时刻起算
        let _ = host.plugin_db_execute_params(
            "UPDATE task_queue SET clear_due_at = NULL, updated_at = datetime('now') WHERE id = ?1",
            &sql_params![task_id],
        );
        host.log_info(&format!(
            "send_due_clears: clear sent for task_id={} session_id={}",
            task_id, session_id
        ));
    }
}

/// 检查 waiting 态超时项（调度入口幂等调用）
///
/// WASM 无系统时钟，超时判断全部由 SQLite 宿侧时间计算：
/// updated_at 距当前超过 WAITING_TIMEOUT_SECONDS 视为超时。
/// 未达最大重试次数 → 重新登记延迟 clear（与首次一致，同样延迟
/// CLEAR_DELAY_SECONDS，由 send_due_clears 到点发送）；否则置 cancelled 并广播。
fn check_waiting_timeouts(host: &WasmHost, session_id: &str) {
    let overdue = host
        .plugin_db_query_params(
            &format!(
                "SELECT id, dispatch_attempts FROM task_queue \
                 WHERE session_id = ?1 AND status = 'waiting' \
                 AND (strftime('%s', 'now') - strftime('%s', updated_at)) > {}",
                WAITING_TIMEOUT_SECONDS
            ),
            &sql_params![session_id],
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    for row in overdue {
        let task_id = row
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let attempts = row
            .get("dispatch_attempts")
            .and_then(|v| v.as_i64())
            .unwrap_or(1);
        if task_id.is_empty() {
            continue;
        }

        if attempts < MAX_DISPATCH_ATTEMPTS {
            // 重试一次 clear：重新登记延迟发送并重置计时，等待下一轮 idle/超时判定
            let clear_due = format!("datetime('now', '+{} seconds')", CLEAR_DELAY_SECONDS);
            let _ = host.plugin_db_execute_params(
                &format!(
                    "UPDATE task_queue SET dispatch_attempts = ?1, clear_due_at = {}, updated_at = datetime('now') WHERE id = ?2",
                    clear_due
                ),
                &sql_params![attempts + 1, task_id],
            );
            host.log_warn(&format!(
                "check_waiting_timeouts: waiting timeout, clear retry scheduled for task_id={} session_id={}",
                task_id, session_id
            ));
        } else {
            // 重试耗尽：取消任务并通知，后续 pending 由下一次终态/idle 触发调度
            let _ = host.plugin_db_execute_params(
                "UPDATE task_queue SET status = 'cancelled', updated_at = datetime('now') WHERE id = ?1",
                &sql_params![task_id],
            );
            host.log_warn(&format!(
                "check_waiting_timeouts: waiting timeout after {} attempts, cancelled task_id={} session_id={}",
                attempts, task_id, session_id
            ));
            let remaining = pending_count(host, session_id);
            broadcast_queue_changed(host, session_id, remaining, "cancel");
        }
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
/// - POST task-queue/update → 更新任务内容
/// - POST task-queue/reorder → 重排序队列
pub fn handle_queue_http(
    host: &WasmHost,
    method: &str,
    path: &str,
    body: &Value,
    query: &Value,
) -> Value {
    host.log_debug(&format!("handle_queue_http: {} {}", method, path));

    match (method, path) {
        ("POST", "add") => handle_add(host, body, query),
        ("DELETE", "remove") => handle_remove(host, body, query),
        ("GET", "list") => handle_list(host, query),
        ("POST", "clear") => handle_clear(host, body, query),
        ("POST", "update") => handle_update(host, body, query),
        ("POST", "reorder") => handle_reorder(host, body, query),
        _ => {
            host.log_warn(&format!("Unknown queue endpoint: {} {}", method, path));
            http_response::error(404, &format!("Not found: {} {}", method, path))
        }
    }
}

// ==================== HTTP Handler Implementations ====================

/// POST task-queue/add
fn handle_add(host: &WasmHost, body: &Value, _query: &Value) -> Value {
    let session_id = body
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let prompt = body.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

    if session_id.is_empty() {
        return http_response::error(400, "Missing session_id");
    }
    if prompt.is_empty() {
        return http_response::error(400, "Missing prompt");
    }

    // 入队（count_before 用于判断是否触发首次调度，此处仅保留日志语义）
    let (task_id, position) = add_task(host, session_id, prompt);

    // 广播队列变更
    let count_after = pending_count(host, session_id);
    broadcast_queue_changed(host, session_id, count_after, "add");

    // 自动执行开启且会话空闲时立即调度；关闭时仅入队（与 auto-task.add-task 命令一致）
    if crate::state::auto_execute_on(host, session_id)
        && !crate::state::has_active_task(host, session_id)
    {
        try_dispatch_next(host, session_id);
    }

    http_response::ok_with_data(serde_json::json!({
        "task_id": task_id,
        "position": position,
    }))
}

/// DELETE task-queue/remove
fn handle_remove(host: &WasmHost, body: &Value, _query: &Value) -> Value {
    let session_id = body
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let task_id = body.get("task_id").and_then(|v| v.as_str()).unwrap_or("");

    if session_id.is_empty() {
        return http_response::error(400, "Missing session_id");
    }
    if task_id.is_empty() {
        return http_response::error(400, "Missing task_id");
    }

    let removed = remove_task(host, session_id, task_id);
    if !removed {
        return http_response::error(404, "Task not found");
    }

    // 删除后广播队列变更
    let remaining = pending_count(host, session_id);
    broadcast_queue_changed(host, session_id, remaining, "remove");

    http_response::ok()
}

/// GET task-queue/list
fn handle_list(host: &WasmHost, query: &Value) -> Value {
    let session_id = query
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if session_id.is_empty() {
        return http_response::error(400, "Missing session_id");
    }

    let tasks = list_queue(host, session_id);
    let queue_count = tasks.len() as i64;

    http_response::ok_with_data(serde_json::json!({
        "session_id": session_id,
        "tasks": tasks,
        "queue_count": queue_count,
    }))
}

/// POST task-queue/clear
fn handle_clear(host: &WasmHost, body: &Value, _query: &Value) -> Value {
    let session_id = body
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if session_id.is_empty() {
        return http_response::error(400, "Missing session_id");
    }

    clear_queue(host, session_id);
    broadcast_queue_changed(host, session_id, 0, "clear");

    http_response::ok()
}

/// POST task-queue/update — 更新任务内容
fn handle_update(host: &WasmHost, body: &Value, _query: &Value) -> Value {
    let session_id = body
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let task_id = body.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
    let prompt = body.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

    if session_id.is_empty() {
        return http_response::error(400, "Missing session_id");
    }
    if task_id.is_empty() {
        return http_response::error(400, "Missing task_id");
    }
    if prompt.is_empty() {
        return http_response::error(400, "Missing prompt");
    }

    let updated = update_task(host, session_id, task_id, prompt);
    if !updated {
        return http_response::error(404, "Task not found or not pending");
    }

    let remaining = pending_count(host, session_id);
    broadcast_queue_changed(host, session_id, remaining, "update");

    http_response::ok()
}

/// POST task-queue/reorder — 重排序队列
fn handle_reorder(host: &WasmHost, body: &Value, _query: &Value) -> Value {
    let session_id = body
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let task_ids: Vec<String> = body
        .get("task_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    if session_id.is_empty() {
        return http_response::error(400, "Missing session_id");
    }
    if task_ids.is_empty() {
        return http_response::error(400, "Missing task_ids");
    }

    let reordered = reorder_queue(host, session_id, &task_ids);
    if !reordered {
        return http_response::error(400, "Task ID set mismatch");
    }

    let remaining = pending_count(host, session_id);
    broadcast_queue_changed(host, session_id, remaining, "reorder");

    http_response::ok()
}

// ==================== Internal Helpers ====================

/// 查询指定会话的最大 position
fn get_max_position(host: &WasmHost, session_id: &str) -> i64 {
    host.plugin_db_query_params(
        "SELECT MAX(position) as max_pos FROM task_queue WHERE session_id = ?1 AND status = 'pending'",
        &sql_params![session_id],
    )
    .ok()
    .flatten()
    .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
    .and_then(|row| row.get("max_pos").cloned())
    .and_then(|v| v.as_i64())
    .unwrap_or(-1)
}

/// 重排指定会话的 pending 任务 position（填补删除后的空缺）
fn reorder_positions(host: &WasmHost, session_id: &str) {
    let tasks = host
        .plugin_db_query_params(
            "SELECT id FROM task_queue WHERE session_id = ?1 AND status = 'pending' ORDER BY position ASC, created_at ASC",
            &sql_params![session_id],
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    for (idx, task) in tasks.iter().enumerate() {
        let id = task.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let _ = host.plugin_db_execute_params(
            "UPDATE task_queue SET position = ?1, updated_at = datetime('now') WHERE id = ?2",
            &sql_params![idx as i64, id],
        );
    }
}

/// 广播队列变更事件
pub fn broadcast_queue_changed(host: &WasmHost, session_id: &str, queue_count: i64, action: &str) {
    host.broadcast_sync(&SyncEvent::TaskQueueChanged {
        session_id: session_id.to_string(),
        queue_count,
        action: action.to_string(),
    });

    let _ = host.bus_publish(
        EVENT_TASK_QUEUE_CHANGED,
        &serde_json::json!({
            "session_id": session_id,
            "queue_count": queue_count,
            "action": action,
        }),
    );
    // 通知前端 UI 实时刷新（事件名与前端 context.events.on 监听一致）
    host.emit_event(
        EVENT_TASK_QUEUE_CHANGED,
        &serde_json::json!({
            "session_id": session_id,
            "queue_count": queue_count,
            "action": action,
        }),
    );
}
