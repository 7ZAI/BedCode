//! File Transfer Plugin (WASM, Desktop) — HostPeer 薄代理
//!
//! issue 12 切换后插件不再自带任务状态机/队列/挂载/意图协调：对等传输的
//! 全部能力由宿主 `host-peer` 接口提供（真源 = 宿主 peer_net/peer_transfer/
//! peer_receive/peer_remote）。本 crate 只做两件事：
//!
//! 1. **命令转发**：`invoke_command("file-transfer.*")` → `host.peer_*`，
//!    并把宿主批级 DTO 翻译成前端既有 wire 形状；
//! 2. **事件桥接**：订阅总线 `peer:*` topic，翻译后经 emit_event 推给前端。

use bedcode_plugin_api::host::{HostBus, HostEvents, HostLog, HostPeer};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm_host::WasmHost;
use bedcode_plugin_api::{BusMessage, WasmPlugin};
use peer::PLUGIN_ID;
use std::sync::Mutex;
use std::sync::OnceLock;

mod peer;

/// 当前选中的对端节点 ID（单对端 UX：桌面端 ⇄ 移动端）
static ACTIVE_NODE: OnceLock<Mutex<String>> = OnceLock::new();

fn active_node() -> &'static Mutex<String> {
    ACTIVE_NODE.get_or_init(|| Mutex::new(String::new()))
}

fn host() -> WasmHost {
    WasmHost
}

struct FileTransferPlugin;

impl WasmPlugin for FileTransferPlugin {
    const ID: &'static str = PLUGIN_ID;

