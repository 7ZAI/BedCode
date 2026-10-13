//! WIT 包版本单点化锁（ADR 0051 D3/D4 · `wit-package-semver` 票 02）
//!
//! 契约版本的唯一手写处 = 仓根 `contracts/bedcode-plugin-package.rs`（Rust 常量，
//! `PACKAGE_NAME` + `PACKAGE_VERSION`；票 03 由 JSON 迁为 Rust 常量——能力域 crate 的
//! `inventory` 静态自报要求编译期常量，JSON 只能运行期解析）。
//! 双端 `compose.json` 的 `package` 字段只声明**包名**（不持版本字面量——否则
//! bump 要改三处且清单退化成第二真源）；端生成物 `bedcode.wit` 的 package 行由
//! `scripts/compose-wit.mjs` 按真源拼装；WIT 源文件（10 个）自带版本字面量并被
//! 逐字拷贝进生成物。
//!
//! ## 为什么 `--check` 的 sha256 漂移锁抓不到，要另立本锁
//!
//! 漂移锁只比「生成物 vs 真源现算」：源文件手写的版本若与仓根常量不一致，重拼后
//! 一切自洽、漂移锁 9/9 全绿，但双端 import 名会带上与常量不同的版本——**编译面全绿、
//! 运行期实例化才炸**。版本一致性是唯一能挡住这个方向的门禁。
//!
//! ## 判据
//!
//! 1. **真源形状**：`contracts/bedcode-plugin-package.rs` 在场，`PACKAGE_NAME` 为
//!    `bedcode:<ns>` 形态、`PACKAGE_VERSION` 为三段 semver（防把版本写成 `0.1` /
//!    `v0.1.0` / 空串这类 linker 不认的形态）、`PACKAGE` 与前两者自洽；并断言**能力域
//!    crate 的 const 导出名清单**（`routing.rs` 的 `EXPORTS`，inventory 静态自报要求
//!    编译期常量、字面量无法跨文件派生）与真源逐字一致。
//! 2. **端清单只声明包名**：双端 `compose.json` 的 `package` == 真源 `name` 且**不含
//!    `@`**（版本回写清单 = 第二真源）。
//! 3. **10 个源文件 package 行 == 真源整行**（`package bedcode:plugin@<version>;`）。
//! 4. **生成物 package 行 == 真源整行**：清单驱动派生（`out` + `core` + 各
//!    `cap-<key>`），新增分片无需改锁；漏重拼即红。
//!
//! 共享产物场景（file-transfer 双端单产物，ADR 0050）另由
//! `bedcode-file-transfer-core/src/wiring_lock.rs` 断言双端版本对齐——本锁管
//! 「单点一致性」，那把锁管「共享产物跨端一致性」，两者互补。

use std::fs;
use std::path::{Path, PathBuf};

/// 仓根（`bedcode-desktop/src-tauri` → `../..`）
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 仓根 WIT 包版本真源（ADR 0051 D3）——Rust 常量文件，`const` 上下文可用
const PACKAGE_SOURCE: &str = "contracts/bedcode-plugin-package.rs";

/// 能力域 crate 里必须与真源逐字一致的 const 导出名清单（inventory 静态自报，
/// 字面量无法经 `contract::export_item` 派生，故由本锁钉住）
const DOMAIN_EXPORT_LITERALS: &[&str] = &["packages/bedcode-discovery-engine/src/routing.rs"];

/// 生产 WIT 源文件（10 个）——`package` 行必须与真源逐字一致
///
/// 构成：共享核心真源 1（`core_binding.rs` 直接绑它，不能改绑生成物）+ 能力域自持分片
/// 5（provider 侧 bindgen，注册名须与 guest import 名逐字一致）+ 端 cap-src 4
/// （compose 逐字拷贝进生成物）。能力域分片里的 pty / http / ws 会被双端 compose 拷贝，
/// peer / mdns 只由能力 crate 自绑——两类都要在本锁在场，否则 provider 侧与 guest 侧
/// 版本漂移无人拦。
const WIT_SOURCES: &[&str] = &[
    "packages/bedcode-wasm-core/wit/core.wit",
    "packages/bedcode-pty-engine/wit/pty.wit",
    "packages/bedcode-server-http/wit/http.wit",
    "packages/bedcode-server-websocket/wit/ws.wit",
    "packages/bedcode-server-peer-net/wit/peer.wit",
    "packages/bedcode-discovery-engine/wit/mdns.wit",
    "bedcode-desktop/packages/plugin-sdk-desktop/rust/wit-src/cap-http.wit",
    "bedcode-desktop/packages/plugin-sdk-desktop/rust/wit-src/cap-ws.wit",
    "bedcode-desktop/packages/plugin-sdk-desktop/rust/wit-src/cap-desktop.wit",
    "bedcode-mobile/packages/plugin-sdk-mobile/rust/wit-src/cap-mobile.wit",
];

