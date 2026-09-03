//! 调度引擎：任务/执行记录数据模型 + tick 状态机 + HTTP 端点
//!
//! 执行记录状态机（spec §6）：
//!
//! ```text
//! waiting ──(槽位空出)──▶ running ──(exit 0)──▶ succeeded
//!                        running ──(exit≠0)──▶ failed
//!                        running ──(超时 kill)─▶ timeout
//! waiting ──(应用重启)──▶ missed
//! (activate 时错过检查直接插入 missed)
//! ```
//!
//! 任务定义无生命周期状态机：持续存活（enabled 维度 + 删除），
//! 终态概念只存在于执行记录。
//!
//! 时间基准（spec §13，WASM 无系统时钟）：
//! - tick 的 `now_local` 由宿主注入（本地时间字符串，字典序即时间序）
//! - 执行记录时间戳用 SQLite `datetime('now','localtime')`（宿主 DB 计算）
//! - 完成后的 `next_at` 推进 = `next_after(schedule, 旧 next_at)` —— 纯字符串
//!   函数、不需要时钟：旧 next_at 即本次触发时刻，标准 cron 语义为从"计划
//!   时刻"推算下一计划时刻，与运行耗时无关（长跑任务不漂移）
//! - 恢复/建任务/编辑的 `next_at` 重算 = `next_after(schedule, now_local)`
//!   （"下一未来时刻"，错过补跑不做——spec §2 明确排除 catch-up）
//!
//! SQL 一律使用参数绑定（`*_params` + `?N` 占位符），无手写转义。

use bedcode_plugin_api::events::SyncEvent;
use bedcode_plugin_api::host::{
    ConfigKey, HostBus, HostConfig, HostEvents, HostLog, HostPluginDatabase, HostProcess,
    HostStorage,
};
use bedcode_plugin_api::http_response;
use bedcode_plugin_api::sql_params;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::Value;

use crate::cron;

/// 宿主定时器轮询间隔（秒）——tick 按秒级粒度匹配 next_at
pub const SCHEDULER_INTERVAL_SECS: u64 = 1;

/// 并发上限默认值（可配：storage key `max_concurrency`）
pub const DEFAULT_MAX_CONCURRENCY: i64 = 3;

/// 到期宽限（秒）：next_at 已过且超过宽限视为错过（应用当时未运行），
/// 只在 activate 恢复时检查一次（幂等，spec §5.2/§13）
const MISSED_GRACE_SECONDS: i64 = 120;

/// 执行记录保留封顶（超限删最旧，参照 file-transfer HistoryStore::trim_to_cap）
const MAX_EXECUTION_ROWS: i64 = 500;

/// 默认执行超时（秒，对应 host-process timeout_ms = 600000）
const DEFAULT_TIMEOUT_SEC: i64 = 600;

/// 事件 topic（bus + emit_event 通道）；broadcast_sync 通道复用
/// SDK `SyncEvent::TaskScheduledChanged`（线协议与移动端已兼容，避免跨项目改枚举）
const EVENT_SCHEDULER_CHANGED: &str = "scheduler:changed";

/// 执行输出目录（相对用户主目录，host-process 自动建父目录）
const OUTPUT_DIR: &str = ".bedcode/scheduler";

// ==================== Schema ====================

/// 任务定义表建表 SQL（按语句拆分：宿主 plugin_db_execute 仅执行单条语句）
pub const SCHEDULED_JOBS_SCHEMA: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS scheduled_jobs (
    id          TEXT PRIMARY KEY,          -- lower(hex(randomblob(16)))
    name        TEXT,                      -- 可选显示名
    schedule    TEXT NOT NULL,             -- cron 6 段表达式（本地时区）
    exec_type   TEXT NOT NULL,             -- 'script' | 'inline'
    exec_value  TEXT NOT NULL,             -- 脚本路径 | 内联命令字符串
    cwd         TEXT,                      -- 工作目录（缺省用户主目录）
    env         TEXT,                      -- JSON 对象 {K: V}，附加环境变量
    timeout_sec INTEGER NOT NULL DEFAULT 600,
    enabled     INTEGER NOT NULL DEFAULT 1,
    once        INTEGER NOT NULL DEFAULT 0, -- 触发成功后自动停用
    next_at     TEXT,                      -- 下次触发（本地时间字符串）
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
)"#,
    "CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_enabled_next ON scheduled_jobs(enabled, next_at)",
];

/// 执行记录表建表 SQL
///
/// run_id 为 host-process 返回的进程句柄 id：完成事件（on_process_done）
/// 以 run_id 反查执行记录回写结果；waiting/running 行为 NULL
pub const JOB_EXECUTIONS_SCHEMA: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS job_executions (
    exec_id     TEXT PRIMARY KEY,
    job_id      TEXT NOT NULL,
    status      TEXT NOT NULL,             -- waiting|running|succeeded|failed|timeout|missed
    trigger     TEXT NOT NULL,             -- 'cron' | 'manual'
    started_at  TEXT,
    finished_at TEXT,
    exit_code   INTEGER,
    output_path TEXT,                      -- stdout/stderr 落盘文件路径
    run_id      TEXT
)"#,
    "CREATE INDEX IF NOT EXISTS idx_job_executions_job ON job_executions(job_id, status)",
    "CREATE INDEX IF NOT EXISTS idx_job_executions_run ON job_executions(run_id)",
];

// ==================== 任务定义（CRUD） ====================

/// 任务定义（内部行模型）
#[derive(Debug, Clone)]
pub struct JobDef {
    pub id: String,
    pub name: Option<String>,
    pub schedule: String,
    pub exec_type: String,
    pub exec_value: String,
    pub cwd: Option<String>,
    pub env: Option<String>,
    pub timeout_sec: i64,
    pub enabled: bool,
    pub once: bool,
    pub next_at: Option<String>,
}

