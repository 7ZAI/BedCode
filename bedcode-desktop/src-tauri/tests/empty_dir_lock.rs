//! 空目录残留锁（`foo.rs` + 空 `foo/` 双壳）
//!
//! ## 锁什么
//!
//! 一方源码目录里**不得存在空目录**。这些空目录几乎全是同一件事的残留：把
//! `foo/mod.rs` 拍平成同层兄弟文件 `foo.rs`（AGENTS.md §6「模块入口文件与目录同名，
//! 不用 `mod.rs`」的目标形态）时，旧目录没删掉，于是同一个模块在文件树上留下
//! `foo.rs` 与空 `foo/` 两层壳。
//!
//! ## 为什么必须上锁，而不是当一次性清理
//!
//! **git 对空目录完全不可见**：`git status` / `git diff` / 任何评审都看不到它们，
//! `.gitignore` 也不会记录。于是它们既不会被提交、也不会被清理，还能在磁盘上无声堆积
//! ——本锁写下时桌面宿主已有 21 个、移动端 11 个、能力 crate 3 个，全都没有任何痕迹。
//!
//! 三个具体代价：
//!
//! 1. **模块形态二义**：rustc 的模块解析只看 `foo.rs` 与 `foo/mod.rs`，两者都存在
//!    才会报 E0761。所以 `pty/pty_process.rs` + 空 `pty/pty_process/` 读起来像
//!    「目录模块」，实际是扁平兄弟文件；靠 `ls` / 文件树做判断的**人和 agent 都会
//!    答错**（本仓库的检索与评审链路正是这一类）。
//! 2. **误导后续落点**：有人（或 agent）对着这个空目录新建文件、或让工具生成
//!    `foo/mod.rs`，得到的是与预期不符的模块结构，或干脆 E0761。
//! 3. **掩盖成因**：目录结构变更是从 `mod.rs` 拍平来的，空壳就是那次变更的影子；
//!    影子留着，看不出「这棵树刚被重排过」。
//!
//! ## 为什么门禁在这里（而非 scripts/）
//!
//! CI 只跑两端 `cargo test`（`test.yml`）。锁写成 `src-tauri/tests/` 下的集成测试
//! 才真的进门禁；写成脚本就退化成手工项。双端一并扫、锁只写一份，与
//! `capabilities_lock.rs` 同一形态——「不检查的锁比没有锁更危险」。
//!
//! ## 覆盖面按约定推导，不逐 crate 枚举
//!
//! 见 [`SOURCE_ROOT_PATTERNS`]：列的是**「哪一层放 crate」的目录约定**，不是
//! crate 名单。新增 crate / 插件 / 能力 crate 时自动进覆盖面，不需要改本文件
//! （枚举名单会随每次新建 crate 而腐化，腐化即漏扫）。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// 一方源码根的约定清单：`*` 处放「那一层放 crate 的目录」的**直接子目录**。
///
/// - 定值项（无 `*`）：必须存在，缺失即锁失效并显性报错（不静默跳过）。
/// - 通配项：列出该目录下**含 `src/` 的直接子目录**；一个都没匹配到即锁失效
///   （说明路径写错了，本锁在空跑）。
///
/// 刻意**不含**：`.scratch/`（临时探针，§11 受保护路径）、`gen/android`
/// （生成产物）、任何 `target/` 与 `node_modules/`（构建/包管理器产物）。
const SOURCE_ROOT_PATTERNS: [&str; 10] = [
    // 桌面端：宿主 crate（模块树主战场）
    "bedcode-desktop/src-tauri/src",
    // 桌面端：能力 / 传输 / SDK crate
    "bedcode-desktop/packages/*/src",
    // 桌面端：wasm 应用（业务主场）
    "bedcode-desktop/wasm-apps/*/src",
    // 桌面端：前端
    "bedcode-desktop/src",
    // 移动端：宿主 crate
    "bedcode-mobile/src-tauri/src",
    // 移动端：插件
    "bedcode-mobile/plugins/*/src",
    // 移动端：SDK / 测试插件
    "bedcode-mobile/packages/*/src",
    // 移动端：前端
    "bedcode-mobile/src",
    // 仓库根：机制内核 crate
    "packages/*/src",
    // 跨端契约测试 crate
    "cross-end-tests/src",
];

