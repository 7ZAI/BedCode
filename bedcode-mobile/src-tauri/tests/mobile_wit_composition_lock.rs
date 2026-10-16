//! 移动端 WIT 分片拼装组合锁（票 2026-10-09 wasm-core-single-crate 票 07 批次 02）
//!
//! 背景：端 `rust/wit/` 自票 03 起是**生成物**（`bedcode.wit` = package + `world plugin
//! { include core; include cap-…; }`，分片由端清单 `compose.json` 驱动拼装）。随移动
//! fork crate 退役（票 06 批次 04），票 19 时代的 A1「WIT 接口清单 + ABI」契约锁
//! （原 fork crate `tests/sdk_wit_contract_locks.rs`）随目录一并退役——本锁以最小形态
//! 承接其**结构/计数面**职责（语义面 = `stale_artifact_rebuild_hint` 旧产物点名 +
//! 双端 ABI 锁步流程 ADR 0019）：
//!
//! 1. 端清单 `compose.json` 的 package 版本 == 移动侧 WIT 源文件 package 行
//!    （票 05 替代原 abi.version 双查；版本真源 = 仓根 `contracts/bedcode-plugin-package.rs`，
//!    票 03 单点化——「组合了什么」的版本只有一个答案；`abi.version` 随票 04 退役，
//!    本锁不读它）；
//! 2. 生成物 `bedcode.wit` 的 `world plugin` include 集 == `core` + 各 caps 键
//!    （拼装脚本产物与端清单一致；手改生成物另有 `scripts/compose-wit.mjs --check`
//!    的逐字漂移锁，本锁管**结构**，两者互补）；
//! 3. `compose.json` 的 `worlds` 列表在生成物目录中确实有声明（世界名漂移即红）。
//!
//! 只读源文件，不依赖 app crate；判据按代码行/结构比对（跳注释行）。

use std::path::{Path, PathBuf};

/// `bedcode-mobile/src-tauri`
fn mobile_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 移动 SDK 根（`bedcode-mobile/packages/plugin-sdk-mobile`）
fn sdk_root() -> PathBuf {
    mobile_root().join("../packages/plugin-sdk-mobile")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{} 不可读: {e}", path.display()))
}

/// ① 端清单 package 版本 == 源文件 package 行（票 05 替代原 abi.version 双查）
///
/// 版本真源 = 仓根 `contracts/bedcode-plugin-package.rs`（ADR 0051 D3 单点化，票 03
/// 迁为 Rust 常量：能力域 `inventory` 静态自报要求编译期常量）：端清单 `package`
/// 只声明包名，版本由 `scripts/compose-wit.mjs` 注入；本判据钉住「移动端组合面的
/// package 版本 == 移动侧 WIT 源文件 package 行」——版本化后 import 名逐字匹配，
/// 源文件版本漂移＝运行期实例化失败（编译面不红）。`abi.version` 是票 04 的退役面，
/// 本判据不读它。
#[test]
fn compose_manifest_package_version_matches_wit_sources() {
    let compose = read(&sdk_root().join("compose.json"));
    let m: serde_json::Value = serde_json::from_str(&compose).expect("端清单 JSON 可解析");
    let declared = m["package"].as_str().expect("compose.json 缺 package");

    // 仓根版本真源（`bedcode-mobile/src-tauri` → `../..`）：Rust 常量文件，
    // 抽取只认 `pub const <NAME>: &str = "<值>";` 形态并断言 PACKAGE 自洽。
    let root = mobile_root().join("../..");
    let raw = read(&root.join("contracts/bedcode-plugin-package.rs"));
    let const_of = |name: &str| -> String {
        let needle = format!("pub const {name}: &str = \"");
        let start = raw.find(&needle).unwrap_or_else(|| {
            panic!("contracts/bedcode-plugin-package.rs 缺常量 {name}")
        }) + needle.len();
        let rest = &raw[start..];
        let end = rest.find('"').unwrap_or_else(|| {
            panic!("contracts/bedcode-plugin-package.rs 常量 {name} 未闭合")
        });
        rest[..end].to_string()
    };
    let name = const_of("PACKAGE_NAME");
    let version = const_of("PACKAGE_VERSION");
    assert_eq!(
        const_of("PACKAGE"),
        format!("{name}@{version}"),
        "contracts/bedcode-plugin-package.rs 的 PACKAGE 与 NAME@VERSION 自洽（ADR 0051 D3）"
    );
    assert_eq!(
        declared, name,
        "端清单 package 与仓根版本真源 PACKAGE_NAME 漂移（ADR 0051 D3 单点化）"
    );
    assert!(
        !declared.contains('@'),
        "端清单 package 不应持版本字面量 `{declared}`——版本真源单点化在 \
         contracts/bedcode-plugin-package.rs，清单回写版本=第二真源"
    );

    let expected = format!("package {name}@{version};");
    for rel in [
        "packages/bedcode-wasm-core/wit/core.wit",
        "bedcode-mobile/packages/plugin-sdk-mobile/rust/wit-src/cap-mobile.wit",
    ] {
        let path = root.join(rel);
        let line = read(&path)
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with("package "))
            .unwrap_or_else(|| panic!("{} 缺 package 声明行", path.display()))
            .to_string();
        assert_eq!(
            line, expected,
            "{rel} 的 package 行与仓根 contracts/bedcode-plugin-package.rs 漂移——版本化 \
             后 import 名逐字匹配，源文件版本漂移＝运行期实例化失败（编译面不红）"
        );
    }
}

