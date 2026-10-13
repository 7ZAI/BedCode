//! 契约结构锁（ADR 0052 · domain-owned-contracts 票 05）：core 机制纯净 / 域契约组合 /
//! 能力域端中立——三把静态断言，改坏即红。
//!
//! 背景：ADR 0052 把契约面重划为三层——core 只收机制必需（8 import + 4 export）、
//! 双端功能级交集住能力域 crate 的 `wit/<domain>.wit`（由端清单 `compose.json` 的 caps
//! 直接消费）、端独有功能住端 `wit-src/cap-*.wit`。这三条边界若只靠注释约定，任何一次
//! 「顺手放回 core / 塞进能力域 crate」都能全绿通过——本文件把它们变成可执行判据。
//!
//! ## 判据
//!
//! - [`core_wit_stays_mechanism_only`]：`core.wit` 的 interface 定义集 == 机制白名单
//!   （storage / plugin-database / log / config / events / bus / fs / platform +
//!   command / lifecycle / manifest / events-binary），`world core` 的 import/export
//!   集逐项钉死；业务能力 interface 回流即红并点名。
//! - [`compose_caps_map_to_the_right_contract_houses`]：双端 caps 值逐条对照「域契约
//!   真源 = 能力域 crate 的 wit 文件；端扩展 = 端点 wit-src」；移动 caps 不含任何端扩展 /
//!   桌面独有键（组合了无人实现的接口 ⇒ 移动实例化必炸）。
//! - [`capability_domains_stay_end_neutral`]：能力域 crate 的默认形态零端 WIT / 零端
//!   专属 impl——所有 `bindgen!` 与 `impl bedcode::plugin::host_*::Host` 必须在
//!   `#[cfg(feature = "common-host")]` 之后；域分片 interface 只能来自共享白名单；
//!   生产代码不得出现端扩展 interface 词汇。
//!
//! 与既有锁的分工：`plugin/bindings.rs` 的 cfg(test) 锁管「组合面 ↔ 装配面」双向覆盖
//! （模块自报）；`wit_package_version_lock.rs` 管版本单点化；`capability_crates_no_product_ids.rs`
//! 管产品身份词汇；本文件管**契约归属边界**本身。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("{rel} 不可读: {e}"))
}

/// 源码里的非注释行（`//` 开头整行跳过——模块文档正当地提到历史与落点）
fn code_lines(src: &str) -> impl Iterator<Item = (usize, &str)> {
    src.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim_start().starts_with("//"))
}

/// 收集 `interface <name> {` 定义名
fn interface_defs(src: &str) -> BTreeSet<String> {
    src.lines()
        .filter_map(|l| {
            let t = l.trim();
            let rest = t.strip_prefix("interface ")?;
            rest.split_whitespace().next().map(str::to_string)
        })
        .collect()
}

/// 收集 `world <name> { … }` 体内的成员名（`import <x>;` / `export <x>;`）
fn world_members(src: &str, world: &str, kind: &str) -> BTreeSet<String> {
    let marker = format!("world {world} {{");
    let pos = src.find(&marker).unwrap_or_else(|| panic!("{world} 未在源文件声明"));
    let body = &src[pos + marker.len()..];
    let end = body.find('}').unwrap_or(body.len());
    body[..end]
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            t.strip_prefix(&format!("{kind} "))
                .and_then(|rest| rest.strip_suffix(';'))
                .map(|name| name.trim().to_string())
        })
        .collect()
}

// ==================== ① core 机制纯净锁 ====================

/// `core.wit` 的机制 interface 白名单（ADR 0052 D1）：8 import + 4 export
const CORE_IMPORTS: &[&str] = &[
    "host-storage",
    "host-plugin-database",
    "host-log",
    "host-config",
    "host-events",
    "host-bus",
    "host-fs",
    "host-platform",
];
/// export 白名单（`events-binary` 是可选导出，定义在文件内但不在 `world core`）
const CORE_EXPORTS: &[&str] = &["command", "lifecycle", "manifest", "events-binary"];

