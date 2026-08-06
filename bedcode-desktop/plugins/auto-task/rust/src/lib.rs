//! Auto Task Plugin (WASM)
//!
//! Claude Code 任务状态同步与自动授权
//! 使用 bedcode-plugin-api WasmPlugin trait 实现，通过 wasm_entry! 宏生成导出
//!
//! 架构说明：业务逻辑在本插件内实现，文件 I/O 通过宿主 Host Function
//! （fs_read/fs_write/fs_copy）执行，配置读取通过 config_get。
//! HTTP 端点通过 /api/plugin/com.bedcode.auto-task/{path} 代理路由，
//! 由宿主 plugin_http_endpoint 调用本插件的 _http_endpoint command。

mod hooks;
mod state;
mod queue;
mod agent;
mod scheduled;
mod preset;

use bedcode_plugin_api::events::{InputSubmittedEvent, SessionLifecycleEvent};
use bedcode_plugin_api::host::{ConfigKey, HostConfig, HostLog, HostPluginDatabase, HostSession, HostStorage, HostTimer};
use bedcode_plugin_api::{CommandArgs, WasmHost, WasmPlugin};
use bedcode_plugin_api::types::PluginManifest;

/// 任务历史表建表 SQL（按语句拆分，初始化时逐条执行）
///
/// 宿主 `plugin_db_execute` 为 rusqlite 单语句版本（后续语句被静默忽略），
/// 多语句 schema 必须拆分，否则 CREATE INDEX 永远不会执行
const TASK_HISTORY_SCHEMA: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS task_history (
    id              TEXT PRIMARY KEY,
    description     TEXT,
    status          TEXT NOT NULL DEFAULT 'pending',
    agent           TEXT,
    source          TEXT,
    session_id      TEXT,
    claude_sid      TEXT,
    working_dir     TEXT,
    exit_reason     TEXT,
    questions       TEXT,
    auto_approve    INTEGER DEFAULT 0,
    event_time      TEXT,
    input_tokens    INTEGER,
    output_tokens   INTEGER,
    created_at      TEXT NOT NULL,
    started_at      TEXT,
    completed_at    TEXT,
    updated_at      TEXT NOT NULL
)"#,
    "CREATE INDEX IF NOT EXISTS idx_task_history_status ON task_history(status)",
    "CREATE INDEX IF NOT EXISTS idx_task_history_session_id ON task_history(session_id)",
    "CREATE INDEX IF NOT EXISTS idx_task_history_created_at ON task_history(created_at)",
];

/// Claude Code session ↔ BedCode PTY session 映射表建表 SQL（按语句拆分）
///
/// SessionStart 时仅存储映射关系，不创建空壳任务记录。
/// 任务行由宿主 on_input_submitted 会话扩展在用户提交输入时创建，
/// 输入作为任务内容写入 description 字段。
const SESSION_MAPPING_SCHEMA: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS session_mapping (
    claude_sid  TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    created_at  TEXT NOT NULL
)"#,
    "CREATE INDEX IF NOT EXISTS idx_session_mapping_session ON session_mapping(session_id)",
];

/// 定时自动任务表建表 SQL（按语句拆分）
///
/// 一次性定时任务：指定时刻新建会话（config_id）执行一组 prompt（JSON 数组）。
/// 状态机：pending（待触发）→ creating（会话已创建，等 Created 事件入队）
/// → executed（prompts 已入队）；failed（会话创建失败）/ missed（错过不补跑）。
/// session_id 列关联触发时创建的会话，是 Created 事件的匹配键。
const SCHEDULED_JOBS_SCHEMA: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS scheduled_jobs (
    id          TEXT PRIMARY KEY,
    name        TEXT,
    config_id   TEXT NOT NULL,
    trigger_at  TEXT NOT NULL,
    prompts     TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'pending',
    session_id  TEXT,
    created_at  TEXT NOT NULL,
    executed_at TEXT,
    error       TEXT
)"#,
    "CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_status ON scheduled_jobs(status, trigger_at)",
];

