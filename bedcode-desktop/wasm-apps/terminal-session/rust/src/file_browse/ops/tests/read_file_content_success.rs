//! 文件内容 — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/file_browse/ops.rs 迁出）

use super::*;

use crate::file_browse::source::tests::MockFs;

#[test]
fn read_file_content_success_and_errors() {
    let (root, _dir) = temp_workspace();
    let fs = MockFs;
    let ok_file = root.join("ok.txt");
    std::fs::write(&ok_file, "hello").unwrap();
    let (content, name) = read_file_content(&fs, &root.to_string_lossy(), "ok.txt")
        .unwrap()
        .unwrap();
    assert_eq!(content, "hello");
    assert_eq!(name, "ok.txt");

    // 不存在 → 404
    let err = read_file_content(&fs, &root.to_string_lossy(), "ghost.txt")
        .unwrap()
        .unwrap_err();
    assert_eq!(err.0, 404);
    // 越界 → 403（先创建外部文件：宿主对「不存在路径」先答 404，越过 root 的
    // 存在文件才落到 is_within_root 判定）
    let outside = root.parent().unwrap().join("evil.txt");
    std::fs::write(&outside, "evil").unwrap();
    let err = read_file_content(&fs, &root.to_string_lossy(), "../evil.txt")
        .unwrap()
        .unwrap_err();
    assert_eq!(err.0, 403);
    // 目录 → 400
    std::fs::create_dir_all(root.join("adir")).unwrap();
    let err = read_file_content(&fs, &root.to_string_lossy(), "adir")
        .unwrap()
        .unwrap_err();
    assert_eq!(err.0, 400);
    assert_eq!(err.1, "Path is not a file");
}