#[test]
fn core_wit_stays_mechanism_only() {
    let src = read("packages/bedcode-wasm-core/wit/core.wit");

    // ① interface 定义集 == 白名单（业务能力 interface 回流即点名）
    let expected: BTreeSet<String> = CORE_IMPORTS.iter().chain(CORE_EXPORTS).map(|s| s.to_string()).collect();
    let actual = interface_defs(&src);
    let unexpected: Vec<&String> = actual.difference(&expected).collect();
    assert!(
        unexpected.is_empty(),
        "core.wit 出现机制白名单之外的 interface {unexpected:?}——业务能力契约应住 \
         能力域 crate 的 wit/<domain>.wit 或端 cap（ADR 0052 D1/D2）"
    );
    let missing: Vec<&String> = expected.difference(&actual).collect();
    assert!(
        missing.is_empty(),
        "core.wit 缺机制 interface {missing:?}（白名单是 8 import + 4 export，ADR 0052 D1）"
    );

    // ② world core 成员逐项钉死：
    //    import = 8 机制域；export = 3 必选（events-binary 不在 world 内，可选导出）
    let imports = world_members(&src, "core", "import");
    let expected_imports: BTreeSet<String> = CORE_IMPORTS.iter().map(|s| s.to_string()).collect();
    assert_eq!(imports, expected_imports, "world core 的 import 集漂移（机制白名单）");
    let exports = world_members(&src, "core", "export");
    let expected_exports: BTreeSet<String> = ["command", "lifecycle", "manifest"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        exports, expected_exports,
        "world core 的 export 集漂移：events-binary 是可选导出（宿主按名探测），不进 world"
    );
}

// ==================== ② 域契约组合锁 ====================

/// 域契约（双端功能级交集）→ 能力域 crate 的真源文件
const DOMAIN_CONTRACT_HOUSES: &[(&str, &str)] = &[
    ("pty", "packages/bedcode-pty-engine/wit/pty.wit"),
    ("mdns", "packages/bedcode-discovery-engine/wit/mdns.wit"),
    ("peer", "packages/bedcode-server-peer-net/wit/peer.wit"),
    ("http", "packages/bedcode-server-http/wit/http.wit"),
    ("ws", "packages/bedcode-ws-client-engine/wit/ws.wit"),
    ("notify", "packages/bedcode-notify-engine/wit/notify.wit"),
];

/// 桌面端扩展（声明与实现都在端宿主）→ 端 wit-src
const DESKTOP_END_CAPS: &[(&str, &str)] = &[
    (
        "http-endpoint",
        "bedcode-desktop/packages/plugin-sdk-desktop/rust/wit-src/cap-http.wit",
    ),
    (
        "ws-server",
        "bedcode-desktop/packages/plugin-sdk-desktop/rust/wit-src/cap-ws.wit",
    ),
    (
        "desktop",
        "bedcode-desktop/packages/plugin-sdk-desktop/rust/wit-src/cap-desktop.wit",
    ),
];

/// 移动端扩展 → 端 wit-src
const MOBILE_END_CAPS: &[(&str, &str)] = &[(
    "mobile",
    "bedcode-mobile/packages/plugin-sdk-mobile/rust/wit-src/cap-mobile.wit",
)];

fn caps_of(manifest_rel: &str) -> std::collections::BTreeMap<String, String> {
    let raw = read(manifest_rel);
    let m: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{manifest_rel} JSON 不可解析: {e}"));
    m["caps"]
        .as_object()
        .unwrap_or_else(|| panic!("{manifest_rel} 缺 caps 对象"))
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                v.as_str()
                    .unwrap_or_else(|| panic!("{manifest_rel} caps.{k} 非字符串"))
                    .to_string(),
            )
        })
        .collect()
}

