//! 双端接线防漂移锁（本 crate 的**对外契约**：核是单点，端内只有适配器）。
//!
//! ## 为什么需要它
//!
//! 本重构的全部价值在「可共享面只有一份」。而它最可能的失效形态不是崩溃，而是**慢性回流**：
//! 某天有人在移动端 `transfer_store.rs` 里「顺手补一个判据」，双份实现就此复活，且没有任何
//! 测试会红——两份实现各自自洽时，所有单元测试都绿。
//!
//! 本锁把「端内不得再定义核内判据」变成可执行断言。四条判据：
//!
//! 1. **零回流**：核内 23 个纯逻辑函数名，在双端 `rust/src/**` 里**不得有同名 `fn` 定义**；
//! 2. **台账是纯转出**：双端 `transfer_store.rs` 必须含逐字相同的转出行；
//! 3. **两端都在场**：双端都必须有 `adapters.rs` 且实现核的**已消费端口**（实现即接线证据）；
//! 4. **端口面登记在册**：核 `ports.rs` 的 `pub trait` 集合与钉死清单精确相等——新增差异面
//!    必须回到本锁改清单（一次显式决定），而不是悄悄多一个没人实现的 trait。
//!
//! 扫描纪律（与宿主侧同款锁一致）：**剥行注释**（注释里点名函数解释「为什么」是正常的）、
//! 引号感知剥注释、needle 取不跨行的最短片段；测试子树（路径含 `tests` 段）跳过。
//!
//! 位置说明：本文件在 `src/`（由 `#[cfg(test)] mod wiring_lock;` 引入）而非 crate 根
//! `tests/`，理由同 [`crate::boundary_lock`]。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// 核内纯逻辑函数：双端**不得**再定义同名 `fn`
///
/// 注意**不包含**端内合法的包装名（`load` / `save_and_push` / `load_all` / `ensure_table` /
/// `load_snapshot` / `save_snapshot` / `apply_and_push`）——那些正是适配层的职责。
const CORE_ONLY_FNS: &[&str] = &[
    "fnv1a",
    "root_id",
    "upsert",
    "remove",
    "merge_snapshot",
    "terminal_status_of",
    "reduce_event",
    "insert_active_projections",
    "prune_absent",
    "reconcile_diff",
    "mark_active_interrupted",
    "evict_overflow",
    "clear_terminal",
    "active_send_entries",
    "active_receive_entries",
    "mark_cancelled",
    "mark_paused",
    "apply_retry",
    "retry_source",
    "send_slot_open",
    "push_pull_intent",
    "take_pull_intent",
    "entry_from_dto",
];

/// 双端 `transfer_store.rs` 必须逐字含本行（台账 = 纯转出）
const TRANSFER_STORE_REEXPORT: &str = "pub(crate) use bedcode_file_transfer_core::transfer::*;";

/// 核内端口面（`ports.rs` 的 `pub trait`）登记清单——新增差异面必须回本锁登记
const REGISTERED_PORTS: &[&str] = &[
    "BusPort",
    "ConsentGate",
    "EventPort",
    "KvStore",
    "LogPort",
    "MdnsPort",
    "NodePower",
    "PeerPort",
    "PlatformPort",
    "PluginProfile",
    "RootWireCodec",
    "RootsStore",
];

/// 各端 `adapters.rs` 必须实现的**已消费端口**（缺一即接线不完整）
const END_MUST_IMPLEMENT: &[&str] =
    &["KvStore", "PeerPort", "RootWireCodec", "RootsStore", "PluginProfile"];

// ==================== 扫描器 ====================

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("packages/<crate> 的上两级应是仓库根")
        .to_path_buf()
}

/// 端内插件源码目录：(端名, 路径)
fn end_src_dirs() -> Vec<(&'static str, PathBuf)> {
    vec![
        ("desktop", repo_root().join("bedcode-desktop/wasm-apps/file-transfer/rust/src")),
        ("mobile", repo_root().join("bedcode-mobile/wasm-apps/file-transfer/rust/src")),
    ]
}