/// 双端端清单（`package` 只声明包名）
const END_MANIFESTS: &[&str] = &[
    "bedcode-desktop/packages/plugin-sdk-desktop/compose.json",
    "bedcode-mobile/packages/plugin-sdk-mobile/compose.json",
];

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{} 不可读: {e}", path.display()))
}

/// 取文件里首个 `package …;` 声明行（trim 后整行）
///
/// 判据用**整行**比对而非只抽版本段：缺版本、版本段写法异常、包名漂移三种形态
/// 一次拦住，避免「只比版本」时漏掉 `package bedcode:plugin;` 这种未版本化形态。
fn package_line(path: &Path) -> String {
    read(path)
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("package "))
        .unwrap_or_else(|| panic!("{} 缺 package 声明行", path.display()))
        .to_string()
}

/// 读真源并做形状校验（与拼装脚本 `readPackage()` 同判据：fail-visible，不猜默认值）
///
/// 真源是 Rust 常量文件（不是 JSON）：能力域的 `inventory` 静态自报要求编译期常量，
/// JSON 只能运行期解析。抽取只认 `pub const <NAME>: &str = "<值>";` 形态。
fn package_source() -> (String, String, String) {
    let path = repo_root().join(PACKAGE_SOURCE);
    let raw = read(&path);
    let const_of = |name: &str| -> String {
        let needle = format!("pub const {name}: &str = \"");
        let start = raw
            .find(&needle)
            .unwrap_or_else(|| panic!("{} 缺常量 {name}", path.display()))
            + needle.len();
        let rest = &raw[start..];
        let end = rest
            .find('"')
            .unwrap_or_else(|| panic!("{} 常量 {name} 未闭合", path.display()));
        rest[..end].to_string()
    };
    let name = const_of("PACKAGE_NAME");
    let version = const_of("PACKAGE_VERSION");
    let full = const_of("PACKAGE");
    assert!(
        name.contains(':'),
        "{} 的 PACKAGE_NAME 应为 `bedcode:<ns>` 形态，实际 {name}",
        path.display()
    );
    let parts: Vec<&str> = version.split('.').collect();
    assert!(
        parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "{} 的 PACKAGE_VERSION 应为三段 semver（0.1.0 形态），实际 {version}",
        path.display()
    );
    assert_eq!(
        full,
        format!("{name}@{version}"),
        "{} 的 PACKAGE 与 NAME/VERSION 不自洽",
        path.display()
    );
    (name, version, full)
}

/// 真源拼出的期望 package 行
fn expected_package_line() -> String {
    let (name, version, _) = package_source();
    format!("package {name}@{version};")
}

/// ① 真源形状：name / version 字段在场且写法合法（防空转：字段缺失时后续判据会假绿）
#[test]
fn package_source_is_wellformed() {
    let (name, version, _) = package_source();
    assert_eq!(
        name, "bedcode:plugin",
        "包名真源应保持 bedcode:plugin（双端 world / world plugin 引用它）"
    );
    assert!(
        !version.is_empty(),
        "版本不得为空——空版本会让 import 名退回无版本形态而漂移锁全绿"
    );
}

