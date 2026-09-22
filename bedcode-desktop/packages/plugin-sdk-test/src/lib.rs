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
//! 宿主测试以 pinned nightly + wasm32-wasip3 构建本 crate：cdylib 直出组件
//! （magic \0asm 0d，免 ComponentEncoder/componentize，见票 03）。

use bedcode_plugin_api::host::{
    ConfigKey, HostAuth, HostBus, HostConfig, HostDatabase, HostEvents, HostLog, HostSession, HostStorage,
};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm::WasmPlugin;
use bedcode_plugin_api::wasm_host::WasmHost;
use bedcode_plugin_api::{BusMessage, plugin_api};

// 最近一次收到的二进制消息（v11 events-binary 回调）——SDK 链路字节完整性
// 验证：`on_message_binary` 写入，命令 `test_binary_received` 读出供宿主断言
// （含非 UTF-8、MB 级载荷；票据 06 补齐 SDK 二进制发布/订阅缺口后启用）
// 实例级全局（票 03：wasip3 thread_local 按宿主调用线程隔离，投递线程写 /
// 查询线程读会读空；wasm 单线程内 Mutex 无竞争）
static LAST_BINARY: std::sync::Mutex<Option<(String, String, Vec<u8>)>> = std::sync::Mutex::new(None);

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
        // v11：以二进制格式偏好订阅（经 SDK HostBus，票据 06 补齐该能力）——
        // 宿主 `publish_binary` 才会投递到 on_message_binary 回调
        WasmHost.bus_subscribe_binary("sdk:binary-topic")?;
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

    /// v11 二进制消息入口（票据 06 补齐 SDK 二进制订阅能力后启用）：
    /// 记录最近一次 topic/sender/字节列，供宿主测试断言字节完整性
    fn on_message_binary(msg: &BusMessage) -> anyhow::Result<()> {
        let payload = msg.payload_binary.clone().unwrap_or_default();
        *LAST_BINARY.lock().unwrap() = Some((msg.topic.clone(), msg.sender.clone(), payload));
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
            // v11：二进制发布（票据 06 补齐 SDK 能力）——args.bytes 为 0-255
            // 数字数组，args.topic 缺省 sdk:binary-topic
            "test_binary_publish" => {
                let topic = args
                    .get("topic")
                    .and_then(|v| v.as_str())
                    .unwrap_or("sdk:binary-topic");
                let bytes: Vec<u8> = args
                    .get("bytes")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_u64().map(|n| n as u8))
                            .collect()
                    })
                    .unwrap_or_default();
                host.bus_publish_binary(topic, &bytes)?;
                Ok(serde_json::json!({ "published": bytes.len() }))
            }
            // v11：读回 on_message_binary 最近一次收到的消息（宿主断言字节完整性）
            "test_binary_received" => {
                let received = LAST_BINARY.lock().unwrap().as_ref().map(
                    |(topic, sender, payload)| serde_json::json!({ "topic": topic, "sender": sender, "bytes": payload }),
                );
                Ok(serde_json::json!({ "received": received }))
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
            // ==================== 会话中心互调闭环（票 05 改指） ====================
            // caller 角色指向 com.bedcode.terminal-session（宿主测试加载真实会话中心产物）：
            // `consent-decide` 两阶段决策流 + `trust-list` / `trust-revoke` 统一视图与
            // 撤销 + 未声明 api 门禁拒绝（ADR 0017）。消费方参数经 args 传入
            // （宿主测试断言 wire 形状）。

            // 阶段 1：仅传 peer 信息（无用户意向）→ 已信任免确认 / 未知 → ask
            "test_session_consent_decide" => {
                let peer = args.get("peerInfo").cloned().unwrap_or(serde_json::json!({
                    "requestId": "req-consent-1",
                    "nodeId": "aabbccdd11223344aabbccdd11223344aabbccdd11223344aabbccdd11223344",
                    "fingerprintShort": "aabbccdd",
                    "deviceName": "消费方模拟对端",
                }));
                let client = SdkTestApiClient::new("com.bedcode.terminal-session").with_timeout(5000);
                match client.call_json("consent-decide", peer) {
                    Ok(v) => Ok(serde_json::json!({ "decision": v })),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                }
            }
            // 阶段 2：回传用户意向（accept / deny / one_time）→ 最终决策。
            // peerInfo 经 args 传入（宿主测试断言 wire 形状），userDecision 单独
            // 传入并合并进请求（peerInfo 缺省时用内置默认对端）
            "test_session_consent_decide_explicit" => {
                let mut peer = args.get("peerInfo").cloned().unwrap_or(serde_json::json!({
                    "requestId": "req-consent-2",
                    "nodeId": "aabbccdd11223344aabbccdd11223344aabbccdd11223344aabbccdd11223344",
                }));
                if let Some(ud) = args.get("userDecision").and_then(|v| v.as_str()) {
                    if let Some(obj) = peer.as_object_mut() {
                        obj.insert("userDecision".to_string(), serde_json::json!(ud));
                    }
                }
                let client = SdkTestApiClient::new("com.bedcode.terminal-session").with_timeout(5000);
                match client.call_json("consent-decide", peer) {
                    Ok(v) => Ok(serde_json::json!({ "decision": v })),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                }
            }
            // 统一信任视图（零参 api）
            "test_session_trust_list" => {
                let client = SdkTestApiClient::new("com.bedcode.terminal-session").with_timeout(5000);
                match client.call_json("trust-list", serde_json::Value::Null) {
                    Ok(v) => Ok(v),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                }
            }
            // 撤销统一条目（id 经 args 传入；未经内核 `pairings` 真源不可逆）
            "test_session_trust_revoke" => {
                let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("ghost");
                let client = SdkTestApiClient::new("com.bedcode.terminal-session").with_timeout(5000);
                match client.call_json("trust-revoke", serde_json::json!(id)) {
                    Ok(v) => Ok(v),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                }
            }
            // 未声明 api（com.bedcode.terminal-session.ghost-api 不在 manifest）：宿主门禁拒绝
            "test_session_undeclared" => {
                let client = SdkTestApiClient::new("com.bedcode.terminal-session").with_timeout(2000);
                match client.call_json("ghost-api", serde_json::Value::Null) {
                    Ok(v) => Ok(serde_json::json!({ "unexpected": v })),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                }
            }
            // ==================== host-auth 记录面（v18）四原语闭环探针 ====================
            // 产品侧消费方（设置分组 / 设备视图）归后续票；本探针让宿主 S1 闭环能贯穿
            // WIT → SDK → host_impl 全链，覆盖「读原始记录 / 撤销 / 连接历史 / 设置写入」
            // 四函数的真实 wasm 行为。权限由宿主测试显式授予（auth）。
            //
            // args：{ deviceId?, revokeId?, settingKey?, settingValue? }
            "test_auth_record_face" => {
                let device_id = args.get("deviceId").and_then(|v| v.as_str()).unwrap_or("p-1");
                let setting_key = args
                    .get("settingKey")
                    .and_then(|v| v.as_str())
                    .unwrap_or("pairing_code_ttl");
                let setting_value = args.get("settingValue").and_then(|v| v.as_str()).unwrap_or("777");
                let before = host.auth_trusted_devices_list()?;
                let history = host.auth_connection_history_list(device_id)?;
                host.auth_setting_set(setting_key, setting_value)?;
                let revoked = match args.get("revokeId").and_then(|v| v.as_str()) {
                    Some(id) => Some(host.auth_trusted_device_revoke(id)?),
                    None => None,
                };
                let after = host.auth_trusted_devices_list()?;
                Ok(serde_json::json!({
                    "before": before,
                    "after": after,
                    "history": history,
                    "revoked": revoked,
                }))
            }
            // ==================== host-session 配置面（v19）闭环探针 ====================
            // 票 07 的验收点是原语自身在真实运行时可用：新建 → 读回 → 覆盖 →
            // v22：host-session 配置面只读化（config-upsert/delete 已退役），
            // 探针只走读取面：config-list 拿 id 清单 → config-get 逐条全量。
            "test_session_config_face" => {
                let list = host.session_config_list()?.unwrap_or_else(|| serde_json::json!([]));
                let mut rows = Vec::new();
                if let Some(arr) = list.as_array() {
                    for row in arr {
                        if let Some(id) = row.get("id").and_then(|v| v.as_str()) {
                            rows.push(
                                host.session_config_get(id)?.unwrap_or_else(|| serde_json::Value::Null),
                            );
                        }
                    }
                }
                Ok(serde_json::json!({
                    "list": list,
                    "rows": rows,
                }))
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