impl JobDef {
    fn from_row(row: &Value) -> Option<JobDef> {
        Some(JobDef {
            id: row.get("id")?.as_str()?.to_string(),
            name: row.get("name").and_then(|v| v.as_str()).map(str::to_string),
            schedule: row.get("schedule")?.as_str()?.to_string(),
            exec_type: row.get("exec_type")?.as_str()?.to_string(),
            exec_value: row.get("exec_value")?.as_str()?.to_string(),
            cwd: row.get("cwd").and_then(|v| v.as_str()).map(str::to_string),
            env: row.get("env").and_then(|v| v.as_str()).map(str::to_string),
            timeout_sec: row.get("timeout_sec").and_then(|v| v.as_i64()).unwrap_or(DEFAULT_TIMEOUT_SEC),
            enabled: row.get("enabled").and_then(|v| v.as_i64()).unwrap_or(1) != 0,
            once: row.get("once").and_then(|v| v.as_i64()).unwrap_or(0) != 0,
            next_at: row.get("next_at").and_then(|v| v.as_str()).map(str::to_string),
        })
    }
}

/// 生成任务/执行记录 ID（lower(hex(randomblob(16)))）
fn gen_id(host: &WasmHost) -> String {
    host.plugin_db_query("SELECT lower(hex(randomblob(16))) AS id")
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|row| row.get("id").and_then(|v| v.as_str().map(str::to_string)))
        .unwrap_or_default()
}

/// 宿主 DB 计算当前本地时间（spec §13：时间由宿主注入/DB 计算）
fn db_now_local(host: &WasmHost) -> Option<String> {
    host.plugin_db_query("SELECT datetime('now','localtime') AS now_local")
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|row| row.get("now_local").and_then(|v| v.as_str().map(str::to_string)))
}

/// 单行查询辅助：取结果首行
fn query_first(host: &WasmHost, sql: &str, params: &[Value]) -> Option<Value> {
    host.plugin_db_query_params(sql, params)
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
}

/// 按 id 读取任务定义
fn get_job(host: &WasmHost, job_id: &str) -> Option<JobDef> {
    let row = query_first(host, "SELECT * FROM scheduled_jobs WHERE id = ?1", &sql_params![job_id])?;
    JobDef::from_row(&row)
}

/// 创建任务
///
/// 校验：schedule 可解析且存在未来触发、exec_type 合法、exec_value 非空、
/// timeout_sec > 0。next_at 初值 = 从现在起算的下一次未来触发。
pub fn add_job(
    host: &WasmHost,
    name: &str,
    schedule: &str,
    exec_type: &str,
    exec_value: &str,
    cwd: Option<&str>,
    env: Option<&Value>,
    timeout_sec: Option<i64>,
    once: bool,
) -> Result<String, String> {
    let spec = cron::parse(schedule).map_err(|e| format!("invalid schedule: {}", e))?;
    if exec_type != "script" && exec_type != "inline" {
        return Err("exec_type must be 'script' or 'inline'".to_string());
    }
    if exec_value.trim().is_empty() {
        return Err("exec_value must not be empty".to_string());
    }
    let timeout_sec = timeout_sec.unwrap_or(DEFAULT_TIMEOUT_SEC);
    if timeout_sec <= 0 {
        return Err("timeout_sec must be > 0".to_string());
    }
    let env_json = match env {
        Some(v) if v.is_object() => v.to_string(),
        Some(_) => return Err("env must be a JSON object".to_string()),
        None => "{}".to_string(),
    };

    let now_local = db_now_local(host).ok_or("scheduler: local time unavailable")?;
    let next_at =
        cron::next_after(&spec, &now_local).ok_or("schedule never matches (e.g. Feb 30)")?;

    let id = gen_id(host);
    if id.is_empty() {
        return Err("scheduler: failed to generate job id".to_string());
    }
    let name_v = if name.is_empty() { Value::Null } else { Value::String(name.to_string()) };
    let cwd_v = cwd
        .filter(|c| !c.is_empty())
        .map(|c| Value::String(c.to_string()))
        .unwrap_or(Value::Null);

    match host.plugin_db_execute_params(
        "INSERT INTO scheduled_jobs \
         (id, name, schedule, exec_type, exec_value, cwd, env, timeout_sec, enabled, once, next_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?10, datetime('now','localtime'), datetime('now','localtime'))",
        &sql_params![id, name_v, schedule, exec_type, exec_value, cwd_v, env_json, timeout_sec, if once { 1 } else { 0 }, next_at],
    ) {
        Ok(_) => {
            host.log_info(&format!(
                "scheduler: job created id={} schedule={} exec_type={} next_at={}",
                id, schedule, exec_type, next_at
            ));
            broadcast_changed(host, &id, "enabled", "create");
            Ok(id)
        }
        Err(e) => {
            host.log_error(&format!("scheduler: add_job insert failed: {}", e));
            Err("scheduler: failed to create job".to_string())
        }
    }
}

/// 列出全部任务（按 next_at 升序），附带最近一次执行摘要
pub fn list_jobs(host: &WasmHost) -> Vec<Value> {
    host.plugin_db_query(
        "SELECT j.*, \
         (SELECT e.status FROM job_executions e WHERE e.job_id = j.id ORDER BY e.rowid DESC LIMIT 1) AS last_status, \
         (SELECT e.finished_at FROM job_executions e WHERE e.job_id = j.id ORDER BY e.rowid DESC LIMIT 1) AS last_finished_at \
         FROM scheduled_jobs j ORDER BY j.next_at ASC, j.id",
    )
    .ok()
    .flatten()
    .and_then(|v| v.as_array().cloned())
    .unwrap_or_default()
}

