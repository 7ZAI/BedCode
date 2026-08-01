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

use bedcode_plugin_api::events::{InputSubmittedEvent, SessionLifecycleEvent};
use bedcode_plugin_api::host::{ConfigKey, HostConfig, HostLog, HostPluginDatabase, HostSession};
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
    name            TEXT NOT NULL,
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
/// UserPromptSubmit 时才真正创建 task_history 行，此时 name 有值。
const SESSION_MAPPING_SCHEMA: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS session_mapping (
    claude_sid  TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    created_at  TEXT NOT NULL
)"#,
    "CREATE INDEX IF NOT EXISTS idx_session_mapping_session ON session_mapping(session_id)",
];

struct AutoTaskPlugin;

impl WasmPlugin for AutoTaskPlugin {
    const ID: &'static str = "com.bedcode.auto-task";

    fn manifest() -> PluginManifest {
        let json = serde_json::json!({
            "id": "com.bedcode.auto-task",
            "name": "Auto Task",
            "version": "1.0.0",
            "description": "Claude Code 任务状态同步与自动授权",
            "author": "BedCode",
            "main": "index.js",
            "sandbox": "inline",
            "pluginType": "rust-ts",
            "rustLibrary": "bedcode_plugin_auto_task",
            "permissions": ["storage", "broadcast", "terminal:input", "terminal:output", "terminal:observe", "session:read", "fs:read", "fs:write", "ui:sidebar"],
            "contributes": {
                "commands": [
                    { "id": "auto-task.cleanup-project-hooks", "title": "Cleanup Project Hooks" },
                    { "id": "auto-task.get-task-status", "title": "Get Task Status" },
                    { "id": "auto-task.set-auto-mode", "title": "Set Auto Mode" },
                    { "id": "auto-task.list-task-history", "title": "List Task History" },
                    { "id": "auto-task.list-task-queue", "title": "List Task Queue by Session" }
                ],
                "views": [
                    { "id": "auto-task.history", "type": "sidebar", "title": "任务历史", "component": "TaskHistoryView", "icon": "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01" }
                ],
                "lifecycle": {
                    "onStartup": true,
                    "onShutdown": true
                },
                "provides": ["task:status-changed", "session:mode-changed", "task:queue-changed"]
            }
        });
        serde_json::from_value(json).expect("Invalid manifest JSON")
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
                let session_id = args.str_or("session_id", "");
                state::list_task_history(&host, &session_id)
            }
            "auto-task.list-task-queue" => {
                let session_id = args.str_or("session_id", "");
                let tasks = queue::list_queue(&host, &session_id);
                Ok(serde_json::json!({ "tasks": tasks, "session_id": session_id }))
            }
            "auto-task.set-auto-mode" => {
                let session_id = args.str_or("session_id", "");
                let auto_approve = args.bool_or("auto_approve", false);
                state::set_auto_mode(&host, &session_id, auto_approve)
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

                // 只为 Claude 命令设置 hooks
                if !command.to_lowercase().contains("claude") {
                    return Ok(());
                }

                // 读取宿主配置（port）
                // hook 脚本通过 HTTP 推送任务状态，端点由网关中间件本地放行，无需 token
                let port = host.config_get(ConfigKey::NetworkPort)
                    .ok()
                    .flatten()
                    .and_then(|s| s.parse::<u16>().ok())
                    .unwrap_or(8765);

                let result = hooks::ensure_project_hooks(&host, working_dir, port, resource_dir);

                if result.success {
                    host.log_info(&format!("Session lifecycle: hooks setup for {}", working_dir));
                } else if result.skipped {
                    host.log_debug(&format!("Session lifecycle: hooks skipped for {}", working_dir));
                } else {
                    host.log_warn(&format!("Session lifecycle: hooks setup failed for {}: {}", working_dir, result.message));
                }
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

        // TODO(auto-task): 基于提交输入行的业务处理（如任务队列指令解析、输入审计统计）。
        // 当前仅观察日志；注意回调中避免调用 terminal_send 造成自触发循环（见 ADR 0001）
        Ok(())
    }
}

bedcode_plugin_api::wasm_entry!(AutoTaskPlugin);
