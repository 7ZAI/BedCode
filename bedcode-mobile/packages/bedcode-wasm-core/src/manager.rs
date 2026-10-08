//! 插件管理模块（core-plugin-manager）——内核核心模块（移动 fork 形态）
//!
//! 插件加载、注册与校验机制：
//! - [`loader`] / [`registry`]：插件包加载、注册表
//! - [`types`] / [`validation`]：类型与校验
//! - [`downloader`]：插件 zip 分发包解压安装（core-plugin-manager 的安装职责）
//! - `watcher`（debug）：dev 热重载监听
//!
//! 桌面 fork 面差异（票 17 §3.2）：桌面 `host/`（PluginHost 装配与 26 个
//! tauri 命令桥）、`runtime/`（桌面 Engine/Linker/component 绑定层）、
//! `task`（host-task 执行引擎）、`capability`（L1 系统组件能力路由）**不
//! fork**——桌面形状与移动宿主装配差异大，批次 1b/2 以移动形状重建
//! （运行时与 host_impl 域迁入 + 宿主引擎端口注入），不做无谓的桌面死码
//! 搬运。宿主对外接口（host-* 原语实现）归位到 `crate::host_api`；插件
//! 存储（`PluginStorage`）中立化到 `crate::storage`。

pub mod downloader;
pub mod loader;
pub mod registry;
/// WASM 运行时（批次 2 自宿主 `plugin/wasm_runtime.rs` 迁入）：Engine/Linker/
/// Store 生命周期、组件绑定（bindgen 移动 WIT v17）、16 域 host_impl
pub mod runtime;
pub mod types;
pub mod validation;
