//! WASM 宿主能力实现层（Component Model 绑定调用）
//!
//! 迁移阶段 C 后宿主能力只剩 Component Model 一种形态：
//! 本模块提供宿主能力的功能域实现（权限校验 + 宿主服务调用），
//! 由 `wasm_runtime::component` 的 Host trait 绑定逐接口调用。
//!
//! 各功能域与 SDK `host/*` trait 一一对应（仍在内核者）：
//! api / auth_center / bus / config / database / events（交集 emit）/ fs（交集 6 +
//! 桌面扩展三函数实现本体）/ http（执行器）/ log / platform（交集 pick-*）/
//! sqlite* / status / storage / unit_executor / wsl_fs
//! （`wsl_fs`：WSL UNC 路径桥接，被交集 fs 函数的底层 helper 消费的**机制**，
//! 桌面独有但随核心子集留守；`system::wsl`——WSL 发行版列举——已随桌面扩展迁宿主）
//!
//! **已迁宿主侧的能力域 adapter 不再在本目录**（wasm-core 纯净性收口票 02）：
//! 批次 02 迁 pty、批次 03 迁 mdns / peer / ws / http（端口 adapter 落
//! `bedcode-desktop/src-tauri/src/plugin/<domain>.rs`），批次 04 路径 B 迁 auth /
//! crypto，批次 05 路径 B 迁 task / process / app / timer / connection（整域：
//! WIT impl + 域函数 + 单测随域走宿主）与 host-api-call 的 **WIT impl**（薄转发；
//! 回复道编排是内核互调机制——`intercall` / `auth_center` / `call_plugin_api_host`
//! 消费，与 peer/ws/http「内核仍消费引擎面」同判据留内核）。经 host-kit 的装配自报面
//! （`expect_host_module!`；执行器另有 `submit_unit_executor!` 自报）接入本文的装配链；
//! 内核不点名任何具体能力域。mdns 五条原语的**能力路由**判据仍只有内核一份
//! （见本文 [`forward_mdns_browse`] 一族的再导出）。本目录 `http.rs` 只剩
//! `HttpUnitExecutor`（host-task 面执行器，注册点与留 core 的任务引擎同侧）。
//!
//! **v36 交集接口切片（批次 06）**：host-fs / host-platform / host-events / abi
//! 四接口各自拆出桌面独有函数为新 interface（host-fs-desktop /
//! host-platform-desktop / host-events-desktop / abi-form），其 **WIT impl 落宿主**
//! `src-tauri/src/plugin/{fs,platform,events}.rs`（路径 B；abi-form 是 guest 导出，
//! SDK 宏与宿主 verify_abi 消费）。本目录只留交集子集的域实现：`events` 只剩
//! `emit_event`；`platform` 只剩 `pick-files` / `pick-folder`（含 `authorize_picked`
//! 授权链）；`fs` 的交集 6 函数 + 桌面扩展三函数**实现本体**（`fs_read_dir` /
//! `fs_canonicalize` / `fs_stat` 提 pub，双消费者：内核 `FsUnitExecutor` 的
//! `fs.read-dir` / `fs.stat` 单元与宿主 WIT impl——与 `check_permission` 提 pub 同款）。
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

// ==================== 域子模块（形态分叉，票 06 批次 03） ====================
//
// 桌面 WIT impl 域（`desktop-host`）：由 `runtime/desktop/component.rs` 的
// Host trait 绑定逐接口调用。移动形态的对应实现在 `manager::runtime::host_impl`
// 16 域（fork 迁入），本目录不重复。
// 移动 adapter（`mobile-host`）：http_engine / ports / sql_guard / mobile_context
// 自 fork `host_api/` 迁入（fork 的 context.rs 与桌面同名不同物，改名消歧）。
#[cfg(feature = "desktop-host")]
pub mod api;
#[cfg(feature = "desktop-host")]
pub mod auth_center;
#[cfg(feature = "desktop-host")]
pub(super) mod bus;
#[cfg(feature = "desktop-host")]
pub(super) mod config;
#[cfg(feature = "desktop-host")]
pub mod context;
#[cfg(feature = "desktop-host")]
pub(super) mod database;
#[cfg(feature = "desktop-host")]
pub(super) mod events;
#[cfg(feature = "desktop-host")]
pub(crate) mod fs;
#[cfg(feature = "desktop-host")]
pub(crate) mod http;
#[cfg(feature = "desktop-host")]
pub(super) mod log;

