//! mDNS 广播死面退役锁（2026-10-10 能力域集中化票 05）：移动宿主不得复活广播面。
//!
//! ## 锁的是什么
//!
//! mDNS 广播（`MdnsAdvertiser` + `mdns_start_advertise` / `mdns_stop_advertise`
//! 命令）在移动宿主侧是**零活消费者死面**（App.vue 全注释，2026-10-10 前已无
//! 生产调用）；引擎广播能力由共享守护 `bedcode-discovery-engine::engine` 承载
//! （`shared_daemon` / `register_host_service` / `stop_host_service`），移动端
//! `MdnsPort` 已直接复用（`src/peer/ports.rs` 适配层）。谁把这些符号加回移动
//! 宿主，谁就再造了第二份广播实现与第二份死面——本锁直接测红。
//!
//! ## 两面判据
//!
//! 1. **退役面缺席**（Rust + 前端扫非注释行）：`MdnsAdvertiser` / `advertiser` /
//!    广播命令字面量 / `AdvertiseConfig` / 自建 `ServiceDaemon::new` 归零；
//! 2. **browse 薄壳在场**（防「删 browse 绕过锁」）：宿主浏览面 `MdnsDiscovery`
//!    与其三条命令必须仍在（browse 有活消费者：terminal-session 宿主页经
//!    `mobileApi` 消费）；共享守护复用锚点在 `src/peer/ports.rs` 在场。

use std::path::{Path, PathBuf};

fn src_tauri_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn mobile_root() -> PathBuf {
    src_tauri_root()
        .parent()
        .expect("src-tauri 上必有项目根")
        .to_path_buf()
}

/// 退役面扫描范围（Rust 宿主 + 前端 src/；wasm-core 的 `bare/host_impl/mdns.rs`
/// 是薄转发面不在扫描内——移动独有域 adapter 在 `src/plugin/`，同扫）
const SCAN_FILES: &[&str] = &[
    "src/mdns.rs",
    "src/mdns/discovery.rs",
    "src/mdns/types.rs",
    "src/commands/mdns.rs",
    "src/lib.rs",
    "src/plugin/peer_ports.rs",
    // 前端：广播命令字面量归零（死 composable 已删，回接即红）
    "../src/App.vue",
];

/// 退役面：出现即回接（广播能力真源 = discovery-engine 共享守护）
const RETIRED_NEEDLES: &[&str] = &[
    "MdnsAdvertiser",
    "advertiser",
    "mdns_start_advertise",
    "mdns_stop_advertise",
    "AdvertiseConfig",
    // 自建广播守护实例（共享守护复用应经 discovery-engine 引擎函数，不 new 第二份）
    "ServiceDaemon::new",
];

/// browse 薄壳必须在场（活消费者；避免删 browse 后只剩共享守护、宿主浏览面消失）
const KEPT_BROWSE_NEEDLES: &[&str] = &[
    "struct MdnsDiscovery",
    "mdns_start_discovery",
    "mdns_stop_discovery",
    "mdns_get_discovered_services",
];

/// 共享守护复用锚点（移动 MdnsPort 经 `src/peer/ports.rs` 接 discovery-engine 三函数）
const SHARED_DAEMON_ANCHOR: &str = "bedcode_discovery_engine::engine";

/// 逐行扫描（跳过纯注释行），返回命中记录
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

/// 退役面缺席：广播符号 / 命令字面量 / 自建守护不得回到移动宿主与前端
#[test]
fn mobile_mdns_advertise_face_is_retired() {
    let root = src_tauri_root();
    let mut violations: Vec<String> = Vec::new();
    for rel in SCAN_FILES {
        let path = root.join(rel);
        if !path.is_file() {
            continue; // 死文件已删（advertiser.rs / useMdnsAdvertiser.ts）——缺席即合规
        }
        violations.extend(scan_source_lines(&path, RETIRED_NEEDLES));
    }
    assert!(
        violations.is_empty(),
        "移动宿主/前端重新引入 mDNS 广播面（广播能力真源 = \
         bedcode-discovery-engine 共享守护，宿主零第二份实现）：\n{}",
        violations.join("\n")
    );
}

/// browse 薄壳在场 + 共享守护复用锚点在适配层在场
#[test]
fn mobile_mdns_browse_shell_stays_and_daemon_is_shared() {
    let root = src_tauri_root();

    let browse = std::fs::read_to_string(root.join("src/mdns/discovery.rs"))
        .unwrap_or_else(|e| panic!("读取 discovery.rs 失败：{e}"));
    let commands = std::fs::read_to_string(root.join("src/commands/mdns.rs"))
        .unwrap_or_else(|e| panic!("读取 commands/mdns.rs 失败：{e}"));
    for needle in KEPT_BROWSE_NEEDLES {
        let hay = if needle.contains("mdns_start_discovery") || needle.contains("mdns_stop_discovery") || needle.contains("mdns_get_discovered_services") {
            &commands
        } else {
            &browse
        };
        assert!(
            hay.contains(needle),
            "browse 薄壳缺失：找不到 `{needle}`（浏览面有活消费者，不得随广播面一起退役）"
        );
    }

    let peer_ports = std::fs::read_to_string(root.join("src/peer/ports.rs"))
        .unwrap_or_else(|e| panic!("读取 src/peer/ports.rs 失败：{e}"));
    assert!(
        peer_ports.contains(SHARED_DAEMON_ANCHOR),
        "移动 MdnsPort 未复用共享守护（`{SHARED_DAEMON_ANCHOR}`）：广播/发现能力必须\
         经 discovery-engine 引擎三函数接线，宿主不得自持 daemon"
    );

    let _ = mobile_root(); // 保留路径锚点，避免未来扫描范围扩展时遗漏前端根
}
