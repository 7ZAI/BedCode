//! 对等网络能力域集中化锁（2026-10-10）：移动宿主不得重新持有引擎控制面。
//!
//! ## 锁的是什么
//!
//! 移动宿主的对等网络引擎控制面（节点装配 / 生命周期闸门 / 会话句柄表 /
//! 接收回执表与设置落盘 / 事件翻译载荷 / 远端拉取编排）已整体下沉共享能力域
//! `packages/bedcode-server-peer-net`（双端同引纯引擎形态）；移动侧只剩适配层
//! ——2026-10-14 模块化收拢后住在 `src/peer/`（原五个 `src/peer_*.rs` 平铺文件：
//! 原 `peer_net.rs` 拆为 `net.rs`（装配 + 生命周期 + 委托）/ `ports.rs`（端口实现）/
//! `seam.rs`（平台缝），`transfer/receive/remote/migration` 原样入模块）。谁把这些
//! 符号加回宿主，谁就先把引擎拆回了第二份真源——本锁直接测红。
//!
//! ## 三面判据（缺一不可）
//!
//! 1. **退役面缺席**：引擎状态机 / 事件桥 / 设置落盘 / 载荷构造的符号与字面量
//!    在 `src/peer/` 适配面归零（扫非注释行——文档里记账「为什么迁」不算回接；
//!    扫描面 = 拆分前三文件内容并集，路径迁移不放松覆盖）；
//! 2. **适配面在场**（防「删新面绕过锁」）：`PeerEnginePorts` / `PeerPlatformSeam`
//!    / `peer_ctx` / 插件 id / mDNS 共享守护复用必须在适配面各文件在场；
//! 3. **共享依赖在场**：`Cargo.toml` 必须引 `bedcode-server-peer-net`（否则有人
//!    把适配层改回自持实现也能让前两面通过——依赖缺席即回接的信号）。

use std::path::{Path, PathBuf};

fn src_tauri_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 模块入口：子模块声明必需，缺它整组 peer 代码不在编译面内（适配面在场判据之一）
const MODULE_ENTRY: &str = "src/peer.rs";

/// 适配层模块（退役面扫描面；对应拆分前 `peer_net.rs` + 三个委托模块的内容并集）
const ADAPTER_FILES: &[&str] = &[
    "src/peer/net.rs",
    "src/peer/ports.rs",
    "src/peer/seam.rs",
    "src/peer/transfer.rs",
    "src/peer/receive.rs",
    "src/peer/remote.rs",
];

/// 宿主侧迁移模块：**职责就是读旧文件**（把历史 `transfer_settings.json` 等
/// 一次性幂等迁入插件存储键）——只做在场断言，不参与退役面扫描
const HOST_MIGRATION_FILES: &[&str] = &["src/peer/migration.rs"];

/// 退役面：出现即回接（引擎控制面真源在 `packages/bedcode-server-peer-net`）
const RETIRED_ENGINE_NEEDLES: &[&str] = &[
    // 节点装配 / 生命周期闸门
    "fn start_locked(",
    "fn stop_locked(",
    "fn spawn_session_watch(",
    "fn drive_gate(",
    "fn resolve_device_name(",
    // 发送 / 接收 / 远端编排
    "fn drive_serve_events(",
    "fn drive_receive_events(",
    "fn register_handler(",
    "fn register_offer(",
    "fn announce_remote_pull_started(",
    "fn announce_local_failure(",
    "PULL_FILES_CAP",
    // 接收设置落盘（R1：业务真源归插件）
    "transfer_settings.json",
    "fn persist_settings(",
    "fn read_settings_file(",
    "fn ensure_settings_loaded(",
    // 引擎状态类型定义（adapter 只许 `pub use` 再导出）
    "struct PeerNetState",
    "struct PeerTransferState",
    "struct PeerReceiveState",
    "struct PeerRemoteState",
    "struct PendingConsent",
    "struct OutboundSession",
    // 引擎事件载荷构造（真源在能力域；含 R2/R4 统一后的形状）
    "fn engine_progress_payload(",
    "fn engine_terminal_payload(",
    "fn engine_offer_pending_payload(",
    "fn engine_offer_payload(",
    "fn engine_pull_served_payload(",
    "fn engine_pull_started_payload(",
    "fn engine_node_stopped_payload(",
];

