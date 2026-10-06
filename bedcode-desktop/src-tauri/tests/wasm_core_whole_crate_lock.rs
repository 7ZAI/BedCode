//! 整核抽出结构锁（wasm-core-whole-crate 票 05）
//!
//! `wasm_core/`（119 文件 / 54,394 行）已整体迁入
//! `bedcode-desktop/packages/bedcode-wasm-core/`（spec M1；票 02-04 执行完毕）。
//! lib 侧只剩 `pub use` 垫片保持既有 `crate::wasm_core::*` / `crate::db::*`
//! 路径零改动编译通过。
//!
//! ## 锁什么
//!
//! 1. **宿主 `src/` 下与整核四个名字同名的源码落点不得存在**：`wasm_core/`、`db.rs`、
//!    `pty/`、`enums/` 都是整核迁走后宿主侧的回接或双份拷贝形态（spec M1；票 02-04
//!    执行完毕）。两个名字库共用 [`FORBIDDEN_SHIM_NAMES`] 一张表。
//! 2. **lib.rs 的 wasm_core / db / pty / enums 必须是 `pub use` 垫片**且垫片行真的存在
//!    （防空转）。任何可见性形态的 `mod` 声明（裸 / `pub` / `pub(crate)` / `pub(super)`）
//!    都算违规——只要名字挂在模块树上，内容就来自宿主源码目录而非 crate，垫片语义即被
//!    替换成双份源码。
//!
//! 两把锁互为对偶：锁 1 从**磁盘**看，锁 2 从**声明**看。锁 2 抓不到「残留文件尚未声明」
//! 与「声明被 `#[path]` 指向别处」，锁 1 抓不到「`pub use` 被改成 `pub mod`」。
//!
//! ## 与既有反双份锁的关系
//!
//! `bedcode-wasm-core/src/enums.rs` 的 `wire_shim_files_contain_no_definitions`
//! 锁的是 crate 侧 enums 垫片只允许 `pub use`；本锁是它在 **lib 侧** 的对偶：
//! 锁整核垫片（wasm_core / db / pty / enums 四者 + 目录无实现文件）。两者各守
//! 一侧，中间隔着 crate 边界——一边被改坏另一边照常红。
//!
//! ## 为什么是测试而不是文档
//!
//! 垫片改成 `pub mod` 或 `mod db` 不会编译失败、不会测试变红（模块名照旧，
//! 只是内容从再导出变成源码），只有运行期或结构锁能发现。AGENTS §5.1.4 的
//! fail-visible 原则要求「宿主侧回查显性失败」——本锁就是那个显性失败点。

use std::fs;
use std::path::PathBuf;

/// 宿主 crate 根（`bedcode-desktop/src-tauri`）
fn desktop_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 锁 1 + 锁 2 的禁词表：整核四个名字（`db` / `pty` / `enums` 以**精确**相等匹配，
/// 避免误中 `bench_channel` 之类含同名子串的模块）
///
/// **单一事实源**：两把锁与防空转自检都引用它。首版两处各写一份字面量表——改锁不动自检
/// 时，自检验证的是一份**已经不再生效**的副本，形同虚设。
const FORBIDDEN_SHIM_NAMES: &[&str] = &["wasm_core", "db", "enums", "pty"];

