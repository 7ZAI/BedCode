//! 对等网络引擎原始事件桥（票 07 抽出的双方向共用面）。
//!
//! 发送方向（[`super::peer_transfer`]）与接收方向（[`super::peer_receive`]）
//! 各自只持有**引擎控制面**（发送会话句柄表 / 接收询问回执表 + 策略闸门），
//! 而「引擎事实 → 插件总线事件」的翻译单点在本模块：载荷字段即引擎事实
//! （camelCase 直译），宿主**零业务加工**——原因码映射、终态判定、任务行建行、
//! 进度入账全在插件侧 `transfer_store::reduce_event` 归约。
//!
//! 两条 topic（topic 形态即 ACL，精确匹配无前缀）：
//! - [`TOPIC_TRANSFER_EVENT`] `peer:transfer-event`：send 方向（本端发起批 +
//!   serve 供流记账批），票 06 建立；
//! - [`TOPIC_RECEIVE_EVENT`] `peer:receive-event`：receive 方向（入站询问批 +
//!   本端拉取批 + 节点停止），票 07 建立。
//!
//! 旧快照 topic（`peer:transfer` / `peer:receive`）已随对应方向的状态机退役：
//! 任务状态机删除后无快照可推，留着映射等于给「空快照回流」留静默降级入口。
//!
//! Progress 节流窗口（150ms）同属纯性能参数，两方向各自在事件循环内自持
//! （各自的事件到达率不同，共用一个窗口会把一侧的节流记账算到另一侧头上）。

use bedcode_peer_net::{FileMeta, NodeId, TerminalState};

/// 进度事件最小发射间隔：引擎按 ≤64KiB chunk 发射 Progress，不加节流会在高速
/// 链路打爆总线；数值远低于人眼感知阈值
pub(crate) const PROGRESS_EMIT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);

/// 引擎原始事件 topic（发送方向 = 本端发起批 + serve 供流记账批）
pub(crate) const TOPIC_TRANSFER_EVENT: &str = "peer:transfer-event";

/// 引擎原始事件 topic（接收方向 = 入站询问批 + 本端拉取批 + 节点停止）
pub(crate) const TOPIC_RECEIVE_EVENT: &str = "peer:receive-event";

/// 当前毫秒时间戳（事件载荷时间字段真源；引擎不产时间戳，宿主补齐）
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ==================== 载荷构造（引擎事实直译，纯函数） ====================

/// 引擎 Progress 事件载荷（camelCase；字段即引擎事实，宿主零加工）
pub(crate) fn engine_progress_payload(
    batch_id: &str,
    transferred: u64,
    total: u64,
    rate_bps: f64,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "progress",
        "batchId": batch_id,
        "transferred": transferred,
        "total": total,
        "rateBps": rate_bps,
        "tsMs": now_ms(),
    })
}

/// 引擎 Terminal 事件载荷：终态形状随 TerminalState 直译（type + 变体字段），
/// 不做「cancelled-by-*」等产品原因码映射（前端 i18n 约定归插件）
pub(crate) fn engine_terminal_payload(batch_id: &str, state: &TerminalState) -> serde_json::Value {
    serde_json::json!({
        "kind": "terminal",
        "batchId": batch_id,
        "state": terminal_state_payload(state),
        "tsMs": now_ms(),
    })
}

/// TerminalState → wire 形状（纯函数，单测锚点）
pub(crate) fn terminal_state_payload(state: &TerminalState) -> serde_json::Value {
    match state {
        TerminalState::Completed => serde_json::json!({ "type": "completed" }),
        TerminalState::Rejected { reason } => {
            serde_json::json!({ "type": "rejected", "reason": reason.as_str() })
        }
        TerminalState::Cancelled { by_peer } => {
            serde_json::json!({ "type": "cancelled", "byPeer": by_peer })
        }
        TerminalState::Failed { detail } => {
            serde_json::json!({ "type": "failed", "detail": detail })
        }
    }
}

/// 引擎暂停/恢复事件载荷（kind = "paused" | "resumed"）
pub(crate) fn engine_pause_payload(kind: &str, batch_id: &str) -> serde_json::Value {
    serde_json::json!({ "kind": kind, "batchId": batch_id, "tsMs": now_ms() })
}

/// 引擎供流记账事件载荷（pull-served）：本端为对端拉取供流的事实——插件据此
/// 自建 direction=send 记账视图（宿主不再代记 send 任务）
pub(crate) fn engine_pull_served_payload(
    remote: &NodeId,
    batch_id: &str,
    files: &[FileMeta],
    total_size: u64,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "pull-served",
        "batchId": batch_id,
        "nodeId": remote.as_str(),
        "files": files_payload(files),
        "totalSize": total_size,
        "tsMs": now_ms(),
    })
}

/// 引擎入站询问事件载荷（offer-pending）：对端 offer 一批待本端应答——插件据此
/// 自建 direction=receive 待应答行（宿主只保留应答回执表，不持任务行）
pub(crate) fn engine_offer_pending_payload(
    remote: &NodeId,
    batch_id: &str,
    files: &[FileMeta],
    total_size: u64,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "offer-pending",
        "batchId": batch_id,
        "nodeId": remote.as_str(),
        "files": files_payload(files),
        "totalSize": total_size,
        "tsMs": now_ms(),
    })
}

/// 本端拉取会话发起事实载荷（pull-started）：拉取 batch_id 由引擎铸造、调用方
/// （插件）事前拿不到，故经本事件回灌建行——否则首条 Progress 无处归约
pub(crate) fn engine_pull_started_payload(
    remote: &NodeId,
    batch_id: &str,
    rel_path: &str,
    size: u64,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "pull-started",
        "batchId": batch_id,
        "nodeId": remote.as_str(),
        "files": [{ "path": rel_path, "size": size }],
        "totalSize": size,
        "tsMs": now_ms(),
    })
}