/// 查看任务详情 + 最近执行记录
pub fn show_job(host: &WasmHost, job_id: &str) -> Option<Value> {
    let job = query_first(host, "SELECT * FROM scheduled_jobs WHERE id = ?1", &sql_params![job_id])?;
    let executions = host
        .plugin_db_query_params(
            "SELECT * FROM job_executions WHERE job_id = ?1 ORDER BY rowid DESC LIMIT 20",
            &sql_params![job_id],
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    Some(serde_json::json!({ "job": job, "executions": executions }))
}

/// 删除任务及其执行记录
pub fn remove_job(host: &WasmHost, job_id: &str) -> bool {
    let affected = host
        .plugin_db_execute_params(
            "DELETE FROM scheduled_jobs WHERE id = ?1",
            &sql_params![job_id],
        )
        .unwrap_or(-1);
    if affected > 0 {
        let _ = host.plugin_db_execute_params(
            "DELETE FROM job_executions WHERE job_id = ?1",
            &sql_params![job_id],
        );
        host.log_info(&format!("scheduler: job removed id={}", job_id));
        broadcast_changed(host, job_id, "deleted", "remove");
        true
    } else {
        false
    }
}

/// 编辑任务（仅更新提供的字段）；schedule 变更或 next_at 已过期时重算 next_at
pub fn edit_job(
    host: &WasmHost,
    job_id: &str,
    name: Option<&str>,
    schedule: Option<&str>,
    exec_type: Option<&str>,
    exec_value: Option<&str>,
    cwd: Option<&str>,
    env: Option<&Value>,
    timeout_sec: Option<i64>,
    once: Option<bool>,
) -> Result<(), String> {
    let job = get_job(host, job_id).ok_or_else(|| format!("job not found: {}", job_id))?;

    let schedule = schedule.unwrap_or(&job.schedule);
    let spec = cron::parse(schedule).map_err(|e| format!("invalid schedule: {}", e))?;
    let exec_type = exec_type.unwrap_or(&job.exec_type);
    if exec_type != "script" && exec_type != "inline" {
        return Err("exec_type must be 'script' or 'inline'".to_string());
    }
    let exec_value = exec_value.unwrap_or(&job.exec_value);
    if exec_value.trim().is_empty() {
        return Err("exec_value must not be empty".to_string());
    }
    let timeout_sec = timeout_sec.unwrap_or(job.timeout_sec);
    if timeout_sec <= 0 {
        return Err("timeout_sec must be > 0".to_string());
    }
    let env_json = match env {
        Some(v) if v.is_object() => v.to_string(),
        Some(_) => return Err("env must be a JSON object".to_string()),
        None => job.env.clone().unwrap_or_else(|| "{}".to_string()),
    };

    // next_at：schedule 变更 / 原值缺失 / 已过期（编辑时历史欠账不补触发）→ 重算
    let now_local = db_now_local(host).ok_or("scheduler: local time unavailable")?;
    let schedule_changed = schedule != job.schedule;
    let next_at = if schedule_changed || job.next_at.is_none() {
        cron::next_after(&spec, &now_local).ok_or("schedule never matches (e.g. Feb 30)")?
    } else if job.next_at.as_deref().is_some_and(|n| n <= &now_local) {
        cron::next_after(&spec, &now_local).ok_or("schedule never matches (e.g. Feb 30)")?
    } else {
        job.next_at.clone().unwrap_or_default()
    };

    let name_v = name
        .map(|n| if n.is_empty() { Value::Null } else { Value::String(n.to_string()) })
        .unwrap_or_else(|| job.name.clone().map(Value::String).unwrap_or(Value::Null));
    let cwd_v = cwd
        .map(|c| if c.is_empty() { Value::Null } else { Value::String(c.to_string()) })
        .unwrap_or_else(|| job.cwd.clone().map(Value::String).unwrap_or(Value::Null));
    let once_v = if once.unwrap_or(job.once) { 1 } else { 0 };

    match host.plugin_db_execute_params(
        "UPDATE scheduled_jobs SET name=?2, schedule=?3, exec_type=?4, exec_value=?5, cwd=?6, \
         env=?7, timeout_sec=?8, once=?9, next_at=?10, updated_at=datetime('now','localtime') \
         WHERE id=?1",
        &sql_params![job_id, name_v, schedule, exec_type, exec_value, cwd_v, env_json, timeout_sec, once_v, next_at],
    ) {
        Ok(affected) if affected > 0 => {
            host.log_info(&format!("scheduler: job edited id={} next_at={}", job_id, next_at));
            broadcast_changed(host, job_id, if job.enabled { "enabled" } else { "disabled" }, "edit");
            Ok(())
        }
        _ => Err(format!("job not found: {}", job_id)),
    }
}

/// 启用/停用；启用时若 next_at 已过期（停用期间欠账）推进到下一未来触发
pub fn set_enabled(host: &WasmHost, job_id: &str, enabled: bool) -> Result<(), String> {
    let job = get_job(host, job_id).ok_or_else(|| format!("job not found: {}", job_id))?;
    if enabled == job.enabled {
        return Ok(()); // 幂等
    }
    if enabled {
        let now_local = db_now_local(host).ok_or("scheduler: local time unavailable")?;
        let next_at = if job.next_at.as_deref().is_some_and(|n| n <= &now_local) {
            let spec = cron::parse(&job.schedule).map_err(|e| format!("invalid schedule: {}", e))?;
            cron::next_after(&spec, &now_local).ok_or("schedule never matches (e.g. Feb 30)")?
        } else {
            job.next_at.clone().unwrap_or_default()
        };
        host.plugin_db_execute_params(
            "UPDATE scheduled_jobs SET enabled=1, next_at=?2, updated_at=datetime('now','localtime') WHERE id=?1",
            &sql_params![job_id, next_at],
        )
        .map_err(|e| format!("scheduler: enable failed: {}", e))?;
        broadcast_changed(host, job_id, "enabled", "enable");
    } else {
        host.plugin_db_execute_params(
            "UPDATE scheduled_jobs SET enabled=0, updated_at=datetime('now','localtime') WHERE id=?1",
            &sql_params![job_id],
        )
        .map_err(|e| format!("scheduler: disable failed: {}", e))?;
        broadcast_changed(host, job_id, "disabled", "disable");
    }
    Ok(())
}

// ==================== 触发与调度 ====================

/// 执行记录输出路径（<home>/.bedcode/scheduler/<exec_id>.log）
fn output_path_for(host: &WasmHost, exec_id: &str) -> String {
    match host.config_get(ConfigKey::HomeDir).ok().flatten() {
        Some(home) if !home.is_empty() => format!("{}/{}/{}.log", home, OUTPUT_DIR, exec_id),
        _ => format!("{}/{}.log", OUTPUT_DIR, exec_id), // 兜底相对路径（home 不可用）
    }
}

/// 插入执行记录（waiting），超限裁剪，返回 exec_id
fn insert_execution(host: &WasmHost, job_id: &str, trigger: &str) -> String {
    let exec_id = gen_id(host);
    if exec_id.is_empty() {
        // ID 生成失败（宿主 DB 异常）：记录日志并跳过本次触发
        host.log_error("scheduler: insert_execution failed to generate exec id");
        return String::new();
    }
    let output_path = output_path_for(host, &exec_id);
    let _ = host.plugin_db_execute_params(
        "INSERT INTO job_executions (exec_id, job_id, status, trigger, started_at, finished_at, exit_code, output_path, run_id) \
         VALUES (?1, ?2, 'waiting', ?3, NULL, NULL, NULL, ?4, NULL)",
        &sql_params![exec_id, job_id, trigger, output_path],
    );
    trim_executions(host);
    exec_id
}

/// 执行记录封顶裁剪（保留最近 MAX_EXECUTION_ROWS 条，rowid 即插入序）
fn trim_executions(host: &WasmHost) {
    let _ = host.plugin_db_execute_params(
        "DELETE FROM job_executions WHERE rowid NOT IN \
         (SELECT rowid FROM job_executions ORDER BY rowid DESC LIMIT ?1)",
        &sql_params![MAX_EXECUTION_ROWS],
    );
}

/// 并发上限（storage `max_concurrency`，>=1；缺省/非法回退默认）
fn concurrency_limit(host: &WasmHost) -> i64 {
    host.storage_get("max_concurrency")
        .ok()
        .flatten()
        .and_then(|v| v.as_i64().or_else(|| v.as_u64().map(|u| u as i64)))
        .filter(|n| *n >= 1)
        .unwrap_or(DEFAULT_MAX_CONCURRENCY)
}

fn count_running(host: &WasmHost) -> i64 {
    query_first(host, "SELECT COUNT(*) AS n FROM job_executions WHERE status = 'running'", &[])
        .and_then(|row| row.get("n").and_then(|v| v.as_i64()))
        .unwrap_or(0)
}

/// 最早的等待执行（FIFO 按插入序）
fn next_waiting(host: &WasmHost) -> Option<(String, String)> {
    let row = query_first(
        host,
        "SELECT exec_id, job_id FROM job_executions WHERE status = 'waiting' ORDER BY rowid ASC LIMIT 1",
        &[],
    )?;
    Some((
        row.get("exec_id")?.as_str()?.to_string(),
        row.get("job_id")?.as_str()?.to_string(),
    ))
}

/// 任务是否有进行中的执行（waiting/running）——tick 幂等保护
fn has_active_execution(host: &WasmHost, job_id: &str) -> bool {
    query_first(
        host,
        "SELECT COUNT(*) AS n FROM job_executions WHERE job_id = ?1 AND status IN ('waiting','running')",
        &sql_params![job_id],
    )
    .and_then(|row| row.get("n").and_then(|v| v.as_i64()))
    .unwrap_or(0)
        > 0
}

/// 组装 host-process 请求 JSON（纯函数，可单测）
///
/// 命令包装按宿主平台（spec 8.3 语义）：
/// - Windows：script/inline 均经 `cmd /C`（.bat/.cmd 与内联命令）
/// - unix：script 直接执行（需 shebang + 可执行位）；inline 经 `sh -c`
pub fn build_process_request(
    job: &JobDef,
    platform: &str,
    home_dir: &str,
    output_path: &str,
) -> String {
    let (command, args): (String, Vec<String>) = if platform == "windows" {
        ("cmd".to_string(), vec!["/C".to_string(), job.exec_value.clone()])
    } else if job.exec_type == "inline" {
        ("sh".to_string(), vec!["-c".to_string(), job.exec_value.clone()])
    } else {
        (job.exec_value.clone(), Vec::new())
    };
    let cwd = if job.cwd.as_deref().is_some_and(|c| !c.is_empty()) {
        job.cwd.clone().unwrap_or_default()
    } else {
        home_dir.to_string()
    };
    let env: Value = job
        .env
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    serde_json::json!({
        "command": command,
        "args": args,
        "cwd": cwd,
        "env": env,
        "timeout_ms": (job.timeout_sec * 1000).max(1),
        "output_path": output_path,
    })
    .to_string()
}

/// 提升单条执行记录为 running 并启动宿主进程；失败置 failed
///
/// 返回是否成功启动（失败时记录已置 failed，调用方继续调度下一个）
fn dispatch_single(host: &WasmHost, exec_id: &str, job: &JobDef) -> bool {
    let platform = host
        .config_get(ConfigKey::OsPlatform)
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());
    let home_dir = host
        .config_get(ConfigKey::HomeDir)
        .ok()
        .flatten()
        .unwrap_or_else(|| ".".to_string());
    let output_path = output_path_for(host, exec_id);
    let request = build_process_request(job, &platform, &home_dir, &output_path);

    match host.process_run(&request) {
        Ok(run_id) => {
            let _ = host.plugin_db_execute_params(
                "UPDATE job_executions SET status='running', started_at=datetime('now','localtime'), run_id=?2 \
                 WHERE exec_id=?1",
                &sql_params![exec_id, run_id],
            );
            host.log_info(&format!(
                "scheduler: execution started exec_id={} job_id={} run_id={}",
                exec_id, job.id, run_id
            ));
            broadcast_changed(host, &job.id, "running", "start");
            true
        }
        Err(e) => {
            // spawn 失败（命令不存在等）：执行记录置 failed，可审计
            let _ = host.plugin_db_execute_params(
                "UPDATE job_executions SET status='failed', finished_at=datetime('now','localtime') \
                 WHERE exec_id=?1",
                &sql_params![exec_id],
            );
            host.log_error(&format!(
                "scheduler: process_run failed exec_id={} job_id={}: {}",
                exec_id, job.id, e
            ));
            broadcast_changed(host, &job.id, "failed", "start-failed");
            true // 失败不代表队列停摆：继续调度下一个等待项
        }
    }
}