#[test]
fn compose_caps_map_to_the_right_contract_houses() {
    // 域契约（交集）双端都消费同一份真源；pty 桌面独有（移动无 PTY 插件面）
    let desktop = caps_of("bedcode-desktop/packages/plugin-sdk-desktop/compose.json");
    let mobile = caps_of("bedcode-mobile/packages/plugin-sdk-mobile/compose.json");

    for (key, path) in DOMAIN_CONTRACT_HOUSES {
        assert_eq!(
            desktop.get(*key).map(String::as_str),
            Some(*path),
            "桌面 caps.{key} 必须指向能力域 crate 的契约分片（ADR 0052 D2，pty 先例）"
        );
        if *key != "pty" {
            assert_eq!(
                mobile.get(*key).map(String::as_str),
                Some(*path),
                "移动 caps.{key} 必须与桌面消费同一份域契约真源（ADR 0052 D2/D3）"
            );
            assert!(
                path.starts_with("packages/bedcode-"),
                "域契约真源必须落能力域 crate（{path}），不得挂回 core / 端 wit-src"
            );
        }
    }

    // 端扩展只准出现在自己的端：桌面扩展键集逐项对照；移动侧不得出现任何端扩展 /
    // 桌面独有键（组合进移动 caps 的接口无人实现 ⇒ 实例化即炸，spec §4.3 明文）
    for (key, path) in DESKTOP_END_CAPS {
        assert_eq!(
            desktop.get(*key).map(String::as_str),
            Some(*path),
            "桌面 caps.{key} 必须指向端点 wit-src（ADR 0052 D4）"
        );
        assert!(
            !mobile.contains_key(*key),
            "移动 caps 不得含桌面端扩展键 `{key}`（端差异归宿主，ADR 0052 D4）"
        );
    }
    for (key, path) in MOBILE_END_CAPS {
        assert_eq!(
            mobile.get(*key).map(String::as_str),
            Some(*path),
            "移动 caps.{key} 必须指向移动端 wit-src"
        );
        assert!(!desktop.contains_key(*key), "桌面 caps 不得含移动端扩展键 `{key}`");
    }
    // `pty`：host-pty 是桌面独有插件面（移动无 PTY 能力）——不得进移动组合
    assert!(
        !mobile.contains_key("pty"),
        "移动 caps 不得含 `pty`（host-pty 是桌面能力域，移动不跟演）"
    );

    // 每个 cap 真源必须声明自己的 `world cap-<key>`（拼装脚本按 key 拼接 include）
    for (end, caps) in [("desktop", &desktop), ("mobile", &mobile)] {
        for (key, path) in caps {
            let src = read(path);
            assert!(
                src.lines()
                    .any(|l| l.trim_start().starts_with(&format!("world cap-{key}"))),
                "[{end}] caps.{key} 真源 {path} 未声明 `world cap-{key}`（组合名与 world 名必须闭合）"
            );
        }
    }
}

// ==================== ③ 能力域端中立锁 ====================

/// 能力域共享契约 interface 白名单：域分片只准定义这些（端扩展名一律违规）
const SHARED_DOMAIN_INTERFACES: &[&str] = &[
    "host-pty",
    "host-mdns",
    "host-peer",
    "host-http",
    "host-websocket",
    "host-notify",
];

/// 端扩展 / 端独有 interface 的 Rust 模块路径词（域 crate 生产代码不得出现）
const END_EXTENSION_TOKENS: &[&str] = &[
    "host_http_endpoint",
    "host_websocket_server",
    "host_fs_desktop",
    "host_fs_mobile",
    "host_notify_mobile",
    "host_events_desktop",
    "host_platform_desktop",
    "host_terminal_stream",
    "host_auth_center",
    "host_auth_client",
    "host_connection_target",
    "host_connection_registry",
    "host_api_call",
];

/// 默认形态零 WIT 的域 crate：绑定层必须全部 bit `common-host` 门控
const COMMON_HOST_CRATES: &[&str] = &[
    "bedcode-pty-engine",
    "bedcode-server-http",
    "bedcode-server-websocket",
    "bedcode-server-peer-net",
    "bedcode-discovery-engine",
];

/// 另两个能力域 crate：无 `common-host` 绑定层（ws-client-engine 是纯引擎；
/// notify-engine 是轻量契约房）——仍需域分片白名单扫描
const WIT_ONLY_DOMAIN_CRATES: &[&str] = &["bedcode-ws-client-engine", "bedcode-notify-engine"];

