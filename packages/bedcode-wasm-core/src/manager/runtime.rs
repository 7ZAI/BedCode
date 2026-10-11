//! WASM 插件运行时（形态分叉壳，票 06 批次 03 · ADR 0045 D6）
//!
//! 「只有一份 wasm-core，双端核心机制完全一致」的终局下，装配树按宿主形态
//! cfg 挂载（spec D5/D6）：
//!
//! - **桌面**（`common-host`）：[`common`] —— wasmtime Engine/Linker/Store
//!   生命周期 + WASI p3 装配 + `component`（桌面 WIT 绑定，host-* WIT impl
//!   面在 `crate::host_api` 各域）
//! - **移动**（`bare-host`）：[`bare`] + [`component`] + [`host_impl`]
//!   —— 自 `bedcode-mobile/packages/bedcode-wasm-core`（fork，票 17）迁入的
//!   移动装配面（17 组 Host 接线 + host_impl 16 域 + 无 WASI，移动插件为
//!   wasm32-unknown-unknown）；`WasmPluginState` 统一用 host-kit 版
//!   （`wasi-store` 关闭形态，票 06 批次 02）
//!
//! 机制面（Engine 构建参数）随各形态分支自持，真差异记录在案：桌面由
//! `CoreConfig` 驱动（`build_engine_config` 是带单测的纯函数），移动侧硬编码且
//! `config.rs` 另有 `bare-host` 变体——统一需先裁决「桌面 `CoreConfig` 是否为
//! 唯一真源」，不在本轮范围。
//!
//! 与 WIT 无关的机制面（组件编译 + AOT 缓存、Store 装配与资源限制接线、燃料注入
//! 与续费、实例生死观测）两形态共用一份，见 [`engine`]。

// ==================== 组件引擎机制（双端唯一一份） ====================
//
// `bindgen!`、`impl … Host`、linker 装配、`verify_abi`、`Loaded*Plugin` 导出调用面
// 留在各形态壳内——那些是 WIT 绑定层，合并的前提是双端 world 合一（ADR 0018
// 明确不成立：移动契约独立演进，不跟演桌面 ABI 破坏性变更）。
pub(crate) mod engine;

// ==================== Core 交集绑定层（双端唯一一份，票 03 Option A） ====================
//
// 绑契约真源 `wit/core.wit` 的 `world core`（双端全等的交集），cap 组合面由各端宿主
// 自有绑定 + `HostModule` 自报汇入同一个 Linker。形态差异（cap / WASI / 端 SDK 类型）
// 不进本模块。
pub(crate) mod core_binding;

// ==================== 桌面装配树（common-host） ====================

#[cfg(feature = "common-host")]
mod common;

#[cfg(all(test, feature = "common-host"))]
pub(crate) use common::fixture_build;
#[cfg(all(test, feature = "common-host"))]
pub(crate) use common::fixture_target;
// glob 过壳：common 分支的全部 pub 符号（WasmHostContext / WasmPluginState 引用 /
// InstanceMeta / LoadedWasmPlugin / WasmRuntime / OptionalExports …）保持
// `crate::manager::runtime::X` 路径不变（宿主与 crate 内引用零改动）
#[cfg(feature = "common-host")]
pub use common::*;

// ==================== 移动装配树（bare-host，fork 迁入） ====================

#[cfg(feature = "bare-host")]
pub mod bare;

// glob 对齐 common 分支口径：bare 分支全部 pub 符号过壳（WasmPluginState /
// WasmRuntime / LoadedComponentPlugin / WasmHostContext…），且 `host_impl` 模块
// 别名到壳顶层——移动 src-tauri 的 `plugin::wasm_runtime` 整模块垫片（pub use
// manager::runtime）经此解析 `wasm_runtime::host_impl::register_host_service` 等
// 宿主消费路径（fork 原状零改动）
#[cfg(feature = "bare-host")]
pub use bare::{LoadedComponentPlugin, StoreSpec, WasmHostContext, WasmPluginState, WasmRuntime};
#[cfg(feature = "bare-host")]
pub use bare::host_impl;
