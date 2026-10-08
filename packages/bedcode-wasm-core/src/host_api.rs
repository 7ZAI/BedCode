//! WASM 宿主能力实现层（Component Model 绑定调用）
//!
//! 迁移阶段 C 后宿主能力只剩 Component Model 一种形态：
//! 本模块提供宿主能力的功能域实现（权限校验 + 宿主服务调用），
//! 由 `wasm_runtime::component` 的 Host trait 绑定逐接口调用。
//!
//! 各功能域与 SDK `host/*` trait 一一对应：
//! app / bus / connection / crypto / database / events / fs / http / log / mdns /
//! peer / platform / process / pty（adapter） / storage / task / timer / unit_executor / ws
//!
//! v27（票 10）：**`session` 域整体退役**（`host-session` 整 interface 删除），
//! 同域内的 `lifecycle` 子模块（两条观察面注册入口）在票 03 已降级为退役占位、
//! 本票随 interface 一并删除。会话事实的宿主侧出口只剩 `host-pty`（PTY 引擎）
//! 与 `host-connection`（宿主 WS 连接清单）。
//!
//! ADR 0036：**`database` / `plugin-database` / `storage` 三域（13 条原语）留在核心**
//! ——wasm-core-lib-split 票 07/08 曾把它们连同 SQLite 引擎搬进
//! `bedcode-sqlite-engine` 能力域 crate，同日撤销（机制实现与机制真源不分家）。
//! 本目录的 `sqlite` 是三域共用的**端口实现**（权限门 / 库句柄 / 唯一那份
//! 同步↔异步桥 / kv 与能力路由），`sqlite_ports` 是端口 trait（可测性缝，不是架构
//! 边界），`sqlite_scaffold` 是两域共用的假端口（仅测试）。
//!
//! 历史：阶段 A/B 时本目录名为 `host_functions`，包含 core module 胶水层
//! （(ptr,len) 内存搬运 + Linker 注册）；阶段 C 已删除胶水层，仅保留实现层。

pub(super) mod api;
pub(super) mod app;
pub(super) mod auth;
pub mod auth_center;
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
pub mod pty;
pub(crate) mod sqlite;
pub(crate) mod sqlite_ports;
pub(super) mod status;
pub(super) mod storage;
pub(crate) mod task;
pub(super) mod timer;
pub(crate) mod unit_executor;
/// host-websocket WIT 绑定面（票 05c 提 pub：lib 集成测试 ws_e2e 消费
/// `purge_for_plugin` / `HostWsPorts`；机制面无业务语义）
pub mod ws;
mod wsl_fs;

// ==================== 测试夹具出口 ====================

/// 三域共用假端口（`sqlite_scaffold` 内的项是 `pub(super)`，即
/// `pub(in wasm_core::host_api)`，故 `database::tests` / `storage::tests` 都够得着）
#[cfg(test)]
pub(crate) mod sqlite_scaffold;

/// 测试夹具出口：以宿主身份往某插件属主的 secret-store 写一个键
///
/// `auth` 模块本身是 `pub(super)`（只在 `wasm_core` 内可见），而消费方在 `utils` 域
/// 与 lib 侧（`src-tauri/src/utils/auth/test_tokens.rs`，票 05 回迁）——故从这里定点
/// 再导出**一个**函数，而不是把整个模块放宽到 `pub(crate)`（那会顺带放开
/// `auth_secret_*` 全部原语）。整核抽出：test_tokens 迁入本 crate 后需常编译
/// （lib 集成测试消费），故本再导出不带 `#[cfg(test)]`；票 05 从 `pub(crate)` 提为
pub use auth::test_seed_plugin_secret;

// 票 05c：顶层 `grant_permissions`（常编译 pub）签名需要 WasmHostContext，
// 本 use 提常编译（不再 cfg(test)）
use crate::host_api::context::WasmHostContext;
// ==================== Shared Guards ====================

/// 统一权限守卫
///
/// 校验通过返回 true；拒绝时记录结构化日志并返回 false，调用方据此返回 Err。
/// 替换原先约 30 处重复的 check/log 三连。
/// 签名收 `&dyn PermissionScope`（票 05 ISP）：域函数不再依赖上帝对象。
///
/// `#[must_use]`（R-01）：裸 bool 返回值若被调用方忽略（忘记把 false 转 Err 就
/// 继续执行宿主操作）会产生编译警告——「宿主 API 入口必须消费权限判定结果」
/// 由编译器强制，而非靠 review 记得。
///
/// 拒绝日志用 `warn!`（R-08）：策略性拒绝是**预期结果**而非系统错误——不受信任
/// 插件反复探测若每探一次 error!，日志会被无限冲刷（log-DoS）；warn! 仍默认可见，
/// 但语义与「影响功能的失败」区分开。
#[must_use]
pub(super) fn check_permission(
    scope: &dyn crate::host_api::context::PermissionScope,
    plugin_id: &str,
    permission: &str,
    api: &str,
) -> bool {
    if scope.permission().check(plugin_id, permission) {
        true
    } else {
        tracing::warn!(plugin_id = %plugin_id, permission = %permission, api = %api, "permission denied");
        false
    }
}

// ==================== 能力域端口装配链（唯一入口） ====================

