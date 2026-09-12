//! 插件管理模块（core-plugin-manager）——内核核心模块
//!
//! 插件加载、注册、生命周期与 WASM 运行时管理：
//! - [`loader`] / [`registry`] / [`watcher`]：插件包加载、注册表、dev 监听
//! - [`host`]：PluginHost 装配与命令面
//! - [`wasm_runtime`]：wasmtime Engine/Linker/Store/Instance 生命周期
//! - [`api_bridge`]：插件 API 桥接；[`storage`]：插件存储；[`types`] / [`validation`]：类型与校验
//!
//! 系统组件（system）与应用插件（application）的类型划分与能力装配
//! 在票据 06 落地。

pub mod api_bridge;
pub mod host;
pub mod loader;
pub mod registry;
pub mod storage;
pub mod types;
pub mod validation;
pub mod wasm_runtime;
#[cfg(debug_assertions)]
pub mod watcher;
