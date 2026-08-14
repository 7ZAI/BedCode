//! SDK 组件形态测试插件（迁移阶段 B 验证）
//!
//! 与 `plugin-component-test`（手写 wit-bindgen 绑定）区分：本插件走真实 SDK
//! 链路 —— `WasmPlugin` trait 实现 + `wasm_entry!` 宏（生成组件 world 导出）
//! + `WasmHost`（组件 import 调用宿主）。验证：
//! - `wasm_entry!` 宏产物的组件导出（宿主加载 + abi form=1 协商）
//! - `WasmHost` 各 host trait 经组件 import 的正确往返
//!   （storage / 主库 db / config / session / events / bus / log / notify）
//! - 插件互调机制（issue 04，ADR-0017）：`#[plugin_api]` 宏生成的实现方分派
//!   + 调用方 client + 构建期防漂移比对（trait 方法 vs 本目录 plugin.json 的
//!   `api` 字段，不一致构建失败）
//!
//! 宿主测试构建本 crate 后以 `wit_component::ComponentEncoder` 编码为组件
//! （等价于 `wasm-tools component new`，生产插件构建脚本内置同一编码步骤）。

use bedcode_plugin_api::host::{
    ConfigKey, HostBus, HostConfig, HostDatabase, HostEvents, HostLog, HostSession, HostStorage,
};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm::WasmPlugin;
use bedcode_plugin_api::wasm_host::WasmHost;
use bedcode_plugin_api::{BusMessage, plugin_api};

/// 插件互调 api 声明（issue 04）：trait 方法名 ↔ manifest.api 条目
/// （`com.bedcode.sdk-test.<method>`），宏在编译期比对防漂移
#[plugin_api]
pub trait SdkTestApi {
    /// 回声：参数原样回传（请求/响应配对成功验收）
    fn echo(text: String) -> Result<String, String>;
    /// 恒失败：目标方法返回 error（错误传播验收）
    fn fail() -> Result<String, String>;
}

/// SDK 测试插件 — 覆盖组件 import 的主要能力
pub struct SdkTestPlugin;

impl SdkTestApi for SdkTestPlugin {
    fn echo(text: String) -> Result<String, String> {
        Ok(format!("echo: {}", text))
    }

    fn fail() -> Result<String, String> {
        Err("boom".to_string())
    }
}

impl WasmPlugin for SdkTestPlugin {
    const ID: &'static str = "com.bedcode.sdk-test";

    fn manifest() -> PluginManifest {
        // ADR-0005 单一真源：plugin.json（与 #[plugin_api] 防漂移比对同一份）
        serde_json::from_str(include_str!("../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        // 订阅互调请求 topic（宏生成）：`bedcode.api.<api>` 逐个订阅，
        // 宿主订阅去重幂等
        SdkTestApiDispatcher::register()?;
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        Ok(())
    }

    /// 总线消息入口：互调请求先经宏生成的分派器（命中 api topic 则处理并回复），
    /// 其余消息保持原语义（本插件无其他订阅，直接忽略）
    fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
        SdkTestApiDispatcher::dispatch::<Self>(msg)?;
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
            // ==================== 插件互调（issue 04） ====================

            // 调用方 client：请求/响应配对成功（目标 com.bedcode.sdk-test 由宿主
            // 测试以第二个实例加载；本命令由 caller 实例调用）
            "test_api_echo" => {
                let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("hello");
                let client = SdkTestApiClient::new(Self::ID);
                match client.echo(text.to_string()) {
                    Ok(v) => Ok(serde_json::json!({ "echo": v })),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                }
            }
            // 错误传播：目标方法返回 error → JSON-RPC error 对象 → client 报错
            "test_api_fail" => {
                let client = SdkTestApiClient::new(Self::ID);
                match client.fail() {
                    Ok(v) => Ok(serde_json::json!({ "unexpected": v })),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                }
            }
            // 超时：目标声明且订阅但未实现（宿主测试绕过声明登记 + 订阅
            // `no-response` topic），短超时快速失败
            "test_api_timeout" => {
                let client = SdkTestApiClient::new(Self::ID).with_timeout(800);
                match client.call_json("no-response", serde_json::json!([])) {
                    Ok(v) => Ok(serde_json::json!({ "unexpected": v })),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                }
            }
            // 门禁拒绝：目标 api 未声明（com.bedcode.sdk-test.ghost 不在注册表）
            "test_api_undeclared" => {
                let client = SdkTestApiClient::new(Self::ID).with_timeout(800);
                match client.call_json("ghost", serde_json::json!([])) {
                    Ok(v) => Ok(serde_json::json!({ "unexpected": v })),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                }
            }
            // ==================== 跨插件调用计划任务插件（issue 06 E2E） ====================

            // 调用方指向 com.bedcode.scheduler 调 `list`（api 已在插件声明，宿主注册表有登记），
            // 成功时返回任务列表 JSON（空列表或现有任务）
            "test_schedule_list" => {
                let client = SdkTestApiClient::new("com.bedcode.scheduler").with_timeout(3000);
                match client.call_json("list", serde_json::json!([])) {
                    Ok(v) => Ok(v),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                }
            }
            // 调用方调未声明 api（com.bedcode.scheduler.ghost 不在其 manifest）：宿主门禁拒绝
            "test_schedule_undeclared" => {
                let client = SdkTestApiClient::new("com.bedcode.scheduler").with_timeout(800);
                match client.call_json("ghost", serde_json::json!([])) {
                    Ok(v) => Ok(serde_json::json!({ "unexpected": v })),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                }
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