/// 节点停止事实载荷（node-stopped）：引擎通道关闭即节点下线，在飞批全部中止；
/// 插件据此把在册进行中条目标注 `interrupted`（宿主无任务表可写终态）
pub(crate) fn engine_node_stopped_payload() -> serde_json::Value {
    serde_json::json!({ "kind": "node-stopped", "tsMs": now_ms() })
}

/// 引擎 FileMeta 清单 → wire 文件数组（`{ path, size }`，零加工）
fn files_payload(files: &[FileMeta]) -> Vec<serde_json::Value> {
    files
        .iter()
        .map(|f| serde_json::json!({ "path": f.path, "size": f.size }))
        .collect()
}

// ==================== 直推 ====================

/// 引擎原始事件直推插件总线：不经宿主状态机加工，Progress 节流由调用方各自复用
/// 150ms 窗口；事件流为高频通道，统一 debug 级（日志红线：热路径 info 噪音禁止）。
/// 无总线上下文（插件管理器未就绪）静默跳过
pub(crate) fn publish_engine_event(topic: &str, payload: serde_json::Value) {
    tracing::debug!(
        topic,
        kind = %payload["kind"].as_str().unwrap_or("?"),
        "peer engine event pushed to plugin bus"
    );
    if let Some(pm) = crate::state::try_get_plugin_manager() {
        pm.message_bus().publish(topic, "host", payload);
    }
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// Progress 载荷：字段即引擎事实（camelCase 直译），无业务加工
    #[test]
    fn progress_payload_carries_engine_facts() {
        let p = engine_progress_payload("b-1", 42, 100, 3.5);
        assert_eq!(p["kind"], "progress");
        assert_eq!(p["batchId"], "b-1");
        assert_eq!(p["transferred"], 42);
        assert_eq!(p["total"], 100);
        assert_eq!(p["rateBps"], 3.5);
        assert!(p["tsMs"].as_u64().is_some());
    }

    /// Terminal 载荷：终态形状随 TerminalState 直译——reason 是 wire 枚举串
    /// 而非产品文案；「cancelled-by-*」等 i18n 约定不得在宿主出现
    #[test]
    fn terminal_payload_translates_state_variants_without_reason_codes() {
        let cases = [
            (TerminalState::Completed, "completed"),
            (
                TerminalState::Rejected {
                    reason: bedcode_peer_net::transfer::RejectReason::PolicyDenied,
                },
                "rejected",
            ),
            (TerminalState::Cancelled { by_peer: true }, "cancelled"),
            (TerminalState::Failed { detail: "io".into() }, "failed"),
        ];
        for (state, expect_type) in cases {
            let p = engine_terminal_payload("b-2", &state);
            assert_eq!(p["kind"], "terminal");
            assert_eq!(p["batchId"], "b-2");
            assert_eq!(p["state"]["type"], expect_type);
            assert!(
                p["state"]
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .is_none_or(|r| !r.starts_with("cancelled-by")),
                "宿主不得产出产品原因码（归插件归约）"
            );
        }
    }

    /// pull-served / offer-pending 载荷：供流记账与入站询问的建行事实同形
    #[test]
    fn anchor_payloads_carry_files_and_total() {
        let node_id = NodeId::parse(&"a".repeat(64)).expect("node id");
        let files = vec![FileMeta {
            path: "x.bin".to_string(),
            size: 9,
        }];
        let served = engine_pull_served_payload(&node_id, "b-4", &files, 9);
        assert_eq!(served["kind"], "pull-served");
        assert_eq!(served["batchId"], "b-4");
        assert_eq!(served["totalSize"], 9);
        assert_eq!(served["files"][0]["path"], "x.bin");
        assert_eq!(served["files"][0]["size"], 9);

        let offer = engine_offer_pending_payload(&node_id, "b-5", &files, 9);
        assert_eq!(offer["kind"], "offer-pending");
        assert_eq!(offer["nodeId"], node_id.as_str());
        assert_eq!(offer["totalSize"], 9);
        assert_eq!(offer["files"][0]["path"], "x.bin");
    }

    /// pull-started 载荷：拉取单文件批的建行事实（rel_path + size 直传）
    #[test]
    fn pull_started_payload_carries_single_file_spec() {
        let node_id = NodeId::parse(&"b".repeat(64)).expect("node id");
        let p = engine_pull_started_payload(&node_id, "pull-1-0", "docs/a.txt", 12);
        assert_eq!(p["kind"], "pull-started");
        assert_eq!(p["batchId"], "pull-1-0");
        assert_eq!(p["nodeId"], node_id.as_str());
        assert_eq!(p["totalSize"], 12);
        assert_eq!(p["files"][0]["path"], "docs/a.txt");
        assert_eq!(p["files"][0]["size"], 12);
    }

    /// paused/resumed 载荷：kind + batchId 最小事实
    #[test]
    fn pause_payload_carries_kind_and_batch() {
        let p = engine_pause_payload("paused", "b-5");
        assert_eq!(p["kind"], "paused");
        assert_eq!(p["batchId"], "b-5");
        let p = engine_pause_payload("resumed", "b-6");
        assert_eq!(p["kind"], "resumed");
    }

    /// node-stopped 载荷：节点下线是全方向事实（无 batchId），插件据此标注中断
    #[test]
    fn node_stopped_payload_has_no_batch_scope() {
        let p = engine_node_stopped_payload();
        assert_eq!(p["kind"], "node-stopped");
        assert!(p.get("batchId").is_none());
        assert!(p["tsMs"].as_u64().is_some());
    }
}
