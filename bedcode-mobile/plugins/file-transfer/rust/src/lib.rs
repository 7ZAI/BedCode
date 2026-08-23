//! File Transfer Plugin (WASM, Mobile) — HostPeer 薄代理
//!
//! issue 12 切换后插件不再自带任务状态机/队列/挂载/握手：对等传输的
//! 全部能力（发现/拨号/收发/接收应答/浏览拉取/设置）由宿主 `host-peer`
//! 接口提供（真源 = 宿主 peer_net/peer_transfer/peer_receive/peer_remote）。
//! 本 crate 只做两件事：
//!
//! 1. **命令转发**：`invoke_command("file-transfer.*")` → `host.peer_*`，
//!    并把宿主 DTO 翻译成前端既有 wire 形状（camelCase Task/PendingBatch/
//!    ReceivingTask/HistoryEntry），前端 composables 改动最小化；
//! 2. **事件桥接**：订阅总线 `peer:*` topic，翻译后经 emit_event 推给
//!    前端既有事件名（tasks-changed / batches-changed / receiving-changed /
//!    history-changed / devices-changed）。

use bedcode_plugin_api_mobile::host::{HostBus, HostEvents, HostLog, HostPeer};
use bedcode_plugin_api_mobile::types::PluginManifest;
use bedcode_plugin_api_mobile::wasm_host::WasmHost;
use bedcode_plugin_api_mobile::{BusMessage, WasmPlugin};
use peer::PLUGIN_ID;
use std::sync::Mutex;
use std::sync::OnceLock;

mod peer;

/// 当前选中的对端节点 ID（单对端 UX：移动端 ⇄ 桌面端）。
/// set-active-peer 写入；后续 send/pull/browse 未显式带 nodeId 时使用。
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
        h.log_info("File Transfer plugin activating (host-peer proxy, mobile)");
        // 订阅宿主桥接的对等事件 topic（emit_json 同步发布）
        let _ = h.bus_subscribe("peer:devices");
        let _ = h.bus_subscribe("peer:transfer");
        let _ = h.bus_subscribe("peer:receive");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let h = host();
        let _ = h.bus_unsubscribe("peer:devices");
        let _ = h.bus_unsubscribe("peer:transfer");
        let _ = h.bus_unsubscribe("peer:receive");
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let h = host();
        match name {
            // ==================== 设备 ====================
            "file-transfer.list-peers" => Ok(peer::list_peers(&h)?),
            "file-transfer.query-peer" => Ok(peer::list_devices_raw(&h)?),
            "file-transfer.set-active-peer" => {
                let id = args
                    .get("peerId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                *active_node().lock().expect("active node lock") = id;
                Ok(serde_json::json!({ "ok": true }))
            }

            // ==================== 发送（上传方向） ====================
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
                peer::transfer_to_task(&dto)
                    .map_err(|e| anyhow::anyhow!("retry map failed: {e}"))
            }
            // 宿主托管生命周期：暂停/恢复/删除单任务不再支持（前端已移除入口）
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

            // ==================== 远端浏览 / 拉取 ====================
            "file-transfer.list-remote" => Ok(peer::list_remote(&h, &args, active_node())?),
            "file-transfer.pull-files" => Ok(peer::pull_files(&h, &args, active_node())?),

            // ==================== 设置 ====================
            "file-transfer.get-settings" => Ok(peer::get_settings(&h)?),
            "file-transfer.set-settings" => Ok(peer::set_settings(&h, &args)?),
            "file-transfer.mount-local" => Ok(peer::mount_local(&h)?),
            "file-transfer.update-roots" => Ok(peer::update_roots(&h, &args)?),
            // 移动端接收落点固定 MediaStore.Downloads，不支持自定义
            "file-transfer.pick-download-dir" => Err(anyhow::anyhow!(
                "unsupported on mobile: downloads land in MediaStore.Downloads"
            )),

            _ => Err(anyhow::anyhow!("unknown command: {}", name)),
        }
    }

    fn on_bus_message(msg: &BusMessage) -> anyhow::Result<()> {
        let h = host();

        if msg.topic == "peer:devices" {
            // 透传 DiscoveredPeerDto 数组；前端挑选具备文件传输能力的设备
            h.emit_event("plugin:file-transfer:devices-changed", &msg.payload);
            return Ok(());
        }

        if msg.topic == "peer:transfer" {
            // 发送方向 → 队列快照；终态并入历史
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
        host().log_info("File Transfer plugin shut down (host-peer proxy, mobile)");
        Ok(())
    }
}

/// 取字符串参数（缺省报错）
fn require_str(args: &serde_json::Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("missing {key}"))
}

bedcode_plugin_api_mobile::wasm_entry!(FileTransferPlugin);
