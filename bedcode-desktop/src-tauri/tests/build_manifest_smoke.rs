//! 构建清单冒烟测试
//!
//! 仅用于确保本包存在 test 目标：`build.rs` 通过 `cargo:rustc-link-arg-tests`
//! 为测试二进制注入 comctl32 v6 清单，而该指令要求包内定义了 test 目标
//! （本包 `tests/` 原为空、无 `[[test]]`，否则 `cargo build --bin` 会报
//! "does not have a test target"）。保留一个最小测试用例以满足该约束。

/// 恒真断言，保证 test 目标始终存在。
#[test]
fn build_manifest_smoke() {
    assert!(true);
}