/// 剥行注释（引号感知：字符串内的 `//` 不是注释起点）
fn strip_line_comments(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0usize;
    let (mut in_str, mut in_char, mut escaped) = (false, false, false);
    while i < bytes.len() {
        let b = bytes[i];
        if escaped {
            out.push(b as char);
            escaped = false;
            i += 1;
            continue;
        }
        match b {
            b'\\' if in_str || in_char => {
                out.push(b as char);
                escaped = true;
            }
            b'"' if !in_char => {
                in_str = !in_str;
                out.push(b as char);
            }
            b'\'' if !in_str => {
                in_char = !in_char;
                out.push(b as char);
            }
            b'/' if !in_str && !in_char && i + 1 < bytes.len() && bytes[i + 1] == b'/' => break,
            _ => out.push(b as char),
        }
        i += 1;
    }
    out
}

/// 该行是否开启 `#[cfg(test…)]` 区（其后的夹具按设计要复刻形状，不参与判据）
fn opens_cfg_test(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("#[cfg(") && t.contains("test")
}

/// 收集目录下全部 `.rs`（路径含 `tests` 段跳过），返回 (相对路径, 生产代码行)
fn collect_prod(dir: &Path) -> Vec<(String, Vec<String>)> {
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<String>)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            // 路径含 `tests` 段（目录化的测试子树）整个跳过
            if p.file_name().and_then(|n| n.to_str()) == Some("tests") {
                continue;
            }
            walk(root, &p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
            let Ok(text) = fs::read_to_string(&p) else { continue };
            let mut lines = Vec::new();
            for line in text.lines() {
                if opens_cfg_test(line) {
                    break;
                }
                lines.push(strip_line_comments(line.trim_end_matches('\r')));
            }
            let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            out.push((rel, lines));
        }
    }
}

/// 是否定义了名为 `name` 的函数（`fn name(`，允许 `pub(crate) fn` / `async fn` 前缀）
fn defines_fn(lines: &[String], name: &str) -> bool {
    let needle = format!("fn {name}(");
    lines.iter().any(|l| l.contains(&needle))
}

/// 是否出现某一 needle
fn contains(lines: &[String], needle: &str) -> bool {
    lines.iter().any(|l| l.contains(needle))
}

// ==================== 用例 ====================

/// 扫描器自检：既不能假绿（漏真回流），也不能假红（把注释当代码）
#[test]
fn scanner_is_not_vacuous() {
    let sample =
        vec!["// fn reduce_event(...) 解释性注释".to_string(), "let s = \"fn upsert(\";".to_string()];
    // 保守方向：抹平「字符串字面量里出现 `fn upsert(`」也照报（宁吵不静守——静守的代价是回流被漏掉）
    assert!(
        defines_fn(&sample, "upsert"),
        "字符串字面量里的函数名照报（保守方向：宁可假警报，不可静默失守）"
    );
    assert!(
        !defines_fn(&[strip_line_comments("// fn reduce_event(x: u8) {")], "reduce_event"),
        "注释里的函数名不得算定义，否则锁会被解释性注释淹没"
    );
    let kept = strip_line_comments("fn a(); // fn b()");
    assert!(kept.contains("fn a()"), "注释前的真代码必须保留");
    assert!(!kept.contains("fn b()"), "注释必须被剥掉");
    // 端内源码目录必须真实存在（判据路径漂移 = 扫描器空转）
    for (end, dir) in end_src_dirs() {
        assert!(dir.is_dir(), "{end} 插件源码目录不存在：{}", dir.display());
    }
}

