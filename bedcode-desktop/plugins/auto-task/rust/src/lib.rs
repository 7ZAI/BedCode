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

use bedcode_plugin_api::{WasmHost, WasmPlugin};
use bedcode_plugin_api::types::PluginManifest;

/// 任务历史表建表 SQL
const TASK_HISTORY_SCHEMA: &str = r#"
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
);
CREATE INDEX IF NOT EXISTS idx_task_history_status ON task_history(status);
CREATE INDEX IF NOT EXISTS idx_task_history_session_id ON task_history(session_id);
CREATE INDEX IF NOT EXISTS idx_task_history_created_at ON task_history(created_at);
"#;

/// Claude Code session ↔ BedCode PTY session 映射表建表 SQL
///
/// SessionStart 时仅存储映射关系，不创建空壳任务记录。
/// UserPromptSubmit 时才真正创建 task_history 行，此时 name 有值。
const SESSION_MAPPING_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS session_mapping (
    claude_sid  TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_session_mapping_session ON session_mapping(session_id);
"#;

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
            "permissions": ["storage", "broadcast", "terminal:input", "terminal:output", "session:read", "fs:read", "fs:write", "ui:sidebar"],
            "contributes": {
                "commands": [
                    { "id": "auto-task.cleanup-project-hooks", "title": "Cleanup Project Hooks" },
                    { "id": "auto-task.get-task-status", "title": "Get Task Status" },
                    { "id": "auto-task.set-auto-mode", "title": "Set Auto Mode" },
                    { "id": "auto-task.add-task", "title": "Add Task to Queue" },
                    { "id": "auto-task.remove-task", "title": "Remove Task from Queue" },
                    { "id": "auto-task.list-queue", "title": "List Task Queue" },
                    { "id": "auto-task.clear-queue", "title": "Clear Task Queue" },
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
        let host = WasmHost::new(Self::ID);
        host.log_info("Auto Task plugin activated");

        // 注册会话生命周期监听器
        // 会话创建前会收到 creating 事件，用于自动设置项目 hooks
        if host.session_lifecycle_register() {
            host.log_info("Registered session lifecycle listener");
        } else {
            host.log_error("Failed to register session lifecycle listener");
        }

        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let host = WasmHost::new(Self::ID);
        host.log_info("Auto Task plugin deactivated");
        Ok(())
    }

    fn invoke_command(name: &str, args_json: &str) -> anyhow::Result<serde_json::Value> {
        let host = WasmHost::new(Self::ID);
        let args: serde_json::Value = serde_json::from_str(args_json).unwrap_or(serde_json::json!({}));

        match name {
            "_http_endpoint" => {
                let method = args.get("method").and_then(|v| v.as_str()).unwrap_or("");
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let body = args.get("body").cloned().unwrap_or(serde_json::Value::Null);
                let query = args.get("query").cloned().unwrap_or(serde_json::json!({}));

                // 队列端点路由
                if path.starts_with("task-queue/") {
                    let queue_path = path.strip_prefix("task-queue/").unwrap_or("");
                    Ok(queue::handle_queue_http(&host, method, queue_path, &body, &query))
                } else {
                    Ok(state::handle_http_endpoint(&host, method, path, &body, &query))
                }
            }
            "cleanup-project-hooks" => {
                let working_dir = args.get("working_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let result = hooks::cleanup_project_hooks(&host, &working_dir);

                Ok(serde_json::json!({
                    "success": result.success,
                    "message": result.message,
                }))
            }
            "get-task-status" => {
                let session_id = args.get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                state::get_task_status(&host, session_id)
            }
            "list-task-history" => {
                let session_id = args.get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                state::list_task_history(&host, session_id)
            }
            "list-task-queue" => {
                let session_id = args.get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let tasks = queue::list_queue(&host, session_id);
                Ok(serde_json::json!({ "tasks": tasks, "session_id": session_id }))
            }
            "set-auto-mode" => {
                let session_id = args.get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let auto_approve = args.get("auto_approve")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                state::set_auto_mode(&host, session_id, auto_approve)
            }
            _ => Err(anyhow::anyhow!("Unknown command: {}", name)),
        }
    }

    fn on_startup() -> anyhow::Result<()> {
        let host = WasmHost::new(Self::ID);
        host.log_info("Auto Task plugin on_startup");

        // 1. 清理旧版全局 hooks
        hooks::cleanup_global_hooks(&host);

        // 2. 初始化插件独立数据库（建表 + 索引）
        let affected = host.plugin_db_execute(TASK_HISTORY_SCHEMA);
        if affected < 0 {
            host.log_error("Failed to initialize task_history table");
        } else {
            host.log_info("task_history table initialized");
        }

        // 3. 初始化 session 映射表
        let affected = host.plugin_db_execute(SESSION_MAPPING_SCHEMA);
        if affected < 0 {
            host.log_error("Failed to initialize session_mapping table");
        } else {
            host.log_info("session_mapping table initialized");
        }

        // 4. 初始化任务队列表
        let affected = host.plugin_db_execute(queue::TASK_QUEUE_SCHEMA);
        if affected < 0 {
            host.log_error("Failed to initialize task_queue table");
        } else {
            host.log_info("task_queue table initialized");
        }

        Ok(())
    }

    fn on_shutdown() -> anyhow::Result<()> {
        let host = WasmHost::new(Self::ID);
        host.log_info("Auto Task plugin on_shutdown");
        Ok(())
    }

    fn on_session_lifecycle(event: &serde_json::Value) -> anyhow::Result<()> {
        let event_type = event.get("event_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match event_type {
            "creating" => {
                let host = WasmHost::new(Self::ID);

                let command = event.get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // 只为 Claude 命令设置 hooks
                if !command.to_lowercase().contains("claude") {
                    return Ok(());
                }

                let working_dir = event.get("working_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                // resource_dir 由宿主注入，指向插件安装目录（包含 bedcode_hook.py）
                let resource_dir = event.get("resource_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                // 读取宿主配置（port + token）
                // token 必须非空，否则 hooks 中的 BEDCODE_TOKEN 环境变量为空，hook 脚本无法推送状态
                let port = host.config_get("network.port")
                    .and_then(|s| s.parse::<u16>().ok())
                    .unwrap_or(8765);
                let token = host.config_get("plugin.token")
                    .unwrap_or_default();

                if token.is_empty() {
                    host.log_error("Session lifecycle: plugin token not available, hooks require BEDCODE_TOKEN for HTTP push");
                    return Ok(());
                }

                let result = hooks::ensure_project_hooks(&host, &working_dir, port, &token, &resource_dir);

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
}

bedcode_plugin_api::wasm_entry!(AutoTaskPlugin);
