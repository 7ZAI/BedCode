//! WASM 宿主能力实现层（Component Model 绑定调用）
//!
//! 迁移阶段 C 后宿主能力只剩 Component Model 一种形态：
//! 本模块提供 15 组宿主能力的功能域实现（权限校验 + 宿主服务调用），
//! 由 `wasm_runtime::component` 的 Host trait 绑定逐接口调用。
//!
//! 各功能域与 SDK `host/*` trait 一一对应：
//! storage / database / terminal / session / events / http / log / fs / config /
//! bus / lifecycle / file_service / transfer / process / app
//!
//! 历史：阶段 A/B 时本目录名为 `host_functions`，包含 core module 胶水层
//! （(ptr,len) 内存搬运 + Linker 注册）；阶段 C 已删除胶水层，仅保留实现层。

pub(super) mod app;
pub(super) mod api;
pub(super) mod bus;
pub(super) mod config;
pub(super) mod database;
pub(super) mod events;
pub(super) mod file_service;
pub(super) mod fs;
pub(super) mod http;
pub(super) mod lifecycle;
pub(super) mod log;
pub(super) mod process;
pub(super) mod session;
pub(super) mod status;
pub(super) mod storage;
pub(super) mod terminal;
pub(super) mod timer;
pub(super) mod transfer;
mod wsl_fs;

use crate::plugin::wasm_runtime::WasmHostContext;

// ==================== Shared Guards ====================

/// 统一权限守卫
///
/// 校验通过返回 true；拒绝时记录结构化错误日志并返回 false，
/// 调用方据此返回 Err。替换原先约 30 处重复的 check/log 三连。
pub(super) fn check_permission(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    permission: &str,
    api: &str,
) -> bool {
    if host_ctx.permission.check(plugin_id, permission) {
        true
    } else {
        tracing::error!(plugin_id = %plugin_id, permission = %permission, "{}: permission denied", api);
        false
    }
}

// ==================== Tests ====================

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::db::Database;
    use crate::plugin::file_service::FileServiceRegistry;
    use crate::plugin::fs_auth::FsAuthChecker;
    use crate::plugin::message_bus::MessageBus;
    use crate::plugin::permission::PermissionManager;
    use crate::plugin::storage::PluginStorage;
    use crate::session::{SessionConfigManager, SessionManager};
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
        let session_manager = Arc::new(SessionManager::from_database(
            Database::new(&Path::new(":memory:")).expect("in-memory db"),
            Arc::new(std::path::PathBuf::from(".")),
        ));
        let config_manager = Arc::new(SessionConfigManager::new(Arc::new(Mutex::new(
            {
                let db = Database::new(&Path::new(":memory:")).expect("in-memory db");
                db.init_schema().expect("init schema");
                db
            }
        ))));
        let permission = Arc::new(PermissionManager::new());
        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), None));
        let message_bus = Arc::new(MessageBus::new());
        let file_service = FileServiceRegistry::new(fs_auth.clone(), None);
        Arc::new(WasmHostContext::new(
            db,
            Arc::new(Mutex::new(HashMap::new())),
            storage,
            session_manager,
            config_manager,
            None,
            permission,
            fs_auth,
            message_bus,
            file_service,
        ))
    }

    /// 为插件授予权限（manifest 授权路径的测试等价物）
    pub(crate) fn grant_permissions(ctx: &WasmHostContext, plugin_id: &str, perms: &[&str]) {
        let requested: Vec<String> = perms.iter().map(|s| s.to_string()).collect();
        ctx.permission.grant_permissions(plugin_id, &requested);
    }

    // ==================== check_permission ====================

    /// 已授权插件：校验通过
    #[test]
    fn check_permission_granted_returns_true() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "p1", &[crate::plugin::permission::PERMISSION_STORAGE]);
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
        grant_permissions(&ctx, "p1", &[crate::plugin::permission::PERMISSION_STORAGE]);
        assert!(!check_permission(&ctx, "p1", "fs:read", "host_test"));
    }
}