/// 装**全部**能力域的宿主端口——生产与每个测试夹具的**唯一**装配入口
///
/// 四域的实现已迁进 `packages/` 的能力 crate，`impl Host for WasmPluginState`
/// 住在那边，只能经本 crate 的端口 trait 反取宿主实现，于是形成「开机装一次」的
/// 进程级单向装配。**漏装的后果是运行期 panic 而不是编译错**：域的 `ports()`
/// 取不到时 fail-visible 崩，且只在 guest 首次调该域原语时才触发。
///
/// **为什么必须单一入口**：装配链曾有三份拷贝（`PluginHost::new` 与两个测试
/// 夹具），mdns 一度只在生产那一份里——测试夹具漏装，于是依赖 mdns 的闭环用例
/// 只能在「别的用例恰好先装过」的同一进程里绿，单跑即崩（顺序依赖假绿）。
/// 新增能力域只需在本函数加一行，三条链同时生效。
///
/// 幂等：各域 `install_ports` 是 `OnceLock::set`，重复装配被忽略而非替换。
pub(crate) fn install_capability_domain_ports(
    host_ctx: &std::sync::Arc<crate::host_api::context::WasmHostContext>,
) {
    // 宿主上下文注册表（整核抽出 §4.4）：mdns adapter 的零大小类型经它取
    // `WasmHostContext`（原经 lib `AppContext::try_global()`）。单入口纪律：
    // 装配链只有本函数一个入口，注册表写也一并在此（生产 + 两个测试夹具共用）。
    crate::host_context_registry::install(host_ctx);
    // mDNS 的宿主端口实现是零大小类型（`HostDiscoveryPorts`），无状态可注入
    mdns::install();
    ws::install(host_ctx.clone());
    peer::install(host_ctx.clone());
    http::install(host_ctx.clone());
    // pty 能力域同理（pty-capability-domain 票 D1：域机制与 WIT 接线在
    // `bedcode-pty-engine`，本 crate 只装端口）
    pty::install(host_ctx.clone());
}

// ==================== Tests ====================

/// 为插件授予权限（manifest 授权路径的测试等价物）
///
/// **常编译 pub（wasm-core 纯净性收口票 05c）**：原在 `#[cfg(test)] mod tests`
/// （wasm-core 内部测试可见）；lib 集成测试（task_e2e 等，独立测试二进制）
/// 看不到 cfg(test) 项，提为常编译 pub 供 `bedcode_desktop_lib::wasm_core::
/// host_api::grant_permissions` 消费（调用点形态与原名一致，零逻辑改动）。
pub fn grant_permissions(ctx: &WasmHostContext, plugin_id: &str, perms: &[&str]) {
    let requested: Vec<String> = perms.iter().map(|s| s.to_string()).collect();
    ctx.permission.grant_permissions(plugin_id, &requested);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::Database;
    use crate::bus::MessageBus;
    use crate::permission::PermissionManager;
    use crate::security::fs_auth::FsAuthChecker;
    use crate::storage::PluginStorage;
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
        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), None, Vec::new()));
        let message_bus = Arc::new(MessageBus::new());
        Arc::new(WasmHostContext::new(
            db,
            Arc::new(Mutex::new(HashMap::new())),
            storage,
            None,
            permission,
            fs_auth,
            message_bus,
            crate::manager::capability::test_registry(),
        ))
    }

    /// 权限五同步点的②③：打包 CLI 与前端**生成物**必须认识该权限
    ///
    /// 票 01 起两份列表都是 SDK 真源的生成物（不再有手抄清单可比字面量）：
    /// 生成物 ↔ 真源的集合相等由 `plugin/permission.rs` 的词汇漂移锁负责，
    /// 本函数只确认「新增权限位确实进了两份生成物」——漏跑生成器即转红。
    pub(crate) fn generated_vocabulary_know(permission: &str) {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        // 2026-10-08 迁根：SDK 留在 bedcode-desktop/packages/，前端在 bedcode-desktop/src/
        let cli = std::fs::read_to_string(
            manifest_dir.join("../../bedcode-desktop/packages/plugin-sdk-desktop/bin/permission-vocabulary.json"),
        )
        .expect("CLI 权限词汇生成物可读");
        let frontend = std::fs::read_to_string(
            manifest_dir.join("../../bedcode-desktop/src/plugin/permission-vocabulary.ts"),
        )
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
        grant_permissions(&ctx, "p1", &[crate::permission::PERMISSION_STORAGE]);
        assert!(check_permission(ctx.as_ref(), "p1", "storage", "host_test"));
    }

    /// 从未授权的插件：一律拒绝（grant 前 storage 也拿不到）
    #[test]
    fn check_permission_ungranted_plugin_rejected() {
        let ctx = build_host_ctx();
        assert!(!check_permission(ctx.as_ref(), "p1", "storage", "host_test"));
    }

    /// 授权了 A 权限但请求 B 权限：拒绝（权限粒度隔离）
    #[test]
    fn check_permission_wrong_permission_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "p1", &[crate::permission::PERMISSION_STORAGE]);
        assert!(!check_permission(ctx.as_ref(), "p1", "fs:read", "host_test"));
    }
}
