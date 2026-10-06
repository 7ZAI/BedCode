//! 整核抽出结构锁（wasm-core-whole-crate 票 05）
//!
//! `wasm_core/`（119 文件 / 54,394 行）已整体迁入
//! `bedcode-desktop/packages/bedcode-wasm-core/`（spec M1；票 02-04 执行完毕）。
//! lib 侧只剩 `pub use` 垫片保持既有 `crate::wasm_core::*` / `crate::db::*`
//! 路径零改动编译通过。
//!
//! ## 锁什么
//!
//! 1. **`src-tauri/src/wasm_core/` 不得存在实现文件**：整核迁走后宿主侧任何
//!    落在 `src/wasm_core/` 下的 `.rs` 都是回接（把机制搬回 bin crate，或
//!    摊开第二份拷贝）。目录本身允许存在（git 不跟踪空目录，它通常不存在），
//!    但其中不得有源码。
//! 2. **lib.rs 的 wasm_core / db 必须是 `pub use` 垫片**：垫片是「名字逐字
//!    一致的再导出」，一旦有人改回 `pub mod wasm_core` / 自行 `mod db` 挂
//!    模块树，垫片语义被替换成双份源码——编译能过但边界悄悄被绕开。
//! 3. **垫片必须真的存在**（防空转）：`pub use bedcode_wasm_core as wasm_core;`
//!    与 `pub use bedcode_wasm_core::{db, enums, pty};` 两行缺失即锁失效。
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

/// 锁 1：`src-tauri/src/wasm_core/` 不得存在任何 `.rs` 实现文件
#[test]
fn wasm_core_dir_has_no_implementation_files() {
    let dir = desktop_root().join("src/wasm_core");
    if !dir.is_dir() {
        // 目录不存在 = 迁走后干净的常态（git 不跟踪空目录）
        return;
    }
    let rs_files = collect_rs_files(&dir);
    assert!(
        rs_files.is_empty(),
        "整核迁走后宿主侧 src/wasm_core/ 出现 {} 个实现文件——\n\
         `wasm_core/` 真源在 bedcode-desktop/packages/bedcode-wasm-core/，\n\
         宿主侧任何 .rs 都是回接或双份拷贝（AGENTS §5.1.4 fail-visible）：\n  {}",
        rs_files.len(),
        rs_files
            .iter()
            .map(|f| format!("  {}", dir.join(f).display()))
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
    // 用词边界匹配避免误中 `bench_channel` 等含 db 子串的模块名。
    let forbidden: [&str; 4] = ["wasm_core", "db", "enums", "pty"];
    let mut violations: Vec<String> = Vec::new();
    for (idx, raw) in content.lines().enumerate() {
        let line = raw.trim_start();
        if !line.starts_with("mod ") {
            continue;
        }
        let name = line.trim_start_matches("mod ").trim_end_matches(';').trim();
        if forbidden.contains(&name) {
            violations.push(format!("lib.rs:{}: {line}", idx + 1));
        }
    }
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

    // 锁 2 的判据：`mod wasm_core` 必须被判违规，`mod bench_channel` 不算
    let probe = "pub mod server;\nmod wasm_core;\nmod bench_channel;\n";
    let mut violations: Vec<String> = Vec::new();
    for (idx, raw) in probe.lines().enumerate() {
        let line = raw.trim_start();
        if !line.starts_with("mod ") {
            continue;
        }
        let name = line.trim_start_matches("mod ").trim_end_matches(';').trim();
        if ["wasm_core", "db", "enums", "pty"].contains(&name) {
            violations.push(format!("probe:{}: {line}", idx + 1));
        }
    }
    assert_eq!(
        violations,
        vec!["probe:2: mod wasm_core;".to_string()],
        "锁 2 的 mod 判据漂移：wasm_core 必须红、bench_channel 必须绿"
    );
}
