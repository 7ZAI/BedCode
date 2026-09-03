//! Windows manifest 注入验证（对齐桌面端 build_manifest_smoke.rs）
//!
//! 移动端 build.rs 对 Windows 目标注入 common-controls v6 manifest
//! （/MANIFESTINPUT），保证本包所有链接产物（含 tests/ 下测试二进制）
//! 不加载 System32 comctl32 5.82 兼容桩。若注入失效，测试进程启动即崩溃
//! （0xc0000139），本文件的恒真断言不会执行——存在本身即验证。

#[test]
fn manifest_injection_smoke() {
    assert!(true);
}
