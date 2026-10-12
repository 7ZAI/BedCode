//! wasm-core 移动 fork / rename 织体退役锁（票 2026-10-09 wasm-core-single-crate
//! 票 06 批次 04 / 票 07 批次 02 收口；票 2026-10-11 wasm-core-single-form 票 05
//! 语义泛化）
//!
//! 票 06 把移动 fork crate `bedcode-mobile/packages/bedcode-wasm-core`
//! （package `bedcode-wasm-core-mobile`）整面退役：移动 `src-tauri` 改为依赖**仓库根
//! 单一 wasm-core**（`packages/bedcode-wasm-core` + `bare-host` feature），
//! 当时以 **package rename 别名**（依赖键 `bedcode-wasm-core-mobile`）过渡。
//! **票 05 收口**：别名删除——移动侧直连单 crate（依赖键就是 `bedcode-wasm-core`），
//! 源码 `bedcode_wasm_core::` 导入路径。本锁自此泛化为「禁止任何 fork / rename
//! 织体回归」：第二 crate、独立包名、别名键、fork 形态路径、同名新目录（判据 5，
//! 票 06 补），五者任一落回即红。
//!
//! 退役走 fail-visible 三形态（AGENTS §5.1.4）：
//! - ① 旧读路径删除：fork 目录删除 + 别名键删除，残留引用编译期即红；
//! - ② 旧 ABI 产物实例化期点名：`stale_artifact_rebuild_hint` 管线（票 04 扩展）；
//! - ③ 本锁：回接形态 = fork 目录 / 独立 crate 包名 / rename 别名键 / 旧路径引用。
//!
//! 判据（只扫**构建面文件**，不扫文档与记忆——迁移记账的注释不算回接）：
//! 1. fork 目录不存在；
//! 2. 任何 `Cargo.toml` 不得以 `bedcode-wasm-core-mobile` 为 **package 名**
//!    （包名回来 = fork crate 复活）；
//! 3. 移动侧构建面文件不得出现 fork 形态路径（上跳一级 / 裸引用
//!    `"../packages/bedcode-wasm-core"`；根 crate 的合法形态是上跳两级
//!    `"../../packages/bedcode-wasm-core"`）；
//! 4. 移动 `src-tauri` **直连**根 crate：依赖键为 `bedcode-wasm-core` + 走
//!    `bare-host` feature，且不存在任何 `bedcode-wasm-core-mobile` 依赖键（rename
//!    别名回归）；移动 `src` / `tests` 源码不得再出现 `bedcode_wasm_core_mobile`
//!    标识符；
//! 5. **（票 06 补）仓库内只有一份 wasm-core 织体**：`packages/bedcode-wasm-core`
//!    之外不得再有任何 `bedcode-wasm-core*` 目录。
//!
//! 判据 5 与 1–4 的分工：1–4 盯的是「移动 fork 这一个已知织体」的各种回接字形；
//! 判据 5 盯的是**任何**第二份织体——哪怕换个新名字（`bedcode-wasm-core-shim` /
//! `wasm-core` / `-core` 后缀）重新长出来。票 04 / 05 把双端 WIT 绑定全部迁出
//! wasm-core 之后，「复制一份内核自带绑定」成了最省事也最危险的动作（两份机制
//! 面同名不同型，装配期 `defined twice` 或静默双写）。

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
         `packages/bedcode-wasm-core`，移动侧经 `bare-host` feature 消费）",
        fork_dir.display()
    );
}

