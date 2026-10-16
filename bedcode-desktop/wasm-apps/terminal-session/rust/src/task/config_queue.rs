//! 任务队列（按配置分组 · 未绑定会话）域
//!
//! 「创建新的 {配置} 会话」的落点（2026-10-03 语义变更）：任务先进入按配置
//! 分组的**任务队列**，**不立即创建会话**；队列可被「启动」（手动）或
//! 「自动模式」（开启且已有任务）触发，届时才按配置新建会话并把整列任务
//! 迁移过去执行——等价「一批任务在全新会话中依次执行」。
//!
//! 与既有队列机制的键关系：任务以**队列 id 作为 `task_queue.session_id` 占位键**
//! 挂账（`list_queue` / `pending_count` / `add_task_with_source` 全部按 session_id
//! 过滤，传入队列 id 即命中，零改动复用）；「启动」时将 pending 行迁移到真实
//! 会话 id（[`crate::task::queue::rotate_session_for_next_task`] 的同款迁移 UPDATE）。
//!
//! 生命周期：`ready`（未绑定会话，任务可继续并入）→ `start` → `active`（会话已
//! 建，队列整体移交会话内队列机制）。任务队列区只展示 `ready`；`active` 队列的
//! 任务在「执行任务」区按会话展示。启动后同配置的新增任务**不再回到该队列**
//! （新批次 = 又一枚新建会话），符合「创建新的 {配置} 会话」语义。
//!
//! 自动模式（`auto_mode`）：
//! - `ready` 态可切换；**开启且队列已有任务 → 立即自动启动**（建会话并入执行，
//!   无人值守语义，与定时任务一致）；开启时队列为空只记录标志，来任务时自动启动。
//! - 启动后映射为会话 `auto_execute` 开关（会话卡片上的切换即队列级「自动模式」）。
//! - 任务间隔离沿用会话轮换机制（`rotate_session_for_next_task`）：上一任务终态后
//!   关闭旧会话、同配置新建再执行下一任务。
//!
//! 会话关闭与前端窗口联动：轮换关闭旧会话走 `session::close_via_pty` → pty 退出
//! 发布 `session:stopped`；桌面终端窗口（TerminalWindowView）订阅该事件匹配关闭
//! 自己（见该组件 onSessionShutdown 逻辑）。
//!
//! 广播：本域事件 `task:config-queue-changed` 仅经 emit_event 发桌面前端（无移动
//! 端消费者，不进 bus/WS，与 preset.rs 的桌面-only 广播同一口径）；会话 mode
//! 变更与队列变更沿用既有收口（session-mode 三连广播 / broadcast_queue_changed）。

use bedcode_plugin_api::host::{HostBus, HostEvents, HostLog, HostPluginDatabase};
use bedcode_plugin_api::sql_params;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::Value;

/// `task:config-queue-changed`（桌面任务队列区刷新通道；payload 带 queue_id/action）
pub const EVENT_CONFIG_QUEUE_CHANGED: &str = "task:config-queue-changed";

/// 建表语句（幂等，注册进 `task::ensure_schema_via_host`）
pub const CONFIG_QUEUE_SCHEMA: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS task_config_queues (
    id          TEXT PRIMARY KEY,
    config_id   TEXT NOT NULL,
    session_id  TEXT,
    auto_mode   INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL DEFAULT 'ready',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
)"#,
    "CREATE INDEX IF NOT EXISTS idx_task_config_queues_config ON task_config_queues(config_id, status)",
];

/// 桌面任务队列区事件（query row serde 友好：全部字符串/整数标量）
fn emit_changed(host: &WasmHost, queue_id: &str, action: &str) {
    host.emit_event(
        EVENT_CONFIG_QUEUE_CHANGED,
        &serde_json::json!({ "queue_id": queue_id, "action": action }),
    );
}

