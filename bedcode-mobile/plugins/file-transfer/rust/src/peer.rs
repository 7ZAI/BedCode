//! host-peer 翻译层 —— 宿主 DTO ↔ 前端既有 wire 形状
//!
//! 宿主批级模型（PeerTransferDto）→ 前端文件级历史模型（Task/PendingBatch/
//! ReceivingTask/HistoryEntry）：一批 = 一条队列/接收记录（聚合大小与进度），
//! 文件清单并入展示名。字段命名沿用旧 WASM 快照的 snake_case wire 约定，
//! composable 的 mapWire* 归一化逻辑无需改动。

use bedcode_plugin_api_mobile::host::{HostEvents, HostLog, HostPeer};
use bedcode_plugin_api_mobile::wasm_host::WasmHost;
use std::sync::Mutex;

pub(crate) const PLUGIN_ID: &str = "com.bedcode.file-transfer";

type Result<T> = anyhow::Result<T>;

// ==================== 设备 ====================

/// 发现设备列表 → 旧 PeerInfo wire：[{ deviceId, name }]
/// （仅保留具备文件传输能力的节点；无能力节点不可作为传输对端）
pub(crate) fn list_peers(h: &WasmHost) -> Result<serde_json::Value> {
    let devices = h.peer_list_devices()?;
    let peers: Vec<serde_json::Value> = devices
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|d| d.get("fileTransfer").and_then(|v| v.as_bool()) != Some(false))
                .map(|d| {
                    serde_json::json!({
                        "deviceId": d.get("nodeId").cloned().unwrap_or_default(),
                        "name": d.get("deviceName").cloned().unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(serde_json::Value::Array(peers))
}

/// 原始设备列表透传（query-peer 刷新用）
pub(crate) fn list_devices_raw(h: &WasmHost) -> Result<serde_json::Value> {
    Ok(h.peer_list_devices()?)
}

// ==================== 发送 ====================

/// 系统文件选择器（SAF 多选；返回真实路径数组 JSON）
pub(crate) fn pick_files(h: &WasmHost) -> Result<serde_json::Value> {
    let paths = h.peer_pick_files()?;
    Ok(serde_json::to_value(paths)?)
}

/// 入队发送：args.localPaths（旧单文件 local_path 兼容）或 paths 数组
pub(crate) fn enqueue(
    h: &WasmHost,
    args: &serde_json::Value,
    active_node: &'static Mutex<String>,
) -> Result<serde_json::Value> {
    let node_id = args
        .get("peerId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| Some(active_node.lock().expect("active node lock").clone()))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("no active peer"))?;

    let mut paths: Vec<String> = args
        .get("paths")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if let Some(single) = args.get("localPath").and_then(|v| v.as_str()) {
        if !single.is_empty() {
            paths.push(single.to_string());
        }
    }
    if paths.is_empty() {
        anyhow::bail!("enqueue: no files to send");
    }

    let dto = h.peer_send_files(&node_id, &paths)?;
    transfer_to_task(&dto).map_err(|e| anyhow::anyhow!("send map failed: {e}"))
}

// ==================== 批 DTO → 前端 wire 映射 ====================

/// 批内文件展示名：首文件名（多文件追加 +N）
fn display_name(dto: &serde_json::Value) -> String {
    let files = dto.get("files").and_then(|v| v.as_array());
    let first = files
        .and_then(|f| f.first())
        .and_then(|f| f.get("path"))
        .and_then(|v| v.as_str())
        .unwrap_or("file");
    let name = first.rsplit('/').next().unwrap_or(first);
    let extra = files.map(|f| f.len().saturating_sub(1)).unwrap_or(0);
    if extra > 0 {
        format!("{name} +{extra}")
    } else {
        name.to_string()
    }
}

/// PeerTransferDto（direction=send）→ 旧 Task wire 形状（snake_case）
pub(crate) fn transfer_to_task(dto: &serde_json::Value) -> Result<serde_json::Value> {
    let status = dto.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let state = match status {
        "running" => "transferring",
        "completed" | "rejected" | "cancelled" | "failed" => status,
        _ => "failed",
    };
    let reason = dto
        .get("detail")
        .and_then(|v| v.as_str())
        .or_else(|| dto.get("rejectReason").and_then(|v| v.as_str()));
    Ok(serde_json::json!({
        "id": dto.get("batchId").cloned().unwrap_or_default(),
        "direction": "upload",
        "peer": {
            "device_id": dto.get("nodeId").cloned().unwrap_or_default(),
            "name": dto.get("peerName").cloned().unwrap_or_default(),
        },
        "remote_path": display_name(dto),
        "local_path": serde_json::Value::Null,
        "size": dto.get("totalBytes").and_then(|v| v.as_u64()).unwrap_or(0),
        "offset": dto.get("transferredBytes").and_then(|v| v.as_u64()).unwrap_or(0),
        "rate_bps": dto.get("rateBps").and_then(|v| v.as_f64()).unwrap_or(0.0),
        "state": state,
        "reason": reason,
        "initiator": "me",
        "batch_id": dto.get("batchId").cloned().unwrap_or_default(),
        "place": serde_json::Value::Null,
        "created_at": dto.get("createdAtMs").and_then(|v| v.as_u64()).unwrap_or(0),
        "updated_at": dto.get("updatedAtMs").and_then(|v| v.as_u64()).unwrap_or(0),
    }))
}

/// PeerTransferDto（非 pending 接收）→ 旧 ReceivingTask wire 形状
pub(crate) fn transfer_to_receiving(dto: &serde_json::Value) -> Result<serde_json::Value> {
    let status = dto.get("status").and_then(|v| v.as_str()).unwrap_or("running");
    Ok(serde_json::json!({
        "session_id": dto.get("batchId").cloned().unwrap_or_default(),
        "batch_id": dto.get("batchId").cloned().unwrap_or_default(),
        "remote_path": display_name(dto),
        "size": dto.get("totalBytes").and_then(|v| v.as_u64()).unwrap_or(0),
        "offset": dto.get("transferredBytes").and_then(|v| v.as_u64()).unwrap_or(0),
        "state": status,
        "reason": dto.get("detail").and_then(|v| v.as_str()),
        "peer_id": dto.get("nodeId").cloned().unwrap_or_default(),
        "peer_name": dto.get("peerName").cloned().unwrap_or_default(),
        "created_at": dto.get("createdAtMs").and_then(|v| v.as_u64()).unwrap_or(0),
        "updated_at": dto.get("updatedAtMs").and_then(|v| v.as_u64()).unwrap_or(0),
    }))
}

/// 终态批 → 旧 HistoryEntry wire 形状（direction: upload=我发出 / download=我接收）
fn terminal_history(list: &[serde_json::Value]) -> Vec<serde_json::Value> {
    list.iter()
        .filter(|t| {
            matches!(
                t.get("status").and_then(|v| v.as_str()),
                Some("completed") | Some("failed") | Some("rejected") | Some("cancelled")
            )
        })
        .map(|t| {
            let direction = if t.get("direction").and_then(|v| v.as_str()) == Some("receive") {
                "download"
            } else {
                "upload"
            };
            serde_json::json!({
                "id": t.get("batchId").cloned().unwrap_or_default(),
                "direction": direction,
                "initiator": if direction == "upload" { "me" } else { "peer" },
                "fileName": display_name(t),
                "size": t.get("totalBytes").and_then(|v| v.as_u64()).unwrap_or(0),
                "state": t.get("status").cloned().unwrap_or_default(),
                "reason": t.get("detail").and_then(|v| v.as_str())
                    .or_else(|| t.get("rejectReason").and_then(|v| v.as_str())),
                "peer_name": t.get("peerName").cloned().unwrap_or_default(),
                "localPath": serde_json::Value::Null,
                "created_at": t.get("createdAtMs").and_then(|v| v.as_u64()).unwrap_or(0),
                "updated_at": t.get("updatedAtMs").and_then(|v| v.as_u64()).unwrap_or(0),
            })
        })
        .collect()
}

/// 从任一方向的批数组推导并推送历史快照
pub(crate) fn emit_history(h: &impl HostEvents, batch_list: &[serde_json::Value]) {
    let history = terminal_history(batch_list);
    if !history.is_empty() {
        // 仅增量提示：前端整表替换时以 created_at 去重合并（composable 侧处理）
        h.emit_event(
            "plugin:file-transfer:history-changed",
            &serde_json::Value::Array(history),
        );
    }
}

// ==================== 队列 / 接收 / 历史 拉取 ====================

/// 发送方向任务列表
pub(crate) fn list_tasks(h: &WasmHost) -> Result<serde_json::Value> {
    let all = h.peer_list_transfers()?;
    let tasks: Vec<serde_json::Value> = all
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|t| t.get("direction").and_then(|v| v.as_str()) == Some("send"))
                .filter_map(|t| transfer_to_task(t).ok())
                .collect()
        })
        .unwrap_or_default();
    Ok(serde_json::Value::Array(tasks))
}

/// pending 接收批（应答卡数据源）——宿主 DTO 即旧 PendingBatch 形状的 camelCase
pub(crate) fn list_batches(h: &WasmHost) -> Result<serde_json::Value> {
    let all = h.peer_list_receiving()?;
    let pending: Vec<serde_json::Value> = all
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|t| t.get("status").and_then(|v| v.as_str()) == Some("pending"))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    Ok(serde_json::Value::Array(pending))
}