/// ② 任何 crate 都不得以 `bedcode-wasm-core-mobile` 为包名
#[test]
fn no_crate_is_packaged_as_bedcode_wasm_core() {
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

/// ④ 移动 src-tauri **直连**根 crate（票 05 收口：rename 别名已删除）
#[test]
fn mobile_wasm_core_dep_is_direct_single_crate() {
    let manifest = std::fs::read_to_string(mobile_root().join("Cargo.toml"))
        .expect("读取 bedcode-mobile/src-tauri/Cargo.toml 失败");
    let mut direct = 0usize;
    let mut violations: Vec<String> = Vec::new();
    for (idx, line) in manifest.lines().enumerate() {
        if is_comment(line) {
            continue;
        }
        let t = line.trim();
        // 别名键回归即红（无论写成裸键还是带 package rename）
        if t.starts_with("bedcode-wasm-core-mobile") {
            violations.push(format!(
                "Cargo.toml:{}: 出现 rename 别名键 `bedcode-wasm-core-mobile`（票 05 已删除，直连单 crate）：{t}",
                idx + 1
            ));
            continue;
        }
        if !t.starts_with("bedcode-wasm-core ") && !t.starts_with("bedcode-wasm-core=") {
            continue;
        }
        direct += 1;
        if !t.contains("path = \"../../packages/bedcode-wasm-core\"") {
            violations.push(format!(
                "Cargo.toml:{}: 未指向仓库根 crate（`path = \"../../packages/bedcode-wasm-core\"`）：{t}",
                idx + 1
            ));
        }
        if !t.contains("bare-host") {
            violations.push(format!(
                "Cargo.toml:{}: 未走 bare-host feature（移动形态必须 default-features=false）：{t}",
                idx + 1
            ));
        }
    }
    assert!(
        direct >= 1,
        "直连依赖行（`bedcode-wasm-core = …`）消失——移动侧依赖切换被回退？"
    );
    assert!(
        violations.is_empty(),
        "移动 src-tauri 对单一 wasm-core 的引用形态漂移：\n{}",
        violations.join("\n")
    );
}

/// ⑤ 移动源码不得再出现 rename 别名标识符（`bedcode_wasm_core_mobile`）
///
/// 别名删除后，任何 `bedcode_wasm_core_mobile::` 路径都编译不过；本锁把
/// 「编译期即红」再前移一层：源码 / 测试里连**字符串形式**都不许留，
/// 免得后续有人在文档注释里追认别名、再顺手把 Cargo 键写回来。
#[test]
fn mobile_sources_never_use_the_rename_alias_identifier() {
    let mut files = Vec::new();
    for dir in [mobile_root().join("src"), mobile_root().join("tests")] {
        collect_build_files(&dir, &mut files);
    }
    // rust 源（构建面扩展名过滤器不含 rs，单独收）
    for base in [mobile_root().join("src"), mobile_root().join("tests")] {
        let mut stack = vec![base];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if path.is_dir() {
                    if SKIP_DIRS.contains(&name.as_str()) {
                        continue;
                    }
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    files.push(path);
                }
            }
        }
    }
    assert!(
        files.len() >= 20,
        "源码文件收集过少（{}），锁可能空转",
        files.len()
    );
    let mut violations: Vec<String> = Vec::new();
    // 自排除：本锁的禁用名单（字面量）与说明注释自身就带该标识符——不排除即恒红
    // （「锁自己判红 = 锁空转」，T-04 `desktop_binding_surface…` 同款纪律）。
    let self_path = mobile_root().join("tests/retired_mobile_wasm_core_fork_lock.rs");
    for f in &files {
        if f == &self_path {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(f) else {
            continue;
        };
        for (idx, line) in content.lines().enumerate() {
            if line.contains("bedcode_wasm_core_mobile") {
                violations.push(format!("{}:{}: {}", f.display(), idx + 1, line.trim()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "移动源码出现 rename 别名标识符 `bedcode_wasm_core_mobile`（票 05 已删除别名，统一 `bedcode_wasm_core::`）：\n{}",
        violations.join("\n")
    );
}

/// ⑤ 仓库内只有一份 wasm-core 织体（票 06 补）
///
/// 判据 ①–④ 盯的是「移动 fork 这一个已知织体」的各类回接字形。本判据盯的是
/// **任何**第二份：哪怕换个新名字（`bedcode-wasm-core-shim` / `wasm-core` /
/// `-core` 后缀）重新长出来，也一样是「第二份机制面同名不同型」——装配期
/// `defined twice` 或静默双写，且没有任何既有测试会红。票 04 / 05 把双端 WIT 绑定
/// 全部迁出 wasm-core 之后，「复制一份内核自带绑定」成了最省事也最危险的动作。
///
/// 扫描范围 = 仓库根下 4 层（覆盖 `packages/` 与两端自带的包目录），跳过
/// `target` / `node_modules` / `.git`。4 层是刻意设的上限：既覆盖真实的 crate 落点，
/// 又不会把 `target/` 里 cargo 生成的 `build/…` 结构拖进来。
#[test]
fn repo_has_exactly_one_wasm_core_body() {
    const MAX_DEPTH: usize = 4;
    let root = workspace_root();
    let mut found: Vec<String> = Vec::new();
    let mut stack = vec![(root.clone(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !path.is_dir() {
                continue;
            }
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            if name.starts_with("bedcode-wasm-core") {
                found.push(
                    path.strip_prefix(&root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
            if depth < MAX_DEPTH {
                stack.push((path, depth + 1));
            }
        }
    }
    found.sort();
    assert_eq!(
        found,
        vec!["packages/bedcode-wasm-core".to_string()],
        "仓库内出现第二份 wasm-core 织体（唯一合法落点 `packages/bedcode-wasm-core`）——\
         两份机制面同名不同型，装配期 `defined twice` 或静默双写，且无既有测试会红"
    );
}