/// 适配面：必须在场（正面断言，防「删面代替迁移」）——(文件, needle)
const KEPT_ADAPTER_FACE: &[(&str, &str)] = &[
    // 端口 + 平台缝的对象在装配点可见
    ("src/peer/net.rs", "PeerEnginePorts"),
    ("src/peer/net.rs", "fn peer_ctx("),
    ("src/peer/net.rs", "FILE_TRANSFER_PLUGIN_ID"),
    // mDNS 复用：共享守护经 discovery-engine 引擎三函数（零第二份 mDNS 代码）
    ("src/peer/ports.rs", "bedcode_discovery_engine::engine::shared_daemon"),
    // 平台缝 trait 实现（SAF 共享访问 / 落点提升 / 内置共享条目）
    ("src/peer/seam.rs", "impl PeerPlatformSeam for MobilePlatformSeam"),
];

/// 逐行扫描（跳过纯注释行——模块头记账段落不算回接），返回命中记录
fn scan_source_lines(path: &Path, needles: &[&str]) -> Vec<String> {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("读取 {} 失败：{e}", path.display()));
    let mut hits: Vec<String> = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        for needle in needles {
            if line.contains(needle) {
                hits.push(format!("{}:{} 命中 `{needle}`", path.display(), idx + 1));
            }
        }
    }
    hits
}

/// 退役面缺席：移动宿主适配面（+ 迁移模块 / 模块入口）不得重新定义引擎控制面
#[test]
fn mobile_peer_engine_control_plane_is_not_reintroduced() {
    let root = src_tauri_root();
    let mut violations: Vec<String> = Vec::new();
    for rel in ADAPTER_FILES
        .iter()
        .chain(HOST_MIGRATION_FILES.iter())
        .chain(std::iter::once(&MODULE_ENTRY))
    {
        let path = root.join(rel);
        assert!(
            path.is_file(),
            "边界锁空转：适配层文件不存在 {}（删 / 搬文件绕过锁与回接同样是破坏，\
             搬动后须同步本锁路径）",
            path.display()
        );
    }
    for rel in ADAPTER_FILES {
        violations.extend(scan_source_lines(&root.join(rel), RETIRED_ENGINE_NEEDLES));
    }
    assert!(
        violations.is_empty(),
        "移动宿主重新持有对等网络引擎控制面（真源 = packages/bedcode-server-peer-net，\
         只允许适配层委托 + 平台缝注入）：\n{}",
        violations.join("\n")
    );
}

/// 适配面在场：端口面 / 平台缝 / 装配入口 / 插件 id / mDNS 复用缺一不可
#[test]
fn mobile_peer_adapter_face_stays() {
    let root = src_tauri_root();
    for (rel, needle) in KEPT_ADAPTER_FACE {
        let path = root.join(rel);
        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {} 失败：{e}", path.display()));
        assert!(
            content.contains(needle),
            "适配面缺失：{rel} 中找不到 `{needle}`（能力域集中化后宿主仍须装配端口 / \
             平台缝 / 生命周期入口，并复用共享 mDNS 守护）"
        );
    }
}

/// 共享依赖在场：宿主必须引能力域 crate（依赖缺席 = 自持实现的信号）
#[test]
fn mobile_depends_on_shared_peer_capability() {
    let path = src_tauri_root().join("Cargo.toml");
    let content =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {} 失败：{e}", path.display()));
    for needle in ["bedcode-server-peer-net", "bedcode-server-base"] {
        assert!(
            content.contains(needle),
            "Cargo.toml 缺少共享能力域依赖 `{needle}`：移动宿主必须装配\
             packages/ 下的共享引擎与端口词汇，不得自持第二份实现"
        );
    }
}
