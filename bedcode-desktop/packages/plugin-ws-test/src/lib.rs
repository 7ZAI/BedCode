//! host-websocket（ABI v14）fixture 插件
//!
//! 演示 spec `.scratch/2026-09-18-ws-base-service/spec.md` 的插件侧用法，
//! 并作为宿主测试套件的端到端载体（`plugin-sdk-test` 不并入：避免权限与
//! 依赖混杂，见 spec §B7）：
//!
//! - **activate 期订阅** 属主私有状态事件
//!   （`ws:open/error/close.<owner>`、`ws:client-connect/disconnect.<owner>`）：
//!   宿主不缓冲、不重放，晚订阅期间的事件永久丢失（D3 硬约束），故必须在
//!   首次 connect / register-endpoint 之前完成订阅；
//! - 命令驱动出站连接：`ws-connect` / `ws-send-text` / `ws-send-binary` /
//!   `ws-close` / `ws-is-connected`；
//! - 命令驱动入站端点（服务端域）：`ws-register-endpoint` /
//!   `ws-unregister-endpoint` / `ws-send-to-client` / `ws-send-binary-to-client` /
//!   `ws-broadcast-text` / `ws-broadcast-binary` / `ws-close-client` /
//!   `ws-list-clients` / `ws-list-endpoints`；`ws-endpoint-echo` 打开回显开关后
//!   `on_ws_client_message` 会把收到的帧原样回给该客户端（端点回显闭环载体）；
//! - `events-ws` 回调（`on_ws_message` / `on_ws_client_message`）收集收到的帧，
//!   经 `ws-state` 命令读出，供宿主断言回文与保序；
//! - 总线事件与帧都存实例级全局（票 03：wasm32-wasip3 的 thread_local 是
//!   真 TLS——按宿主调用线程隔离，「投递线程记录 / 查询线程读取」跨线程时
//!   读空；改静态 Mutex，wasm 单线程内无竞争）。

use bedcode_plugin_api::host::{
    ws_event_topic, HostBus, HostLog, HostWebsocket, WS_CLIENT_CONNECT, WS_CLIENT_DISCONNECT, WS_CLOSE, WS_ERROR,
    WS_OPEN,
};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm::WasmPlugin;
use bedcode_plugin_api::wasm_host::WasmHost;
use bedcode_plugin_api::BusMessage;

/// 收到的 WS 帧：`(标识, kind, payload)`——客户端域标识为 `wsc-<uuid>`，
/// 服务端域为 `wse-<uuid>/wsc-<uuid>`（端点/对端）。实例级全局（见文件头注释）
static FRAMES: std::sync::Mutex<Vec<(String, String, Vec<u8>)>> = std::sync::Mutex::new(Vec::new());
/// 收到的总线状态事件 payload（属主私有 topic 的投递内容）
static EVENTS: std::sync::Mutex<Vec<serde_json::Value>> = std::sync::Mutex::new(Vec::new());
static TRACE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
/// 端点回显开关（`ws-endpoint-echo` 命令控制；false = 只收集不回显）
static ECHO_ENABLED: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

/// WS fixture 插件
pub struct WsTestPlugin;

impl WasmPlugin for WsTestPlugin {
    const ID: &'static str = "com.bedcode.ws-test";

    fn manifest() -> PluginManifest {
        // ADR-0005 单一真源：plugin.json
        serde_json::from_str(include_str!("../plugin.json")).expect("plugin.json must be valid PluginManifest")
    }