#[cfg(feature = "desktop-host")]
pub(super) mod platform;
#[cfg(feature = "desktop-host")]
pub(crate) mod sqlite;
#[cfg(feature = "desktop-host")]
pub(crate) mod sqlite_ports;
#[cfg(feature = "desktop-host")]
pub(super) mod status;
#[cfg(feature = "desktop-host")]
pub(super) mod storage;
#[cfg(feature = "desktop-host")]
pub mod unit_executor;

// v36 交集接口切片（票 02 批次 06）：`host-fs-desktop`（桌面扩展 3 函数）的 WIT
// impl 在宿主 `src-tauri/src/plugin/fs.rs`，实现本体显式留内核作双消费者机制
// （内核 `FsUnitExecutor` 的 `fs.read-dir` / `fs.stat` 单元 + 宿主 WIT impl，与
// `check_permission` 提 pub 同款裁决）——此处显式公开三个函数供宿主经
// `bedcode_wasm_core::host_api::fs::{fs_read_dir, fs_canonicalize, fs_stat}` 引用
// （fs 模块本体保持 `pub(crate)`，不整模块曝面）。
#[cfg(feature = "desktop-host")]
pub use crate::host_api::fs::{fs_canonicalize, fs_read_dir, fs_stat};
#[cfg(feature = "desktop-host")]
mod wsl_fs;

// ==================== 移动 adapter（mobile-host，fork 迁入） ====================

#[cfg(feature = "mobile-host")]
pub mod http_engine;
#[cfg(feature = "mobile-host")]
pub mod ports;
#[cfg(feature = "mobile-host")]
pub mod sql_guard;
#[cfg(feature = "mobile-host")]
pub mod mobile_context;
// 移动装配面的宿主上下文再导出：fork 面路径 `crate::host_api::WasmHostContext`
// 零改动解析（host_impl.rs / host_context_registry 等迁入文件的原引用形态）
#[cfg(feature = "mobile-host")]
pub use mobile_context::WasmHostContext;
// ports 端口符号集过 host_api 顶层（fork host_api.rs:24 同款清单）：
// test_support/mobile.rs 的 MockPorts impl / host_impl 域经此路径引用
#[cfg(feature = "mobile-host")]
pub use ports::{
    AuthEnginePort, ConnectionEnginePort, FsAuthGate, FsAuthOp, HostEnginePorts, PrimaryTarget,
    SafIoPort, UnimplementedPorts, WsReconnectPolicyPort, PORT_NOT_WIRED,
};

// ==================== 测试夹具出口 ====================

/// 三域共用假端口（`sqlite_scaffold` 内的项是 `pub(super)`，即
/// `pub(in wasm_core::host_api)`，故 `database::tests` / `storage::tests` 都够得着）
#[cfg(test)]
pub(crate) mod sqlite_scaffold;

// `test_seed_plugin_secret` 再导出已随 host-auth 域迁宿主退役（票 02 批次 04）：
// 种子函数与 `test_tokens` 夹具现住宿主 `src-tauri/src/{plugin/auth.rs, utils/auth/}`。