/// 会话级开关表建表 SQL（按语句拆分）
///
/// 两个独立开关：
/// - auto_execute：自动执行 — 开启后入队任务自动调度执行；关闭时仅入队，
///   可先添加多个任务再统一开启执行（手动控制入口：AutoTaskModal 弹窗）
/// - auto_answer：自动应答 — 开启后 Agent 提问（权限请求 / AskUserQuestion）
///   由 hook 自动回答；关闭时走 Claude Code 原生交互，用户手动回答
const SESSION_SETTINGS_SCHEMA: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS session_settings (
    session_id   TEXT PRIMARY KEY,
    auto_execute INTEGER NOT NULL DEFAULT 0,
    auto_answer  INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT NOT NULL
)"#,
];

struct AutoTaskPlugin;

impl WasmPlugin for AutoTaskPlugin {
    const ID: &'static str = "com.bedcode.auto-task";

    fn manifest() -> PluginManifest {
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Auto Task plugin activated");

        // 注册会话生命周期监听器
        // 会话创建前会收到 creating 事件，用于自动设置项目 hooks
        match host.session_lifecycle_register() {
            Ok(()) => host.log_info("Registered session lifecycle listener"),
            Err(e) => host.log_error(&format!("Failed to register session lifecycle listener: {}", e)),
        }

        // 注册提交输入行监听器（需要 terminal:observe 权限，见 ADR 0001）
        // 用户提交输入（回车触发）时异步收到重建后的完整输入行
        match host.session_input_register() {
            Ok(()) => host.log_info("Registered session input listener"),
            Err(e) => host.log_error(&format!("Failed to register session input listener: {}", e)),
        }

        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Auto Task plugin deactivated");

        // 插件禁用时清理所有项目的 hooks 配置
        // 避免残留的 hooks 在插件停用后仍被 Claude Code 调用
        let result = hooks::cleanup_all_project_hooks(&host);
        host.log_info(&format!(
            "Hooks cleanup on deactivate: cleaned={}, skipped={}, failed={}",
            result.cleaned, result.skipped, result.failed
        ));

        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let host = WasmHost;
        // CommandArgs 统一字段提取（内部已做 Null 归一化）
        let args = CommandArgs::new(args);

        match name {
            "_http_endpoint" => {
                let method = args.str_or("method", "");
                let path = args.str_or("path", "");
                let body = args.value_owned("body").unwrap_or(serde_json::Value::Null);
                let query = args.value_owned("query").unwrap_or(serde_json::json!({}));

                // 队列端点路由
                if let Some(queue_path) = path.strip_prefix("task-queue/") {
                    Ok(queue::handle_queue_http(&host, &method, queue_path, &body, &query))
                } else if let Some(scheduled_path) = path.strip_prefix("scheduled-jobs/") {
                    Ok(scheduled::handle_scheduled_http(&host, &method, scheduled_path, &body, &query))
                } else {
                    Ok(state::handle_http_endpoint(&host, &method, &path, &body, &query))
                }
            }
            // 命令 ID 与 manifest contributes.commands 声明保持一致（全名含前缀）
            // 队列操作（add/remove/list/clear）仅通过 HTTP task-queue 端点暴露，不是 command
            "auto-task.cleanup-project-hooks" => {
                let working_dir = args.str_or("working_dir", "");

                let result = hooks::cleanup_project_hooks(&host, &working_dir);

                Ok(serde_json::json!({
                    "success": result.success,
                    "message": result.message,
                }))
            }
            "auto-task.get-task-status" => {
                let session_id = args.str_or("session_id", "");
                state::get_task_status(&host, &session_id)
            }
            "auto-task.list-task-history" => {
                let filter = task_history_filter_from_args(&args);
                state::list_task_history(&host, &filter)
            }
            "auto-task.task-history-stats" => {
                let filter = task_history_filter_from_args(&args);
                state::task_history_stats(&host, &filter)
            }
            "auto-task.list-task-queue" => {
                let session_id = args.str_or("session_id", "");
                let tasks = queue::list_queue(&host, &session_id);
                Ok(serde_json::json!({ "tasks": tasks, "session_id": session_id }))
            }
            "auto-task.list-running-sessions" => {
                // 运行中的会话（含最新任务摘要），供前端「当前任务」Tab 展示与创建任务下拉选择
                let sessions = state::list_running_sessions(&host);
                Ok(serde_json::json!({ "sessions": sessions }))
            }
            "auto-task.set-platform" => {
                // 前端在插件激活时通过 @tauri-apps/plugin-os 读取宿主平台并上报。
                // queue.rs 调度据此选择终端输入提交符（Windows=CR，Linux=LF）
                let platform = args.str_or("platform", "");
                if platform.is_empty() {
                    return Err(anyhow::anyhow!("set-platform: missing platform"));
                }
                // 白名单校验：仅接受宿主 OS 平台名，非法值直接拒绝
                if !["windows", "linux", "macos", "android", "ios"].contains(&platform.as_str()) {
                    return Err(anyhow::anyhow!("set-platform: unknown platform: {}", platform));
                }
                host.storage_set("platform", &serde_json::json!(platform))?;
                host.log_info(&format!("Platform recorded: {}", platform));
                Ok(serde_json::json!({ "platform": platform }))
            }
            "auto-task.set-auto-mode" => {
                let session_id = args.str_or("session_id", "");
                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("set-auto-mode: missing session_id"));
                }
                // 两个独立开关：auto_execute（任务是否自动执行）与 auto_answer（Agent 提问是否自动回答），
                // 均可单独设置；未提供的字段保持当前值（兼容旧调用方仅传 auto_approve）
                let auto_execute = args.value("auto_execute").and_then(|v| v.as_bool());
                let auto_answer = args
                    .value("auto_answer")
                    .and_then(|v| v.as_bool())
                    .or_else(|| args.value("auto_approve").and_then(|v| v.as_bool()));
                state::set_auto_mode(&host, &session_id, auto_execute, auto_answer)
            }
            "auto-task.get-session-settings" => {
                let session_id = args.str_or("session_id", "");
                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("get-session-settings: missing session_id"));
                }
                state::get_session_settings(&host, &session_id)
            }
            "auto-task.add-task" => {
                let session_id = args.str_or("session_id", "");
                let prompt = args.str_or("prompt", "");

                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("add-task: missing session_id"));
                }
                if prompt.is_empty() {
                    return Err(anyhow::anyhow!("add-task: missing prompt"));
                }

                let (task_id, position) = queue::add_task(&host, &session_id, &prompt);

                // 自动执行开启且会话空闲时立即调度；关闭时仅入队（可先添加多个任务再统一执行），
                // 调度链由会话 idle / 任务终态事件驱动（try_dispatch_next 内部以 auto_execute 为门）
                if state::auto_execute_on(&host, &session_id) && !state::has_active_task(&host, &session_id) {
                    queue::try_dispatch_next(&host, &session_id);
                }

                let count_after = queue::pending_count(&host, &session_id);
                queue::broadcast_queue_changed(&host, &session_id, count_after, "add");

                Ok(serde_json::json!({ "task_id": task_id, "position": position }))
            }
            "auto-task.list-preset-tasks" => {
                // 预设任务列表（全局，创建时间倒序），供侧边栏「当前任务」Tab 与终端弹窗展示
                let presets = preset::list_presets(&host);
                Ok(serde_json::json!({ "presets": presets }))
            }
            "auto-task.create-preset-task" => {
                let prompt = args.str_or("prompt", "");
                if prompt.is_empty() {
                    return Err(anyhow::anyhow!("create-preset-task: missing prompt"));
                }

                let preset_id = preset::create_preset(&host, &prompt);
                preset::broadcast_preset_changed(&host, &preset_id, "create");

                Ok(serde_json::json!({ "preset_id": preset_id }))
            }
            "auto-task.delete-preset-task" => {
                let preset_id = args.str_or("preset_id", "");
                if preset_id.is_empty() {
                    return Err(anyhow::anyhow!("delete-preset-task: missing preset_id"));
                }

                if !preset::delete_preset(&host, &preset_id) {
                    return Err(anyhow::anyhow!("delete-preset-task: preset not found: {}", preset_id));
                }
                preset::broadcast_preset_changed(&host, &preset_id, "delete");

                Ok(serde_json::json!({ "deleted": true }))
            }
            "auto-task.add-preset-to-queue" => {
                let session_id = args.str_or("session_id", "");
                let preset_id = args.str_or("preset_id", "");

                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("add-preset-to-queue: missing session_id"));
                }
                if preset_id.is_empty() {
                    return Err(anyhow::anyhow!("add-preset-to-queue: missing preset_id"));
                }

                let (task_id, position) = preset::add_preset_to_queue(&host, &session_id, &preset_id)
                    .map_err(|e| anyhow::anyhow!(e))?;

                // 与手动 add-task 同语义：自动执行开启且会话空闲时立即调度
                if state::auto_execute_on(&host, &session_id) && !state::has_active_task(&host, &session_id) {
                    queue::try_dispatch_next(&host, &session_id);
                }

                let count_after = queue::pending_count(&host, &session_id);
                queue::broadcast_queue_changed(&host, &session_id, count_after, "add");
                preset::broadcast_preset_changed(&host, &preset_id, "enqueue");

                Ok(serde_json::json!({ "task_id": task_id, "position": position }))
            }
            "auto-task.remove-task" => {
                let session_id = args.str_or("session_id", "");
                let task_id = args.str_or("task_id", "");

                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("remove-task: missing session_id"));
                }
                if task_id.is_empty() {
                    return Err(anyhow::anyhow!("remove-task: missing task_id"));
                }

                let removed = queue::remove_task(&host, &session_id, &task_id);
                if !removed {
                    return Err(anyhow::anyhow!("remove-task: task not found: {}", task_id));
                }

                let remaining = queue::pending_count(&host, &session_id);
                queue::broadcast_queue_changed(&host, &session_id, remaining, "remove");

                Ok(serde_json::json!({ "removed": true }))
            }
            "auto-task.clear-queue" => {
                let session_id = args.str_or("session_id", "");

                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("clear-queue: missing session_id"));
                }

                let cleared = queue::clear_queue(&host, &session_id);
                queue::broadcast_queue_changed(&host, &session_id, 0, "clear");

                Ok(serde_json::json!({ "cleared": cleared }))
            }
            "auto-task.update-task" => {
                let session_id = args.str_or("session_id", "");
                let task_id = args.str_or("task_id", "");
                let prompt = args.str_or("prompt", "");

                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("update-task: missing session_id"));
                }
                if task_id.is_empty() {
                    return Err(anyhow::anyhow!("update-task: missing task_id"));
                }
                if prompt.is_empty() {
                    return Err(anyhow::anyhow!("update-task: missing prompt"));
                }

                let updated = queue::update_task(&host, &session_id, &task_id, &prompt);
                if !updated {
                    return Err(anyhow::anyhow!("update-task: task not found or not pending: {}", task_id));
                }

                let remaining = queue::pending_count(&host, &session_id);
                queue::broadcast_queue_changed(&host, &session_id, remaining, "update");

                Ok(serde_json::json!({ "updated": true }))
            }
            "auto-task.reorder-queue" => {
                let session_id = args.str_or("session_id", "");
                let task_ids: Vec<String> = args
                    .value("task_ids")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("reorder-queue: missing session_id"));
                }
                if task_ids.is_empty() {
                    return Err(anyhow::anyhow!("reorder-queue: missing task_ids"));
                }

                let reordered = queue::reorder_queue(&host, &session_id, &task_ids);
                if !reordered {
                    return Err(anyhow::anyhow!(
                        "reorder-queue: id set mismatch for session {}",
                        session_id
                    ));
                }

                let remaining = queue::pending_count(&host, &session_id);
                queue::broadcast_queue_changed(&host, &session_id, remaining, "reorder");

                Ok(serde_json::json!({ "reordered": true }))
            }
            "auto-task.list-session-configs" => {
                // 供前端定时任务表单选择会话配置（含 name/workingDir/command）
                let configs = host
                    .session_config_list()
                    .ok()
                    .flatten()
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default();
                Ok(serde_json::json!({ "configs": configs }))
            }
            "auto-task.list-scheduled-jobs" => {
                let jobs = scheduled::list_jobs(&host);
                Ok(serde_json::json!({ "jobs": jobs }))
            }
            "auto-task.create-scheduled-job" => {
                let name = args.str_or("name", "");
                let config_id = args.str_or("config_id", "");
                let trigger_at = args.str_or("trigger_at", "");
                let prompts: Vec<String> = args
                    .value("prompts")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();

                if config_id.is_empty() {
                    return Err(anyhow::anyhow!("create-scheduled-job: missing config_id"));
                }
                if trigger_at.is_empty() {
                    return Err(anyhow::anyhow!("create-scheduled-job: missing trigger_at"));
                }
                if prompts.is_empty() {
                    return Err(anyhow::anyhow!("create-scheduled-job: missing prompts"));
                }

                match scheduled::create_job_with_broadcast(&host, &name, &config_id, &trigger_at, &prompts) {
                    Some(job_id) => Ok(serde_json::json!({ "job_id": job_id })),
                    None => Err(anyhow::anyhow!(
                        "create-scheduled-job: failed to create job for config {}",
                        config_id
                    )),
                }
            }
            "auto-task.delete-scheduled-job" => {
                let job_id = args.str_or("job_id", "");
                if job_id.is_empty() {
                    return Err(anyhow::anyhow!("delete-scheduled-job: missing job_id"));
                }
                if scheduled::delete_job_with_broadcast(&host, &job_id) {
                    Ok(serde_json::json!({ "deleted": true }))
                } else {
                    Err(anyhow::anyhow!(
                        "delete-scheduled-job: job not found or not pending: {}",
                        job_id
                    ))
                }
            }
            "auto-task.scheduler-tick" => {
                // 宿主定时器到点回调（ADR 0003）：now_utc 与 SQLite datetime('now')
                // 同格式（UTC），WASM 无系统时钟，插件全部时间判断以宿主注入为准
                let now_utc = args.str_or("now_utc", "");
                if now_utc.is_empty() {
                    return Err(anyhow::anyhow!("scheduler-tick: missing now_utc"));
                }
                scheduled::handle_scheduler_tick(&host, &now_utc);
                Ok(serde_json::json!({ "ticked": true }))
            }
            _ => Err(anyhow::anyhow!("Unknown command: {}", name)),
        }
    }

    fn on_startup() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Auto Task plugin on_startup");

        // 1. 清理旧版全局 hooks
        hooks::cleanup_global_hooks(&host);

        // 2. 初始化插件独立数据库（建表 + 索引）
        // 宿主 plugin_db_execute 仅执行单条语句，schema 按语句数组逐条执行
        for stmt in TASK_HISTORY_SCHEMA {
            match host.plugin_db_execute(stmt) {
                Ok(_) => {}
                Err(e) => {
                    host.log_error(&format!("Failed to initialize task_history table: {}", e));
                    break;
                }
            }
        }
        host.log_info("task_history table initialized");

        // 2.0 初始化会话级开关表（auto_execute / auto_answer）
        for stmt in SESSION_SETTINGS_SCHEMA {
            match host.plugin_db_execute(stmt) {
                Ok(_) => {}
                Err(e) => {
                    host.log_error(&format!("Failed to initialize session_settings table: {}", e));
                    break;
                }
            }
        }
        host.log_info("session_settings table initialized");

        // 2.1 迁移：移除旧版 task_history.name 列（标题字段）
        // 旧库的 name 列为 NOT NULL，新 INSERT 不再写入会触发约束错误；
        // 先通过 PRAGMA table_info 确认列存在再 DROP：宿主 plugin_db_execute
        // 对失败语句会记 ERROR 日志，不能把可预期的"无此列"场景当作错误处理
        let legacy_name_column_exists = host
            .plugin_db_query("PRAGMA table_info(task_history)")
            .ok()
            .flatten()
            .and_then(|v| v.as_array().cloned())
            .map(|rows| {
                rows.iter().any(|row| {
                    row.get("name")
                        .and_then(|n| n.as_str())
                        .map(|n| n == "name")
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        if legacy_name_column_exists {
            match host.plugin_db_execute("ALTER TABLE task_history DROP COLUMN name") {
                Ok(_) => host.log_info("task_history migration: dropped legacy 'name' column"),
                Err(e) => host.log_warn(&format!(
                    "task_history migration: failed to drop 'name' column: {}",
                    e
                )),
            }
        } else {
            host.log_debug("task_history migration: 'name' column not present, skip drop");
        }

        // 3. 初始化 session 映射表
        for stmt in SESSION_MAPPING_SCHEMA {
            match host.plugin_db_execute(stmt) {
                Ok(_) => {}
                Err(e) => {
                    host.log_error(&format!("Failed to initialize session_mapping table: {}", e));
                    break;
                }
            }
        }
        host.log_info("session_mapping table initialized");

        // 4. 初始化任务队列表
        for stmt in queue::TASK_QUEUE_SCHEMA {
            match host.plugin_db_execute(stmt) {
                Ok(_) => {}
                Err(e) => {
                    host.log_error(&format!("Failed to initialize task_queue table: {}", e));
                    break;
                }
            }
        }
        host.log_info("task_queue table initialized");

        // 4.1 迁移：旧库 task_queue 无 dispatch_attempts 列（调度重试计数），按需补建
        ensure_column(&host, "task_queue", "dispatch_attempts", "ALTER TABLE task_queue ADD COLUMN dispatch_attempts INTEGER NOT NULL DEFAULT 0");

        // 4.2 迁移：task_queue.source 列（queue / scheduled，定时任务 v6 引入）
        ensure_column(&host, "task_queue", "source", "ALTER TABLE task_queue ADD COLUMN source TEXT NOT NULL DEFAULT 'queue'");

        // 4.3 迁移：task_history 预留 token 统计列（v1 不解析，JSONL 深化需求回填）
        ensure_column(&host, "task_history", "input_tokens", "ALTER TABLE task_history ADD COLUMN input_tokens INTEGER");
        ensure_column(&host, "task_history", "output_tokens", "ALTER TABLE task_history ADD COLUMN output_tokens INTEGER");

        // 4.3.1 迁移：task_history.event_time 列（事件发生时刻，时序保护基线）。
        // 脚本每次推送携带 event_time，宿主仅在 event_time >= 行内已应用事件时应用，
        // 拒绝网络阻塞导致的迟到旧事件覆盖最新状态（状态回跳 / 队列调度错乱）
        ensure_column(&host, "task_history", "event_time", "ALTER TABLE task_history ADD COLUMN event_time TEXT");

        // 4.4 初始化预设任务表（无会话/未选会话时创建的待投递任务，一次性消耗）
        for stmt in preset::PRESET_TASKS_SCHEMA {
            match host.plugin_db_execute(stmt) {
                Ok(_) => {}
                Err(e) => {
                    host.log_error(&format!("Failed to initialize preset_tasks table: {}", e));
                    break;
                }
            }
        }
        host.log_info("preset_tasks table initialized");

        // 5. 初始化定时自动任务表
        for stmt in SCHEDULED_JOBS_SCHEMA {
            match host.plugin_db_execute(stmt) {
                Ok(_) => {}
                Err(e) => {
                    host.log_error(&format!("Failed to initialize scheduled_jobs table: {}", e));
                    break;
                }
            }
        }
        host.log_info("scheduled_jobs table initialized");

        // 5.1 迁移：旧库 scheduled_jobs 无 session_id 列（触发时会话关联键）
        ensure_column(&host, "scheduled_jobs", "session_id", "ALTER TABLE scheduled_jobs ADD COLUMN session_id TEXT");

        // 6. 启动恢复 + 定时器注册（定时自动任务，ADR 0003）
        // 重启前处于 creating 态的任务：其会话已随上次进程退出而丢失，
        // 无法等到 Created 事件，标记 failed 避免永久卡在中间态
        scheduled::recover_creating_jobs(&host);

        // 宿主周期定时器：到点回调 scheduler-tick command（附当前时间），
        // 到期/错过判定全部在插件侧以 DB trigger_at 完成（幂等归插件）
        match host.timer_register(scheduled::SCHEDULER_INTERVAL_SECS, "auto-task.scheduler-tick") {
            Ok(()) => host.log_info(&format!(
                "Scheduler timer registered: interval={}s",
                scheduled::SCHEDULER_INTERVAL_SECS
            )),
            Err(e) => host.log_error(&format!("Failed to register scheduler timer: {}", e)),
        }

        Ok(())
    }

    fn on_shutdown() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Auto Task plugin on_shutdown");

        // 应用关闭时清理所有项目的 hooks 配置
        // 确保退出后不残留引用已停止服务的 hooks
        let result = hooks::cleanup_all_project_hooks(&host);
        host.log_info(&format!(
            "Hooks cleanup on shutdown: cleaned={}, skipped={}, failed={}",
            result.cleaned, result.skipped, result.failed
        ));

        Ok(())
    }

    fn on_session_lifecycle(event: &SessionLifecycleEvent) -> anyhow::Result<()> {
        match event {
            // creating：会话创建前（同步阻塞），为 Claude 会话准备项目级 hooks。
            // resource_dir 由宿主注入，指向插件安装目录（包含 auto_task_hook.py）
            SessionLifecycleEvent::Creating { command, working_dir, resource_dir, .. } => {
                let host = WasmHost;

                host.log_debug(&format!(
                    "on_session_lifecycle: Creating event command={:?} working_dir={:?} resource_dir={:?}",
                    command, working_dir, resource_dir
                ));

                // 只为 Claude 命令设置 hooks
                if !command.to_lowercase().contains("claude") {
                    host.log_debug(&format!(
                        "on_session_lifecycle: command {:?} does not contain 'claude', skip hooks setup",
                        command
                    ));
                    return Ok(());
                }

                // 读取宿主配置（port）
                // hook 脚本通过 HTTP 推送任务状态，端点由网关中间件本地放行，无需 token
                let port = host.config_get(ConfigKey::NetworkPort)
                    .ok()
                    .flatten()
                    .and_then(|s| s.parse::<u16>().ok())
                    .unwrap_or(8765);
                host.log_debug(&format!(
                    "on_session_lifecycle: claude session detected, using port={}",
                    port
                ));

                let result = hooks::ensure_project_hooks(&host, working_dir, port, resource_dir);
                host.log_debug(&format!(
                    "on_session_lifecycle: ensure_project_hooks result success={} skipped={} message={:?}",
                    result.success, result.skipped, result.message
                ));

                if result.success {
                    host.log_info(&format!("Session lifecycle: hooks setup for {}", working_dir));
                } else if result.skipped {
                    host.log_debug(&format!("Session lifecycle: hooks skipped for {}", working_dir));
                } else {
                    host.log_warn(&format!("Session lifecycle: hooks setup failed for {}: {}", working_dir, result.message));
                }
            }
            // Created：会话创建完成（PTY 已启动），定时自动任务的会话就绪信号：
            // 按 session_id 匹配处于 creating 态的定时任务，把 prompts 注入队列
            // （创建时机与字段见 ADR 0003）
            SessionLifecycleEvent::Created { session_id, config_id, .. } => {
                let host = WasmHost;
                host.log_debug(&format!(
                    "on_session_lifecycle: Created event session_id={} config_id={}",
                    session_id, config_id
                ));
                scheduled::handle_session_created(&host, session_id, config_id);
            }
            _ => {}
        }
        Ok(())
    }

    fn on_input_submitted(event: &InputSubmittedEvent) -> anyhow::Result<()> {
        let host = WasmHost;

        // 业务侧过滤：宿主不做语义过滤（空提交同样通知），空行回车直接忽略
        if event.text.trim().is_empty() {
            return Ok(());
        }

        host.log_info(&format!(
            "InputSubmitted: session={}, len={}, text={:?}",
            event.session_id,
            event.text.len(),
            event.text
        ));

        // 命令过滤（ADR-0004）：以 / 开头的提交行是 CLI 命令（如 /clear、/model），
        // 不产生任务记录。白名单预留（未来 /skills 等任务型命令放行）
        if agent::is_command_input(&event.text) {
            host.log_debug(&format!(
                "InputSubmitted: session={} input is a command, skip task creation: {:?}",
                event.session_id,
                event.text.trim_start().chars().take(32).collect::<String>()
            ));
            return Ok(());
        }

        // 仅在 Claude Code 会话中把输入当作任务：非 claude 启动命令的会话直接忽略
        if !state::session_command_is_claude(&host, &event.session_id) {
            host.log_debug(&format!(
                "InputSubmitted: session={} is not a claude session, skip task creation",
                event.session_id
            ));
            return Ok(());
        }

        // 会话已有进行中的任务则不再创建：队列调度由插件自身 terminal_send 投递输入，
        // 此时最新记录已置为 in_progress，依赖此检查避免自触发循环（见 ADR 0001）
        if state::has_active_task(&host, &event.session_id) {
            return Ok(());
        }

        // 把输入作为当前任务写入任务历史（写表职责已从 Claude Code 输入 hook 移交宿主）
        state::create_task_from_input(&host, &event.session_id, &event.text);

        Ok(())
    }
}

bedcode_plugin_api::wasm_entry!(AutoTaskPlugin);

// ==================== 辅助函数 ====================

/// 按需补建表列（幂等迁移）：PRAGMA table_info 确认列不存在才执行 ALTER，
/// 避免可预期的"duplicate column"错误污染宿主 ERROR 日志
fn ensure_column(host: &bedcode_plugin_api::wasm_host::WasmHost, table: &str, column: &str, alter_sql: &str) {
    use bedcode_plugin_api::host::HostPluginDatabase;

    let exists = host
        .plugin_db_query(&format!("PRAGMA table_info({})", table))
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .map(|rows| {
            rows.iter().any(|row| {
                row.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n == column)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    if exists {
        host.log_debug(&format!("migration: {}.{} already exists, skip", table, column));
        return;
    }

    match host.plugin_db_execute(alter_sql) {
        Ok(_) => host.log_info(&format!("migration: added {}.{}", table, column)),
        Err(e) => host.log_warn(&format!("migration: failed to add {}.{}: {}", table, column, e)),
    }
}

/// 从命令参数组装任务历史查询筛选条件
///
/// 支持字段：session_id / status / agent / source / since / until / limit / offset
fn task_history_filter_from_args(args: &bedcode_plugin_api::CommandArgs) -> state::TaskHistoryFilter {
    let opt = |key: &str| -> Option<String> {
        let v = args.str_or(key, "");
        if v.is_empty() { None } else { Some(v) }
    };
    state::TaskHistoryFilter {
        session_id: opt("session_id"),
        status: opt("status"),
        agent: opt("agent"),
        source: opt("source"),
        since: opt("since"),
        until: opt("until"),
        limit: args.value("limit").and_then(|v| v.as_i64()).unwrap_or(100),
        offset: args.value("offset").and_then(|v| v.as_i64()).unwrap_or(0),
    }
}
