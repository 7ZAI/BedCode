//! 移动端 WIT 分片拼装组合锁（票 2026-10-09 wasm-core-single-crate 票 07 批次 02）
//!
//! 背景：端 `rust/wit/` 自票 03 起是**生成物**（`bedcode.wit` = package + `world plugin
//! { include core; include cap-…; }`，分片由端清单 `compose.json` 驱动拼装）。随移动
//! fork crate 退役（票 06 批次 04），票 19 时代的 A1「WIT 接口清单 + ABI」契约锁
//! （原 fork crate `tests/sdk_wit_contract_locks.rs`）随目录一并退役——本锁以最小形态
//! 承接其**结构/计数面**职责（语义面 = `stale_artifact_rebuild_hint` 旧产物点名 +
//! 双端 ABI 锁步流程 ADR 0019）：
//!
//! 1. 端清单 `compose.json` 的 `abi.version` == SDK `abi.rs` 的 `ABI_VERSION`
//!    （「组合了什么」与 SDK 对外承诺的计数只有一个答案；bump 必须两处同步）；
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

/// ① 端清单 abi.version == SDK abi.rs ABI_VERSION
#[test]
fn compose_manifest_abi_matches_sdk_abi_version() {
    let compose = read(&sdk_root().join("compose.json"));
    let m: serde_json::Value = serde_json::from_str(&compose).expect("端清单 JSON 可解析");
    let declared = m["abi"]["version"].as_u64().expect("compose.json 缺 abi.version");

    let abi_src = read(&sdk_root().join("rust/src/abi.rs"));
    let actual = abi_src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .find_map(|l| {
            let t = l.trim();
            t.strip_prefix("pub const ABI_VERSION: u32 =")
                .and_then(|rest| rest.trim().trim_end_matches(';').parse::<u64>().ok())
        })
        .expect("abi.rs 缺 ABI_VERSION 常量（形态漂移？）");

    assert_eq!(
        declared, actual,
        "端清单 abi.version({declared}) 与 SDK abi.rs ABI_VERSION({actual}) 漂移——\
         「组合了什么」与 SDK 对外承诺必须同源（bump 走 ADR 0019 双端锁步流程）"
    );
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
