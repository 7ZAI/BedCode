//! 辅助 — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/file_browse/ops.rs 迁出）

use super::*;

use crate::file_browse::source::tests::MockFs;

#[test]
fn join_path_and_file_name() {
    assert_eq!(join_path("/srv/app", "src/main.rs"), "/srv/app/src/main.rs");
    assert_eq!(join_path("/srv/app/", "src"), "/srv/app/src");
    assert_eq!(join_path("/srv/app", ""), "/srv/app");
    assert_eq!(join_path("/srv/app", "."), "/srv/app/.");
    assert_eq!(file_name_of("a/b/c.txt"), "c.txt");
    assert_eq!(file_name_of("c.txt"), "c.txt");
    assert_eq!(file_name_of("a\\b\\d.txt"), "d.txt");
    assert_eq!(file_name_of(""), "");
    assert!(is_absolute_path("/abs"));
    assert!(is_absolute_path(r"C:\win"));
    assert!(is_absolute_path("\\\\server\\share"));
    assert!(!is_absolute_path("rel/path"));
}
/// 类型占位：FsStat 形状锁定（与宿主 stat 输出一致）
#[test]
fn stat_shape_matches_host() {
    let (root, _dir) = temp_workspace();
    let fs = MockFs;
    std::fs::write(root.join("f.txt"), "12345").unwrap();
    let stat: bedcode_plugin_api::host::FsStat = fs
        .stat(&root.join("f.txt").to_string_lossy())
        .unwrap()
        .unwrap();
    assert_eq!(stat.size, 5);
    assert!(stat.is_file);
    assert!(!stat.is_dir);
}
