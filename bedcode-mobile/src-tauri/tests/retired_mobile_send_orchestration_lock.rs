//! 发送编排退役面防回接锁（票 06 · 移动端）
//!
//! 票 06 把**发送方向**的任务编排整体下沉 `file-transfer` 插件（事件归约状态机，
//! 真源在插件存储）：宿主 `peer_transfer.rs` 收敛为「会话句柄表 + 引擎事件桥」——
//! 旧版所持的任务 DTO 形状、发送并发闸门与队列泵、终态历史文件、serve 供流记账、
//! 原因码映射、批量恢复编排与并发脉冲字段一并退役。谁把这些加回来，谁就要先
//! 推翻票 06 裁决。
//!
//! **范围只限发送域**：`peer_receive.rs` 的接收侧任务表随票 07 收口，届时本锁
//! 扩展到接收面（票 10 一并做命令面锁）。
//!
//! 两条 fail-visible 保险互为补充：
//! - 本锁（结构面）：源码里不得再出现编排构件的定义或调用；
//! - 载荷检测（行为面）：`host_impl::peer_send_files` 显性拒绝已退役的
//!   `concurrency` 字段并点名 ABI v12 重建。
//!
//! 只扫非注释行：模块头「为什么删」的说明段落是记账，不是回接。

use std::path::{Path, PathBuf};

/// 退役构件清单：任务表 / 并发闸门 / 历史文件 / serve 记账 / 批量恢复编排
const RETIRED_SEND_ORCHESTRATION: [&str; 13] = [
    // 任务状态机与 DTO（PeerTransferDto/PeerTransferFileDto 作为接收侧共享
    // wire 形状仍在本文件——故只锁「任务表持有者」SendTask 与历史封顶常量）
    "struct SendTask",
    "struct PeerTransferInner",
    "struct HistoryFile",
    "HISTORY_CAP:",
    "HISTORY_FILE:",
    "evict_history_cap_locked",
    "persist_history",
    // 发送并发闸门与队列泵
    "pick_pending_to_start",
    "pump_send_queue",
    "pump_after_settle",
    "current_concurrency",
    // 批量恢复编排与 serve 记账（归插件）
    "resume_all_peer_transfers",
    "register_serve_task",
];

fn mobile_src_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

#[test]
fn retired_mobile_send_orchestration_is_not_reintroduced() {
    let mut violations: Vec<String> = Vec::new();

    for rel in ["src/peer_transfer.rs", "src/peer_net.rs"] {
        let path = mobile_src_root().join(rel);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (idx, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            for needle in RETIRED_SEND_ORCHESTRATION {
                if line.contains(needle) {
                    violations.push(format!("{rel}:{}: {}", idx + 1, line.trim()));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "移动端发送编排（票 06 已整体下沉 file-transfer 插件）出现回接痕迹：\n{}",
        violations.join("\n")
    );
}

#[test]
fn retired_send_snapshot_event_is_no_longer_bridged_to_plugin_bus() {
    // 旧快照 topic `peer:transfer` 的桥接映射必须摘除：任务状态机删除后无快照
    // 可推，留着映射等于给「空快照回流」留静默降级入口
    let path = mobile_src_root().join("src/peer_net.rs");
    let content = std::fs::read_to_string(&path).expect("read peer_net.rs");
    let mut violations: Vec<String> = Vec::new();
    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim_start();
        if line.starts_with("//") {
            continue;
        }
        if line.contains("\"peer:transfer\"") {
            violations.push(format!("src/peer_net.rs:{}: {}", idx + 1, line.trim()));
        }
    }
    assert!(
        violations.is_empty(),
        "发送快照 topic 桥接（peer:transfer）已随票 06 退役，回流走 peer:transfer-event：\n{}",
        violations.join("\n")
    );
}
