//! wasm-core 移动 fork 退役锁（票 2026-10-09 wasm-core-single-crate 票 06 批次 04 /
//! 票 07 批次 02 收口）
//!
//! 票 06 把移动 fork crate `bedcode-mobile/packages/bedcode-wasm-core`
//! （package `bedcode-wasm-core-mobile`）整面退役：移动 `src-tauri` 改为依赖**仓库根
//! 单一 wasm-core**（`packages/bedcode-wasm-core` + `mobile-host` feature），
//! `bedcode-wasm-core-mobile` 只作为 **package rename 别名**保留（`package =`
//! "bedcode-wasm-core"），源码 `bedcode_wasm_core_mobile::` 导入路径零改动。
//!
//! 退役走 fail-visible 三形态（AGENTS §5.1.4）：
//! - ① 旧读路径删除：fork 目录删除 + 别名指向根 crate，残留引用编译期即红；
//! - ② 旧 ABI 产物实例化期点名：`stale_artifact_rebuild_hint` 管线（票 04 扩展）；
//! - ③ 本锁：回接形态 = fork 目录 / 独立 crate 包名 / 旧路径引用落回来。
//!
//! 判据（只扫**构建面文件**，不扫文档与记忆——迁移记账的注释不算回接）：
//! 1. fork 目录不存在；
//! 2. 任何 `Cargo.toml` 不得以 `bedcode-wasm-core-mobile` 为 **package 名**
//!    （依赖键别名可以，包名不行——包名回来 = fork crate 复活）；
//! 3. 移动侧构建面文件不得出现 fork 形态路径（上跳一级 / 裸引用
//!    `"../packages/bedcode-wasm-core"`；根 crate 的合法形态是上跳两级
//!    `"../../packages/bedcode-wasm-core"`）；
//! 4. 移动 `src-tauri` 的别名必须指向根 crate（package rename + `mobile-host`）。

use std::path::{Path, PathBuf};

/// 构建面文件扩展名（路径引用的载体）
const BUILD_EXTS: [&str; 5] = ["toml", "mjs", "js", "sh", "json"];

/// 跳过目录（产物 / 依赖树 / VCS 元数据）
const SKIP_DIRS: [&str; 3] = ["target", "node_modules", ".git"];

/// `bedcode-mobile/src-tauri`（CARGO_MANIFEST_DIR）
fn mobile_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// `bedcode-mobile/`
fn mobile_repo_root() -> PathBuf {
    mobile_root().parent().expect("src-tauri 的父目录").to_path_buf()
}

/// 仓库根
fn workspace_root() -> PathBuf {
    mobile_repo_root()
        .parent()
        .expect("bedcode-mobile 的父目录")
        .to_path_buf()
}

/// 递归收集构建面文件（按扩展名过滤，跳过产物 / 依赖目录）
fn collect_build_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            collect_build_files(&path, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if BUILD_EXTS.contains(&ext) {
                out.push(path);
            }
        }
    }
}

/// 该行是否注释（toml/sh 用 `#`，js/mjs 用 `//`）
fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('#') || t.starts_with("//")
}

/// ① fork 目录必须不存在
#[test]
fn mobile_wasm_core_fork_directory_is_retired() {
    let fork_dir = mobile_repo_root().join("packages/bedcode-wasm-core");
    assert!(
        !fork_dir.exists(),
        "移动 fork crate 目录复活：{}（票 06 批次 04 已整面退役——单一 wasm-core 在仓库根 \
         `packages/bedcode-wasm-core`，移动侧经 `mobile-host` feature 消费）",
        fork_dir.display()
    );
}