/// 遍历时跳过的目录名（一方源码里不该出现；命中即视为非源码，不参与判定）
const SKIP_DIR_NAMES: [&str; 5] = ["node_modules", "target", "gen", "dist", "build"];

/// 仓库根（`bedcode-desktop/src-tauri` 的两级上级）
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .expect("src-tauri 的两级上级应是仓库根")
}

/// 把一条约定展开成实际存在的源码根
fn expand_pattern(root: &Path, pattern: &str) -> Vec<PathBuf> {
    // 通配形态：`*` 落在**父目录**这一段（`a/*/src`），不是末段。
    // 这里用 `rsplit_once("*/")` 而不是 `rsplit_once('/')`：后者会把 `a/*/src`
    // 切成 (父=`a/*`, 末=`src`) 而误走定值分支，去找一个字面量名为 `*` 的目录。
    if let Some((parent_rel, leaf)) = pattern.rsplit_once("*/") {
        assert_eq!(leaf, "src", "源码根约定的通配形态未定义：{pattern}（只支持 `*/src`）");
        let parent = root.join(parent_rel);
        assert!(
            parent.is_dir(),
            "源码根约定的父目录缺失：{}（{pattern}）—— 空目录锁失效",
            parent.display()
        );

        let mut found: Vec<PathBuf> = fs::read_dir(&parent)
            .unwrap_or_else(|e| panic!("读取 {} 失败：{e}", parent.display()))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir() && !is_skipped(p))
            .map(|p| p.join("src"))
            .filter(|p| p.is_dir())
            .collect();
        found.sort();
        assert!(
            !found.is_empty(),
            "{pattern} 一个子目录都没匹配到（{}/ 下无含 src/ 的 crate）—— 空目录锁失效",
            parent.display()
        );
        return found;
    }

    // 定值形态：必须存在，缺失即锁失效
    assert!(pattern.contains('/'), "源码根约定缺斜杠，无法推导：{pattern}");
    let dir = root.join(pattern);
    assert!(
        dir.is_dir(),
        "源码根缺失：{} —— 空目录锁失效（覆盖面对不上仓库现状，锁会空跑）",
        dir.display()
    );
    vec![dir]
}

/// 目录是否应当跳过（点目录 / 构建产物 / 包管理器产物）
fn is_skipped(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.starts_with('.') || SKIP_DIR_NAMES.contains(&name))
}

/// 目录是否为空（只含点文件如 `.gitkeep` 视为**非空**——那是显式占位，与残留不同）
fn is_empty_dir(path: &Path) -> bool {
    fs::read_dir(path)
        .map(|mut it| it.next().is_none())
        .unwrap_or_else(|e| panic!("读取目录 {} 失败：{e}", path.display()))
}

/// 收集 `root` 下的空目录（深度优先；跳过 `SKIP_DIR_NAMES` 与点目录）
fn collect_empty_dirs(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).unwrap_or_else(|e| panic!("读取目录 {} 失败：{e}", dir.display()));
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() && !is_skipped(&path) {
                stack.push(path);
            }
        }
        // 非递归根自身不判（源码根本来就含文件，判了也没意义）
        if dir != root && is_empty_dir(&dir) {
            found.push(dir);
        }
    }

    found.sort();
    found
}

/// 收齐全部被覆盖的源码根（去重 + 排序）
fn covered_source_roots() -> Vec<PathBuf> {
    let root = repo_root();
    let mut roots: BTreeSet<PathBuf> = BTreeSet::new();
    for pattern in SOURCE_ROOT_PATTERNS {
        for dir in expand_pattern(&root, pattern) {
            roots.insert(dir);
        }
    }
    roots.into_iter().collect()
}

/// 相对于仓库根的展示路径（报错信息可读）
fn display_relative(path: &Path) -> String {
    let root = repo_root();
    path.strip_prefix(&root).unwrap_or(path).display().to_string()
}

