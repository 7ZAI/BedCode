//! Auto Task Plugin (WASM — Mobile)
//!
//! 移动端自动任务队列管理的 Rust 后端
//! 极简实现：仅 activate/deactivate 日志，业务逻辑由 TS 前端通过 HTTP API 完成

use bedcode_plugin_api_mobile::{HostLog, WasmHost, WasmPlugin};
use bedcode_plugin_api_mobile::types::PluginManifest;

struct AutoTaskPlugin;

impl WasmPlugin for AutoTaskPlugin {
    const ID: &'static str = "com.bedcode.auto-task";

    fn manifest() -> PluginManifest {
        let json = serde_json::json!({
            "id": "com.bedcode.auto-task",
            "name": "Auto Task",
            "version": "1.0.0",
            "description": "Auto task queue management",
            "author": "BedCode",
            "main": "index.js",
            "pluginType": "wasm",
            "rustLibrary": "bedcode_plugin_auto_task",
            "permissions": ["ui:input", "storage", "session:read"],
            "contributes": {
                "commands": [
                    { "id": "auto-task.list-queue", "title": "List Task Queue" },
                    { "id": "auto-task.add-task", "title": "Add Task to Queue" },
                    { "id": "auto-task.remove-task", "title": "Remove Task from Queue" },
                    { "id": "auto-task.clear-queue", "title": "Clear Task Queue" }
                ],
                "terminal": {
                    "inputHandlers": [],
                    "outputParsers": [],
                    "toolbarItems": [
                        { "id": "auto-task-toolbar", "title": "Auto Task", "icon": "📋" }
                    ]
                },
                "lifecycle": {
                    "onAuthSuccess": true,
                    "onDisconnect": true,
                    "onSessionCreated": true,
                    "onSessionStopped": true
                }
            }
        });
        serde_json::from_value(json).expect("Invalid manifest JSON")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Auto Task plugin activated (mobile)");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Auto Task plugin deactivated (mobile)");
        Ok(())
    }

    fn invoke_command(name: &str, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        // 移动端不执行命令，业务逻辑由 TS 前端通过 HTTP API 完成
        Err(anyhow::anyhow!("Unknown command: {}", name))
    }
}

bedcode_plugin_api_mobile::wasm_entry!(AutoTaskPlugin);
