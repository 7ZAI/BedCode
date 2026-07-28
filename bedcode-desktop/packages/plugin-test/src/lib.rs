//! 测试用 WASM 插件
//!
//! 用于胶水层签名验证和连通性测试（宿主 `wasm_runtime` 测试套件加载本插件）。
//! 覆盖全部 host function 调用路径：存储 / 数据库 / 配置 / 日志 /
//! 事件 / 会话 / 文件系统 / 广播 / 消息总线 / 通知。

use bedcode_plugin_api::events::SyncEvent;
use bedcode_plugin_api::host::{
    ConfigKey, HostBus, HostConfig, HostEvents, HostFs, HostLog, HostPluginDatabase, HostSession,
    HostStorage,
};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm::WasmPlugin;
use bedcode_plugin_api::wasm_host::WasmHost;

/// 测试插件 — 覆盖所有 host function 调用
pub struct TestPlugin;

impl WasmPlugin for TestPlugin {
    const ID: &'static str = "com.bedcode.test";

    fn manifest() -> PluginManifest {
        let json = serde_json::json!({
            "id": Self::ID,
            "name": "Test Plugin",
            "version": "0.1.0",
            "description": "Glue layer test plugin",
            "author": "BedCode",
            "main": "index.js",
            "sandbox": "inline",
            "pluginType": "rust-ts",
            "rustLibrary": "bedcode_plugin_test",
            "permissions": ["storage", "broadcast", "terminal:input", "terminal:output", "session:read", "fs:read", "fs:write"],
            "contributes": {
                "commands": [{ "id": "test.echo", "title": "Echo" }],
                "views": [],
                "lifecycle": { "onStartup": false, "onShutdown": false },
                "provides": []
            }
        });
        serde_json::from_value(json).expect("Invalid manifest JSON")
    }

    fn activate() -> anyhow::Result<()> {
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let host = WasmHost;

        match name {
            "test.echo" => Ok(args),
            "test_storage" => {
                let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("test_key");
                let value = args.get("value").cloned().unwrap_or(serde_json::json!("test_value"));
                host.storage_set(key, &value)?;
                let got = host.storage_get(key)?.unwrap_or(serde_json::Value::Null);
                Ok(serde_json::json!({ "set": value, "got": got }))
            }
            "test_db" => {
                let create = "CREATE TABLE IF NOT EXISTS plugin_com_bedcode_test_data (id INTEGER PRIMARY KEY, val TEXT)";
                host.plugin_db_execute(create)?;
                host.plugin_db_execute("INSERT OR REPLACE INTO plugin_com_bedcode_test_data (id, val) VALUES (1, 'hello')")?;
                let rows = host.plugin_db_query("SELECT val FROM plugin_com_bedcode_test_data WHERE id = 1")?
                    .unwrap_or(serde_json::Value::Null);
                Ok(serde_json::json!({ "rows": rows }))
            }
            "test_config" => {
                let port = host.config_get(ConfigKey::NetworkPort)?.unwrap_or_default();
                Ok(serde_json::json!({ "port": port }))
            }
            "test_log" => {
                host.log_info("test info");
                host.log_debug("test debug");
                host.log_warn("test warn");
                host.log_error("test error");
                Ok(serde_json::json!({ "logged": true }))
            }
            "test_emit" => {
                host.emit_event("test-event", &serde_json::json!({ "source": "test_plugin" }));
                Ok(serde_json::json!({ "emitted": true }))
            }
            "test_session_list" => {
                let sessions = host.session_list()?.unwrap_or(serde_json::Value::Null);
                Ok(serde_json::json!({ "sessions": sessions }))
            }
            "test_fs" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("hello fs");
                host.fs_write(path, content)?;
                let read = host.fs_read(path)?.unwrap_or_default();
                Ok(serde_json::json!({ "wrote": content, "read": read }))
            }
            "test_broadcast" => {
                host.broadcast_sync(&SyncEvent::TaskStatusChanged {
                    session_id: "test-session".to_string(),
                    task_status: "completed".to_string(),
                    task_reason: None,
                    task_questions: None,
                });
                Ok(serde_json::json!({ "broadcast": true }))
            }
            "test_bus" => {
                host.bus_publish("test:topic", &serde_json::json!({ "msg": "hello" }))?;
                Ok(serde_json::json!({ "published": true }))
            }
            "test_notify" => {
                host.notify("test title", "test body")?;
                Ok(serde_json::json!({ "notified": true }))
            }
            _ => Err(anyhow::anyhow!("Unknown command: {}", name)),
        }
    }

    fn on_terminal_input(_session_id: &str, text: &str) -> Option<String> {
        Some(text.to_uppercase())
    }

    fn on_terminal_output(_session_id: &str, data: &str) -> Option<String> {
        Some(data.to_uppercase())
    }
}

bedcode_plugin_api::wasm_entry!(TestPlugin);