/// 非 pending 接收批（正在接收 tab）
pub(crate) fn list_receiving(h: &WasmHost) -> Result<serde_json::Value> {
    let all = h.peer_list_receiving()?;
    let active: Vec<serde_json::Value> = all
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|t| t.get("status").and_then(|v| v.as_str()) != Some("pending"))
                .filter_map(|t| transfer_to_receiving(t).ok())
                .collect()
        })
        .unwrap_or_default();
    Ok(serde_json::Value::Array(active))
}

/// 历史快照（两方向终态合并，按 updated_at 倒序）
pub(crate) fn list_history(h: &WasmHost) -> Result<serde_json::Value> {
    let mut history = terminal_history(h.peer_list_transfers()?.as_array().unwrap_or(&vec![]));
    history.extend(terminal_history(
        h.peer_list_receiving()?.as_array().unwrap_or(&vec![]),
    ));
    history.sort_by_key(|e| -e["updated_at"].as_i64().unwrap_or(0));
    Ok(serde_json::Value::Array(history))
}

// ==================== 远端浏览 / 拉取 ====================

/// 对端共享根清单或目录浏览。
/// args.path 为空 → 返回 { roots: [{ id, name }] }；
/// args.dirId（共享根 id）+ args.path（相对路径）→ { entries, filtered }
pub(crate) fn list_remote(
    h: &WasmHost,
    args: &serde_json::Value,
    active_node: &'static Mutex<String>,
) -> Result<serde_json::Value> {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let dir_id = args.get("dirId").and_then(|v| v.as_str()).unwrap_or("");

    if path.is_empty() && dir_id.is_empty() {
        let roots = h.peer_list_shared_roots(&require_node(active_node))?;
        return Ok(serde_json::json!({ "roots": roots }));
    }

    let root = if dir_id.is_empty() {
        // 旧契约只有路径：以路径首段为根 id 兜底（新 UI 总是显式带 dirId）
        path.split('/').next().unwrap_or("")
    } else {
        dir_id
    };
    let rel = path.strip_prefix(root).unwrap_or(path);
    let rel = rel.strip_prefix('/').unwrap_or(rel);

    let listing = h.peer_browse_directory(&require_node(active_node), root, rel)?;
    let entries = listing.get("entries").cloned().unwrap_or_default();
    let filtered = listing
        .get("filtered")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let notice = if filtered {
        serde_json::json!("filtered")
    } else {
        serde_json::Value::Null
    };
    Ok(serde_json::json!({ "entries": entries, "notice": notice }))
}