/// 读取队列行（不存在 → Ok(None)）
fn row(host: &WasmHost, queue_id: &str) -> Result<Option<Value>, String> {
    host.plugin_db_query_params(
        "SELECT id, config_id, session_id, auto_mode, status FROM task_config_queues WHERE id = ?1",
        &sql_params![queue_id],
    )
    .map_err(|e| format!("config-queue row query failed: {}", e.message))
    .map(|v| {
        v.and_then(|arr| arr.as_array().cloned())
            .and_then(|mut a| a.pop())
    })
}

/// 进程内递增计数器：randomblob 查询失败时的 fallback 唯一化
/// （WASM 无系统时钟不能用时间派生；静态计数保证单进程内主键不冲突）
static FALLBACK_QUEUE_ID_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// 生成队列 id（同 task_queue 的 id 生成，WASM 无系统时钟 → 纯随机 hex）
fn new_queue_id(host: &WasmHost) -> String {
    host.plugin_db_query("SELECT lower(hex(randomblob(16))) AS id")
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|row| {
            row.get("id")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        })
        .unwrap_or_else(|| {
            // 恒定 fallback 会让并发/连续 fallback 撞主键，改用递增计数保证唯一
            format!(
                "fallback-queue-{}",
                FALLBACK_QUEUE_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            )
        })
}

/// 配置存在 + agent 适配守卫（前端只列适配配置；此处防竞态/陈旧选择——
/// 不支持的 agent 会话永远无法调度下发）。读失败显性报错不静默降级。
fn require_supported_config(host: &WasmHost, config_id: &str) -> Result<(), String> {
    let command: String = host
        .plugin_db_query_params(
            "SELECT command FROM session_configs WHERE id = ?1",
            &sql_params![config_id],
        )
        .map_err(|e| format!("create-and-enqueue: config read failed: {}", e.message))?
        .and_then(|v| v.as_array().cloned())
        .and_then(|mut a| a.pop())
        .and_then(|row| {
            row.get("command")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        })
        .ok_or_else(|| format!("create-and-enqueue: 会话配置不存在：{config_id}"))?;
    if !crate::task::agent::is_supported(crate::task::agent::detect_agent(command.as_str())) {
        return Err(format!(
            "create-and-enqueue: 配置 {config_id} 的 agent 未适配自动任务（仅 claude / codex / opencode / pi）"
        ));
    }
    Ok(())
}

/// 「创建新的 {配置} 会话」入队：进按配置分组的任务队列（**不立即建会话**）
///
/// - 该配置已有 `ready` 队列（未绑定会话）→ 并入（同一枚未来新会话批次）；
/// - 无 → 新建队列再入队；
/// - 队列处于 `auto_mode` → 入队即自动启动（建会话执行，失败只记日志——
///   任务已安全在队，用户仍可手动「启动」重试）。
///
/// 返回 `(queue_id, task_id, position)`。
pub fn create_or_enqueue(
    host: &WasmHost,
    config_id: &str,
    prompt: &str,
) -> Result<(String, String, i64), String> {
    require_supported_config(host, config_id)?;

    let queue_id = match host
        .plugin_db_query_params(
            "SELECT id FROM task_config_queues WHERE config_id = ?1 AND status = 'ready' \
             ORDER BY created_at DESC LIMIT 1",
            &sql_params![config_id],
        )
        .map_err(|e| format!("create-and-enqueue: queue lookup failed: {}", e.message))?
        .and_then(|v| v.as_array().cloned())
        .and_then(|mut a| a.pop())
        .and_then(|row| {
            row.get("id")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        }) {
        Some(id) => id,
        None => {
            let id = new_queue_id(host);
            host.plugin_db_execute_params(
                "INSERT INTO task_config_queues \
                 (id, config_id, auto_mode, status, created_at, updated_at) \
                 VALUES (?1, ?2, 0, 'ready', datetime('now'), datetime('now'))",
                &sql_params![id, config_id],
            )
            .map_err(|e| format!("create-and-enqueue: queue create failed: {}", e.message))?;
            id
        }
    };

    // 入队（占位键 = 队列 id；source='queue' 与手动入队同源）
    let (task_id, position) =
        crate::task::queue::add_task_with_source(host, &queue_id, prompt, "queue")
            .map_err(|e| format!("create-and-enqueue: enqueue failed: {e}"))?;

    // 自动模式：入队即自动启动（失败不阻断入队主契约——任务已安全在队）
    if queue_auto_mode(host, &queue_id) {
        if let Err(e) = start(host, &queue_id) {
            host.log_error(&format!(
                "create-and-enqueue: auto-start failed (queue_id={}): {}",
                queue_id, e
            ));
        }
    }

    emit_changed(host, &queue_id, "enqueue");
    Ok((queue_id, task_id, position))
}

