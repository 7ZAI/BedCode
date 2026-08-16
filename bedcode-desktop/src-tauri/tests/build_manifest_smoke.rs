//! 构建清单冒烟测试
//!
//! 用于验证 build.rs 的 manifest 注入链路：`cargo:rustc-link-arg-tests` 要求包内
//! 定义了 test 目标（本包 tests/ 原为空、无 `[[test]]`，否则 `cargo build --bin` 会报
//! "does not have a test target"）。保留一个最小测试用例以满足该约束。
//!
//! 同时作为集成测试基线：其二进制若含 comctl32 v6 清单，说明 Windows 上
//! 测试二进制已可启动（lib 单元测试依赖 build.rs 的 cargo:rustc-link-arg 同机制）。

/// 恒真断言，保证 test 目标始终存在。
#[test]
fn build_manifest_smoke() {
    assert!(true);
}
