//! 测试基建（形态分叉壳，票 06 批次 03）：
//!
//! - **桌面**（`desktop-host`）：[`desktop`] —— setup_wasm_runtime /
//!   build_host_ctx_at / TestInstanceDispatcher / SDK 夹具字节读取…
//! - **移动**（`mobile-host` + `test-support`）：[`mobile`] —— fork
//!   test_support.rs 原样迁入（MockPorts / 夹具组件构建器 / mock_plugin_ws）

#[cfg(feature = "desktop-host")]
mod desktop;

#[cfg(feature = "desktop-host")]
pub use desktop::*;

// mobile 分支符号过壳（build_host_ctx / build_test_component /
// build_terminal_session_component / MockPorts…）：src-tauri 测试面经
// `test_support::X` 引用，形态切换零改动
#[cfg(all(feature = "mobile-host", feature = "test-support"))]
pub mod mobile;

#[cfg(all(feature = "mobile-host", feature = "test-support"))]
pub use mobile::*;
