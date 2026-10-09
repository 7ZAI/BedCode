//! 共享业务核的边界锁。
//!
//! 三条不变量，任何一条被破坏都直接测红：
//!
//! 1. **零任一端 SDK / 平台依赖**：核里 `use bedcode_plugin_api*` / `wasm_core` / `tauri` /
//!    `tokio` / `wasmtime` / `std::fs|net|process` 即回接——那些都是**适配器**的事。
//!    核一旦回接，双端共享的前提就没了（另一端编不过或被迫跟演）。
//! 2. **零产品身份字面量**：`com.bedcode.<产品段>` 不得出现在生产代码里（插件 id 经
//!    [`PluginIdentity`](crate::identity::PluginIdentity) 注入）。与宿主侧语义锁
//!    `capability_crates_no_product_ids` 的 C-4 判据同源——本锁是它在核内的**就地**版本，
//!    不必等宿主构建就能测。
//! 3. **依赖面收口**：`Cargo.toml` 的 `[dependencies]` 只允许 `serde` / `serde_json`
//!    （共享核引第三方 = 双端一起背，引入前先问「这还是共享核吗」）。
//!
//! 扫描纪律（踩坑记录沿用宿主侧同款锁）：
//! - **剥注释**（引号感知）：注释里点名 SDK 解释「为什么」是正常的，当代码判会产出噪音锁；
//! - **剥测试区**：文件内首个 `#[cfg(test…)]` 之后的内容不算生产代码；
//! - **自排除本锁文件**：本锁与 `wiring_lock` 含被禁 needle 的**字面量常量表**，不自排除会扫到
//!    自己而恒红（它们由 `#[cfg(test)] mod` 引入，本就是测试面）。
//!
//! 位置说明：本文件在 `src/` 而非 crate 根 `tests/`——治理面按 `packages/bedcode-*` 目录约定
//! 推导，crate 根出现 `tests/` 目录即被宿主侧 `capability_crates_unit_tests_only.rs` 判红
//! （crate 根 `tests/` = 独立测试二进制 = 只能经 `pub` API 访问的对外行为面）。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// 禁止出现在生产代码里的 SDK / 平台 needle。
///
/// 取「最短且不跨行」的片段（rustfmt 折行会把整行 needle 打成假红）。
const FORBIDDEN_NEEDLES: &[&str] = &[
    "bedcode_plugin_api",
    "bedcode_wasm_core",
    "bedcode_host_api_core",
    "wasmtime",
    "tauri",
    "tokio",
    "std::fs",
    "std::net",
    "std::process",
];

/// 产品身份前缀（与宿主侧语义锁同值）
const PRODUCT_ID_PREFIX: &str = "com.bedcode.";

/// 格式占位段（中性词，不是任何真实插件）
const PLACEHOLDER_SEGMENTS: &[&str] = &["xxx", "test", "other", "example", "sample"];

/// 依赖白名单（crate 名，仅约束 `[dependencies]` 段）
const ALLOWED_DEPS: &[&str] = &["serde", "serde_json"];

/// 自排除的锁文件（测试面，含被禁 needle 的字面量常量表）
const SELF_LOCK_FILES: &[&str] = &["boundary_lock.rs", "wiring_lock.rs"];

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 剥掉行注释（引号感知：字符串 / 字符字面量内的 `//` 不算注释起点）
///
/// 引号感知是必须的：朴素实现会把 `"http://…"; let x = "bedcode_plugin_api"` 后半段
/// 的真命中一起丢掉，锁就被静默解除。
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

/// 该行是否开启 `#[cfg(test…)]` 区
fn opens_cfg_test(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("#[cfg(") && t.contains("test")
}

