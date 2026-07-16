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
mod token;
mod state;

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
            "permissions": ["storage", "broadcast", "terminal:input", "terminal:output", "session:read", "fs:read", "fs:write"],
            "contributes": {
                "commands": [
                    { "id": "auto-task.setup-project-hooks", "title": "Setup Project Hooks" },
                    { "id": "auto-task.cleanup-project-hooks", "title": "Cleanup Project Hooks" },
                    { "id": "auto-task.get-task-status", "title": "Get Task Status" },
                    { "id": "auto-task.set-auto-mode", "title": "Set Auto Mode" }
                ],
                "lifecycle": {
                    "onStartup": true,
                    "onShutdown": true
                }
            }
        });
        serde_json::from_value(json).expect("Invalid manifest JSON")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost::new(Self::ID);
        host.log_info("Auto Task plugin activated");
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
                Ok(state::handle_http_endpoint(&host, method, path, &body, &query))
            }
            "setup-project-hooks" => {
                let working_dir = args.get("working_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let port = args.get("port")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(8765) as u16;
                let token = args.get("token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let resource_dir = args.get("resource_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let result = hooks::ensure_project_hooks(&host, &working_dir, port, &token, &resource_dir);

                Ok(serde_json::json!({
                    "success": result.success,
                    "message": result.message,
                    "skipped": result.skipped,
                }))
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

        // 2. Token 校验
        let _token_result = token::ensure_token(&host);

        // 3. 初始化插件独立数据库（建表 + 索引）
        let affected = host.plugin_db_execute(TASK_HISTORY_SCHEMA);
        if affected < 0 {
            host.log_error("Failed to initialize task_history table");
        } else {
            host.log_info("task_history table initialized");
        }

        Ok(())
    }

    fn on_shutdown() -> anyhow::Result<()> {
        let host = WasmHost::new(Self::ID);
        host.log_info("Auto Task plugin on_shutdown");
        Ok(())
    }
}

bedcode_plugin_api::wasm_entry!(AutoTaskPlugin);
