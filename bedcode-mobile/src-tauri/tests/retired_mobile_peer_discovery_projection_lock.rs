//! 发现 / 设备列表投影退役面防回接锁（票 09 · 移动端）
//!
//! 票 09 把**发现投影与共享目录注册表**的宿主侧残留清干净：设备列表真源是
//! 插件前端自建缓存（`deviceState` 状态机，host-mdns 属主定向 found/lost 驱动，
//! last-seen 经插件 storage 落盘），共享目录注册表真源是插件 `roots_registry`
//! （host-storage 单键 + `set-shared-roots` 全量推送引擎镜像面）。宿主因此
//! 不再持有设备列表 DTO / 缓存解析版拨号 / 注册表 CRUD，也不再有任何前端
//! 命令面（节点启停、首连应答、信任管理全部只经 host-peer 原语与插件
//! activate-deactivate 外壳）。
//!
//! 与发送面（`retired_mobile_send_orchestration_lock.rs`）、接收面
//! （`retired_mobile_receive_orchestration_lock.rs`）锁同款结构面扫描，三锁
//! 互不重叠：本锁只扫发现投影 / 注册表 / 节点生命周期命令面。
//!
//! 两条 fail-visible 保险：
//! - 结构面锁：退役命令字面量与投影 DTO 不得在宿主源码再现；
//! - topic 锁：`peer:devices` 的**发布与订阅**都不得复活（宿主无发布者，
//!   插件曾长期订阅一个死 topic 做规模对账——两边都清掉才算真退役）。
//!
//! 只扫非注释行：模块头「为什么删」的记账段落与本锁自身的说明不算回接。

use std::path::{Path, PathBuf};

/// 退役构件清单：设备列表投影 + 共享目录注册表 CRUD + 无主节点启停
const RETIRED_DISCOVERY_PROJECTION: [&str; 7] = [
    // 设备列表派生视图（DTO + 查询命令）
    "DiscoveredPeerDto",
    "list_discovered_peers",
    "peer_net::dial_peer,",
    // 共享目录注册表宿主侧第二份真源（DTO + 三命令）
    "SharedDirDto",
    "list_shared_directories",
    "add_shared_directory_saf",
    "remove_shared_directory",
];

/// 无主节点启停命令面：节点生命周期只经 host-peer 属主原语与插件外壳驱动
///
/// needle 同时覆盖「函数定义」与「注册引用」两种回接形态——只锁注册会让
/// 重新加回 `#[tauri::command]` 函数却暂不注册的实现溜过去。
const RETIRED_HOST_COMMAND_FACE: [&str; 4] = [
    "fn start_peer_node(",
    "fn stop_peer_node(",
    "peer_net::start_peer_node",
    "peer_net::stop_peer_node",
];

fn mobile_src_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 逐行扫描宿主源码（跳过纯注释行），返回命中的违规记录
fn scan(retired: &[&str]) -> Vec<String> {
    let mut violations: Vec<String> = Vec::new();
    for rel in ["src/peer_net.rs", "src/lib.rs"] {
        let path = mobile_src_root().join(rel);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (idx, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            for needle in retired {
                if line.contains(needle) {
                    violations.push(format!("{rel}:{}: {}", idx + 1, line.trim()));
                }
            }
        }
    }
    violations
}

#[test]
fn retired_mobile_peer_discovery_projection_is_not_reintroduced() {
    let violations = scan(&RETIRED_DISCOVERY_PROJECTION);
    assert!(
        violations.is_empty(),
        "移动端发现投影 / 共享目录注册表（票 09 已下沉 file-transfer 插件）出现回接痕迹：\n{}",
        violations.join("\n")
    );
}

#[test]
fn retired_host_peer_command_face_is_not_reintroduced() {
    // 节点启停曾以「不认领属主」的宿主命令面存在，与 host-peer 谁起谁停的属主
    // 记账直接冲突（无主启动会让插件停用后节点仍在跑）。真入口只有
    // host-peer `start-node` / `stop-node` 与插件 activate/deactivate 外壳。
    let violations = scan(&RETIRED_HOST_COMMAND_FACE);
    assert!(
        violations.is_empty(),
        "宿主 peer 命令面（票 09 起零前端命令面）出现回接痕迹：\n{}",
        violations.join("\n")
    );
}

#[test]
fn retired_peer_devices_topic_is_neither_published_nor_subscribed() {
    // `peer:devices` 是宿主设备列表快照的插件总线 topic。快照链路退役后宿主
    // 无发布者，而插件曾长期订阅它做「规模对账」——一个永远收不到消息的
    // 对账分支会让设备列表问题排查时误以为对账正常。发布与订阅两端都要清。
    let mut violations: Vec<String> = Vec::new();
    let roots = [
        mobile_src_root().join("src"),
        mobile_src_root().join("../wasm-apps/file-transfer/rust/src"),
    ];
    for root in roots {
        let Ok(dir) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (idx, raw_line) in content.lines().enumerate() {
                let line = raw_line.trim_start();
                if line.starts_with("//") {
                    continue;
                }
                if line.contains("\"peer:devices\"") {
                    violations.push(format!("{}:{}: {}", path.display(), idx + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "已退役的设备列表快照 topic `peer:devices` 出现回接（发布或订阅）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn endpoint_dial_is_the_only_peer_dial_entry() {
    // 拨号寻址来源必须显式（ADR 0022 v2）：插件从自身设备缓存解析 endpoint 后
    // 传入，宿主只留 `dial_peer_endpoint`。锁「缓存解析版」函数定义不得复活
    // （`dial_peer_endpoint` 是唯一允许的拨号函数名，故 needle 带 `(` 收窄）。
    let path = mobile_src_root().join("src/peer_net.rs");
    let content = std::fs::read_to_string(&path).expect("read peer_net.rs");
    let mut violations: Vec<String> = Vec::new();
    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim_start();
        if line.starts_with("//") {
            continue;
        }
        if line.contains("fn dial_peer(") {
            violations.push(format!("src/peer_net.rs:{}: {}", idx + 1, line.trim()));
        }
    }
    assert!(
        violations.is_empty(),
        "缓存解析版拨号（`dial_peer`）已随票 09 退役，拨号只走 endpoint 显式寻址：\n{}",
        violations.join("\n")
    );
}