    /// 订阅属主私有状态事件（**必须在任何 connect / register-endpoint 之前**：
    /// 宿主不重放）
    ///
    /// 订阅失败降级为日志，理由同 host-pty fixture：隔离用例把同一产物以第二个
    /// 属主 id 实例化，而 guest 只能按编译期 `Self::ID` 拼命名空间，票 05 门禁
    /// 本就该拒这种跨属主订阅；属主本体的投递由 e2e 的收事件断言行为性兜住。
    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;
        for topic in [
            ws_event_topic(WS_OPEN, Self::ID),
            ws_event_topic(WS_ERROR, Self::ID),
            ws_event_topic(WS_CLOSE, Self::ID),
            ws_event_topic(WS_CLIENT_CONNECT, Self::ID),
            ws_event_topic(WS_CLIENT_DISCONNECT, Self::ID),
        ] {
            match host.bus_subscribe(&topic) {
                Ok(()) => host.log_info(&format!("ws-test fixture: subscribed {topic}")),
                Err(e) => host.log_info(&format!("ws-test fixture: subscribe {topic} skipped: {e}")),
            }
        }
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let host = WasmHost;
        match name {
            // 建立出站连接（阻塞至握手完成）→ `{ handle }`
            "ws-connect" => {
                let url = args
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("ws-connect: url is required"))?;
                let config = serde_json::json!({ "url": url }).to_string();
                let handle = host.ws_connect(&config).map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(serde_json::json!({ "handle": handle }))
            }
            "ws-send-text" => {
                let handle = require_str(&args, "handle")?;
                let text = require_str(&args, "text")?;
                host.ws_send_text(&handle, &text).map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(serde_json::json!({ "ok": true }))
            }
            // 发二进制帧：`bytes` 为 u8 数组（超出 u8 范围视为无效入参）
            "ws-send-binary" => {
                let handle = require_str(&args, "handle")?;
                let bytes: Vec<u8> = args
                    .get("bytes")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect())
                    .unwrap_or_default();
                host.ws_send_binary(&handle, &bytes)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(serde_json::json!({ "ok": true, "len": bytes.len() }))
            }
            "ws-close" => {
                let handle = require_str(&args, "handle")?;
                let mut close = serde_json::Map::new();
                if let Some(code) = args.get("code").and_then(|v| v.as_u64()) {
                    close.insert("code".to_string(), serde_json::json!(code));
                }
                if let Some(reason) = args.get("reason").and_then(|v| v.as_str()) {
                    close.insert("reason".to_string(), serde_json::json!(reason));
                }
                let hit = host
                    .ws_close(&handle, &serde_json::Value::Object(close).to_string())
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(serde_json::json!({ "hit": hit }))
            }
            "ws-is-connected" => {
                let handle = require_str(&args, "handle")?;
                let connected = host.ws_is_connected(&handle).map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(serde_json::json!({ "connected": connected }))
            }
            // ==================== 服务端域（入站端点） ====================
            // 注册端点 → `{ endpointId }`；完整挂载路径 = `/ws/plugin/<id>/<path>`
            "ws-register-endpoint" => {
                let path = require_str(&args, "path")?;
                let mut config = serde_json::Map::new();
                config.insert("path".to_string(), serde_json::json!(path));
                if let Some(auth) = args.get("auth").and_then(|v| v.as_str()) {
                    config.insert("auth".to_string(), serde_json::json!(auth));
                }
                if let Some(max_clients) = args.get("maxClients").and_then(|v| v.as_u64()) {
                    config.insert("maxClients".to_string(), serde_json::json!(max_clients));
                }
                let endpoint_id = host
                    .ws_register_endpoint(&serde_json::Value::Object(config).to_string())
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(serde_json::json!({ "endpointId": endpoint_id }))
            }
            "ws-unregister-endpoint" => {
                let endpoint_id = require_str(&args, "endpointId")?;
                let hit = host
                    .ws_unregister_endpoint(&endpoint_id)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(serde_json::json!({ "hit": hit }))
            }
            "ws-send-to-client" => {
                let endpoint_id = require_str(&args, "endpointId")?;
                let client_id = require_str(&args, "clientId")?;
                let text = require_str(&args, "text")?;
                host.ws_send_text_to_client(&endpoint_id, &client_id, &text)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(serde_json::json!({ "ok": true }))
            }
            "ws-send-binary-to-client" => {
                let endpoint_id = require_str(&args, "endpointId")?;
                let client_id = require_str(&args, "clientId")?;
                let bytes = require_bytes(&args)?;
                host.ws_send_binary_to_client(&endpoint_id, &client_id, &bytes)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(serde_json::json!({ "ok": true, "len": bytes.len() }))
            }
            "ws-broadcast-text" => {
                let endpoint_id = require_str(&args, "endpointId")?;
                let text = require_str(&args, "text")?;
                let sent = host
                    .ws_broadcast_text(&endpoint_id, &text)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(serde_json::json!({ "sent": sent }))
            }
            "ws-broadcast-binary" => {
                let endpoint_id = require_str(&args, "endpointId")?;
                let bytes = require_bytes(&args)?;
                let sent = host
                    .ws_broadcast_binary(&endpoint_id, &bytes)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(serde_json::json!({ "sent": sent }))
            }
            // 踢出客户端（缺省 4004；缺省 reason 由宿主填）
            "ws-close-client" => {
                let endpoint_id = require_str(&args, "endpointId")?;
                let client_id = require_str(&args, "clientId")?;
                let mut close = serde_json::Map::new();
                if let Some(code) = args.get("code").and_then(|v| v.as_u64()) {
                    close.insert("code".to_string(), serde_json::json!(code));
                }
                if let Some(reason) = args.get("reason").and_then(|v| v.as_str()) {
                    close.insert("reason".to_string(), serde_json::json!(reason));
                }
                let hit = host
                    .ws_close_client(&endpoint_id, &client_id, &serde_json::Value::Object(close).to_string())
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(serde_json::json!({ "hit": hit }))
            }
            // 端点在线的客户端清单（宿主返回 JSON 数组字符串，原样透出）
            "ws-list-clients" => {
                let endpoint_id = require_str(&args, "endpointId")?;
                let raw = host
                    .ws_list_clients(&endpoint_id)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let clients: serde_json::Value = serde_json::from_str(&raw)?;
                Ok(serde_json::json!({ "clients": clients }))
            }
            "ws-list-endpoints" => {
                let raw = host.ws_list_endpoints().map_err(|e| anyhow::anyhow!("{e}"))?;
                let endpoints: serde_json::Value = serde_json::from_str(&raw)?;
                Ok(serde_json::json!({ "endpoints": endpoints }))
            }
            // 端点回显开关（打开后 on_ws_client_message 原样回给该客户端）
            "ws-endpoint-echo" => {
                let enabled = args.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                *ECHO_ENABLED.lock().unwrap() = enabled;
                Ok(serde_json::json!({ "enabled": enabled }))
            }
            // 快照：收到的帧 + 收到的状态事件（宿主测试的轮询入口）
            "ws-state" => {
                let frames: Vec<serde_json::Value> = FRAMES
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(target, kind, payload)| {
                        serde_json::json!({
                            "target": target,
                            "kind": kind,
                            "len": payload.len(),
                            "text": String::from_utf8(payload.clone()).ok(),
                        })
                    })
                    .collect();
                let events = EVENTS.lock().unwrap().clone();
                let trace = TRACE.lock().unwrap().clone();
                Ok(serde_json::json!({ "frames": frames, "events": events, "trace": trace }))
            }
            // 清空收集缓冲（多次断言之间隔离）
            "ws-reset" => {
                FRAMES.lock().unwrap().clear();
                EVENTS.lock().unwrap().clear();
                TRACE.lock().unwrap().clear();
                Ok(serde_json::json!({ "ok": true }))
            }
            other => Err(anyhow::anyhow!("Unknown command: {other}")),
        }
    }

    /// 总线消息入口：记录状态事件（本插件只订阅 `ws:*.<owner>` 三个 topic）
    fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
        TRACE.lock().unwrap().push(format!("event:{}", msg.topic));
        EVENTS.lock().unwrap().push(serde_json::json!({
            "topic": msg.topic,
            "sender": msg.sender,
            "payload": msg.payload,
        }));
        Ok(())
    }

    /// 客户端域帧回调（handle = `wsc-<uuid>`）
    fn on_ws_message(handle: &str, kind: &str, payload: &[u8]) -> anyhow::Result<()> {
        FRAMES
            .lock()
            .unwrap()
            .push((handle.to_string(), kind.to_string(), payload.to_vec()));
        Ok(())
    }

    /// 服务端域帧回调（endpoint-id / client-id 组合标识）
    ///
    /// 回显开关打开时把收到的帧原样回给该客户端（端点回显闭环；宿主零业务语义，
    /// 回不回、怎么回完全由插件决定）
    fn on_ws_client_message(endpoint_id: &str, client_id: &str, kind: &str, payload: &[u8]) -> anyhow::Result<()> {
        TRACE
            .lock()
            .unwrap()
            .push(format!("frame:{kind}:{endpoint_id}/{client_id}"));
        FRAMES.lock().unwrap().push((
            format!("{endpoint_id}/{client_id}"),
            kind.to_string(),
            payload.to_vec(),
        ));
        if *ECHO_ENABLED.lock().unwrap() {
            let host = WasmHost;
            let echoed = match kind {
                "text" => {
                    let text = String::from_utf8_lossy(payload).into_owned();
                    host.ws_send_text_to_client(endpoint_id, client_id, &text)
                }
                _ => host.ws_send_binary_to_client(endpoint_id, client_id, payload),
            };
            if let Err(e) = echoed {
                // 回显失败只记日志（观察型回调不中断后续帧投递）
                host.log_warn(&format!("endpoint echo failed: {e}"));
            }
        }
        Ok(())
    }
}

/// 必填字符串参数（缺失即报错，避免静默用默认值掩盖用例拼装错误）
fn require_str(args: &serde_json::Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("{} is required", key))
}

/// 必填字节数组参数（`bytes` 为 u8 数组；缺失即报错）
fn require_bytes(args: &serde_json::Value) -> anyhow::Result<Vec<u8>> {
    args.get("bytes")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect())
        .ok_or_else(|| anyhow::anyhow!("bytes is required"))
}

bedcode_plugin_api::wasm_entry!(WsTestPlugin);