/// 主锁：一方源码目录不得存在空目录
#[test]
fn no_empty_source_dirs_anywhere() {
    let roots = covered_source_roots();
    let residue: Vec<String> = roots
        .iter()
        .flat_map(|root| collect_empty_dirs(root))
        .map(|dir| format!("  - {}", display_relative(&dir)))
        .collect();

    assert!(
        residue.is_empty(),
        "一方源码里出现 {} 个空目录（git 完全看不见它们，评审与 CI 都不会报）：\n{}\n\
         成因几乎都是把 `foo/mod.rs` 拍平成兄弟文件 `foo.rs` 后漏删旧目录，于是同一模块留下双壳。\n\
         处置：`rmdir` 掉（空目录对 rustc 惰性——模块解析只看 `foo.rs` / `foo/mod.rs`，\n\
         无 `#[path]` / `include!` 指向它们时删掉不影响编译）。\n\
         确实需要占位目录时放一个 `.gitkeep`，本锁按「非空」计。",
        residue.len(),
        residue.join("\n")
    );
}

/// 正例自检：扫描器确实能认出空目录（防「锁自身永远绿」的假阴性）
///
/// 在**临时目录**里造树，不往仓库里留东西。
#[test]
fn empty_dir_scanner_detects_a_real_empty_dir() {
    let tmp = std::env::temp_dir().join(format!("bedcode-empty-dir-lock-{}", std::process::id()));
    // 清掉上一轮可能残留的同名树（正常路径下不存在）
    let _ = fs::remove_dir_all(&tmp);

    let empty_leaf = tmp.join("mod_with_residue"); // 模拟 `foo.rs` 旁的空 `foo/`
    let filled = tmp.join("mod_ok");
    let dot_only = tmp.join("placeholder");
    let dot_dir = tmp.join(".hidden");
    let build_dir = tmp.join("target");
    fs::create_dir_all(&empty_leaf).expect("建临时树");
    fs::create_dir_all(&filled).expect("建临时树");
    fs::write(filled.join("mod_ok.rs"), "// 占位\n").expect("写临时文件");
    fs::create_dir_all(&dot_only).expect("建临时树");
    fs::write(dot_only.join(".gitkeep"), "").expect("写占位文件");
    fs::create_dir_all(&dot_dir).expect("建临时树");
    fs::create_dir_all(build_dir).expect("建临时树");

    let found = collect_empty_dirs(&tmp);
    let found_display: Vec<String> = found.iter().map(|p| display_relative(p)).collect();

    let _ = fs::remove_dir_all(&tmp);

    assert_eq!(
        found_display,
        vec![display_relative(&empty_leaf)],
        "扫描器必须且只能认出那个空目录：点文件目录算非空（显式占位）、点目录与 \
         `target/` 算跳过"
    );
}

/// 正例自检：临时树清理干净了（否则本用例自己会污染下一次运行）
#[test]
fn empty_dir_scanner_temp_tree_is_cleaned_up() {
    let tmp = std::env::temp_dir().join(format!("bedcode-empty-dir-lock-cleanup-{}", std::process::id()));
    fs::create_dir_all(tmp.join("leaf")).expect("建临时树");
    collect_empty_dirs(&tmp);
    assert!(tmp.is_dir(), "用例自身应先确认临时树被造出来（否则下面的断言是空跑）");
    fs::remove_dir_all(&tmp).expect("清临时树");
    assert!(!tmp.exists(), "临时树必须在断言前删掉，不能留在 /tmp 里反复堆积");
}

/// 防空跑：覆盖面必须真的落到源码文件上，且双端都在内
#[test]
fn empty_dir_scan_covers_both_ends_and_reaches_source_files() {
    let roots = covered_source_roots();

    for end_root in ["bedcode-desktop/src-tauri/src", "bedcode-mobile/src-tauri/src"] {
        let want = repo_root().join(end_root);
        assert!(
            roots.contains(&want),
            "覆盖面缺 {end_root} —— 两端源码形状不同，只扫一端的锁等于半边锁"
        );
    }

    for root in &roots {
        let file_count = fs::read_dir(root)
            .unwrap_or_else(|e| panic!("读取 {} 失败：{e}", root.display()))
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .count();
        assert!(
            file_count > 0,
            "源码根 {} 里一个文件都没有 —— 路径写错或 crate 已迁走，本锁在该根上空跑",
            root.display()
        );
    }
}
