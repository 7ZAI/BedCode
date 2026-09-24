//! WASM 宿主能力实现层（Component Model 绑定调用）
//!
//! 迁移阶段 C 后宿主能力只剩 Component Model 一种形态：
//! 本模块提供宿主能力的功能域实现（权限校验 + 宿主服务调用），
//! 由 `wasm_runtime::component` 的 Host trait 绑定逐接口调用。
//!
//! 各功能域与 SDK `host/*` trait 一一对应：
//! storage / database / terminal / events / http / log / fs / config /
//! bus / peer / process / app
//!
//! v27（票 10）：**`session` 域整体退役**（`host-session` 整 interface 删除），
//! 同域内的 `lifecycle` 子模块（两条观察面注册入口）在票 03 已降级为退役占位、
//! 本票随 interface 一并删除。会话事实的宿主侧出口只剩 `host-pty`（PTY 引擎）
//! 与 `host-connection`（宿主 WS 连接清单）。
//!
//! 历史：阶段 A/B 时本目录名为 `host_functions`，包含 core module 胶水层
//! （(ptr,len) 内存搬运 + Linker 注册）；阶段 C 已删除胶水层，仅保留实现层。

pub(super) mod api;
pub(super) mod app;
pub(super) mod auth;
pub(super) mod bus;
pub(super) mod config;
pub(super) mod connection;
pub mod context;
pub(crate) mod crypto;
pub(super) mod database;
pub(super) mod events;
pub(crate) mod fs;
pub(crate) mod http;
pub(super) mod log;
pub(crate) mod mdns;
pub(super) mod peer;
pub(super) mod platform;
pub(crate) mod process;
pub(crate) mod pty;
pub(super) mod status;
pub(super) mod storage;
pub(crate) mod task;
pub(super) mod timer;
pub(crate) mod ws;
mod wsl_fs;

use crate::wasm_core::host_api::context::WasmHostContext;

// ==================== Shared Guards ====================

/// 统一权限守卫
///
/// 校验通过返回 true；拒绝时记录结构化错误日志并返回 false，
/// 调用方据此返回 Err。替换原先约 30 处重复的 check/log 三连。
pub(super) fn check_permission(host_ctx: &WasmHostContext, plugin_id: &str, permission: &str, api: &str) -> bool {
    if host_ctx.permission.check(plugin_id, permission) {
        true
    } else {
        tracing::error!(plugin_id = %plugin_id, permission = %permission, api = %api, "permission denied");
        false
    }
}

// ==================== Tests ====================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::Database;
    use crate::wasm_core::bus::MessageBus;
    use crate::wasm_core::storage::PluginStorage;
    use crate::wasm_core::permission::PermissionManager;
    use crate::wasm_core::security::fs_auth::FsAuthChecker;
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// 构造全内存、无头（AppHandle=None）的宿主上下文
    ///
    /// 与 wasm_runtime.rs 测试的 setup_wasm_runtime 等价，但不创建 WasmRuntime：
    /// host_impl 测试只验证宿主能力实现本身，不加载 wasm 组件（免 wasmtime 依赖）
    pub(crate) fn build_host_ctx() -> Arc<WasmHostContext> {
        let db = Database::new(&Path::new(":memory:")).expect("in-memory db");
        db.init_schema().expect("init schema");
        let db = Arc::new(Mutex::new(db));
        let storage = Arc::new(PluginStorage::new(db.clone()));
        let permission = Arc::new(PermissionManager::new());
        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), None));
        let message_bus = Arc::new(MessageBus::new());
        Arc::new(WasmHostContext::new(
            db,
            Arc::new(Mutex::new(HashMap::new())),
            storage,
            None,
            permission,
            fs_auth,
            message_bus,
            crate::wasm_core::manager::capability::test_registry(),
        ))
    }

    /// 为插件授予权限（manifest 授权路径的测试等价物）
    pub(crate) fn grant_permissions(ctx: &WasmHostContext, plugin_id: &str, perms: &[&str]) {
        let requested: Vec<String> = perms.iter().map(|s| s.to_string()).collect();
        ctx.permission.grant_permissions(plugin_id, &requested);
    }

    /// 权限五同步点的②③：打包 CLI 与前端**生成物**必须认识该权限
    ///
    /// 票 01 起两份列表都是 SDK 真源的生成物（不再有手抄清单可比字面量）：
    /// 生成物 ↔ 真源的集合相等由 `plugin/permission.rs` 的词汇漂移锁负责，
    /// 本函数只确认「新增权限位确实进了两份生成物」——漏跑生成器即转红。
    pub(crate) fn generated_vocabulary_know(permission: &str) {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let cli =
            std::fs::read_to_string(manifest_dir.join("../packages/plugin-sdk-desktop/bin/permission-vocabulary.json"))
                .expect("CLI 权限词汇生成物可读");
        let frontend = std::fs::read_to_string(manifest_dir.join("../src/plugin/permission-vocabulary.ts"))
            .expect("前端权限词汇生成物可读");
        assert!(
            cli.contains(&format!("\"{permission}\"")),
            "CLI 权限词汇生成物缺 {permission}（重跑 SDK 的 pnpm run gen:permissions）"
        );
        assert!(
            frontend.contains(&format!("'{permission}'")),
            "前端权限词汇生成物缺 {permission}（重跑 SDK 的 pnpm run gen:permissions）"
        );
    }

    // ==================== check_permission ====================

    /// 已授权插件：校验通过
    #[test]
    fn check_permission_granted_returns_true() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "p1", &[crate::wasm_core::permission::PERMISSION_STORAGE]);
        assert!(check_permission(&ctx, "p1", "storage", "host_test"));
    }

    /// 从未授权的插件：一律拒绝（grant 前 storage 也拿不到）
    #[test]
    fn check_permission_ungranted_plugin_rejected() {
        let ctx = build_host_ctx();
        assert!(!check_permission(&ctx, "p1", "storage", "host_test"));
    }

    /// 授权了 A 权限但请求 B 权限：拒绝（权限粒度隔离）
    #[test]
    fn check_permission_wrong_permission_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "p1", &[crate::wasm_core::permission::PERMISSION_STORAGE]);
        assert!(!check_permission(&ctx, "p1", "fs:read", "host_test"));
    }
}