/// 拉取远端文件：args.files = [{ name, size }]（当前目录勾选项）+ args.dirId
pub(crate) fn pull_files(
    h: &WasmHost,
    args: &serde_json::Value,
    active_node: &'static Mutex<String>,
) -> Result<serde_json::Value> {
    let dir_id = args
        .get("dirId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing dirId"))?;
    let base = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let names: Vec<String> = args
        .get("files")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .ok_or_else(|| anyhow::anyhow!("missing files"))?;
    if names.is_empty() {
        anyhow::bail!("pull-files: no files selected");
    }

    let files: Vec<serde_json::Value> = names
        .iter()
        .map(|name| {
            let rel = if base.is_empty() {
                name.clone()
            } else {
                format!("{base}/{name}")
            };
            serde_json::json!({ "relPath": rel, "size": 0 })
        })
        .collect();

    let n = h.peer_pull_files(&require_node(active_node), dir_id, &files)?;
    Ok(serde_json::json!({ "count": n }))
}

fn require_node(active_node: &'static Mutex<String>) -> String {
    active_node.lock().expect("active node lock").clone()
}

// ==================== 设置 ====================

/// 设置读取：接收策略 + 共享目录 + 固定下载目录说明
pub(crate) fn get_settings(h: &WasmHost) -> Result<serde_json::Value> {
    let policy = h.peer_get_receive_settings()?;
    let roots = h.peer_list_shared_directories()?;
    Ok(serde_json::json!({
        "roots": roots,
        "policy_mode": policy.get("policyMode").cloned().unwrap_or_else(|| "ask".into()),
        "ask_timeout_sec": policy.get("askTimeoutSecs").and_then(|v| v.as_u64()).unwrap_or(60),
        // 移动端固定落点（只读展示）
        "download_dir": "MediaStore/Downloads",
        "concurrency": 1,
        "encryption": policy.get("encryptionEnabled").and_then(|v| v.as_bool()).unwrap_or(false),
    }))
}

