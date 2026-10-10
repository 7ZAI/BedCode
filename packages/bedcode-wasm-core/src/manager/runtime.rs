//! WASM 插件运行时（形态分叉壳，票 06 批次 03 · ADR 0045 D6）
//!
//! 「只有一份 wasm-core，双端核心机制完全一致」的终局下，装配树按宿主形态
//! cfg 挂载（spec D5/D6）：
//!
//! - **桌面**（`desktop-host`）：[`desktop`] —— wasmtime Engine/Linker/Store
//!   生命周期 + WASI p3 装配 + `component`（桌面 WIT 绑定，host-* WIT impl
//!   面在 `crate::host_api` 各域）
//! - **移动**（`mobile-host`）：[`mobile`] + [`mobile_component`] + [`host_impl`]
//!   —— 自 `bedcode-mobile/packages/bedcode-wasm-core`（fork，票 17）迁入的
//!   移动装配面（17 组 Host 接线 + host_impl 16 域 + 无 WASI，移动插件为
//!   wasm32-unknown-unknown）；`WasmPluginState` 统一用 host-kit 版
//!   （`wasi-store` 关闭形态，票 06 批次 02）
//!
//! 机制面（Engine 构建参数 / 燃料 / AOT 缓存键等共享工具）随各形态分支自持，
//! 同名工具的合一裁决在迁移时逐项落（`aot_cache_key` 等真差异记录在案）。

// ==================== 桌面装配树（desktop-host） ====================

#[cfg(feature = "desktop-host")]
mod desktop;

#[cfg(all(test, feature = "desktop-host"))]
pub(crate) use desktop::fixture_build;
#[cfg(all(test, feature = "desktop-host"))]
pub(crate) use desktop::fixture_target;
// glob 过壳：desktop 分支的全部 pub 符号（WasmHostContext / WasmPluginState 引用 /
// InstanceMeta / LoadedWasmPlugin / WasmRuntime / OptionalExports …）保持
// `crate::manager::runtime::X` 路径不变（宿主与 crate 内引用零改动）
#[cfg(feature = "desktop-host")]
pub use desktop::*;

// ==================== 移动装配树（mobile-host，fork 迁入） ====================

#[cfg(feature = "mobile-host")]
pub mod mobile;

// glob 对齐 desktop 分支口径：mobile 分支全部 pub 符号过壳（WasmPluginState /
// WasmRuntime / LoadedComponentPlugin / WasmHostContext…），且 `host_impl` 模块
// 别名到壳顶层——移动 src-tauri 的 `plugin::wasm_runtime` 整模块垫片（pub use
// manager::runtime）经此解析 `wasm_runtime::host_impl::register_host_service` 等
// 宿主消费路径（fork 原状零改动）
#[cfg(feature = "mobile-host")]
pub use mobile::{WasmPluginState, WasmRuntime, LoadedComponentPlugin, WasmHostContext};
#[cfg(feature = "mobile-host")]
pub use mobile::host_impl;
