//! exclude 过滤 — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/file_browse/ops.rs 迁出）

use super::*;

#[test]
fn exclude_filters_match_host_semantics() {
    let filters = build_exclude_filters(&["node_modules".to_string(), "src/generated".to_string()]);
    assert_eq!(filters.len(), 2);
    // Name 匹配任意层级同名目录
    assert!(should_exclude("", "node_modules", &filters));
    assert!(should_exclude("a/b", "node_modules", &filters));
    // Path 匹配 parent + name
    assert!(should_exclude("src", "generated", &filters));
    assert!(!should_exclude("lib", "generated", &filters));
    assert!(!should_exclude("", "generated", &filters));
}