/// 设置写入：仅策略可改（mode: ask|accept|reject → 宿主 ask|always_accept|always_deny）
pub(crate) fn set_settings(h: &WasmHost, args: &serde_json::Value) -> Result<serde_json::Value> {
    if let Some(policy) = args.get("receivingPolicy").and_then(|v| v.as_str()) {
        let mode = match policy {
            "accept" => "always_accept",
            "reject" => "always_deny",
            _ => "ask",
        };
        let timeout = args
            .get("approvalTimeoutSec")
            .and_then(|v| v.as_u64())
            .unwrap_or(60)
            .clamp(10, 600);
        h.peer_set_receive_policy(mode, timeout)?;
    }
    if let Some(v) = args.get("encryption").and_then(|v| v.as_bool()) {
        h.peer_set_transfer_encryption(v)?;
    }
    // concurrency / downloadDir：移动端不支持，静默忽略（UI 已隐藏入口）
    h.log_info("set-settings: policy/encryption applied (concurrency/download-dir not supported)");
    Ok(serde_json::json!({ "ok": true }))
}

/// 添加共享目录：弹 SAF 目录树选择器（授权由宿主持久化），取消报错
pub(crate) fn mount_local(h: &WasmHost) -> Result<serde_json::Value> {
    let dto = h.peer_add_shared_directory(&serde_json::json!({}))?;
    if dto.is_null() {
        anyhow::bail!("cancelled");
    }
    Ok(dto)
}

/// 移除共享目录：args.remove = 条目 id
pub(crate) fn update_roots(h: &WasmHost, args: &serde_json::Value) -> Result<serde_json::Value> {
    let id = args
        .get("remove")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing remove id"))?;
    let removed = h.peer_remove_shared_directory(id)?;
    Ok(serde_json::json!({ "removed": removed }))
}
