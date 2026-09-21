//! WASM 内核（wasm-core）——插件系统根 facade
//!
//! 以 wasmtime 为运行时核心的五模块组合，本模块是唯一组合点：
//!
//! - [`config`]：配置模块（core-config）——Engine/Store 运行参数
//! - [`monitor`]：监控模块（core-monitor）——运行时指标埋点
//! - [`security`]：安全模块（core-security）——资源授权框架
//! - [`manager`]：插件管理模块（core-plugin-manager）——加载/注册/生命周期/运行时
//! - [`bus`]：消息总线模块（core-bus）——插件间 topic 消息
//!
//! 模块间协作只经本 facade 再导出或 trait 注入（如 [`bus::MessageDispatcher`]），
//! 禁止新增横向耦合；[`permission`] 为共享词汇（bedcode-plugin-api 再导出），
//! 所有模块可用。

pub mod bus;
pub mod config;
/// 插件 zip 分发包解压安装（dev 合入）：按本分支结构应归位 `manager`
/// （core-plugin-manager 的安装职责），当前保留 dev 路径以免改动引用方
/// （`manager/host.rs` 以 `crate::plugin::downloader` 引用），后续一并归位。
pub mod downloader;
pub mod manager;
pub mod monitor;
pub mod permission;
/// 快捷指令 legacy 主库 → session 插件私有库的一次性搬运（票 02，宿主侧 handoff）
pub mod quick_actions_migration;
pub mod security;
/// 旧 auto-task 私有库任务数据一次性搬运（票 17，宿主侧一次性迁移）
pub mod task_data_migration;

// ==================== Facade 再导出 ====================
// 外部消费方（Tauri 命令层、system、peer 等）只经 facade 引用，
// 不感知模块内部结构

pub use bus::{BusMessageHandler, MessageBus};
pub use manager::api_bridge;
pub use manager::host;
pub use manager::host::PluginHost;
pub use manager::storage::PluginStorage;
#[cfg(debug_assertions)]
pub use manager::watcher;
pub use security::fs_auth::FsAuthChecker;