/// 生产代码行（剥注释 + 截断测试区）；路径含 `tests` 段或属本锁文件时返回 None
fn prod_lines(path: &Path) -> Option<Vec<String>> {
    if path.components().any(|c| c.as_os_str() == "tests") {
        return None;
    }
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if SELF_LOCK_FILES.contains(&file_name) {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    let mut lines = Vec::new();
    for line in text.lines() {
        if opens_cfg_test(line) {
            break;
        }
        lines.push(strip_line_comments(line));
    }
    Some(lines)
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            collect_rs(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}

/// 扫描器的自检：这些函数必须真的能认出命中 / 真的会跳过该跳的
#[test]
fn scanner_is_not_vacuous() {
    // 正例：代码里的命中必须留下
    let kept = strip_line_comments("let u = \"http://a\"; use bedcode_plugin_api::host::PeerPort;");
    assert!(
        kept.contains("bedcode_plugin_api"),
        "字符串内的 `//` 不得被当注释起点：{kept:?}"
    );
    // 反例：注释里的命中必须被剥掉
    for src in ["// use bedcode_plugin_api", "let a = 1; // bedcode_plugin_api"] {
        assert!(
            !strip_line_comments(src).contains("bedcode_plugin_api"),
            "注释必须被剥掉，否则锁会被解释性注释淹没：{src:?}"
        );
    }
    // 测试区必须被截断
    let dir = tempfile::tempdir().expect("临时目录");
    let file = dir.path().join("probe.rs");
    fs::write(
        &file,
        "use bedcode_plugin_api::x;\n// bedcode_plugin_api\n#[cfg(test)]\nmod t { use bedcode_plugin_api::y; }\n",
    )
    .expect("写夹具");
    let text = prod_lines(&file).expect("非测试路径必须能取出生产行").join("\n");
    assert!(text.contains("bedcode_plugin_api"), "生产区命中必须保留：{text:?}");
    assert!(!text.contains("mod t"), "`#[cfg(test)]` 之后必须截断：{text:?}");
    assert!(!text.contains("//"), "注释必须被剥掉：{text:?}");
    // 路径含 tests 段整个跳过
    let tdir = dir.path().join("tests");
    fs::create_dir_all(&tdir).expect("建 tests");
    fs::write(tdir.join("x.rs"), "use bedcode_plugin_api::z;").expect("写");
    assert!(prod_lines(&tdir.join("x.rs")).is_none(), "tests 路径下的文件必须整个跳过");
    // 本锁文件自排除（否则扫到自己的 needle 常量表即恒红）
    assert!(
        prod_lines(&crate_root().join("src/boundary_lock.rs")).is_none(),
        "锁文件必须自排除"
    );
    assert!(
        prod_lines(&crate_root().join("src/wiring_lock.rs")).is_none(),
        "同上：wiring_lock 也必须自排除"
    );
    // 真生产文件不得被自排除规则误伤
    assert!(
        prod_lines(&crate_root().join("src/ports.rs")).is_some(),
        "自排除不得扩大化到真正生产文件（那会让整把锁空转）"
    );
}

#[test]
fn core_carries_no_sdk_or_platform_dependency() {
    let mut files = Vec::new();
    collect_rs(&crate_root().join("src"), &mut files);
    assert!(!files.is_empty(), "src 下没有 .rs — 扫描器空转，本锁失效");

    let mut scanned = 0usize;
    let mut hits = Vec::new();
    for file in &files {
        let Some(lines) = prod_lines(file) else { continue };
        scanned += 1;
        let text = lines.join("\n");
        for needle in FORBIDDEN_NEEDLES {
            if text.contains(needle) {
                hits.push(format!("{}: `{needle}`", file.display()));
            }
        }
    }
    assert!(scanned > 0, "一个文件都没扫到 — 自排除规则过宽，锁失效");
    assert!(
        hits.is_empty(),
        "共享业务核不得依赖任一端 SDK / 平台（那是适配器的职责；核一旦回接，\
         双端共享的前提就没了）：\n  - {}",
        hits.join("\n  - ")
    );
}

#[test]
fn core_carries_no_product_identity_literal() {
    let mut files = Vec::new();
    collect_rs(&crate_root().join("src"), &mut files);

    let mut hits: BTreeSet<String> = BTreeSet::new();
    for file in &files {
        let Some(lines) = prod_lines(file) else { continue };
        let text = lines.join("\n");
        let mut rest = text.as_str();
        while let Some(at) = rest.find(PRODUCT_ID_PREFIX) {
            let after = &rest[at + PRODUCT_ID_PREFIX.len()..];
            let seg: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                .collect();
            rest = &after[seg.len()..];
            let first = seg.split('.').next().unwrap_or("");
            if !first.is_empty() && !PLACEHOLDER_SEGMENTS.contains(&first) {
                hits.insert(format!("{}: com.bedcode.{first}", file.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "共享业务核不得硬编码产品身份（插件 id 经 PluginIdentity 注入；\
         硬编码即「核知道了有哪些产品」，损害它对任何宿主可复用的属性）：\n  - {}",
        hits.into_iter().collect::<Vec<_>>().join("\n  - ")
    );
}

#[test]
fn dependency_surface_stays_minimal() {
    let manifest = fs::read_to_string(crate_root().join("Cargo.toml")).expect("读 Cargo.toml");
    let mut in_deps = false;
    let mut deps: Vec<String> = Vec::new();
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_deps = t == "[dependencies]";
            continue;
        }
        if !in_deps || t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some(name) = t.split('=').next() {
            deps.push(name.trim().to_string());
        }
    }
    assert!(!deps.is_empty(), "依赖段解析为空 — 判据失效，须修锁而不是放行");
    let unexpected: Vec<&String> = deps.iter().filter(|d| !ALLOWED_DEPS.contains(&d.as_str())).collect();
    assert!(
        unexpected.is_empty(),
        "共享核的第三方依赖必须收口在 {ALLOWED_DEPS:?}（引入新依赖 = 双端一起背，\
         先问「这还是共享核吗」）：实得 {unexpected:?}"
    );
}

/// 防「删文件绕过内容锁」：锁的正面锚点必须在场
#[test]
fn locked_surface_files_stay_present() {
    for rel in [
        "src/lib.rs",
        "src/ports.rs",
        "src/identity.rs",
        "src/domain.rs",
        "src/settings.rs",
        "src/roots.rs",
        "src/sessions.rs",
        "src/transfer.rs",
        "src/transfer/tests.rs",
        "src/boundary_lock.rs",
        "src/wiring_lock.rs",
    ] {
        let path = crate_root().join(rel);
        assert!(path.is_file(), "锁定的核内文件缺席：{rel}（删文件不能变成绕过内容锁的手段）");
    }
}