/// 调度等待队列：槽位空出即提升（running < 并发上限）
fn dispatch_waiting(host: &WasmHost) {
    let max = concurrency_limit(host);
    loop {
        if count_running(host) >= max {
            break;
        }
        let Some((exec_id, job_id)) = next_waiting(host) else { break };
        let Some(job) = get_job(host, &job_id) else {
            // 任务定义已删（竞态兜底）：执行记录置 missed 归档
            let _ = host.plugin_db_execute_params(
                "UPDATE job_executions SET status='missed', finished_at=datetime('now','localtime') WHERE exec_id=?1",
                &sql_params![exec_id],
            );
            continue;
        };
        dispatch_single(host, &exec_id, &job);
    }
}

/// tick 入口（宿主定时器到点回调，spec §5.2）
///
/// 1. 到期触发：`next_at <= now_local AND enabled=1`，按 next_at 升序；
///    已有非终态执行的 job 跳过（cron 不重叠语义）
/// 2. 调度等待队列（并发上限内提升 waiting → running）
pub fn handle_tick(host: &WasmHost, now_local: &str) {
    let due = host
        .plugin_db_query_params(
            "SELECT id FROM scheduled_jobs WHERE enabled = 1 AND next_at IS NOT NULL AND next_at <= ?1 \
             ORDER BY next_at ASC, id",
            &sql_params![now_local],
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    for row in due {
        let Some(job_id) = row.get("id").and_then(|v| v.as_str()).map(str::to_string) else {
            continue;
        };
        if has_active_execution(host, &job_id) {
            continue;
        }
        let exec_id = insert_execution(host, &job_id, "cron");
        if exec_id.is_empty() {
            continue; // ID 生成失败已记日志，跳过本次
        }
        host.log_info(&format!("scheduler: job due job_id={} exec_id={}", job_id, exec_id));
        broadcast_changed(host, &job_id, "waiting", "trigger");
    }
    dispatch_waiting(host);
}

// ==================== 完成回写 ====================

/// host-process 完成事件回写（on_process_done）
///
/// - 回写 status/exit_code/finished_at
/// - cron 触发：推进 next_at（从旧 next_at 起算，见模块文档）；once 成功自动停用
/// - manual 触发：不改变 next_at
/// - 槽位空出后继续调度排队
pub fn handle_process_done(host: &WasmHost, run_id: &str, exit_code: Option<i32>, timed_out: bool) {
    let Some(exec) = query_first(
        host,
        "SELECT exec_id, job_id, trigger, status FROM job_executions WHERE run_id = ?1",
        &sql_params![run_id],
    ) else {
        host.log_warn(&format!("scheduler: done event for unknown run_id={}", run_id));
        return;
    };
    let Some(exec_id) = exec.get("exec_id").and_then(|v| v.as_str()).map(str::to_string) else {
        return;
    };
    let Some(job_id) = exec.get("job_id").and_then(|v| v.as_str()).map(str::to_string) else {
        return;
    };
    let trigger = exec.get("trigger").and_then(|v| v.as_str()).unwrap_or("").to_string();
    // 幂等：重复完成事件不覆盖终态
    if exec
        .get("status")
        .and_then(|v| v.as_str())
        .is_some_and(|s| matches!(s, "succeeded" | "failed" | "timeout" | "missed"))
    {
        return;
    }

    let status = if timed_out {
        "timeout"
    } else if exit_code == Some(0) {
        "succeeded"
    } else {
        "failed"
    };
    let exit_param = match exit_code {
        Some(code) => Value::from(code),
        None => Value::Null,
    };
    let _ = host.plugin_db_execute_params(
        "UPDATE job_executions SET status=?2, exit_code=?3, finished_at=datetime('now','localtime') WHERE exec_id=?1",
        &sql_params![exec_id, status, exit_param],
    );
    host.log_info(&format!(
        "scheduler: execution done exec_id={} job_id={} status={} exit_code={:?} timed_out={}",
        exec_id, job_id, status, exit_code, timed_out
    ));
    broadcast_changed(host, &job_id, status, "done");

    if trigger == "cron" {
        advance_next_at(host, &job_id, status == "succeeded");
    }
    // 槽位空出 → 继续调度排队
    dispatch_waiting(host);
}

/// 推进任务 next_at（cron 触发完成后）
///
/// - 正常：`next_after(schedule, 旧 next_at)`（与运行耗时无关，不漂移）
/// - once 且成功：停用（spec §5.2：触发成功后自动停用，不重复触发）；
///   失败/超时的 once 任务照常排下一周期（成功前重试语义）
/// - schedule 永不匹配（如 2/30，理论不可达——建任务已拦截）：停用防死循环
/// - 过期触发（停机恢复宽限内被 tick 拾起）：from 取 max(旧 next_at, now)，
///   直接跳到未来周期——错过的时间点不补跑（spec out-of-scope: catch-up）
fn advance_next_at(host: &WasmHost, job_id: &str, succeeded: bool) {
    let Some(job) = get_job(host, job_id) else { return };
    let Ok(spec) = cron::parse(&job.schedule) else { return };
    let from_old = job.next_at.as_deref().unwrap_or("1970-01-01 00:00:00").to_string();
    // 旧 next_at 已过期（如宽限内 miss 后补触发）：以当前时间为基准推进，
    // 避免 next_after 返回仍处过去的时刻导致逐秒补跑（catch-up）
    let now_local = db_now_local(host).unwrap_or_default();
    let from = if from_old < now_local { now_local } else { from_old };
    let Some(next_at) = cron::next_after(&spec, &from) else {
        host.log_warn(&format!(
            "scheduler: schedule never matches, disabling job id={}",
            job_id
        ));
        let _ = host.plugin_db_execute_params(
            "UPDATE scheduled_jobs SET enabled=0, updated_at=datetime('now','localtime') WHERE id=?1",
            &sql_params![job_id],
        );
        broadcast_changed(host, job_id, "disabled", "never-match");
        return;
    };
    if job.once && succeeded {
        let _ = host.plugin_db_execute_params(
            "UPDATE scheduled_jobs SET enabled=0, next_at=?2, updated_at=datetime('now','localtime') WHERE id=?1",
            &sql_params![job_id, next_at],
        );
        host.log_info(&format!("scheduler: once job done, disabled id={}", job_id));
        broadcast_changed(host, job_id, "disabled", "once-done");
    } else {
        let _ = host.plugin_db_execute_params(
            "UPDATE scheduled_jobs SET next_at=?2, updated_at=datetime('now','localtime') WHERE id=?1",
            &sql_params![job_id, next_at],
        );
        broadcast_changed(host, job_id, "scheduled", "reschedule");
    }
}

// ==================== 启动恢复 ====================

/// activate 恢复（幂等，spec §5.2/§13）：
///
/// 1. 残留 waiting/running 执行（应用重启，宿主进程/注册表已亡）→ missed
/// 2. next_at 已过且超过宽限（120s）的任务 → 插入 missed 执行记录，
///    next_at 推进到下一未来时刻（不补跑）
pub fn recover(host: &WasmHost) {
    let affected = host
        .plugin_db_execute_params(
            "UPDATE job_executions SET status='missed', finished_at=datetime('now','localtime') \
             WHERE status IN ('waiting','running')",
            &[],
        )
        .unwrap_or(-1);
    if affected > 0 {
        host.log_warn(&format!(
            "scheduler: {} residual execution(s) marked missed on restart",
            affected
        ));
        broadcast_changed(host, "", "missed", "restart");
    }

    let Some(now_local) = db_now_local(host) else {
        host.log_error("scheduler: recover aborted, local time unavailable");
        return;
    };
    let overdue = host
        .plugin_db_query_params(
            "SELECT id, schedule FROM scheduled_jobs WHERE enabled = 1 AND next_at IS NOT NULL \
             AND next_at <= datetime('now','localtime', ?1)",
            &sql_params![format!("-{} seconds", MISSED_GRACE_SECONDS)],
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    for row in overdue {
        let Some(job_id) = row.get("id").and_then(|v| v.as_str()).map(str::to_string) else {
            continue;
        };
        let schedule = row.get("schedule").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let exec_id = insert_execution(host, &job_id, "cron");
        if !exec_id.is_empty() {
            let _ = host.plugin_db_execute_params(
                "UPDATE job_executions SET status='missed', finished_at=datetime('now','localtime') WHERE exec_id=?1",
                &sql_params![exec_id],
            );
        }
        // next_at 推进到下一未来时刻（从 now 起算，不补跑历史周期）
        let new_next = cron::parse(&schedule)
            .ok()
            .and_then(|spec| cron::next_after(&spec, &now_local));
        match new_next {
            Some(next) => {
                let _ = host.plugin_db_execute_params(
                    "UPDATE scheduled_jobs SET next_at=?2, updated_at=datetime('now','localtime') WHERE id=?1",
                    &sql_params![job_id, next],
                );
                host.log_warn(&format!(
                    "scheduler: job missed while app was down id={} next_at={}",
                    job_id, next
                ));
            }
            None => {
                // schedule 永不匹配（理论不可达）：停用防死循环
                let _ = host.plugin_db_execute_params(
                    "UPDATE scheduled_jobs SET enabled=0, updated_at=datetime('now','localtime') WHERE id=?1",
                    &sql_params![job_id],
                );
                host.log_warn(&format!("scheduler: schedule never matches, disabled id={}", job_id));
            }
        }
        broadcast_changed(host, &job_id, "missed", "missed");
    }
}

// ==================== 手动触发 ====================

/// 手动立即执行一次（bedtask run / HTTP run）
///
/// 插入执行记录（trigger='manual'）并立即尝试占用槽位；
/// 无槽位（并发满）则排队等待。不改变 next_at（spec §5.2）。
pub fn run_job(host: &WasmHost, job_id: &str) -> Result<String, String> {
    let job = get_job(host, job_id).ok_or_else(|| format!("job not found: {}", job_id))?;
    let exec_id = insert_execution(host, job_id, "manual");
    if exec_id.is_empty() {
        return Err("scheduler: failed to create execution record".to_string());
    }
    host.log_info(&format!("scheduler: manual run job_id={} exec_id={}", job_id, exec_id));
    broadcast_changed(host, job_id, "waiting", "run");
    // 立即尝试占用槽位；并发满则排队（由 dispatch_waiting 按 FIFO 提升）
    if count_running(host) < concurrency_limit(host) {
        dispatch_single(host, &exec_id, &job);
    }
    Ok(exec_id)
}

// ==================== 事件广播 ====================

/// 变更广播（三通道，spec §5.2 事件模型）：
/// - broadcast_sync：移动端同步（复用 SyncEvent::TaskScheduledChanged，线协议兼容）
/// - bus + emit_event：桌面端（topic `scheduler:changed`，与 auto-task 的
///   `task:scheduled-changed` 区分，issue 05 只读面板订阅此 topic）
fn broadcast_changed(host: &WasmHost, job_id: &str, status: &str, action: &str) {
    host.broadcast_sync(&SyncEvent::TaskScheduledChanged {
        job_id: job_id.to_string(),
        status: status.to_string(),
        action: action.to_string(),
    });
    let payload = serde_json::json!({
        "job_id": job_id,
        "status": status,
        "action": action,
    });
    let _ = host.bus_publish(EVENT_SCHEDULER_CHANGED, &payload);
    host.emit_event(EVENT_SCHEDULER_CHANGED, &payload);
}

// ==================== HTTP 端点 ====================

/// 处理 HTTP 端点（路由前缀 task-scheduler/，与 CLI 命令一一对应）
///
/// - POST add / GET list / GET show / DELETE remove / POST edit
/// - POST enable / POST disable / POST run / GET logs
pub fn handle_scheduler_http(
    host: &WasmHost,
    method: &str,
    path: &str,
    body: &Value,
    query: &Value,
) -> Value {
    host.log_debug(&format!("scheduler http: {} {}", method, path));
    match (method, path) {
        ("POST", "add") => http_add(host, body),
        ("GET", "list") => http_response::ok_with_data(serde_json::json!({ "jobs": list_jobs(host) })),
        ("GET", "show") => http_show(host, body, query),
        ("DELETE", "remove") => http_remove(host, body, query),
        ("POST", "edit") => http_edit(host, body),
        ("POST", "enable") => http_set_enabled(host, body, query, true),
        ("POST", "disable") => http_set_enabled(host, body, query, false),
        ("POST", "run") => http_run(host, body, query),
        ("GET", "logs") => http_logs(host, body, query),
        _ => {
            host.log_warn(&format!("Unknown scheduler endpoint: {} {}", method, path));
            http_response::error(404, &format!("Not found: {} {}", method, path))
        }
    }
}

fn body_str<'a>(body: &'a Value, query: &'a Value, key: &str) -> Option<&'a str> {
    body.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| query.get(key).and_then(|v| v.as_str()))
}

/// POST task-scheduler/add
///
/// body: { name?, schedule, exec_type: script|inline, exec_value, cwd?, env?,
///         timeout_sec?, once? }
fn http_add(host: &WasmHost, body: &Value) -> Value {
    let schedule = body.get("schedule").and_then(|v| v.as_str()).unwrap_or("");
    let exec_type = body.get("exec_type").and_then(|v| v.as_str()).unwrap_or("");
    let exec_value = body.get("exec_value").and_then(|v| v.as_str()).unwrap_or("");
    if schedule.is_empty() {
        return http_response::error(400, "Missing schedule");
    }
    if exec_value.is_empty() {
        return http_response::error(400, "Missing exec_value");
    }
    let timeout_sec = body.get("timeout_sec").and_then(|v| v.as_i64());
    let once = body.get("once").and_then(|v| v.as_bool()).unwrap_or(false);
    match add_job(
        host,
        body.get("name").and_then(|v| v.as_str()).unwrap_or(""),
        schedule,
        exec_type,
        exec_value,
        body.get("cwd").and_then(|v| v.as_str()),
        body.get("env"),
        timeout_sec,
        once,
    ) {
        Ok(job_id) => http_response::ok_with_data(serde_json::json!({ "job_id": job_id })),
        Err(e) => http_response::error(400, &e),
    }
}

/// GET task-scheduler/show?job_id=
fn http_show(host: &WasmHost, body: &Value, query: &Value) -> Value {
    let job_id = match body_str(body, query, "job_id") {
        Some(id) => id,
        None => return http_response::error(400, "Missing job_id"),
    };
    match show_job(host, job_id) {
        Some(data) => http_response::ok_with_data(data),
        None => http_response::error(404, &format!("Job not found: {}", job_id)),
    }
}

/// DELETE task-scheduler/remove?job_id=
fn http_remove(host: &WasmHost, body: &Value, query: &Value) -> Value {
    let job_id = match body_str(body, query, "job_id") {
        Some(id) => id,
        None => return http_response::error(400, "Missing job_id"),
    };
    if remove_job(host, job_id) {
        http_response::ok()
    } else {
        http_response::error(404, &format!("Job not found: {}", job_id))
    }
}

/// POST task-scheduler/edit
///
/// body: { job_id, name?, schedule?, exec_type?, exec_value?, cwd?, env?, timeout_sec?, once? }
fn http_edit(host: &WasmHost, body: &Value) -> Value {
    let job_id = body.get("job_id").and_then(|v| v.as_str()).unwrap_or("");
    if job_id.is_empty() {
        return http_response::error(400, "Missing job_id");
    }
    match edit_job(
        host,
        job_id,
        body.get("name").and_then(|v| v.as_str()),
        body.get("schedule").and_then(|v| v.as_str()),
        body.get("exec_type").and_then(|v| v.as_str()),
        body.get("exec_value").and_then(|v| v.as_str()),
        body.get("cwd").and_then(|v| v.as_str()),
        body.get("env"),
        body.get("timeout_sec").and_then(|v| v.as_i64()),
        body.get("once").and_then(|v| v.as_bool()),
    ) {
        Ok(()) => http_response::ok(),
        Err(e) => {
            if e.starts_with("job not found") {
                http_response::error(404, &e)
            } else {
                http_response::error(400, &e)
            }
        }
    }
}

/// POST task-scheduler/enable|disable（body 或 query 携带 job_id）
fn http_set_enabled(host: &WasmHost, body: &Value, query: &Value, enabled: bool) -> Value {
    let job_id = match body_str(body, query, "job_id") {
        Some(id) => id,
        None => return http_response::error(400, "Missing job_id"),
    };
    match set_enabled(host, job_id, enabled) {
        Ok(()) => http_response::ok(),
        Err(e) => {
            if e.starts_with("job not found") {
                http_response::error(404, &e)
            } else {
                http_response::error(400, &e)
            }
        }
    }
}

/// POST task-scheduler/run?job_id= —— 手动立即执行一次（不改变 next_at）
fn http_run(host: &WasmHost, body: &Value, query: &Value) -> Value {
    let job_id = match body_str(body, query, "job_id") {
        Some(id) => id,
        None => return http_response::error(400, "Missing job_id"),
    };
    match run_job(host, job_id) {
        Ok(exec_id) => http_response::ok_with_data(serde_json::json!({ "exec_id": exec_id })),
        Err(e) => {
            if e.starts_with("job not found") {
                http_response::error(404, &e)
            } else {
                http_response::error(500, &e)
            }
        }
    }
}

/// 最近执行记录查询（HTTP logs 端点与互调 api `schedule.logs` 共用）
///
/// job_id 不存在时返回空执行列表（与 HTTP 端点一致：任务删除后历史执行
/// 记录随之删除，空列表即等价「无记录」，不做 404）
pub fn job_logs(host: &WasmHost, job_id: &str, limit: i64) -> Value {
    let executions = host
        .plugin_db_query_params(
            "SELECT * FROM job_executions WHERE job_id = ?1 ORDER BY rowid DESC LIMIT ?2",
            &sql_params![job_id, limit],
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    serde_json::json!({
        "job_id": job_id,
        "executions": executions,
    })
}

/// GET task-scheduler/logs?job_id=&limit= —— 最近执行记录（含输出文件路径）
fn http_logs(host: &WasmHost, body: &Value, query: &Value) -> Value {
    let job_id = match body_str(body, query, "job_id") {
        Some(id) => id,
        None => return http_response::error(400, "Missing job_id"),
    };
    let limit = body
        .get("limit")
        .and_then(|v| v.as_i64())
        .or_else(|| query.get("limit").and_then(|v| v.as_i64()))
        .unwrap_or(20)
        .clamp(1, 100);
    http_response::ok_with_data(job_logs(host, job_id, limit))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn job(schedule: &str, exec_type: &str, exec_value: &str) -> JobDef {
        JobDef {
            id: "j1".into(),
            name: None,
            schedule: schedule.into(),
            exec_type: exec_type.into(),
            exec_value: exec_value.into(),
            cwd: None,
            env: None,
            timeout_sec: 600,
            enabled: true,
            once: false,
            next_at: Some("2026-01-01 09:00:00".into()),
        }
    }

    /// Windows 平台：script/inline 均经 cmd /C
    #[test]
    fn build_request_windows_wraps_with_cmd() {
        let req: Value = serde_json::from_str(&build_process_request(
            &job("0 0 9 * * *", "script", "C:\\scripts\\backup.bat"),
            "windows",
            "C:\\Users\\test",
            "C:\\Users\\test\\.bedcode\\scheduler\\e1.log",
        ))
        .unwrap();
        assert_eq!(req["command"], "cmd");
        assert_eq!(req["args"], serde_json::json!(["/C", "C:\\scripts\\backup.bat"]));
        assert_eq!(req["cwd"], "C:\\Users\\test");
        assert_eq!(req["timeout_ms"], 600000);
        assert_eq!(req["output_path"], "C:\\Users\\test\\.bedcode\\scheduler\\e1.log");

        let inline: Value = serde_json::from_str(&build_process_request(
            &job("0 0 9 * * *", "inline", "echo hi && echo there"),
            "windows",
            "C:\\Users\\test",
            "out.log",
        ))
        .unwrap();
        assert_eq!(inline["command"], "cmd");
        assert_eq!(inline["args"], serde_json::json!(["/C", "echo hi && echo there"]));
    }

    /// unix：script 直接执行（shebang 可执行位），inline 经 sh -c
    #[test]
    fn build_request_unix_runs_script_directly() {
        let req: Value = serde_json::from_str(&build_process_request(
            &job("0 0 9 * * *", "script", "/home/u/backup.sh"),
            "linux",
            "/home/u",
            "/home/u/.bedcode/scheduler/e1.log",
        ))
        .unwrap();
        assert_eq!(req["command"], "/home/u/backup.sh");
        assert_eq!(req["args"], serde_json::json!([]));
        assert_eq!(req["cwd"], "/home/u");

        let inline: Value = serde_json::from_str(&build_process_request(
            &job("0 0 9 * * *", "inline", "echo hi"),
            "macos",
            "/Users/t",
            "out.log",
        ))
        .unwrap();
        assert_eq!(inline["command"], "sh");
        assert_eq!(inline["args"], serde_json::json!(["-c", "echo hi"]));
    }

    /// cwd 显式指定时优先于主目录
    #[test]
    fn build_request_respects_explicit_cwd_and_env() {
        let mut j = job("0 0 9 * * *", "inline", "echo hi");
        j.cwd = Some("/work".into());
        j.env = Some("{\"K\": \"V\"}".into());
        let req: Value = serde_json::from_str(&build_process_request(
            &j, "linux", "/home/u", "out.log",
        ))
        .unwrap();
        assert_eq!(req["cwd"], "/work");
        assert_eq!(req["env"], serde_json::json!({ "K": "V" }));
    }
}