/// 队列当前 auto_mode（缺行 = false）
fn queue_auto_mode(host: &WasmHost, queue_id: &str) -> bool {
    host.plugin_db_query_params(
        "SELECT auto_mode FROM task_config_queues WHERE id = ?1",
        &sql_params![queue_id],
    )
    .ok()
    .flatten()
    .and_then(|v| v.as_array().cloned())
    .and_then(|mut a| a.pop())
    .and_then(|row| row.get("auto_mode").and_then(|v| v.as_i64()))
    .map(|m| m != 0)
    .unwrap_or(false)
}

/// 任务队列区列表（只列 `ready`：未绑定会话的批次）
///
/// 每项：`{id, config_id, name, workingDir, auto_mode, tasks, created_at}`
/// （config 已删除时 name/workingDir 为空，前端以配置 id 兜底展示）。
pub fn list(host: &WasmHost) -> Vec<Value> {
    let rows: Vec<Value> = host
        .plugin_db_query_params(
            "SELECT q.id, q.config_id, q.auto_mode, q.created_at, \
                    c.name AS name, c.working_dir AS working_dir \
             FROM task_config_queues q \
             LEFT JOIN session_configs c ON c.id = q.config_id \
             WHERE q.status = 'ready' ORDER BY q.created_at",
            &sql_params![],
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    rows.into_iter()
        .map(|r| {
            let queue_id = r
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mut v = r;
            // 任务列表（复用队列机制，占位键 = 队列 id）
            v["tasks"] = serde_json::json!(crate::task::queue::list_queue(host, &queue_id));
            v
        })
        .collect()
}

/// 队列级「自动模式」开关（仅 ready 队列可切；active 后的自动执行 = 会话开关）
///
/// 开启且队列已有任务 → 立即自动启动（无人值守）；空队列开启只记录标志，
/// 后续任务入队时自动启动。
pub fn set_auto_mode(host: &WasmHost, queue_id: &str, auto_mode: bool) -> Result<Value, String> {
    let current =
        row(host, queue_id)?.ok_or_else(|| format!("set-auto-mode: 队列不存在：{queue_id}"))?;
    if current.get("status").and_then(|v| v.as_str()) != Some("ready") {
        return Err(format!(
            "set-auto-mode: 队列已启动，请用会话开关（{}）",
            queue_id
        ));
    }
    host.plugin_db_execute_params(
        "UPDATE task_config_queues SET auto_mode = ?1, updated_at = datetime('now') WHERE id = ?2",
        &sql_params![auto_mode as i64, queue_id],
    )
    .map_err(|e| format!("set-auto-mode: update failed: {}", e.message))?;

    if auto_mode && crate::task::queue::pending_count(host, queue_id) > 0 {
        if let Err(e) = start(host, queue_id) {
            host.log_warn(&format!(
                "set-auto-mode: auto-start failed (queue_id={}): {}",
                queue_id, e
            ));
        }
    }
    emit_changed(host, queue_id, "auto-mode");
    Ok(serde_json::json!({ "auto_mode": auto_mode }))
}

/// 启动队列：按配置新建会话，整队任务迁移入会话语队列执行
///
/// 顺序（失败回滚不留半成品）：
/// 1. `ready → starting`（条件更新；未命中 = 已被并发/重复启动 → 幂等返回已绑会话
///    或显性报错）；
/// 2. 队列非空才启动（空队列启动 = 白建空会话）；
/// 3. `launch::create_via_host` 建会话（失败 → 回退 `ready` 供重试）；
/// 4. pending 行迁移到新会话 id（[`crate::task::queue::rotate_session_for_next_task`]
///    同款 UPDATE；队列 batch 不产生 waiting 行，只迁 pending）；
/// 5. 会话 auto_execute = 队列 auto_mode（mode 变更按票 06 口径三连广播；
///    首轮下发由新会话就绪的 idle 推送驱动，不在此立即调度——agent CLI 未就绪
///    时投递输入会丢失/重复下发，见 scheduled.rs `handle_session_created` 注释）；
/// 6. `starting → active` + 记 session_id；队列变更广播（新会话键）+ 本域事件。
///
/// 返回绑定后的会话 id。
pub fn start(host: &WasmHost, queue_id: &str) -> Result<String, String> {
    // 1. ready → starting（并发/重复启动守卫）
    let flipped = host
        .plugin_db_execute_params(
            "UPDATE task_config_queues SET status = 'starting', updated_at = datetime('now') \
             WHERE id = ?1 AND status = 'ready'",
            &sql_params![queue_id],
        )
        .map_err(|e| format!("config-queue start: status flip failed: {}", e.message))?
        > 0;
    if !flipped {
        // 幂等：已 active（绑定会话）→ 原样返回；仍在 starting（并发启动在途）→ 报错重试
        if let Some(cur) = row(host, queue_id)? {
            let status = cur.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if status == "active" {
                return cur
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .ok_or_else(|| format!("config-queue active 但缺 session_id：{queue_id}"));
            }
        }
        return Err(format!("config-queue 已启动或正在启动：{queue_id}"));
    }

    // 失败回滚：start 的后续步骤若 Err 需把 starting 退回 ready
    macro_rules! bail_ready {
        ($msg:expr) => {{
            let _ = host.plugin_db_execute_params(
                "UPDATE task_config_queues SET status = 'ready', updated_at = datetime('now') \
                 WHERE id = ?1 AND status = 'starting'",
                &sql_params![queue_id],
            );
            return Err($msg);
        }};
    }

    // 2. 队列非空
    if crate::task::queue::pending_count(host, queue_id) == 0 {
        bail_ready!(format!("config-queue 为空，无法启动：{queue_id}"));
    }

    // 3. 建会话（同源编排入口）
    let queued = row(host, queue_id)?.ok_or_else(|| format!("config-queue 不存在：{queue_id}"))?;
    let config_id = queued
        .get("config_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let auto_mode = queued
        .get("auto_mode")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        != 0;
    if config_id.is_empty() {
        bail_ready!(format!("config-queue 缺 config_id：{queue_id}"));
    }
    let created = crate::launch::create_via_host(&serde_json::json!({
        "configId": config_id,
        "start": true,
    }))
    .map_err(|e| format!("config-queue start: 会话创建失败：{e}"));
    let created = match created {
        Ok(c) => c,
        Err(e) => bail_ready!(e),
    };
    let session_id = created
        .get("sessionId")
        .and_then(|s| s.as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("config-queue start: 回执缺 sessionId：{created}"));
    let session_id = match session_id {
        Ok(s) => s,
        Err(e) => bail_ready!(e),
    };

    // 4. 迁移 pending 行到新会话（batch 不产生 waiting，只迁 pending）。
    // 迁移失败必须回滚回 ready（fail-visible）：`let _` 吞掉 Err 后任务仍挂在
    // 队列 id 占位键下，而队列已 active（ready 区不再展示）、会话区按新 session_id
    // 查不到 → 任务进入不可见、永不会再下发的悬挂态。回滚后任务仍在队列里，用户
    // 可重新「启动」；步骤 3 已建的空闲会话由既有 idle 兜底回收。
    if let Err(e) = host.plugin_db_execute_params(
        "UPDATE task_queue SET session_id = ?1, updated_at = datetime('now') \
         WHERE session_id = ?2 AND status = 'pending'",
        &sql_params![session_id, queue_id],
    ) {
        bail_ready!(format!(
            "config-queue start: 迁移 pending 任务失败（queue_id={} session_id={}）：{}",
            queue_id, session_id, e.message
        ));
    }
    let migrated = crate::task::queue::pending_count(host, &session_id);
    if migrated == 0 {
        host.log_error(&format!(
            "config-queue start: 迁移后新会话无 pending 任务（queue_id={} session_id={}），\
             任务可能被并发移除，仍继续绑定会话",
            queue_id, session_id
        ));
    }

    // 5. 会话 auto_execute = 队列 auto_mode（三连广播同票 06 口径；不立即调度）
    let (_, auto_answer) = crate::task::state::session_flags(host, &session_id);
    crate::task::state::set_session_flags(host, &session_id, Some(auto_mode), None);
    let bus_payload = crate::ws_events::session_mode_payload(&session_id, auto_answer, auto_mode);
    let _ = host.bus_publish(
        bedcode_plugin_api::constants::EVENT_SESSION_MODE_CHANGED,
        &bus_payload,
    );
    crate::ws_events::broadcast_event(
        host,
        bedcode_plugin_api::constants::EVENT_SESSION_MODE_CHANGED,
        &bus_payload,
    );
    host.emit_event(
        bedcode_plugin_api::constants::EVENT_SESSION_MODE_CHANGED,
        &serde_json::json!({
            "session_id": session_id,
            "autoApprove": auto_answer,
            "auto_answer": auto_answer,
            "autoExecute": auto_mode,
            "auto_execute": auto_mode,
        }),
    );
    // 队列变更广播（新会话键）：前端「执行任务」区即时可见
    crate::task::queue::broadcast_queue_changed(host, &session_id, migrated, "add", None, None);

    // 6. 绑定会话（starting → active）
    host.plugin_db_execute_params(
        "UPDATE task_config_queues SET status = 'active', session_id = ?1, \
         updated_at = datetime('now') WHERE id = ?2 AND status = 'starting'",
        &sql_params![session_id, queue_id],
    )
    .map_err(|e| format!("config-queue start: bind failed: {}", e.message))?;

    emit_changed(host, queue_id, "start");
    host.log_info(&format!(
        "config-queue started: queue_id={} config_id={} session_id={} auto_mode={} tasks={}",
        queue_id, config_id, session_id, auto_mode, migrated
    ));
    Ok(session_id)
}

/// 预设任务入队到指定任务队列（一次性消耗：入队后预设行删除）
///
/// 队列处于 auto_mode → 入队即自动启动（失败只记日志，任务已安全在队）。
pub fn preset_enqueue(
    host: &WasmHost,
    queue_id: &str,
    preset_id: &str,
) -> Result<(String, i64), String> {
    if row(host, queue_id)?.is_none() {
        return Err(format!("preset-enqueue-queue: 队列不存在：{queue_id}"));
    }
    let (task_id, position) = crate::task::preset::add_preset_to_queue(host, queue_id, preset_id)
        .map_err(|e| format!("preset-enqueue-queue: {e}"))?;
    if queue_auto_mode(host, queue_id) {
        if let Err(e) = start(host, queue_id) {
            host.log_error(&format!(
                "preset-enqueue-queue: auto-start failed (queue_id={}): {}",
                queue_id, e
            ));
        }
    }
    emit_changed(host, queue_id, "preset-enqueue");
    Ok((task_id, position))
}
