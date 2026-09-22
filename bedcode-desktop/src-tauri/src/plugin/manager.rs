//! 插件管理模块（core-plugin-manager）——内核核心模块
//!
//! 插件加载、注册、生命周期与 WASM 运行时管理：
//! - [`loader`] / [`registry`] / [`watcher`]：插件包加载、注册表、dev 监听
//! - [`host`]：PluginHost 装配与命令面
//! - [`wasm_runtime`]：wasmtime Engine/Linker/Store/Instance 生命周期
//! - [`capability`]：能力注册表与系统组件装配（manifest type/dependencies、
//!   host-* 能力路由与 host-side 转发，票据 06）
//! - [`api_bridge`]：插件 API 桥接；[`storage`]：插件存储；[`types`] / [`validation`]：类型与校验；
//! - [`task`]：host-task 执行引擎（core-task：专用 OS 线程池 + 任务注册表 + 事件管道）

pub mod api_bridge;
pub(crate) mod capability;
// 插件 zip 分发包解压安装：core-plugin-manager 的安装职责，已自 `plugin/downloader`
// 归位到此（票 11 第 5 项），引用方改为 `crate::plugin::manager::downloader`。
pub mod downloader;
pub mod host;
pub mod loader;
pub mod registry;
pub mod storage;
pub(crate) mod task;
pub mod types;
pub mod validation;
pub mod wasm_runtime;
#[cfg(debug_assertions)]
pub mod watcher;
