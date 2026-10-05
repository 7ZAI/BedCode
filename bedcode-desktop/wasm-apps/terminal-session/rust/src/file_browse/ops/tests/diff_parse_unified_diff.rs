//! 单文件 diff 解析（与宿主 parse_unified_diff 对照） — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/file_browse/ops.rs 迁出）

use super::*;

#[test]
fn parse_unified_diff_matches_host_semantics() {
    let diff = "\
diff --git a/main.rs b/main.rs
index 123..456 100644
--- a/main.rs
+++ b/main.rs
@@ -1,5 +1,6 @@
 use std::io;
+fn new_fn() {}
-fn old_fn() {}
 fn unchanged() {}
\\ No newline at end of file
";
    let lines = parse_unified_diff(diff);
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0]["type"], "context");
    assert_eq!(lines[0]["content"], "use std::io;");
    assert_eq!(lines[0]["oldLineNo"], 1);
    assert_eq!(lines[0]["newLineNo"], 1);
    assert_eq!(lines[1]["type"], "added");
    assert_eq!(lines[1]["content"], "fn new_fn() {}");
    assert_eq!(lines[1]["oldLineNo"], serde_json::Value::Null);
    assert_eq!(lines[1]["newLineNo"], 2);
    assert_eq!(lines[2]["type"], "removed");
    assert_eq!(lines[2]["oldLineNo"], 2);
    assert_eq!(lines[2]["newLineNo"], serde_json::Value::Null);
    assert_eq!(lines[3]["type"], "context");
    assert_eq!(lines[3]["oldLineNo"], 3);
    assert_eq!(lines[3]["newLineNo"], 3);
}
#[test]
fn parse_hunk_header_variants() {
    assert_eq!(parse_hunk_header("@@ -1 +1 @@"), Some((1, 1)));
    assert_eq!(parse_hunk_header("@@ -1,5 +1,6 @@"), Some((1, 1)));
    assert_eq!(parse_hunk_header("@@ -12 +34,2 @@"), Some((12, 34)));
    assert_eq!(parse_hunk_header("not a hunk"), None);
}