/// 递归收集 `*.rs`
fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("读目录 {} 失败: {e}", dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn capability_domains_stay_end_neutral() {
    let packages = repo_root().join("packages");

    for crate_name in COMMON_HOST_CRATES {
        let crate_dir = packages.join(crate_name);
        // (a) 默认形态零 WIT：绑定层依赖 + 代码必须门控在 common-host
        let cargo = std::fs::read_to_string(crate_dir.join("Cargo.toml"))
            .unwrap_or_else(|e| panic!("{crate_name}/Cargo.toml 不可读: {e}"));
        assert!(
            cargo.contains("common-host = ["),
            "{crate_name} 缺 common-host feature——绑定层必须先 feature 门控（能力域脱绑）"
        );

        let mut files = Vec::new();
        collect_rs(&crate_dir.join("src"), &mut files);
        for file in &files {
            let src = std::fs::read_to_string(file).unwrap_or_default();
            let lines: Vec<&str> = src.lines().collect();
            for (idx, raw) in lines.iter().enumerate() {
                if raw.trim_start().starts_with("//") {
                    continue;
                }
                let is_binding_item = raw.contains("bindgen!(") || raw.contains("impl bedcode::plugin::host_");
                if !is_binding_item {
                    continue;
                }
                let gated = idx > 0 && lines[idx - 1].trim() == "#[cfg(feature = \"common-host\")]";
                assert!(
                    gated,
                    "{}:{} `{}` 未被 `#[cfg(feature = \"common-host\")]` 门控——\
                     能力域默认形态 = 纯引擎 + 端口抽象（零 WIT 绑定），ADR 0035 D6 / ADR 0052 D4",
                    file.display(),
                    idx + 1,
                    raw.trim()
                );
            }
            // (b) 生产代码零端扩展词汇（端 impl 归宿主，T-03 结论）
            for (idx, raw) in code_lines(&src) {
                for token in END_EXTENSION_TOKENS {
                    assert!(
                        !raw.contains(token),
                        "{}:{} 出现端扩展词汇 `{token}`——端独有 interface 的 impl/绑定归端宿主，\
                         不得回流能力域 crate（ADR 0052 D4 / T-03 逐项盘点）",
                        file.display(),
                        idx + 1
                    );
                }
            }
        }

        // (c) 域分片 interface 只能来自共享白名单（端扩展 WIT 不得住能力域 crate）
        assert_domain_wit_is_shared(crate_name, &crate_dir);
    }

    // 无 common-host 绑定层的两个域 crate 只做域分片白名单扫描
    for crate_name in WIT_ONLY_DOMAIN_CRATES {
        assert_domain_wit_is_shared(crate_name, &packages.join(crate_name));
    }
}

/// (c) `wit/` 目录的分片 interface 全部来自共享白名单（端扩展 WIT 不得住能力域 crate）
fn assert_domain_wit_is_shared(crate_name: &str, crate_dir: &Path) {
    let wit_dir = crate_dir.join("wit");
    if !wit_dir.exists() {
        return;
    }
    let mut wit_files = Vec::new();
    for entry in std::fs::read_dir(&wit_dir).expect("读 wit 目录").flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("wit") {
            wit_files.push(path);
        }
    }
    assert!(
        !wit_files.is_empty(),
        "{crate_name}/wit 存在但无 .wit 文件（盘面漂移？）"
    );
    for path in wit_files {
        let src = std::fs::read_to_string(&path).unwrap_or_default();
        for iface in interface_defs(&src) {
            assert!(
                SHARED_DOMAIN_INTERFACES.contains(&iface.as_str()),
                "{} 定义了端扩展 / 白名单外 interface `{iface}`——域分片只持双端功能级交集\
                 （{SHARED_DOMAIN_INTERFACES:?}），端独有 WIT 住端 wit-src（ADR 0052 D2/D4）",
                path.display()
            );
        }
    }
}
