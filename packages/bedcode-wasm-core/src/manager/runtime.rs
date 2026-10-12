//! WASM 插件运行时（机制一份 + 两薄形态壳，票 06 批次 03 起 · ADR 0045 D6 · ADR 0049）
//!
//! 「只有一份 wasm-core，双端核心机制完全一致」的终局下，装配树按宿主形态
//! cfg 挂载（spec D5/D6）。票 04（桌面）与票 05（移动）把两个壳里的 WIT 绑定
//! 全部迁到各端宿主后，**壳只剩机制接线**：
//!
//! - **桌面**（`common-host`）：[`common`] —— wasmtime Engine/Linker/Store
//!   生命周期 + WASI 装配（[`wasi`]，p2/p3）+ `component`（导出调用骨架 + 宿主
//!   自报模块的装配入口；WIT impl 面在各端宿主 `src-tauri/src/plugin/*.rs`）
//! - **移动**（`bare-host`）：[`bare`] + [`bare::component`] —— 同形的移动壳
//!   （原 fork 迁入的 17 组 Host 接线 + `host_impl` 16 域已随票 04 / 05 迁出到
//!   各端宿主：移动侧真源 `bedcode-mobile/src-tauri/src/plugin/host_domains/`
//!   域逻辑与 interface `Host` impl 同处，移动互调经 `crate::plugin::host_domains`）。
//!   `WasmPluginState` 统一用 host-kit 版（ctx/table 常编译）。
//!
//! **WASI 面是双端共享的核心机制**（[`wasi`] + `CoreConfig::wasi`）：不是形态差异轴
//! ——默认双端全开，外部配置可关；目的是将来 worker 类别插件（ADR 0034 L3.b）
//! 双端都要跑 wasm 时无需各端重建接线。
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
// 组件编译 + AOT 缓存、Store 装配与资源限制接线、燃料注入与续费、实例生死观测、
// Engine 构建纯函数、trap 判定——全部双端一份，见 [`engine`]。
// **`bindgen!` / `impl … Host` / linker 装配 / `verify_abi` / `Loaded*Plugin` 导出
// 调用面不在这里**：core 交集在 [`core_binding`]（唯一一份），cap 与端差异在
// 各端宿主（ADR 0049）。
pub(crate) mod engine;

// ==================== Core 交集绑定层（双端唯一一份，票 03 Option A） ====================
//
// 绑契约真源 `wit/core.wit` 的 `world core`（双端全等的交集），cap 组合面由各端宿主
// 自有绑定 + `HostModule` 自报汇入同一个 Linker。形态差异（cap / WASI / 端 SDK 类型）
// 不进本模块。
pub(crate) mod core_binding;

// ==================== WASI 装配面（双端同源，配置驱动） ====================
//
// 用户裁决（2026-10-12）：WASI（p2/p3）住核心、双端默认全开、开/关由
// `CoreConfig::wasi` 定——形态（桌面 / 移动）不再是有无 WASI 的差异轴。
// 目的：将来 worker 类别插件双端都要跑 wasm，装配面必须已经双端同源。
//
// 形态门控只为「无宿主壳」的编译配置存在：本模块装配的 `WasmPluginState` 由形态壳
// 提供，`--no-default-features`（纯引擎面复用、无宿主壳）下没有可装配对象。
// `common-host` / `bare-host` **两者都编译本模块**（不是「桌面独有」）。
#[cfg(any(feature = "common-host", feature = "bare-host"))]
pub(crate) mod wasi;

// ==================== Guest 事件派发契约面（票 03 / ADR 0022） ====================
//
// 事件循环机制留内核，`events` 导出的调用契约由各端宿主实现本 trait 并注入
// （双端 `events` 词汇完全不同，且带 `result` 返回无法原生探测，见模块文档）。
pub mod guest_events;

// ==================== 桌面装配树（common-host） ====================

#[cfg(feature = "common-host")]
mod common;

// `fixture_build` 的 crate 级转出随票 04 删除（原消费者是内核 host/tests 的夹具
// 用例，已迁宿主——宿主测试只读产物、不构建）；`fixture_target` 仍被
// `common/tests/p3_async_host_import.rs` 消费，转出保留。
#[cfg(all(test, feature = "common-host"))]
pub(crate) use common::fixture_target;
// glob 过壳：common 分支的全部 pub 符号（WasmHostContext / WasmPluginState 引用 /
// InstanceMeta / LoadedWasmPlugin / WasmRuntime / OptionalExports …）保持
// `crate::manager::runtime::X` 路径不变（宿主与 crate 内引用零改动）
#[cfg(feature = "common-host")]
pub use common::*;

// 票 04：内核测试用 guest 事件派发替身（`TestGuestEvents`）随全 world 绑定一并
// 删除——派发器只由宿主注入（桌面 `src-tauri/src/plugin/guest_events.rs`；
// 移动宿主同族），缺省 `UnwiredGuestEvents` 派发即 fail-visible。

// ==================== 移动装配树（bare-host，fork 迁入） ====================

#[cfg(feature = "bare-host")]
pub mod bare;

// glob 对齐 common 分支口径：bare 分支全部 pub 符号过壳（WasmPluginState /
// WasmRuntime / LoadedComponentPlugin / WasmHostContext…）。
// 票 05 绑定归位：`bare::host_impl` 的转出（`wasm_runtime::host_impl::*` 路径）
// 已随 12 域实现迁移动宿主——宿主消费点改 `crate::plugin::host_domains`。
#[cfg(feature = "bare-host")]
pub use bare::{LoadedComponentPlugin, StoreSpec, WasmHostContext, WasmPluginState, WasmRuntime};
