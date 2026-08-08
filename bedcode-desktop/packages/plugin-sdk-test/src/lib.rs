//! SDK 组件形态测试插件（迁移阶段 B 验证）
//!
//! 与 `plugin-component-test`（手写 wit-bindgen 绑定）区分：本插件走真实 SDK
//! 链路 —— `WasmPlugin` trait 实现 + `wasm_entry!` 宏（生成组件 world 导出）
//! + `WasmHost`（组件 import 调用宿主）。验证：
//! - `wasm_entry!` 宏产物的组件导出（宿主加载 + abi form=1 协商）
//! - `WasmHost` 各 host trait 经组件 import 的正确往返
//!   （storage / 主库 db / config / session / events / bus / log / notify）
//!
//! 宿主测试构建本 crate 后以 `wit_component::ComponentEncoder` 编码为组件
//! （等价于 `wasm-tools component new`，生产插件构建脚本内置同一编码步骤）。

use bedcode_plugin_api::host::{
    ConfigKey, HostBus, HostConfig, HostDatabase, HostEvents, HostLog, HostSession, HostStorage,
};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm::WasmPlugin;
use bedcode_plugin_api::wasm_host::WasmHost;

/// SDK 测试插件 — 覆盖组件 import 的主要能力
pub struct SdkTestPlugin;

impl WasmPlugin for SdkTestPlugin {
    const ID: &'static str = "com.bedcode.sdk-test";

    fn manifest() -> PluginManifest {
        let json = serde_json::json!({
            "id": Self::ID,
            "name": "SDK Test Plugin",
            "version": "0.1.0",
            "description": "SDK component bindings test plugin",
            "author": "BedCode",
            "main": "index.js",
            "sandbox": "inline",
            "pluginType": "rust-ts",
            "rustLibrary": "bedcode_plugin_sdk_test",
            "permissions": ["storage", "broadcast", "terminal:input", "terminal:output", "session:read"],
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
                let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("sdk_test_key");
                let value = args.get("value").cloned().unwrap_or(serde_json::json!("sdk_value"));
                host.storage_set(key, &value)?;
                let got = host.storage_get(key)?.unwrap_or(serde_json::Value::Null);
                Ok(serde_json::json!({ "set": value, "got": got }))
            }
            // 主库：表名带插件前缀（宿主侧前缀校验，防跨插件数据访问）。
            // 宿主测试以 TEST_PLUGIN_ID（com.bedcode.test）实例化，前缀按此派生
            "test_db" => {
                let table = "plugin_com_bedcode_test_sdk_data";
                host.db_execute(&format!(
                    "CREATE TABLE IF NOT EXISTS {} (id INTEGER PRIMARY KEY, val TEXT)",
                    table
                ))?;
                host.db_execute(&format!(
                    "INSERT OR REPLACE INTO {} (id, val) VALUES (1, 'sdk-db')",
                    table
                ))?;
                let rows = host
                    .db_query(&format!("SELECT val FROM {} WHERE id = 1", table))?
                    .unwrap_or(serde_json::Value::Null);
                Ok(serde_json::json!({ "rows": rows }))
            }
            "test_config" => {
                let port = host.config_get(ConfigKey::NetworkPort)?.unwrap_or_default();
                Ok(serde_json::json!({ "port": port }))
            }
            "test_log" => {
                host.log_info("sdk test info");
                host.log_debug("sdk test debug");
                host.log_warn("sdk test warn");
                host.log_error("sdk test error");
                Ok(serde_json::json!({ "logged": true }))
            }
            "test_emit" => {
                host.emit_event("sdk-test-event", &serde_json::json!({ "source": "sdk_test" }));
                Ok(serde_json::json!({ "emitted": true }))
            }
            "test_session_list" => {
                let sessions = host.session_list()?.unwrap_or(serde_json::Value::Null);
                Ok(serde_json::json!({ "sessions": sessions }))
            }
            "test_bus" => {
                host.bus_publish("sdk:topic", &serde_json::json!({ "msg": "sdk-hello" }))?;
                Ok(serde_json::json!({ "published": true }))
            }
            // 无头测试上下文无 AppHandle：宿主 notify 返回错误，验证错误透传
            "test_notify" => {
                host.notify("sdk title", "sdk body")?;
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

bedcode_plugin_api::wasm_entry!(SdkTestPlugin);