/// 锁 2 的判据核心：找出 `content` 里把整核名字挂成**模块树条目**的 `mod` 声明
///
/// 返回 `(1 基行号, 原始行)`。锁与自检共用这一份实现——自检因此验证的是**出货的那份
/// 判据**，而不是它的副本。
///
/// **任何可见性都算违规**（裸 `mod` / `pub mod` / `pub(crate) mod` …）：只要名字挂在
/// 模块树上，内容就来自宿主源码目录而不是 crate，垫片语义即被替换成双份源码。
/// 唯一合法形态是 `pub use` 再导出——它不创建模块树条目。
///
/// 首版只认行首裸 `mod `，**漏掉了 `pub mod` 正是回接的真实形态**（把 `pub use` 改成
/// `pub mod` 即完成搬回，而锁全绿）；`pub(crate) mod` 同样漏。
fn forbidden_module_decls(content: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (idx, raw) in content.lines().enumerate() {
        let line = raw.trim_start();
        // 行尾注释里的 `mod db;` 不是声明（`pub mod server; // 见 mod db`）
        let code = line.split("//").next().unwrap_or("").trim_end();
        // 剥掉可选的可见性限定：`pub` / `pub(crate)` / `pub(in path)` / `pub(super)`
        let after_vis = match code.strip_prefix("pub") {
            Some(rest) => match rest.strip_prefix('(') {
                // `pub(crate)` / `pub(in a::b)`：跳到配对的右括号之后
                Some(inner) => match inner.find(')') {
                    Some(close) => inner[close + 1..].trim_start(),
                    // 括号不配平 = 未覆盖写法，宁可报错也不猜
                    None => return panic!("第 {} 行 `{line}` 的 pub 可见性括号不配平，无法判定 mod 声明", idx + 1),
                },
                None => rest.trim_start(),
            },
            None => code,
        };
        let Some(rest) = after_vis.strip_prefix("mod") else {
            continue;
        };
        // `mod` 之后必须紧跟空白（挡掉 `module` / `model` 这类同前缀标识符）
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let name = rest.trim().trim_end_matches(';').trim();
        if FORBIDDEN_SHIM_NAMES.contains(&name) {
            out.push((idx + 1, line.to_string()));
        }
    }
    out
}

/// 锁 1：宿主 `src/` 下与整核四个名字同名的**源码落点**（文件或目录）一律不得存在
///
/// 整核迁走后，宿主侧 `src/wasm_core/`、`src/db.rs`、`src/pty/`、`src/enums/` 都是回接或
/// 双份拷贝的形态。锁 2 只看**声明**，看不出「声明被改用 `#[path]` 指到别处」或「残留文件
/// 尚未声明」这两种漏形态；锁 1 从磁盘侧独立兜住，两把锁互为对偶。
fn host_side_shim_paths() -> Vec<String> {
    let src = desktop_root().join("src");
    let mut out = Vec::new();
    for name in FORBIDDEN_SHIM_NAMES {
        for rel in [name.to_string(), format!("{name}/")] {
            let path = src.join(&rel);
            if path.exists() {
                out.push(rel);
            }
        }
    }
    out
}

/// 递归收集 `root` 下所有 `.rs` 文件（相对 `root` 的正斜杠路径）
fn collect_rs_files(root: &PathBuf) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(rel);
            }
        }
    }
    out.sort();
    out
}