// 票 05c：顶层 `grant_permissions`（常编译 pub）签名需要 WasmHostContext，
// 本 use 提常编译（不再 cfg(test)）。桌面装配面（移动形态的权限模型是
// granted_permissions state 内仲裁，无此守卫——fork host_api.rs 无本段）
#[cfg(feature = "desktop-host")]
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
///
/// **常编译 pub（票 02 批次 02 起）**：迁到宿主侧的域端口 adapter（首个 pty）
/// 复用同一条拒绝路径——两处各写一份会让「拒绝必须留痕」变成两份实现。
#[cfg(feature = "desktop-host")]
#[must_use]
pub fn check_permission(
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

// ==================== 宿主侧 adapter 复用面 ====================

/// host-mdns 五条原语的**能力路由**入口（宿主 adapter 用）
///
/// 路由判据（系统组件提供者优先，无提供者回落本域引擎）是内核机制——通用注册表与
/// 寻址（AGENTS §5.1.3 薄壳③），**只有这一份**；宿主侧 adapter（票 02 批次 03 起
/// `src-tauri/src/plugin/mdns.rs`）迁出内核后经本再导出调用，不得在宿主复制判据。
///
/// **`capability` 参数由调用方给**（取值 `bedcode_discovery_engine::routing::CAPABILITY`）：
/// 能力名字面量的真源在该域自报的词汇里（票 02 批次 03），内核侧零字面量。
#[cfg(feature = "desktop-host")]
pub use crate::manager::capability::{
    forward_mdns_advertise, forward_mdns_browse, forward_mdns_is_advertising,
    forward_mdns_stop_advertise, forward_mdns_stop_browse,
};

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
/// 两条来源都在本函数收口：① 仍住本 crate 的域（逐行，见下）；② **宿主自报**的
/// 域（host-kit 装配自报面，票 02 批次 02 起——域实现迁出内核后内核不点名该域）。
///
/// 幂等：各域 `install_ports` 是 `OnceLock::set`，重复装配被忽略而非替换。
#[cfg(feature = "desktop-host")]
pub(crate) fn install_capability_domain_ports(
    host_ctx: &std::sync::Arc<crate::host_api::context::WasmHostContext>,
) {
    // 迁出本 crate 的域 adapter（pty / mdns / peer / ws / http，票 02 批次 02–03）**全部**
    // 由宿主自报装配器装入：宿主在**自己的适配器文件**（`src-tauri/src/plugin/<domain>.rs`）
    // 用 `bedcode_host_kit::submit_domain_ports_installer!` 自报，这里一次遍历装完（字典序）。
    // 本 crate 的测试二进制不链宿主 lib ⇒ 自报为空表——此时由下面的 cfg(test) 替身补位
    // （见 `test_support::kernel_test_domain_ports` 模块文档）；宿主集成测试链的是不带
    // cfg(test) 的本 crate ⇒ 该分支不存在，不会盖掉宿主装配的真端口。
    // 三条 Cargo 边保留（ws / peer / http）：内核仍消费它们的**引擎面**（ws 帧投递窄端口与
    // 端点登记、peer 的 `release_node_for`/`PeerCtx`、http 的 `HttpUnitExecutor`），那不是
    // 能力域端口 adapter 的范畴。
    let host_ports: std::sync::Arc<dyn bedcode_host_kit::ports::HostPorts> = host_ctx.clone();
    let installed = bedcode_host_kit::install_domain_ports(host_ports);
    if !installed.is_empty() {
        tracing::debug!(domains = ?installed, "host-declared capability domain ports installed");
    }
    // **仅本 crate 的 lib 测试二进制**：迁出域（ws / http）的端口替身（宿主自报不在场）
    #[cfg(test)]
    crate::test_support::kernel_test_domain_ports::install(host_ctx);
}

// ==================== Tests ====================

/// 为插件授予权限（manifest 授权路径的测试等价物）
///
/// **常编译 pub（wasm-core 纯净性收口票 05c）**：原在 `#[cfg(test)] mod tests`
/// （wasm-core 内部测试可见）；lib 集成测试（task_e2e 等，独立测试二进制）
/// 看不到 cfg(test) 项，提为常编译 pub 供 `bedcode_desktop_lib::wasm_core::
/// host_api::grant_permissions` 消费（调用点形态与原名一致，零逻辑改动）。
#[cfg(feature = "desktop-host")]
pub fn grant_permissions(ctx: &WasmHostContext, plugin_id: &str, perms: &[&str]) {
    let requested: Vec<String> = perms.iter().map(|s| s.to_string()).collect();
    ctx.permission.grant_permissions(plugin_id, &requested);
}

#[cfg(all(test, feature = "desktop-host"))]
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