/// 判据 1：零回流——核内判据不得在任一端重新长出定义
#[test]
fn pure_logic_is_not_reimplemented_per_end() {
    let mut violations = Vec::new();
    for (end, dir) in end_src_dirs() {
        assert!(dir.is_dir(), "{end} 插件源码目录不存在：{}（锁的判据路径已漂移）", dir.display());
        for (rel, lines) in collect_prod(&dir) {
            for name in CORE_ONLY_FNS {
                if defines_fn(&lines, name) {
                    violations.push(format!("{end}/{rel}: 重新定义了核内判据 `fn {name}(`"));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "共享核的判据在端内复活（两份实现的漂移就从这里开始，且两边的单测都会绿）：\n  - {}",
        violations.join("\n  - ")
    );
}

/// 判据 2：双端 `transfer_store.rs` 是纯转出
#[test]
fn transfer_store_stays_a_pure_reexport_on_both_ends() {
    let mut missing = Vec::new();
    for (end, dir) in end_src_dirs() {
        let path = dir.join("transfer_store.rs");
        let Ok(text) = fs::read_to_string(&path) else {
            missing.push(format!("{end}: 缺 transfer_store.rs（{}）", path.display()));
            continue;
        };
        let prod: Vec<String> = text
            .lines()
            .take_while(|l| !opens_cfg_test(l))
            .map(|l| strip_line_comments(l.trim_end_matches('\r')))
            .collect();
        if !contains(&prod, TRANSFER_STORE_REEXPORT) {
            missing.push(format!("{end}/transfer_store.rs: 缺转出行 `{TRANSFER_STORE_REEXPORT}`"));
        }
    }
    assert!(
        missing.is_empty(),
        "台账不再是纯转出（端内又自带实现 ⇒ 判据有两个真源）：\n  - {}",
        missing.join("\n  - ")
    );
}

/// 判据 3：两端都在场且实现已消费端口（适配器缺席 = 接线被拆）
#[test]
fn both_ends_ship_an_adapter_implementing_consumed_ports() {
    let mut problems = Vec::new();
    for (end, dir) in end_src_dirs() {
        let path = dir.join("adapters.rs");
        let Ok(text) = fs::read_to_string(&path) else {
            problems.push(format!("{end}: 缺 adapters.rs（{}）", path.display()));
            continue;
        };
        let prod: Vec<String> = text
            .lines()
            .take_while(|l| !opens_cfg_test(l))
            .map(|l| strip_line_comments(l.trim_end_matches('\r')))
            .collect();
        for port in END_MUST_IMPLEMENT {
            // 端口是**被实现的 trait**（在 `for` 之前），不是泛型约束：`impl<H: SdkTrait> Port for X`
            let needle = format!("{port} for ");
            if !contains(&prod, &needle) {
                problems.push(format!(
                    "{end}/adapters.rs: 未见端口 `{port}` 的实现（找 `{needle}`）"
                ));
            }
        }
        if !contains(&prod, "PortError") {
            problems
                .push(format!("{end}/adapters.rs: 未见 `PortError`（错误类型未转换 ⇒ 适配层形态不对）"));
        }
    }
    assert!(
        problems.is_empty(),
        "适配器面不完整（端内接线被拆或未跟上核的端口面）：\n  - {}",
        problems.join("\n  - ")
    );
}

/// 判据 4：核端口面登记在册——`ports.rs` 的 `pub trait` 集合与清单精确相等
#[test]
fn core_port_surface_is_registered() {
    let ports = repo_root().join("packages/bedcode-file-transfer-core/src/ports.rs");
    let text = fs::read_to_string(&ports).expect("读核 ports.rs");
    let prod: Vec<String> = text
        .lines()
        .take_while(|l| !opens_cfg_test(l))
        .map(|l| strip_line_comments(l.trim_end_matches('\r')))
        .collect();

    let mut observed: BTreeSet<String> = BTreeSet::new();
    for line in &prod {
        let Some(rest) = line.trim().strip_prefix("pub trait ") else { continue };
        let name: String =
            rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if !name.is_empty() {
            observed.insert(name);
        }
    }
    let expected: BTreeSet<String> = REGISTERED_PORTS.iter().map(|s| s.to_string()).collect();

    assert!(!observed.is_empty(), "端口扫描器空转（`pub trait` 一个都没扫到）—— 判据失效须修锁");
    assert_eq!(
        observed,
        expected,
        "核端口面与登记清单不一致：新增/改名/删除端口都必须回到本锁改清单——\
         差异面是**显式决定**，不是随手加一个没人实现的 trait"
    );
    assert_eq!(REGISTERED_PORTS.len(), expected.len(), "登记清单自身有重复项");
}
