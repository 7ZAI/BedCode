//! 接收编排退役面防回接锁（票 07 · 移动端）
//!
//! 票 07 把**接收方向**的任务编排整体下沉 `file-transfer` 插件（事件归约状态机，
//! 真源在插件存储）：宿主 `peer/receive.rs` 收敛为「询问回执表 + 引擎事件桥 +
//! 策略闸门」——旧版所持的接收任务表、进度入账与终态结算、暂停状态同步、
//! 终态封顶、原因码映射与全量快照推送（`peer-receive-changed` → 总线
//! `peer:receive`）一并退役。谁把这些加回来，谁就要先推翻票 07 裁决。
//!
//! 与发送面锁（`retired_mobile_send_orchestration_lock.rs`）同款结构面扫描，
//! 互不重叠：发送面锁只扫 `peer/transfer.rs` 与 `peer/net.rs` 的发送构件，本锁
//! 只扫接收构件 + 接收快照桥接（2026-10-14 模块化后 `src/peer/` 下原
//! `peer_net.rs` 拆为 net / ports / seam，扫描面按内容并集保持）。
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

    for rel in [
        "src/peer/receive.rs",
        "src/peer/remote.rs",
        "src/peer/net.rs",
        "src/peer/ports.rs",
        "src/peer/seam.rs",
    ] {
        let path = mobile_src_root().join(rel);
        let content = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {rel} 失败：{e}"));
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
fn receive_orchestration_tests_are_not_orphaned_into_a_dead_subdirectory() {
    // `src/peer_receive/tests/` 曾是一对永不编译的单元测试（票 01–16 的能力域
    // 集中化把 `ReceivePolicy` / `validate_concurrency` / 设置落盘等符号整体搬进
    // `bedcode-server-peer-net` 后，生产文件被清空成纯委托，但 `tests/` 目录与
    // 其 `#[cfg(test)] mod tests` 声明一并消失——Rust 不会自动发现 `tests/*.rs`，
    // 于是两个文件静静躺着、引用着一批已不存在的符号，既不编译也不报错。
    //
    // 这比「测试缺失」更糟：读代码的人以为接收侧有覆盖，实际零覆盖。本锁把该
    // 形态钉死——接收模块下不允许再出现子目录测试面（2026-10-14 收拢后同名子
    // 目录是 `src/peer/receive/`，死测试形态不变）。真要把接收侧用例补回来，必须
    // 写在 `src/peer/receive.rs` 的 `#[cfg(test)]` 模块里，或落到符号真源所在的
    // `bedcode-server-peer-net` crate。
    let orphan_dir = mobile_src_root().join("src/peer/receive");
    assert!(
        !orphan_dir.exists(),
        "src/peer/receive/ 子目录已随能力域集中化退役：其 tests/ 会脱离编译（无 \
         mod 声明）而成为永不编译的死测试。接收侧用例请写进 src/peer/receive.rs \
         的 #[cfg(test)] 模块，或落到 packages/bedcode-server-peer-net 真源处。"
    );
}

#[test]
fn retired_receive_snapshot_event_is_no_longer_bridged_to_plugin_bus() {
    // 旧快照事件 `peer-receive-changed` → topic `peer:receive` 的桥接必须摘除：
    // 接收任务表删除后无快照可推，留着映射等于给「空快照回流」留静默降级入口，
    // 且会让插件误以为「无待应答批」（实为事件流断了）。锁的是 topic 字面量与
    // 快照发射调用——`bus_topic_for` 的反向断言只含事件名字面量，不误伤。
    let mut violations: Vec<String> = Vec::new();
    for rel in ["src/peer/net.rs", "src/peer/ports.rs", "src/peer/seam.rs", "src/peer/receive.rs"] {
        let path = mobile_src_root().join(rel);
        let content = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {rel} 失败：{e}"));
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
