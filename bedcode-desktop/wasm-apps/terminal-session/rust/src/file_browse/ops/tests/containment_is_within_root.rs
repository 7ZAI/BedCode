//! containment（安全红线，与宿主 is_within_root 对照） — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/file_browse/ops.rs 迁出）

use super::*;

use crate::file_browse::source::tests::MockFs;

#[test]
fn traversal_via_parent_chain_rejected() {
    let (root, _dir) = temp_workspace();
    let sub = root.join("sub");
    std::fs::create_dir_all(&sub).expect("create sub");
    let outside = root.parent().unwrap().join("evil");
    std::fs::create_dir_all(&outside).expect("create outside");
    let fs = MockFs;

    assert!(
        !is_within_root(&fs, &root.to_string_lossy(), &outside.to_string_lossy()),
        "同级目录越界必须拒绝"
    );
    let traversal = sub.join("../../evil");
    assert!(
        !is_within_root(&fs, &root.to_string_lossy(), &traversal.to_string_lossy()),
        "../ 穿越必须拒绝"
    );
}
#[test]
fn within_root_allowed() {
    let (root, _dir) = temp_workspace();
    let child = root.join("a").join("b.txt");
    std::fs::create_dir_all(child.parent().unwrap()).expect("create parent");
    std::fs::write(&child, b"x").expect("write file");
    let fs = MockFs;
    assert!(is_within_root(
        &fs,
        &root.to_string_lossy(),
        &child.to_string_lossy()
    ));
    assert!(is_within_root(
        &fs,
        &root.to_string_lossy(),
        &root.to_string_lossy()
    ));
}
#[test]
fn nonexistent_path_rejected_without_panic() {
    let root = temp_root();
    let fs = MockFs;
    let ghost = root.join("nope").join("ghost.txt");
    assert!(!is_within_root(
        &fs,
        &root.to_string_lossy(),
        &ghost.to_string_lossy()
    ));
}
/// 组件级 starts_with（不匹配前缀相似目录）
#[test]
fn path_starts_with_is_component_aware() {
    assert!(path_starts_with("/srv/app/src", "/srv/app"));
    assert!(path_starts_with("/srv/app", "/srv/app"));
    assert!(!path_starts_with("/srv/app2", "/srv/app"));
    assert!(!path_starts_with("/srv/app2/x", "/srv/app"));
    // Windows 反斜杠 canonical 输出
    assert!(path_starts_with(r"C:\work\src", r"C:\work"));
    assert!(!path_starts_with(r"C:\work2", r"C:\work"));
}
/// symlink 逃逸：canonicalize 解析后越界 → 拒绝
#[cfg(unix)]
#[test]
fn symlink_escape_rejected() {
    let (root, _dir) = temp_workspace();
    let outside = root.parent().unwrap().join("secret.txt");
    std::fs::write(&outside, b"secret").expect("write outside");
    std::os::unix::fs::symlink(&outside, root.join("link.txt")).expect("symlink");
    let fs = MockFs;
    assert!(
        !is_within_root(
            &fs,
            &root.to_string_lossy(),
            &root.join("link.txt").to_string_lossy()
        ),
        "symlink 指向 root 外 → 拒绝"
    );
}
