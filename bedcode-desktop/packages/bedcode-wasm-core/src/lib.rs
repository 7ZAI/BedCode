//! WASM 内核（bedcode-wasm-core）——插件系统微内核整核（可复用 crate）
//!
//! 桌面端 `wasm_core` 整核抽出（.scratch/2026-10-06-wasm-core-whole-crate/spec.md）：
//! 插件核心机制（`manager` / `security` / `host_api` / `bus` / `config` / `monitor` /
//! `permission` / `runtime_util` / `intercall` / `storage`）+ 引擎面（`db` / `pty` /
//! `enums` / `system`）+ 宿主胶水（`utils/auth` / `utils/session_gateway`）从
//! bin crate 整体迁出，任何 Tauri 宿主可直接 path 依赖本 crate 获得插件机制。
//!
//! 本模块是唯一组合点：
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
/// 加密引擎（真源 bedcode-crypto-engine，本 crate 内部 `crate::crypto::*` 路径）
pub mod crypto;
/// 引擎面：宿主主库（schema.sql 单一事实源，ADR 0036「机制与真源同侧」）
pub mod db;
pub mod enums;
/// 宿主上下文注册表（§4.4：mdns adapter 的零大小类型按调用取 `WasmHostContext`）
pub(crate) mod host_context_registry;
/// 测试 harness（§4.5）：crate 内测试起 HTTP+WS 服务器的薄壳（lib 组合根不可引用）
#[cfg(test)]
pub(crate) mod host_harness;
/// crate 边界锁共享面（票 05 落点）：拆分产物登记表 + 扫描根单一事源上提本 crate
///
/// 常编译（非 `#[cfg(test)]`）：lib 侧 `server/crate_boundary_lock.rs` 在**非 test
/// 构建**里也要经 `bedcode_wasm_core::crate_boundary_lock` 取 `SPLIT_CRATES` 登记表
/// （lib → crate 单向引用，票 05 收口；lib 的 `cargo check` / 全量断言都依赖它）。
pub mod crate_boundary_lock;
/// 宿主对外接口模块：WASM 宿主能力实现（host-* 原语，权限校验 + 宿主服务调用，
/// 由 `manager::runtime::component` 的 Host trait 绑定逐接口调用）+ 前端 Tauri
/// 命令桥（api_bridge，权限校验后执行操作）
pub mod host_api;
/// 宿主→插件互调客户端（中立层，ADR 0033 从 `utils/auth/auth_center.rs` 上提）：
/// JSON-RPC 2.0 over host-bus 的通用发起端 + 请求 id 分配
pub mod intercall;
pub mod manager;
pub mod monitor;
pub mod permission;
/// PTY 引擎面（host-pty 引擎，零业务语义；业务会话与输出汇在插件侧）
pub mod pty;
/// 异步桥基础设施：`manager` / `host_api` / `security` 共用的中立层，
/// 自身不依赖任何 wasm_core 兄弟模块（票 01）
pub mod runtime_util;
pub mod security;
/// 插件存储中立层（原 manager/storage.rs 下沉，票 03）：`security` / `host_api` /
/// `manager` 皆可引用，自身只依赖 `crate::db`
pub mod storage;
/// 引擎级配置 / 文件定位 / 进程创建（lib 的 `system.rs` 组合根经垫片零改动引用）
pub mod system;
/// 认证中心桥接 + 会话窄转发（lib 的 `utils.rs` 经垫片零改动引用）
pub mod utils;
/// 测试基建（常编译公开，lib 集成测试消费；来源见模块头注释）
pub mod test_support;

// ==================== Facade 再导出 ====================
// 外部消费方（Tauri 命令层、system、peer 等）只经 facade 引用，
// 不感知模块内部结构

pub use bus::{BusMessageHandler, MessageBus};
pub use bedcode_pty_engine::PtyEngineConfig as PtyEngineConfig;
pub use manager::host;
pub use manager::host::api_bridge;
pub use manager::host::PluginHost;
#[cfg(debug_assertions)]
pub use manager::watcher;
pub use security::fs_auth::FsAuthChecker;
pub use storage::PluginStorage;

// 引擎级错误类型（真源 bedcode-server-base，本 crate 与 lib 共用同一份；
// lib 侧 `system/error.rs` 垫片经 `pub use bedcode_wasm_core` 路径不变）
pub use bedcode_server_base::error::{AppError, Result};

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    /// 整核抽出后的模块可见性锁：核心机制模块全部公开（lib 与集成测试经垫片消费）
    #[test]
    fn core_modules_are_public() {
        let _: fn() -> crate::db::Database;
        let _: fn(crate::manager::host::PluginHost);
        let _: fn(crate::host_api::context::WasmHostContext);
        let _: fn(crate::bus::MessageBus);
        let _: fn(crate::security::fs_auth::FsAuthChecker);
        let _: fn(crate::storage::PluginStorage);
        let _: fn(crate::pty::PtySession);
    }
}
