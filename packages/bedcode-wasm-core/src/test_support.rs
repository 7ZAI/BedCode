//! 测试基建（常编译公开，lib 集成测试消费；来源见模块头注释）。
//!
//! 单形态（2026-10-13）后**运行时无形态之分**，但测试基建仍按宿主消费面分两子模块
//! （这是**测试面**分叉，不是运行时形态）：
//!
//! - **桌面**（`desktop-assembly`）：[`common`] —— setup_wasm_runtime /
//!   build_host_ctx_at / TestInstanceDispatcher / SDK 夹具字节读取…
//! - **移动**（`test-support`）：[`bare`] —— fork 自旧 test_support（MockPorts /
//!   夹具组件构建器 / mock_plugin_ws / build_component_state…），移动 src-tauri
//!   测试消费；统一后内部引用已改指统一运行时壳（`LoadedWasmPlugin` / 统一
//!   `add_to_linker`）。

#[cfg(feature = "desktop-assembly")]
mod common;

#[cfg(feature = "desktop-assembly")]
pub use common::*;

// bare 分支符号过壳（build_host_ctx / build_test_component /
// build_terminal_session_component / MockPorts…）：移动 src-tauri 测试面经
// `test_support::X` 引用；门控 = `test-support`（移动宿主 dev-dependencies 开启）
#[cfg(feature = "test-support")]
pub mod bare;

#[cfg(feature = "test-support")]
pub use bare::*;

/// 运行时 Engine 转发（统一壳 `engine()` 是 pub(crate)，跨 crate 消费经此面）
///
/// 引擎生效配置断言用（`Config` 不可回读，`Engine` 才完整）；无条件（desktop-assembly
/// 与 test-support 两形态都编译）——桌面与移动宿主测试共用同一转发面。
pub fn runtime_engine(runtime: &crate::manager::runtime::WasmRuntime) -> &wasmtime::Engine {
    runtime.engine()
}

