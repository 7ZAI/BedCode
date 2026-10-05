//! git diff 树 — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/file_browse/ops.rs 迁出）

use super::*;

use crate::file_browse::source::tests::MockGit;
use std::collections::HashMap;

#[test]
fn diff_file_tree_merges_and_builds_tree() {
    let root = temp_root().to_string_lossy().to_string();
    let mut outputs = HashMap::new();
    outputs.insert(
        (root.clone(), "diff --name-only".to_string()),
        "src/main.rs\nCargo.toml".to_string(),
    );
    outputs.insert(
        (root.clone(), "diff --cached --name-only".to_string()),
        "src/main.rs".to_string(),
    );
    outputs.insert(
        (
            root.clone(),
            "ls-files --others --exclude-standard".to_string(),
        ),
        "src/generated/code.rs\nnew.txt".to_string(),
    );
    let git = MockGit::new(outputs);
    let filters = build_exclude_filters(&["generated".to_string()]);
    let tree = diff_file_tree(&git, &root, &filters).unwrap();

    let names: Vec<&str> = tree.iter().map(|n| n["name"].as_str().unwrap()).collect();
    // 去重 src/main.rs；generated 目录被排除；BTreeMap 字节序 + 文件夹在前
    assert_eq!(names, vec!["src", "Cargo.toml", "new.txt"]);
    let src = &tree[0];
    assert_eq!(src["nodeType"], "folder");
    assert_eq!(src["children"][0]["name"], "main.rs");
}
/// T-G05：git 失败路径（非零退出 / stderr 诊断）经注入后真正被测——
/// 旧 MockGit 恒成功，失败行为从未验证
#[test]
fn diff_file_tree_surfaces_injected_git_failure() {
    let root = temp_root().to_string_lossy().to_string();
    // 第一条 diff --name-only 注入非零退出 + stderr（宿主 run_git_command
    // 失败路径：500 + Internal error 前缀）
    let git = MockGit::fail_with(
        &root,
        &["diff", "--name-only"],
        bedcode_plugin_api::host::ProcessSyncResult {
            exit_code: Some(128),
            stdout: String::new(),
            stderr: "fatal: not a git repository".to_string(),
            timed_out: false,
        },
    );
    let err = diff_file_tree(&git, &root, &[]).expect_err("注入失败必须显性报错");
    assert!(
        err.contains("Internal error: git command failed"),
        "got: {err}"
    );
    assert!(
        err.contains("fatal: not a git repository"),
        "stderr 透传: {err}"
    );
    // 未命中的命令返回成功（失败注入按条命中，不影响其余）
    let ok = git
        .run(&root, &["diff", "--name-only"])
        .expect("命中失败注入");
    assert_eq!(ok.exit_code, Some(128));
    let ok = git
        .run(&root, &["status", "--porcelain"])
        .expect("未命中走默认成功");
    assert_eq!(ok.exit_code, Some(0));
}
