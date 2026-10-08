//! WASM 内核（bedcode-wasm-core-mobile）——移动端插件系统微内核整核
//!
//! 票 17（`.scratch/2026-10-07-mobile-wasm-core-refactor/ticket-17-mobile-wasm-core-fork.md`）：
//! 以桌面整核 `packages/bedcode-wasm-core`（ADR 0037）为源 fork（ADR 0040 D1
//! 选项 C 第一步）。机制同源、契约独立（ADR 0018）——**同名 ≠ 契约同一**：
//! bindgen 绑**移动 WIT**（v17：16 import / 5 export + 可选 events-binary），
//! 权限词汇以 `bedcode-plugin-api-mobile` 为真源；桌面独有域（host_api 桌面 21 域 /
//! system / utils / crypto / manager 装配层 / host-task / L1 能力路由）已删，
//! host_api 移动 16 域自持（源 = 宿主 `plugin/wasm_runtime/host_impl/`）。
//!
//! 本模块是唯一组合点：
//!
//! - [`config`]：配置模块（core-config）——Engine/Store 运行参数
//! - [`monitor`]：监控模块（core-monitor）——运行时指标埋点
//! - [`security`]：安全模块（core-security）——资源授权框架
//! - [`manager`]：插件管理模块（core-plugin-manager）——加载/注册/生命周期/运行时
//! - [`bus`]：消息总线模块（core-bus）——插件间 topic 消息
//! - [`host_api`]：宿主能力面（core-host-api）——移动 16 域 host-* 原语 + 引擎端口
//! - [`runtime_util`]：异步桥基础设施（core-runtime-util）——同步↔异步桥 + 错误边界
//! - [`terminal_stream_gateway`]：终端输出流窄转发表（纯机制，票 12）
//! - [`error`]：crate 级错误类型（移动 `AppError` 形状；批次 2 宿主垫片对齐）
//! - [`test_support`]：测试支持面（`any(test, feature = "test-support")` 门控）
//!
//! fork 面收缩（抽共享核）走票 18/19；桌面 crate **零改动**（ADR 0040 D2 承诺）。

pub mod bus;
pub mod config;
/// crate 边界锁共享面（桌面票 05 形态 fork；移动 fork 面登记见模块内清单）
pub mod crate_boundary_lock;
/// 引擎面：宿主主库（schema.sql 单一事实源，ADR 0036「机制与真源同侧」）
pub mod db;
/// crate 级错误类型（移动 `AppError` 形状，真源对齐宿主 `system/error.rs`）
pub mod error;
/// 宿主上下文注册表（OnceLock<Weak<WasmHostContext>>；范式对齐桌面 §4.4）
pub(crate) mod host_context_registry;
/// 宿主能力面：移动 16 域 host-* 原语（权限校验 + 宿主服务调用，
/// 由 `manager::runtime::component` 的 Host trait 绑定逐接口调用）
pub mod host_api;
pub mod manager;
pub mod monitor;
pub mod permission;
/// 异步桥基础设施：`manager` / `host_api` / `security` 共用的中立层
pub mod runtime_util;
pub mod security;
/// 插件存储中立层：`security` / `host_api` / `manager` 皆可引用
pub mod storage;
/// 引擎面 system 模块（插件机制常量子集 + error 路径兼容层，票 17 fork 自持）
pub mod system;
/// 终端输出流窄转发表（批次 2 自宿主迁入；Tauri 命令薄壳留宿主）
pub mod terminal_stream_gateway;
/// 测试支持面（`any(test, feature = "test-support")` 门控；生产构建为空）
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

// ==================== Facade 再导出 ====================
// 外部消费方（宿主命令层等）只经 facade 引用，不感知模块内部结构

pub use bus::{BusMessageHandler, MessageBus};
pub use error::{AppError, Result};
pub use security::fs_auth::FsAuthChecker;
pub use storage::PluginStorage;
/// wasmtime 再导出（宿主定制钩子面；版本与本 crate 依赖同源，ADR 0019）
pub use wasmtime;

// ==================== 运行时门面（批次 2：宿主垫片消费） ====================
// 宿主 `crate::plugin::wasm_runtime::*` 路径经垫片转发到这里
pub use manager::runtime::{WasmPluginState, WasmRuntime};
pub use manager::types::PluginLifecycleEvent;

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    /// 整核 fork 后的模块可见性锁：核心机制模块全部公开（批次 2 宿主垫片消费）
    #[test]
    fn core_modules_are_public() {
        let _: fn() -> crate::db::Database;
        let _: fn(crate::host_api::WasmHostContext);
        let _: fn(crate::bus::MessageBus);
        let _: fn(crate::security::fs_auth::FsAuthChecker);
        let _: fn(crate::storage::PluginStorage);
    }

    /// fork 面反向锁：桌面独有域不得回到移动内核（票 17 §3.2 删面清单）
    ///
    /// 桌面 host_api 21 域 / system / utils / crypto / manager 装配层 / host-task /
    /// L1 能力路由已删——模块声明或文件回来 = 桌面域回接（双端机制漂移复活）。
    #[test]
    fn desktop_only_domains_must_not_return() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let retired = [
            "src/system",
            "src/utils",
            "src/manager/host",
            "src/manager/capability.rs",
            "src/manager/task.rs",
            "src/crypto.rs",
            // "src/test_support.rs" 已随批次 2 移出禁回清单（票 17 §6）：移动
            // 测试支持面（夹具构建器 + mock WS server，any(test, feature)
            // 门控）是合法新增，与桌面同名文件（桌面 e2e 支持面）内容无涉
            "src/host_harness.rs",
            "src/enums/special_key.rs",
        ];
        for rel in retired {
            let path = manifest_dir.join(rel);
            assert!(
                !path.exists(),
                "{rel} 不得回到移动 wasm-core（桌面独有域，票 17 §3.2 删面；\
                 移动对应物在宿主 crate 或批次 2 绑定层）"
            );
        }
    }
}