/// ② 生成物 world plugin 的 include 集 == core + 各 caps 键
#[test]
fn generated_bedcode_wit_includes_match_compose_caps() {
    let compose = read(&sdk_root().join("compose.json"));
    let m: serde_json::Value = serde_json::from_str(&compose).expect("端清单 JSON 可解析");
    let mut expected: Vec<String> = vec!["core".to_string()];
    for key in m["caps"].as_object().expect("caps 是对象").keys() {
        expected.push(format!("cap-{key}"));
    }
    expected.sort();

    let gen = read(&sdk_root().join("rust/wit/bedcode.wit"));
    let mut includes: Vec<String> = gen
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .filter_map(|l| {
            let t = l.trim();
            t.strip_prefix("include ")
                .and_then(|rest| rest.strip_suffix(';'))
                .map(|name| name.trim().to_string())
        })
        .collect();
    includes.sort();

    assert_eq!(
        includes, expected,
        "生成物 bedcode.wit 的 include 集与端清单 caps 漂移——重跑 \
         `node scripts/compose-wit.mjs mobile`（真源改了没重拼？）"
    );
    // 各分片文件必须在场（只列 include 而分片缺失 = 解析期才炸）
    for name in &expected {
        let p = sdk_root().join(format!("rust/wit/{name}.wit"));
        assert!(p.exists(), "生成物分片缺失: {}", p.display());
    }
}

/// ③ compose.json 的 worlds 列表在生成物中确实有声明
#[test]
fn compose_worlds_are_declared_in_generated_wit() {
    let compose = read(&sdk_root().join("compose.json"));
    let m: serde_json::Value = serde_json::from_str(&compose).expect("端清单 JSON 可解析");
    let worlds: Vec<&str> = m["worlds"]
        .as_array()
        .expect("compose.json 缺 worlds")
        .iter()
        .map(|v| v.as_str().expect("worlds 元素为字符串"))
        .collect();
    assert!(!worlds.is_empty(), "compose.json worlds 为空（判据会空转）");

    // 生成物 = bedcode.wit + 各 cap 文件（世界声明可能在任一文件里）
    let wit_dir = sdk_root().join("rust/wit");
    let mut all = String::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&wit_dir)
        .unwrap_or_else(|e| panic!("{} 不可读: {e}", wit_dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("wit"))
        .collect();
    entries.sort();
    for p in &entries {
        all.push_str(&read(p));
        all.push('\n');
    }

    for world in worlds {
        let needle = format!("world {world} {{");
        assert!(
            all.contains(&needle),
            "端清单声明的 world `{world}` 在生成物目录中找不到 `{needle}`——\
             清单与拼装产物脱节（重拼或修清单）"
        );
    }
}