/// ② 端清单只声明包名，不持版本字面量
#[test]
fn compose_manifests_declare_package_name_only() {
    let (name, _, _) = package_source();
    for rel in END_MANIFESTS {
        let path = repo_root().join(rel);
        let m: serde_json::Value = serde_json::from_str(&read(&path))
            .unwrap_or_else(|e| panic!("{} JSON 不可解析: {e}", path.display()));
        let declared = m["package"]
            .as_str()
            .unwrap_or_else(|| panic!("{} 缺 package 字段", path.display()));
        assert_eq!(
            declared, name,
            "{} 的 package 应等于真源 name（版本由 scripts/compose-wit.mjs 注入）",
            path.display()
        );
        assert!(
            !declared.contains('@'),
            "{} 的 package 不应持版本字面量 `{declared}`——版本真源单点化在 {PACKAGE_SOURCE}，\
             版本回写端清单会退化成第二真源",
            path.display()
        );
    }
}

/// ③ 10 个 WIT 源文件的 package 行 == 真源整行
#[test]
fn wit_source_package_lines_match_source_of_truth() {
    assert!(
        !WIT_SOURCES.is_empty(),
        "源文件清单为空会让本判据空转"
    );
    let expected = expected_package_line();
    for rel in WIT_SOURCES {
        let path = repo_root().join(rel);
        assert_eq!(
            package_line(&path),
            expected,
            "{} 的 package 行与仓根 {} 漂移——版本化后 import 名逐字匹配，\
             源文件版本漂移＝运行期实例化失败（编译面不会红）",
            path.display(),
            PACKAGE_SOURCE
        );
    }
}

/// ④ 生成物 package 行 == 真源整行（清单驱动派生；漏重拼 / 手改生成物即红）
#[test]
fn generated_package_lines_match_source_of_truth() {
    let expected = expected_package_line();
    let mut count = 0usize;
    for rel in END_MANIFESTS {
        let manifest_path = repo_root().join(rel);
        let m: serde_json::Value = serde_json::from_str(&read(&manifest_path))
            .unwrap_or_else(|e| panic!("{} JSON 不可解析: {e}", manifest_path.display()));
        let out = m["out"]
            .as_str()
            .unwrap_or_else(|| panic!("{} 缺 out 字段", manifest_path.display()));
        let caps = m["caps"]
            .as_object()
            .unwrap_or_else(|| panic!("{} 缺 caps 对象", manifest_path.display()));

        let mut files = vec![format!("{out}/bedcode.wit"), format!("{out}/core.wit")];
        for key in caps.keys() {
            files.push(format!("{out}/cap-{key}.wit"));
        }
        for rel_file in files {
            let path = repo_root().join(&rel_file);
            assert_eq!(
                package_line(&path),
                expected,
                "生成物 {} 的 package 行与真源漂移——重跑 `node scripts/compose-wit.mjs --all`",
                path.display()
            );
            count += 1;
        }
    }
    assert!(
        count >= 9,
        "生成物只遍历到 {count} 个（桌面 6 + 移动 3 = 9），清单字段读取可能已漂移"
    );
}

/// ⑤ 能力域 crate 的 const 导出名清单 == 真源派生的期望值
///
/// 这一面**无法**经 `contract::export_item` 派生：`inventory` 静态自报
/// （`static ROUTE` + `submit_routable_capability!`）要求编译期常量，而字面量不能跨
/// 文件派生（Rust 无跨文件字面量导入）。故它是「散落副本」，由本锁钉住——与 10 个 WIT
/// 源文件各写 package 行同款：真源单点 + 锁兜底。
#[test]
fn domain_const_export_literals_match_source_of_truth() {
    let (name, version, _) = package_source();
    let expected_prefix = format!("{name}/host-mdns.");
    let expected_suffix = format!("@{version}");
    for rel in DOMAIN_EXPORT_LITERALS {
        let path = repo_root().join(rel);
        let literals: Vec<String> = read(&path)
            .lines()
            .filter_map(|line| {
                let t = line.trim().trim_end_matches(',');
                let t = t.strip_prefix('"')?.strip_suffix('"')?;
                t.starts_with(&expected_prefix).then(|| t.to_string())
            })
            .collect();
        assert!(
            !literals.is_empty(),
            "{} 未抽到任何 `{expected_prefix}*` 导出名字面量（锁失效即红）",
            path.display()
        );
        for lit in literals {
            assert!(
                lit.ends_with(&expected_suffix),
                "{} 的导出名 `{lit}` 缺版本后缀 `{expected_suffix}`——无版本写法对版本化\
                 产物恒不命中（组件导出实例键 = `ns:pkg/iface@version`）",
                path.display()
            );
        }
    }
}
