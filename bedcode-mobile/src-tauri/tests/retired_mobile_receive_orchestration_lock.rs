//! 接收编排退役面防回接锁（票 07 · 移动端）
//!
//! 票 07 把**接收方向**的任务编排整体下沉 `file-transfer` 插件（事件归约状态机，
//! 真源在插件存储）：宿主 `peer_receive.rs` 收敛为「询问回执表 + 引擎事件桥 +
//! 策略闸门」——旧版所持的接收任务表、进度入账与终态结算、暂停状态同步、
//! 终态封顶、原因码映射与全量快照推送（`peer-receive-changed` → 总线
//! `peer:receive`）一并退役。谁把这些加回来，谁就要先推翻票 07 裁决。
//!
//! 与发送面锁（`retired_mobile_send_orchestration_lock.rs`）同款结构面扫描，
//! 互不重叠：发送面锁只扫 `peer_transfer.rs` / `peer_net.rs` 的发送构件，本锁
//! 只扫接收构件 + 接收快照桥接。
//!
//! 两条 fail-visible 保险：
//! - 本锁（结构面）：源码里不得再出现接收编排构件的定义或调用；
//! - topic 桥接锁：已退役的快照 topic 不得再被映射进插件总线（残留映射会把
//!   空快照回流成静默降级）。
//!
//! 只扫非注释行：模块头「为什么删」的说明段落是记账，不是回接。

use std::path::{Path, PathBuf};

/// 退役构件清单：接收任务表 / 状态机 / 快照推送 / 宿主列表命令
const RETIRED_RECEIVE_ORCHESTRATION: [&str; 12] = [
    // 任务状态机与 DTO（peerName/files/原因码等业务字段的宿主投影形状）
    "PeerTransferDto",
    "tasks: Vec<",
    "inner.tasks",
    "fn publish(app",
    "fn snapshot(app",
    // 终态封顶与历史清理
    "RECEIVE_TERMINAL_CAP",
    "evict_terminal_cap_locked",
    "clear_peer_receiving_history",
    // 进度入账 / 终态结算 / 暂停同步 / 展示名解析
    "update_progress",
    "settle_terminal",
    "set_receive_pause_status",
    "resolve_peer_name",
];

fn mobile_src_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

#[test]
fn retired_mobile_receive_orchestration_is_not_reintroduced() {
    let mut violations: Vec<String> = Vec::new();

    for rel in ["src/peer_receive.rs", "src/peer_remote.rs", "src/peer_net.rs"] {
        let path = mobile_src_root().join(rel);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (idx, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            for needle in RETIRED_RECEIVE_ORCHESTRATION {
                if line.contains(needle) {
                    violations.push(format!("{rel}:{}: {}", idx + 1, line.trim()));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "移动端接收编排（票 07 已整体下沉 file-transfer 插件）出现回接痕迹：\n{}",
        violations.join("\n")
    );
}

#[test]
fn retired_receive_snapshot_event_is_no_longer_bridged_to_plugin_bus() {
    // 旧快照事件 `peer-receive-changed` → topic `peer:receive` 的桥接必须摘除：
    // 接收任务表删除后无快照可推，留着映射等于给「空快照回流」留静默降级入口，
    // 且会让插件误以为「无待应答批」（实为事件流断了）。锁的是 topic 字面量与
    // 快照发射调用——`bus_topic_for` 的反向断言只含事件名字面量，不误伤。
    let mut violations: Vec<String> = Vec::new();
    for rel in ["src/peer_net.rs", "src/peer_receive.rs"] {
        let path = mobile_src_root().join(rel);
        let content = std::fs::read_to_string(&path).expect("read peer source");
        for (idx, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            if line.contains("\"peer:receive\"") || line.contains("emit_json(app, \"peer-receive") {
                violations.push(format!("{rel}:{}: {}", idx + 1, line.trim()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "接收快照桥接（peer:receive / peer-receive-changed）已随票 07 退役，回流走 peer:receive-event：\n{}",
        violations.join("\n")
    );
}
