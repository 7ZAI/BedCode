//! 工作区 git 查询域（票 04） — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/file_browse/ops.rs 迁出）

use super::*;

use crate::file_browse::source::tests::MockGit;
use std::collections::HashMap;

/// 分支名白名单（宿主 git_controller 用例逐条对齐）
#[test]
fn branch_name_whitelist_matches_host() {
    for name in [
        "main",
        "feature/login",
        "v2.0.1",
        "hotfix_1",
        "release/2026-09",
        "a",
    ] {
        assert!(is_valid_branch_name(name), "合法分支名 {name} 应通过白名单");
    }
    for name in [
        "main;rm -rf /",
        "main && echo pwned",
        "--upload-pack=touch /tmp/x",
        "$(id)",
        "main`id`",
        "a b",
        "feature\\login",
        "main|sh",
    ] {
        assert!(!is_valid_branch_name(name), "注入形态 {name} 必须被拒绝");
    }
    // 空串显式拒绝（all() 对空迭代器恒真——变异点：去掉 is_empty 即假绿）
    assert!(!is_valid_branch_name(""));
    // 控制字符不在白名单（Unicode 字母属 alphanumeric 白名单，argv 执行无 shell 风险）
    assert!(!is_valid_branch_name("main\u{0000}"));
    assert!(!is_valid_branch_name("main\u{001b}"));
}
/// 分支列表：show-current 首行 + branch --list 剥 * 前缀滤空行
#[test]
fn git_branches_parses_host_way() {
    let (root, _dir) = temp_workspace();
    let cwd = root.to_string_lossy().to_string();
    let git = MockGit::new(HashMap::from([
        (
            (cwd.clone(), "branch --show-current".to_string()),
            "dev\n".to_string(),
        ),
        (
            (cwd.clone(), "branch --list".to_string()),
            "* main\ndev\n\n  feature/x  \n".to_string(),
        ),
    ]));
    let v = git_branches(&git, &cwd).expect("branches");
    assert_eq!(
        v,
        serde_json::json!({
            "currentBranch": "dev",
            "branches": ["main", "dev", "feature/x"],
            "isGitRepo": true,
        }),
        "* 前缀剥离 + 空行滤除 + trim"
    );
    // 调用顺序：先 show-current 后 --list（宿主同序）
    let calls = git.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].1, vec!["branch", "--show-current"]);
    assert_eq!(calls[1].1, vec!["branch", "--list"]);
}
/// 空 show-current（unborn HEAD / detached）→ currentBranch null（宿主同格）
#[test]
fn git_branches_empty_current_is_null() {
    let (root, _dir) = temp_workspace();
    let cwd = root.to_string_lossy().to_string();
    let git = MockGit::new(HashMap::from([
        (
            (cwd.clone(), "branch --show-current".to_string()),
            String::new(),
        ),
        ((cwd.clone(), "branch --list".to_string()), String::new()),
    ]));
    let v = git_branches(&git, &cwd).expect("branches");
    assert_eq!(v["currentBranch"], serde_json::Value::Null);
    assert_eq!(v["branches"], serde_json::json!([]));
    assert_eq!(v["isGitRepo"], true);
}
/// status 计数：porcelain 非空行数 → hasChanges/changedCount
#[test]
fn git_status_counts_porcelain_lines() {
    let (root, _dir) = temp_workspace();
    let cwd = root.to_string_lossy().to_string();
    let dirty = MockGit::new(HashMap::from([(
        (cwd.clone(), "status --porcelain".to_string()),
        " M a.rs\n?? b.txt\n".to_string(),
    )]));
    let v = git_status(&dirty, &cwd).expect("status");
    assert_eq!(
        v,
        serde_json::json!({ "hasChanges": true, "changedCount": 2 })
    );

    let clean = MockGit::new(HashMap::new());
    let v = git_status(&clean, &cwd).expect("status");
    assert_eq!(
        v,
        serde_json::json!({ "hasChanges": false, "changedCount": 0 })
    );
}
/// checkout：白名单前置拒绝（不经 git，文案逐字）→ 成功返回目标分支
#[test]
fn git_checkout_validates_then_reports_branch() {
    let (root, _dir) = temp_workspace();
    let cwd = root.to_string_lossy().to_string();
    let git = MockGit::new(HashMap::new());

    let err = git_checkout(&git, &cwd, "main;rm -rf /").expect_err("白名单拒绝");
    assert_eq!(err, "Invalid input: Invalid branch name: main;rm -rf /");
    assert!(
        git.calls.lock().unwrap().is_empty(),
        "白名单拒绝不得启动 git 进程"
    );

    let branch = git_checkout(&git, &cwd, "feature/x").expect("checkout");
    assert_eq!(branch, "feature/x");
    let calls = git.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].1, vec!["checkout", "feature/x"]);
}
/// 非零退出 → 500 文案与宿主 AppError::Internal Display 逐字一致
#[test]
fn git_checkout_maps_nonzero_exit_to_host_message() {
    let (root, _dir) = temp_workspace();
    let cwd = root.to_string_lossy().to_string();
    let git = FailingGit {
        stderr: "error: pathspec 'nope' did not match any file(s) known to git\n".to_string(),
    };
    let err = git_checkout(&git, &cwd, "nope").expect_err("非零退出");
    assert_eq!(
        err,
        "Internal error: git checkout failed: error: pathspec 'nope' did not match any file(s) known to git\n",
        "stderr 原样拼接（含尾部换行，宿主同形）"
    );

    let empty = FailingGit {
        stderr: String::new(),
    };
    let err = git_checkout(&empty, &cwd, "nope").expect_err("空 stderr");
    assert_eq!(err, "Internal error: git checkout failed");
}
