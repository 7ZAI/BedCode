//! 工作区 git 查询域（票 04） — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/file_browse/ops.rs 迁出）

use super::*;

use crate::file_browse::source::tests::MockGit;

/// diff 树命令失败 → 文案带宿主前缀（run_git_lines 映射）
#[test]
fn git_failure_messages_carry_host_internal_prefix() {
    let (root, _dir) = temp_workspace();
    let cwd = root.to_string_lossy().to_string();
    let git = FailingGit {
        stderr: "fatal: not a git repository\n".to_string(),
    };
    let err = run_git_lines(&git, &cwd, &["status", "--porcelain"]).expect_err("失败");
    assert_eq!(
        err,
        "Internal error: git command failed: fatal: not a git repository\n"
    );

    let broken = BrokenGit;
    let err = run_git_lines(&broken, &cwd, &["status"]).expect_err("spawn 失败");
    assert!(
        err.starts_with("Internal error: Failed to execute git: "),
        "got: {err}"
    );

    let err = file_diff(&broken, &cwd, "a.rs").expect_err("diff spawn 失败");
    assert!(
        err.starts_with("Internal error: Failed to execute git diff: "),
        "got: {err}"
    );
}
