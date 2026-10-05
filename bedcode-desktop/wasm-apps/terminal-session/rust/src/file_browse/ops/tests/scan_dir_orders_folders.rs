//! 目录树扫描 — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/file_browse/ops.rs 迁出）

use super::*;

use crate::file_browse::source::tests::MockFs;

/// 文件夹在前、文件在后；各自 name 大小写不敏感排序（与宿主一致）
#[test]
fn scan_dir_orders_folders_first_case_insensitive() {
    let root = temp_root();
    std::fs::create_dir_all(root.join("zeta")).unwrap();
    std::fs::create_dir_all(root.join("Alpha")).unwrap();
    std::fs::write(root.join("beta.txt"), "b").unwrap();
    std::fs::write(root.join("Gamma.txt"), "g").unwrap();
    let fs = MockFs;
    let tree = scan_dir(
        &fs,
        &root.to_string_lossy(),
        &root.to_string_lossy(),
        &[],
        0,
    )
    .unwrap();
    let names: Vec<&str> = tree.iter().map(|n| n["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        vec!["Alpha", "zeta", "beta.txt", "Gamma.txt"],
        "文件夹在前 + 大小写不敏感（beta < Gamma）"
    );
}
/// 排除目录不进树；symlink 跳过；文件夹 children 恒有、文件 children 省略
#[test]
fn scan_dir_applies_excludes_and_node_shapes() {
    let root = temp_root();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src").join("main.rs"), "fn main(){}").unwrap();
    std::fs::create_dir_all(root.join("node_modules")).unwrap();
    std::fs::write(root.join("node_modules").join("x.js"), "x").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("/nonexistent", root.join("deadlink")).unwrap();

    let fs = MockFs;
    let filters = build_exclude_filters(&["node_modules".to_string()]);
    let tree = scan_dir(
        &fs,
        &root.to_string_lossy(),
        &root.to_string_lossy(),
        &filters,
        0,
    )
    .unwrap();
    assert_eq!(tree.len(), 1, "node_modules 排除 + symlink 跳过");
    let src = &tree[0];
    assert_eq!(src["nodeType"], "folder");
    assert_eq!(src["path"], "src");
    assert!(src.get("children").is_some(), "文件夹 children 恒有");
    assert_eq!(src["children"][0]["name"], "main.rs");
    assert!(
        src["children"][0].get("children").is_none(),
        "文件 children 省略"
    );
}
/// 深度上限（宿主 MAX_DEPTH=20 语义）：超出层返回空
#[test]
fn scan_dir_stops_at_max_depth() {
    let root = temp_root();
    let mut cur = root.clone();
    for i in 0..25 {
        cur = cur.join(format!("d{i}"));
        std::fs::create_dir_all(&cur).unwrap();
    }
    let fs = MockFs;
    let tree = scan_dir(
        &fs,
        &root.to_string_lossy(),
        &root.to_string_lossy(),
        &[],
        0,
    )
    .unwrap();
    assert_eq!(tree.len(), 1);
    // 第 21 层起返回空（depth=20 可扫，depth=21 空）
    assert!(scan_dir(
        &fs,
        &root.to_string_lossy(),
        &root.to_string_lossy(),
        &[],
        21
    )
    .unwrap()
    .is_empty());
}
/// 单层扫描：不递归、文件夹 children 省略（与 file-tree-children 形状一致）
#[test]
fn scan_dir_single_level_is_non_recursive() {
    let root = temp_root();
    std::fs::create_dir_all(root.join("a").join("deep")).unwrap();
    std::fs::write(root.join("a").join("f.txt"), "x").unwrap();
    let fs = MockFs;
    let children = scan_dir_single_level(
        &fs,
        &root.to_string_lossy(),
        &root.join("a").to_string_lossy(),
        &[],
    )
    .unwrap();
    assert_eq!(children.len(), 2, "a 下直系两项：deep 文件夹 + f.txt 文件");
    assert_eq!(children[0]["name"], "deep");
    assert_eq!(children[0]["nodeType"], "folder");
    assert!(
        children[0].get("children").is_none(),
        "单层扫描文件夹 children 省略（未加载）"
    );
    assert_eq!(children[1]["name"], "f.txt");
    assert_eq!(children[1]["path"], "a/f.txt");
}