/// ② 任何 crate 都不得以 `bedcode-wasm-core-mobile` 为包名
#[test]
fn no_crate_is_packaged_as_bedcode_wasm_core_mobile() {
    let mut files = Vec::new();
    collect_build_files(&workspace_root(), &mut files);
    assert!(
        files.len() >= 10,
        "构建面文件收集过少（{}），锁可能空转（路径错或 read_dir 异常被吞）",
        files.len()
    );
    let mut violations: Vec<String> = Vec::new();
    for f in &files {
        if f.file_name().and_then(|n| n.to_str()) != Some("Cargo.toml") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(f) else {
            continue;
        };
        for (idx, line) in content.lines().enumerate() {
            if is_comment(line) {
                continue;
            }
            // 压平空白后按整行判定（`name="…"` / `name = "…"` 同源）
            let flat: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            if flat == "name=\"bedcode-wasm-core-mobile\"" {
                violations.push(format!("{}:{}: {}", f.display(), idx + 1, line.trim()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "出现 fork crate 的包名（package rename 别名只允许作为依赖键，不允许作为包名）：\n{}",
        violations.join("\n")
    );
}

/// ③ 移动侧构建面文件不得出现 fork 形态路径
#[test]
fn mobile_build_files_never_reference_the_fork_path() {
    let mut files = Vec::new();
    collect_build_files(&mobile_repo_root(), &mut files);
    assert!(
        files.len() >= 10,
        "移动端构建面文件收集过少（{}），锁可能空转",
        files.len()
    );
    // fork 形态：上跳一级 `"../packages/bedcode-wasm-core"` 或裸引用
    // `"packages/bedcode-wasm-core"`；根 crate 合法形态（上跳两级）不含这两个子串。
    let needles = ["\"../packages/bedcode-wasm-core\"", "\"packages/bedcode-wasm-core\""];
    let mut violations: Vec<String> = Vec::new();
    for f in &files {
        let Ok(content) = std::fs::read_to_string(f) else {
            continue;
        };
        for (idx, line) in content.lines().enumerate() {
            if is_comment(line) {
                continue;
            }
            for needle in needles {
                if line.contains(needle) {
                    violations.push(format!("{}:{}: {}", f.display(), idx + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "移动侧构建面出现 fork 形态路径引用（应上跳两级指向仓库根 `../../packages/bedcode-wasm-core`）：\n{}",
        violations.join("\n")
    );
}

/// ④ 移动 src-tauri 的别名必须指向根 crate（package rename + mobile-host）
#[test]
fn mobile_wasm_core_alias_points_at_the_root_crate() {
    let manifest = std::fs::read_to_string(mobile_root().join("Cargo.toml"))
        .expect("读取 bedcode-mobile/src-tauri/Cargo.toml 失败");
    let mut aliases = 0usize;
    let mut violations: Vec<String> = Vec::new();
    for (idx, line) in manifest.lines().enumerate() {
        if is_comment(line) {
            continue;
        }
        let t = line.trim();
        if !t.starts_with("bedcode-wasm-core-mobile") {
            continue;
        }
        aliases += 1;
        if !t.contains("package = \"bedcode-wasm-core\"") {
            violations.push(format!(
                "Cargo.toml:{}: 别名缺 package rename（`package = \"bedcode-wasm-core\"`）：{t}",
                idx + 1
            ));
        }
        if !t.contains("path = \"../../packages/bedcode-wasm-core\"") {
            violations.push(format!(
                "Cargo.toml:{}: 别名未指向仓库根 crate（`path = \"../../packages/bedcode-wasm-core\"`）：{t}",
                idx + 1
            ));
        }
        if !t.contains("mobile-host") {
            violations.push(format!(
                "Cargo.toml:{}: 别名未走 mobile-host feature（移动形态必须 default-features=false）：{t}",
                idx + 1
            ));
        }
    }
    assert!(
        aliases >= 1,
        "别名行（`bedcode-wasm-core-mobile = …`）消失——移动侧依赖切换被回退？"
    );
    assert!(
        violations.is_empty(),
        "移动 src-tauri 对单一 wasm-core 的引用形态漂移：\n{}",
        violations.join("\n")
    );
}