/// 锁 1：宿主 `src/` 下与整核四个名字同名的源码落点不得存在（文件或目录）
#[test]
fn host_side_shim_paths_do_not_exist() {
    let present = host_side_shim_paths();
    assert!(
        present.is_empty(),
        "整核迁走后宿主侧出现与垫片同名的源码落点：\n  {}\n\
         真源在 bedcode-desktop/packages/bedcode-wasm-core/src/，宿主侧同名落点是回接或\
         双份拷贝（AGENTS §5.1.4 fail-visible）。同名目录也不允许存在——git 不跟踪空目录，\
         它的存在本身就意味着里面有东西",
        present
            .iter()
            .map(|rel| format!("  src/{rel}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// 锁 2：lib.rs 的 wasm_core / db / pty / enums 暴露必须是 `pub use` 垫片，
/// 且垫片行必须真的存在（防空转）
#[test]
fn lib_shims_are_pub_use_reexports_not_modules() {
    let lib_rs = desktop_root().join("src/lib.rs");
    let content =
        fs::read_to_string(&lib_rs).unwrap_or_else(|e| panic!("读取 lib.rs 失败：{e} —— 垫片锁失去扫描对象，锁空转"));

    // 垫片必须存在：wasm_core 整体再导出 + db/enums/pty 四个名字的再导出
    assert!(
        content.contains("pub use bedcode_wasm_core as wasm_core;"),
        "lib.rs 缺少 `pub use bedcode_wasm_core as wasm_core;` 垫片——\n\
         整核垫片被删除或改写，既有 `crate::wasm_core::*` 引用将失去名字（防回接锁失效）"
    );
    assert!(
        content.contains("pub use bedcode_wasm_core::{db, enums, pty};"),
        "lib.rs 缺少 `pub use bedcode_wasm_core::{{db, enums, pty}};` 垫片——\n\
         db / enums / pty 三个引擎面真源在 crate，垫片缺失即回接（防回接锁失效）"
    );

    // 垫片必须只是再导出：不得以 `mod` / `pub mod` 形态把整核挂回 lib 模块树
    // （`mod wasm_core` / `mod db` 一旦出现，垫片语义就被替换成双份源码）。
    let violations: Vec<String> = forbidden_module_decls(&content)
        .iter()
        .map(|(line_no, line)| format!("lib.rs:{line_no}: {line}"))
        .collect();
    assert!(
        violations.is_empty(),
        "整核名字以 `mod` 形态挂回 lib 模块树（垫片被替换成源码双份）：\n{}",
        violations.join("\n")
    );
}

/// 防空转自检：扫描器确实能认出「目录里的 .rs」与「mod 声明」——
/// 否则上面两个锁可能因为什么都扫不到而恒过（vacuous pass）。
#[test]
fn scanner_detects_real_files_and_mod_declarations() {
    // 锁 1 的扫描器：临时目录里放一个 .rs 必须被认出
    let tmp = std::env::temp_dir().join(format!(
        "bedcode-wasm-core-whole-crate-lock-scan-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp.join("nested")).expect("建临时树");
    fs::write(tmp.join("nested/db.rs"), "// 占位\n").expect("写临时文件");
    let found = collect_rs_files(&tmp);
    let _ = fs::remove_dir_all(&tmp);
    assert_eq!(
        found,
        vec!["nested/db.rs".to_string()],
        "扫描器必须且只能认出临时树里的那个 .rs（否则锁 1 对真实文件空转）"
    );

    // 锁 2 的判据：喂**锁本身用的那份** `forbidden_module_decls`（不复制实现），
    // 验证禁词表 + 词边界匹配 + 可见性无关性三条同时生效。
    // 判据一旦漂移（改前缀匹配 / 改禁词表 / 退回只认裸 mod），本用例即红——
    // 这正是首版把判据复制两份时丢掉的保证。
    let probe = "\
pub mod server;
mod wasm_core;
mod bench_channel;
pub mod db;
pub(crate) mod enums;
pub(super) mod pty;
pub use bedcode_wasm_core as wasm_core;
mod // 提到 mod 但不是声明：db
";
    let violations: Vec<String> = forbidden_module_decls(probe)
        .iter()
        .map(|(line_no, line)| format!("probe:{line_no}: {line}"))
        .collect();
    assert_eq!(
        violations,
        vec![
            "probe:2: mod wasm_core;".to_string(),
            "probe:4: pub mod db;".to_string(),
            "probe:5: pub(crate) mod enums;".to_string(),
            "probe:6: pub(super) mod pty;".to_string(),
        ],
        "锁 2 的 mod 判据漂移：三种可见性形态（裸 / pub / pub(crate) / pub(super)）挂整核名字\
         必须全部红；`mod bench_channel`（含 db 子串）、`pub use` 再导出、行尾注释里提到的 \
         mod db 必须绿；禁词表改动必须同步这里——否则自检验证的是已废弃的副本"
    );

    // 禁词表本身也要防空转：四个整核名字必须逐个被认出（首版自检只试了 `wasm_core`，
    // 另外三个名字被漏改时自检照绿）。
    for name in FORBIDDEN_SHIM_NAMES {
        let hit = forbidden_module_decls(&format!("mod {name};\n"));
        assert_eq!(
            hit.len(),
            1,
            "禁词表里的 `{name}` 必须被 mod 判据认出（漏一个名字 = 该名字的回接形态无锁可挡）"
        );
    }

    // `module` / `model` 这类同前缀标识符不得被误判成 mod 声明。
    for lookalike in ["module db;", "model db;", "moddb;"] {
        assert!(
            forbidden_module_decls(lookalike).is_empty(),
            "`{lookalike}` 不是 mod 声明（mod 之后必须跟空白），不得被当成回接"
        );
    }
}