    fn manifest() -> PluginManifest {
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let h = host();
        h.log_info("File Transfer plugin activating (host-peer proxy, desktop)");
        let _ = h.bus_subscribe("peer:devices");
        let _ = h.bus_subscribe("peer:transfer");
        let _ = h.bus_subscribe("peer:receive");
        // 首连确认 / 连接态：对等 UI 已迁入本插件前端消费
        let _ = h.bus_subscribe("peer:consent");
        let _ = h.bus_subscribe("peer:connection");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let h = host();
        let _ = h.bus_unsubscribe("peer:devices");
        let _ = h.bus_unsubscribe("peer:transfer");
        let _ = h.bus_unsubscribe("peer:receive");
        let _ = h.bus_unsubscribe("peer:consent");
        let _ = h.bus_unsubscribe("peer:connection");
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let h = host();
        match name {
            // ==================== 设备 ====================
            "file-transfer.list-peers" => Ok(peer::list_peers(&h)?),
            "file-transfer.query-peer" => Ok(peer::list_devices_raw(&h)?),
            "file-transfer.dial-peer" => {
                let node_id = require_str(&args, "nodeId")?;
                Ok(h.peer_dial(&node_id)?)
            }
            "file-transfer.disconnect-peer" => {
                let node_id = require_str(&args, "nodeId")?;
                let existed = h.peer_disconnect(&node_id)?;
                Ok(serde_json::json!({ "existed": existed }))
            }
            "file-transfer.set-active-peer" => {
                let id = args
                    .get("peerId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                *active_node().lock().expect("active node lock") = id;
                Ok(serde_json::json!({ "ok": true }))
            }

            // ==================== 发送 ====================
            "file-transfer.pick-files" => Ok(peer::pick_files(&h)?),
            "file-transfer.enqueue" => Ok(peer::enqueue(&h, &args, active_node())?),
            "file-transfer.list-tasks" => Ok(peer::list_tasks(&h)?),
            "file-transfer.cancel" => {
                let batch_id = require_str(&args, "taskId")?;
                let _ = h.peer_cancel_transfer(&batch_id)?;
                Ok(serde_json::json!({ "ok": true }))
            }
            "file-transfer.retry" => {
                let batch_id = require_str(&args, "taskId")?;
                let dto = h.peer_retry_transfer(&batch_id)?;
                peer::transfer_to_task(&dto).map_err(|e| anyhow::anyhow!("retry map failed: {e}"))
            }
            "file-transfer.pause" | "file-transfer.resume" | "file-transfer.resume-all"
            | "file-transfer.remove-task" => Err(anyhow::anyhow!(
                "unsupported: transfer lifecycle is host-managed"
            )),

            // ==================== 接收端 ====================
            "file-transfer.list-batches" => Ok(peer::list_batches(&h)?),
            "file-transfer.approve-batch" => {
                let batch_id = require_str(&args, "batchId")?;
                h.peer_respond_transfer(&batch_id, true)?;
                Ok(serde_json::json!({ "ok": true }))
            }
            "file-transfer.reject-batch" => {
                let batch_id = require_str(&args, "batchId")?;
                h.peer_respond_transfer(&batch_id, false)?;
                Ok(serde_json::json!({ "ok": true }))
            }
            "file-transfer.list-receiving" => Ok(peer::list_receiving(&h)?),
            "file-transfer.cancel-receiving" => {
                let batch_id = require_str(&args, "sessionId")?;
                h.peer_cancel_receiving(&batch_id)?;
                Ok(serde_json::json!({ "ok": true }))
            }
            "file-transfer.list-history" => Ok(peer::list_history(&h)?),
            "file-transfer.clear-history" => {
                let a = h.peer_clear_transfer_history()?;
                let b = h.peer_clear_receiving_history()?;
                Ok(serde_json::json!({ "cleared": a + b }))
            }

            // ==================== 信任层 ====================
            "file-transfer.respond-consent" => {
                let request_id = require_str(&args, "requestId")?;
                let accepted = args.get("accepted").and_then(|v| v.as_bool()).unwrap_or(false);
                let hit = h.peer_respond_consent(&request_id, accepted)?;
                Ok(serde_json::json!({ "hit": hit }))
            }
            "file-transfer.list-trusted" => Ok(h.peer_list_trusted()?),
            "file-transfer.revoke-trusted" => {
                let node_id = require_str(&args, "nodeId")?;
                let removed = h.peer_revoke_trusted(&node_id)?;
                Ok(serde_json::json!({ "removed": removed }))
            }

            // ==================== 远端浏览 / 拉取 ====================
            "file-transfer.list-remote" => Ok(peer::list_remote(&h, &args, active_node())?),
            "file-transfer.pull-files" => Ok(peer::pull_files(&h, &args, active_node())?),

            // ==================== 设置 ====================
            "file-transfer.get-settings" => Ok(peer::get_settings(&h)?),
            "file-transfer.set-settings" => Ok(peer::set_settings(&h, &args)?),
            "file-transfer.pick-download-dir" => Ok(peer::pick_download_dir(&h)?),
            "file-transfer.mount-local" => Ok(peer::mount_local(&h, &args)?),
            "file-transfer.update-roots" => Ok(peer::update_roots(&h, &args)?),

            _ => Err(anyhow::anyhow!("unknown command: {}", name)),
        }
    }

    fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
        let h = host();

        if msg.topic == "peer:devices" {
            h.emit_event("plugin:file-transfer:devices-changed", &msg.payload);
            return Ok(());
        }

        if msg.topic == "peer:transfer" {
            if let Some(arr) = msg.payload.as_array() {
                let tasks: Vec<serde_json::Value> = arr
                    .iter()
                    .filter(|t| t.get("direction").and_then(|v| v.as_str()) == Some("send"))
                    .filter_map(|t| peer::transfer_to_task(t).ok())
                    .collect();
                h.emit_event(
                    "plugin:file-transfer:tasks-changed",
                    &serde_json::Value::Array(tasks),
                );
                peer::emit_history(&h, arr);
            }
            return Ok(());
        }

        // 首连确认请求：原样转发（payload 已是 camelCase 契约形状），
        // 弹窗/队列/超时编排在前端 useConsent 完成
        if msg.topic == "peer:consent" {
            h.emit_event("plugin:file-transfer:consent-requested", &msg.payload);
            return Ok(());
        }

        // 连接态变化：connected/disconnected 原样转发（{ nodeId, ... }）
        if msg.topic == "peer:connection" {
            h.emit_event("plugin:file-transfer:connection-changed", &msg.payload);
            return Ok(());
        }

        if msg.topic == "peer:receive" {
            if let Some(arr) = msg.payload.as_array() {
                let pending: Vec<serde_json::Value> = arr
                    .iter()
                    .filter(|t| t.get("status").and_then(|v| v.as_str()) == Some("pending"))
                    .cloned()
                    .collect();
                let active: Vec<serde_json::Value> = arr
                    .iter()
                    .filter(|t| t.get("status").and_then(|v| v.as_str()) != Some("pending"))
                    .filter_map(|t| peer::transfer_to_receiving(t).ok())
                    .collect();
                h.emit_event(
                    "plugin:file-transfer:batches-changed",
                    &serde_json::Value::Array(pending),
                );
                h.emit_event(
                    "plugin:file-transfer:receiving-changed",
                    &serde_json::Value::Array(active),
                );
                peer::emit_history(&h, arr);
            }
            return Ok(());
        }

        Ok(())
    }

    fn on_shutdown() -> anyhow::Result<()> {
        host().log_info("File Transfer plugin shut down (host-peer proxy, desktop)");
        Ok(())
    }
}

fn require_str(args: &serde_json::Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("missing {key}"))
}

bedcode_plugin_api::wasm_entry!(FileTransferPlugin);
