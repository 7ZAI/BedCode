//! WASM 内核（wasm-core）——插件系统微内核根 facade
//!
//! 以 wasmtime 为运行时核心的模块组合，本模块是唯一组合点：
//!
//! - [`config`]：配置模块（core-config）——Engine/Store 运行参数
//! - [`monitor`]：监控模块（core-monitor）——运行时指标埋点
//! - [`security`]：安全模块（core-security）——资源授权框架
//! - [`manager`]：插件管理模块（core-plugin-manager）——加载/注册/生命周期/运行时
//! - [`bus`]：消息总线模块（core-bus）——插件间 topic 消息
//! - [`host_api`]：宿主对外接口模块（core-host-api）——宿主向插件（`host-*` 原语）
//!   与前端（Tauri 命令桥）提供的能力面
//! - [`runtime_util`]：异步桥基础设施（core-runtime-util）——同步↔异步桥与 ambient
//!   runtime，中立层（`manager` / `host_api` / `security` 皆可依赖，其自身零兄弟依赖）
//!
//! 模块间协作只经本 facade 再导出或 trait 注入（如 [`bus::MessageDispatcher`]），
//! 禁止新增横向耦合；[`permission`] 为共享词汇（bedcode-plugin-api 再导出），
//! 所有模块可用。

pub mod bus;
pub mod config;
/// 宿主对外接口模块：WASM 宿主能力实现（host-* 原语，权限校验 + 宿主服务调用，
/// 由 `manager::runtime::component` 的 Host trait 绑定逐接口调用）+ 前端 Tauri
/// 命令桥（api_bridge，权限校验后执行操作）
pub mod host_api;
pub mod manager;
pub mod monitor;
pub mod permission;
/// 异步桥基础设施：`manager` / `host_api` / `security` 共用的中立层，
/// 自身不依赖任何 wasm_core 兄弟模块（票 01）
pub(crate) mod runtime_util;
/// 插件存储中立层（原 manager/storage.rs 下沉，票 03）：`security` / `host_api` /
/// `manager` 皆可引用，自身只依赖 `crate::db`
pub mod storage;
pub mod security;

// ==================== Facade 再导出 ====================
// 外部消费方（Tauri 命令层、system、peer 等）只经 facade 引用，
// 不感知模块内部结构

pub use bus::{BusMessageHandler, MessageBus};
pub use host_api::api_bridge;
pub use manager::host;
pub use manager::host::PluginHost;
pub use storage::PluginStorage;
#[cfg(debug_assertions)]
pub use manager::watcher;
pub use security::fs_auth::FsAuthChecker;