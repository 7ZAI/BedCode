//! WASM 内核（bedcode-wasm-core）——插件系统微内核整核（可复用 crate）
//!
//! 桌面端 `wasm_core` 整核抽出（.scratch/2026-10-06-wasm-core-whole-crate/spec.md）：
//! 插件核心机制（`manager` / `security` / `host_api` / `bus` / `config` / `monitor` /
//! `permission` / `runtime_util` / `intercall` / `storage`）+ 引擎面（`db` /
//! `system`）从 bin crate 整体迁出，任何 Tauri 宿主可直接 path 依赖本 crate
//! 获得插件机制。（原 `enums` 引擎面垫片与 `utils/auth` 连接身份垫片均已随
//! 无消费者整体退役：线协议真源在 SDK `bedcode-plugin-api::wire`，连接身份
//! 真源在 `bedcode-server-base`，2026-10-10。）
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

// 形态互斥守卫（票 06 批次 03）：desktop-host 与 mobile-host 各挂一套 bindgen
// （`manager/runtime/desktop/component.rs` 绑桌面生成物 / `mobile/component.rs`
// 绑移动生成物），两套都开会在同 crate 生成同名 `bedcode` 模块冲突——编译期
// 显性拒绝，禁止静默双开。
#[cfg(all(feature = "desktop-host", feature = "mobile-host"))]
compile_error!(
    "features `desktop-host` and `mobile-host` are mutually exclusive: \
     each mounts its own WIT bindgen (same `bedcode` module name)"
);

pub mod bus;
pub mod config;
/// 引擎面：宿主主库（schema.sql 单一事实源，ADR 0036「机制与真源同侧」）
pub mod db;
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
/// 命令桥（api_bridge，权限校验后执行操作）。
/// 子模块按形态 cfg 分叉（票 06 批次 03）：桌面 WIT impl 域随 `desktop-host`，
/// 移动 adapter（http_engine / ports / sql_guard / mobile_context）随 `mobile-host`
pub mod host_api;
/// 宿主→插件互调客户端（中立层，ADR 0033 从 `utils/auth/auth_center.rs` 上提）：
/// JSON-RPC 2.0 over host-bus 的通用发起端 + 请求 id 分配。
/// 桌面装配面（互调回复道编排留内核，票 02 批次 05；移动形态的互调经
/// `mobile_runtime` 的 host_impl/conn 域——fork 无本模块）
#[cfg(feature = "desktop-host")]
pub mod intercall;
pub mod manager;
pub mod monitor;
pub mod permission;
/// 异步桥基础设施：`manager` / `host_api` / `security` 共用的中立层，
/// 自身不依赖任何 wasm_core 兄弟模块（票 01）
pub mod runtime_util;
pub mod security;
/// 插件存储中立层（原 manager/storage.rs 下沉，票 03）：`security` / `host_api` /
/// `manager` 皆可引用，自身只依赖 `crate::db`
pub mod storage;
/// 引擎级配置 / 文件定位（lib 的 `system.rs` 组合根经垫片零改动引用；进程创建垫片已随无消费者退役）
pub mod system;
/// 测试基建（常编译公开，lib 集成测试消费；来源见模块头注释）。
/// 桌面主体（setup_wasm_runtime / build_host_ctx_at / TestInstanceDispatcher…）
/// 随 `desktop-host`；移动面（test_support/mobile.rs = fork test_support.rs 迁入）
/// 随 `mobile-host` + `test-support`
#[cfg(any(
    feature = "desktop-host",
    all(feature = "mobile-host", feature = "test-support")
))]
pub mod test_support;

// ==================== 移动装配面（mobile-host，fork 迁入，票 06 批次 03） ====================

/// 宿主上下文注册表（fork 迁入）：移动装配面的 AppHandle / 端口注册面
#[cfg(feature = "mobile-host")]
pub(crate) mod host_context_registry;
/// 终端输出流网关（fork 迁入）：`host-terminal-stream` 域的流转发闸门
/// （移动独有 interface，桌面无此域——ADR 0018）
#[cfg(feature = "mobile-host")]
pub mod terminal_stream_gateway;
/// 错误类型垫片（票 06 批次 03）：双端 AppError 真源统一到
/// `bedcode-server-base::error`（fork 自持 error.rs 退役）。移动装配面迁入文件
/// 的 `crate::error::` 路径经此零改动解析；桌面侧经文件尾 `pub use` 的路径不变。
pub mod error {
    pub use bedcode_server_base::error::*;
}

// ==================== Facade 再导出 ====================
// 外部消费方（Tauri 命令层、system、peer 等）只经 facade 引用，
// 不感知模块内部结构

pub use bus::{BusMessageHandler, MessageBus};
#[cfg(feature = "desktop-host")]
pub use manager::host;
#[cfg(feature = "desktop-host")]
pub use manager::host::api_bridge;
#[cfg(feature = "desktop-host")]
pub use manager::host::PluginHost;
// 引擎定制面（wasmtime-engine-config A+B）：第三方宿主写 `EngineCustomizer`
// 钩子时需要命名 `wasmtime::Config`，经此再导出即可，不必自己加 wasmtime
// 依赖（ADR 0019 双端锁版：版本只在本 crate 锁一处）
#[cfg(feature = "desktop-host")]
pub use manager::runtime::{EngineCustomizer, EngineSetup};
#[cfg(all(debug_assertions, feature = "desktop-host"))]
pub use manager::watcher;
pub use security::fs_auth::FsAuthChecker;
pub use storage::PluginStorage;
/// wasmtime 再导出（宿主定制钩子面；版本与本 crate 依赖同源，ADR 0019）
pub use wasmtime;

// 引擎级错误类型（真源 bedcode-server-base，本 crate 与 lib 共用同一份；
// lib 侧 `system/error.rs` 垫片经 `pub use bedcode_wasm_core` 路径不变）
pub use bedcode_server_base::error::{AppError, Result};

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    /// 本 crate 禁止出现的模块声明形态（PTY 已整面迁出）
    ///
    /// 独立成函数的原因：禁用名单若内联在断言里，它自己就是一条「含 `mod pty;`
    /// 字面量的代码行」，会被按整行比对的判据误判（锁空转）。
    fn forbidden_module_forms() -> Vec<String> {
        ["pty", "pty_output"]
            .into_iter()
            .flat_map(|name| {
                ["mod ", "pub mod ", "pub(crate) mod ", "pub(super) mod "]
                    .into_iter()
                    .map(move |prefix| format!("{prefix}{name};"))
            })
            .collect()
    }

    /// 行归一化：剥掉 `#[…]` 属性跨度 + 压平空白
    ///
    /// **为什么必须剥属性**：`#[path = "…"] pub mod pty;` 写在同一行时，原始整行既不
    /// 等于任何禁用形态、`starts_with("mod ")` 也不成立（行首是 `[`）——只按整行精确
    /// 比对会放它过去。属性的存在与否与「PTY 模块是否回到内核」无关，故先剥掉再比。
    /// 压平空白同理（`pub  mod pty;` 绕过逐字比对）。
    fn normalized_module_line(line: &str) -> String {
        let mut out = String::new();
        let mut chars = line.trim().chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '#' && chars.peek() == Some(&'[') {
                // 跳过整个属性跨度（`#[` … `]`，含嵌套括号）
                let mut depth = 0usize;
                for inner in chars.by_ref() {
                    match inner {
                        '[' => depth += 1,
                        ']' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                continue;
            }
            if !ch.is_whitespace() {
                out.push(ch);
            }
        }
        out
    }

    /// 整核抽出后的模块可见性锁：核心机制模块全部公开（lib 与集成测试经垫片消费）
    #[test]
    fn core_modules_are_public() {
        let _: fn() -> crate::db::Database;
        let _: fn(crate::manager::host::PluginHost);
        let _: fn(crate::host_api::context::WasmHostContext);
        let _: fn(crate::bus::MessageBus);
        let _: fn(crate::security::fs_auth::FsAuthChecker);
        let _: fn(crate::storage::PluginStorage);
    }

    /// `src/pty.rs` **反向**防回接锁（pty-capability-domain 票 D3/D6）
    ///
    /// 极性翻转的原因：原来的锁守「垫片只允许 re-export」（引擎已迁出、垫片保路径）。
    /// 能力域整面迁出后**垫片本身也删了**——本 crate 不再有任何 PTY 引擎面或
    /// `crate::pty::*` 路径。此时「垫片被回接」有两种形态，都必须红：
    ///
    /// 1. 有人把 `src/pty.rs` 重新落回来（含 `pub use` 垫片或整份引擎拷贝）；
    /// 2. 有人用 `pub mod pty;` 在本 crate 另起一个 PTY 模块。
    ///
    /// 空目录也算回接信号（`src/pty/` 会被 `empty_dir_lock` 单独抓住）。
    #[test]
    fn pty_module_must_not_return_to_wasm_core() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let file = manifest_dir.join("src/pty.rs");
        assert!(
            !file.exists(),
            "src/pty.rs 不得回到 wasm-core（PTY 引擎与 host-pty 能力域的真源是 \
             bedcode-pty-engine；端口 adapter 自票 02 批次 02 起住宿主 \
             `src-tauri/src/plugin/pty.rs`，本 crate 不再持有任何 PTY 面）"
        );
        // 票 02 批次 02：端口 adapter 也迁出了本 crate——回接即宿主侧 adapter 出现
        // 第二份实现（装配期 `defined twice` 或静默双写），必须显性红。
        let adapter = manifest_dir.join("src/host_api/pty.rs");
        assert!(
            !adapter.exists(),
            "src/host_api/pty.rs 不得回到 wasm-core（pty 端口 adapter 与强制引用行 / \
             白名单条目同住宿主 `src-tauri/src/plugin/pty.rs`）"
        );
        let dir = manifest_dir.join("src/pty");
        assert!(
            !dir.exists()
                || std::fs::read_dir(&dir)
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(true),
            "src/pty/ 不得回到 wasm-core（同上：PTY 引擎面不在内核）"
        );
        // **按归一化整行比对模块声明形态**（不按子串）：本锁自己的代码里就带着这些
        // 形态的字面量（禁用名单）与文档注释（说明为什么锁），子串匹配会把它们
        // 判红——锁自己判红 = 锁空转。归一化（剥 `#[…]` 属性 + 压平空白）见
        // [`normalized_module_line`]：只按字面整行比对会放过
        // `#[path = "…"] pub mod pty;` 这类带属性写法。
        let lib_source =
            std::fs::read_to_string(manifest_dir.join("src/lib.rs")).expect("读取 src/lib.rs 失败");
        let forbidden: Vec<String> = forbidden_module_forms()
            .iter()
            .map(|f| normalized_module_line(f))
            .collect();
        let module_decl = lib_source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .find(|line| {
                forbidden
                    .iter()
                    .any(|form| normalized_module_line(line) == *form)
            });
        assert!(
            module_decl.is_none(),
            "lib.rs 出现 PTY 模块声明（行：{module_decl:?}）：PTY 能力域已整面迁出，\
             内核不得回接 PTY 模块"
        );
        assert!(
            !std::path::Path::new(manifest_dir)
                .join("src/enums/pty_status.rs")
                .exists(),
            "enums/pty_status.rs 不得回到 wasm-core（PTY 终态枚举真源在 bedcode-pty-engine）"
        );
    }

    /// `src/host_api/http.rs` 的**反向**防回接锁（wasm-core 纯净性收口票 02 批次 03）
    ///
    /// 与其他四域（pty / mdns / peer / ws）不同，本文件**不是整文件迁走**：内核仍留
    /// `HttpUnitExecutor`（host-task 面执行器，注册点必须与留 core 的任务引擎同侧）。
    /// 回接形态因此不是「文件回来」，而是「端口 adapter 偷偷长回同一个文件」——
    /// `HostHttpPorts` 与 `set_domain_ports`（域端口装配）一旦在本文件复活，就会有
    /// 第二份 adapter 实现 / 二次装配（`OnceLock` 首个装配者胜出 ⇒ 静默投错上下文）。
    ///
    /// 判据按**代码行**比对（跳注释行）：本文件的模块文档正当地提到被迁走的东西，
    /// 注释不该被判红（锁自己判红 = 锁空转）。
    #[test]
    fn http_adapter_must_not_return_to_wasm_core() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("src/host_api/http.rs");
        let source = std::fs::read_to_string(&path)
            .expect("src/host_api/http.rs 必须在场（HttpUnitExecutor 的住所）");
        // 正面锚点（防空转）：执行器本尊必须在，否则本锁可能对着一个空文件恒绿
        assert!(
            source.contains("HttpUnitExecutor"),
            "src/host_api/http.rs 必须仍含 HttpUnitExecutor（本锁的正面锚点）"
        );
        for needle in ["HostHttpPorts", "set_domain_ports"] {
            let hit = source
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .find(|line| line.contains(needle));
            assert!(
                hit.is_none(),
                "src/host_api/http.rs 的代码行出现 `{needle}`（行：{hit:?}）：http 端口 adapter \
                 与装配自报同住宿主 `src-tauri/src/plugin/http.rs`，不得回流内核"
            );
        }
    }

    /// `src/host_api/crypto.rs` **反向**防回接锁（wasm-core 纯净性收口票 02 批次 04）
    ///
    /// host-crypto 已按**路径 B** 整面迁宿主 `src-tauri/src/plugin/crypto.rs`（宿主自带
    /// bindgen + `Host` impl + 域函数同处；算法真源 `bedcode-crypto-engine` 不变）。
    /// 文件删除即护栏：回接形态 = 重新落回 `src/host_api/crypto.rs`（内核侧第二份
    /// WIT 实现 / 第二份权限门），或 `host_api.rs` 重新声明 `mod crypto;`——前者让
    /// linker 出现两个 `host-crypto` 注册来源，后者是空壳回潮的前奏，都必须显性红。
    ///
    /// 内核保留的合法残余（**不在**禁区）：`crate::crypto` shim（bedcode-crypto-engine
    /// 的路径垫片）曾列于此，已随无消费者整体退役（2026-10-10：`host_api/crypto.rs`
    /// 迁宿主后内核无消费方，`src/crypto.rs` 与 `src/system/process.rs` 同批删除；
    /// `permission.rs` 无 CRYPTO 词汇，旧注释与事实不符一并修正）——锁只针对 WIT
    /// 域实现文件本体。
    #[test]
    fn crypto_domain_must_not_return_to_wasm_core() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let file = manifest_dir.join("src/host_api/crypto.rs");
        assert!(
            !file.exists(),
            "src/host_api/crypto.rs 不得回到 wasm-core（host-crypto 域真源在宿主 \
             `src-tauri/src/plugin/crypto.rs`，票 02 批次 04 路径 B；回接即第二份 \
             WIT 实现 / 第二份权限门）"
        );
        // 防空转：真垫片曾作为「锁扫描对象在场」锚点，已随无消费者整体退役
        // （2026-10-10）。锁仍防 host_api/crypto.rs 回潮（文件不存在断言）。
    }

    /// `src/host_api/auth.rs` **反向**防回接锁（wasm-core 纯净性收口票 02 批次 04）
    ///
    /// host-auth 已按**路径 B** 整面迁宿主 `src-tauri/src/plugin/auth.rs`（宿主自带
    /// bindgen + `Host` impl + 域函数同处）。回接形态 = 域实现文件落回
    /// `src/host_api/auth.rs`（内核侧第二份 WIT 实现 / 第二份权限门）。
    ///
    /// 内核保留的合法残余（**不在**禁区）：
    /// - `src/host_api/auth_center.rs`——认证中心注册表真源（内核 boot 启动门 /
    ///   activation 停用回收 / 宿主裁决面 / 测试闸门四方消费，裁决见宿主 adapter 模块
    ///   文档）；
    /// - `permission.rs` 的 `PERMISSION_AUTH` 词汇再导出。
    ///   （`src/utils/auth/identity.rs` 连接身份垫片已随无消费者整体退役，2026-10-10：
    ///   宿主裁决面 `auth_center.rs` 改直连 `bedcode-server-base::identity`。）
    #[test]
    fn auth_domain_must_not_return_to_wasm_core() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let file = manifest_dir.join("src/host_api/auth.rs");
        assert!(
            !file.exists(),
            "src/host_api/auth.rs 不得回到 wasm-core（host-auth 域真源在宿主 \
             `src-tauri/src/plugin/auth.rs`，票 02 批次 04 路径 B；回接即第二份 \
             WIT 实现 / 第二份权限门）"
        );
        // 防空转：刻意留内核的两份残余必须仍在场
        assert!(
            manifest_dir.join("src/host_api/auth_center.rs").exists(),
            "src/host_api/auth_center.rs（认证中心注册表，四方消费的通用注册表薄壳）必须保留"
        );
    }

    /// `src/host_api/{task,process,app,timer,connection}.rs` **反向**防回接锁
    /// （wasm-core 纯净性收口票 02 批次 05）
    ///
    /// host-task / host-process / host-app / host-timer / host-connection 五域已按
    /// **路径 B** 整面迁宿主 `src-tauri/src/plugin/{task,process,app,timer,
    /// connection}.rs`（宿主自带 bindgen + `Host` impl + 域函数同处）。回接形态 =
    /// 域实现文件落回 `src/host_api/<domain>.rs`（内核侧第二份 WIT 实现 / 第二份
    /// 权限门 ⇒ 桌面二进制装配期 `defined twice` 或静默双写）。
    ///
    /// 内核保留的合法残余（**不在**禁区）：
    /// - `src/host_api/unit_executor.rs`——执行器策略接口 + 自报收集面（消费方是
    ///   留内核的任务引擎 `manager::task`，引擎不迁则接口不迁）；
    /// - `src/manager/task.rs`——core-task 任务引擎（`TaskEngine` 实现方）；
    /// - `TaskEngine` / `ProcessScope` / `ServicesScope` 等 scope trait
    ///   （`host_api/context.rs`，宿主域函数的窄入参）。
    #[test]
    fn path_b_domains_must_not_return_to_wasm_core() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        const MIGRATED: &[(&str, &str)] = &[
            ("task", "票 02 批次 05"),
            ("process", "票 02 批次 05"),
            ("app", "票 02 批次 05"),
            ("timer", "票 02 批次 05"),
            ("connection", "票 02 批次 05"),
        ];
        for (domain, ticket) in MIGRATED {
            let file = manifest_dir.join(format!("src/host_api/{domain}.rs"));
            assert!(
                !file.exists(),
                "src/host_api/{domain}.rs 不得回到 wasm-core（host-{domain} 域真源在宿主 \
                 `src-tauri/src/plugin/{domain}.rs`，{ticket} 路径 B；回接即第二份 \
                 WIT 实现 / 第二份权限门）"
            );
        }
        // 防空转：刻意留内核的机制面必须仍在场（否则本锁可能对着早已搬空的机制恒绿）
        assert!(
            manifest_dir.join("src/host_api/unit_executor.rs").exists(),
            "src/host_api/unit_executor.rs（执行器接口 + 自报收集面，留内核任务引擎消费）必须保留"
        );
        assert!(
            manifest_dir.join("src/manager/task.rs").exists(),
            "src/manager/task.rs（core-task 任务引擎，TaskEngine 实现方）必须保留"
        );
    }

    /// `src/host_api/api.rs` **反向**防回接锁（wasm-core 纯净性收口票 02 批次 05）
    ///
    /// host-api-call 的 **WIT impl** 已迁宿主 `src-tauri/src/plugin/api_call.rs`
    /// （路径 B 薄转发），但本文件**刻意留内核**：回复道编排是内核互调机制
    /// （`intercall` / `auth_center` / `call_plugin_api_host` 消费）。回接形态因此
    /// 不是「文件回来」，而是「WIT 实现长回同一个文件」——内核 bindgen 生成的
    /// `host_api_call::Host` impl 一旦复活，桌面二进制会出现第二份注册来源
    /// （`defined twice`）。判据按**代码行**比对（跳注释行）：本文件的模块文档
    /// 正当地提到迁移史，注释不该被判红。
    ///
    /// **合法残余（不在本锁判据内）**：`manager/runtime/component.rs` 的
    /// `#[cfg(test)]` host-api-call 替身 impl——只进内核测试二进制（SDK 夹具经
    /// 互调 client 静态 import 该 interface，迁移后替身顶上转发内核编排），
    /// 生产二进制的唯一注册来源仍是宿主薄转发。
    #[test]
    fn api_call_wit_impl_must_not_return_to_wasm_core() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("src/host_api/api.rs");
        let source = std::fs::read_to_string(&path).expect("src/host_api/api.rs 必须在场（回复道编排的住所）");
        // 正面锚点（防空转）：编排机制本体必须在，否则本锁可能对着一个空文件恒绿
        assert!(
            source.contains("HOST_API_CALLER_ID") && source.contains("ReplyHandler"),
            "src/host_api/api.rs 必须仍含回复道编排（HOST_API_CALLER_ID / ReplyHandler，本锁的正面锚点）"
        );
        for needle in ["host_api_call::Host for WasmPluginState", "add_to_linker"] {
            let hit = source
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .find(|line| line.contains(needle));
            assert!(
                hit.is_none(),
                "src/host_api/api.rs 的代码行出现 `{needle}`（行：{hit:?}）：host-api-call 的 \
                 WIT impl 已迁宿主 `src-tauri/src/plugin/api_call.rs`（票 02 批次 05 路径 B），\
                 不得回流内核"
            );
        }
    }

    /// v36 交集切片四接口**反向**防回接锁（票 02 批次 06）
    ///
    /// `host-fs-desktop`（3）/ `host-platform-desktop`（4）/ `host-events-desktop`
    /// （notify）的 **WIT impl 落宿主** `src-tauri/src/plugin/{fs,platform,events}.rs`
    /// （路径 B，自带 bindgen 装配）；`abi-form` 是 guest **导出**（SDK `wasm_entry!`
    /// 宏 + 宿主 verify_abi 消费），源在 SDK，内核无对应实现。内核只留交集子集的
    /// 域实现：`fs.rs` 的 `fs_read_dir` / `fs_canonicalize` / `fs_stat` **实现本体
    /// 显式留内核**（双消费者：内核 `FsUnitExecutor` 的 `fs.read-dir` / `fs.stat`
    /// 单元 + 宿主 WIT impl，与 `check_permission` 提 pub 同款裁决，见 `host_api.rs`
    /// 模块文档）。
    ///
    /// 回接形态 = 域文件再现桌面扩展 interface 词汇。**合法残余（不在判据内）**：
    /// `manager/runtime/component.rs` 的 `#[cfg(test)]` 三接口替身（只进内核测试
    /// 二进制——SDK 夹具静态 import 整个 world，替身以 headless 显性拒绝顶上；
    /// 生产二进制的唯一注册来源仍是宿主 impl）；`system/wsl.rs` 已删（发行版列举
    /// 迁宿主 `src-tauri/src/system/wsl.rs`），`system.rs` 只留说明注释。
    #[test]
    fn desktop_sliced_interfaces_must_not_return_to_wasm_core() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // 正面锚点（防空转）：交集核心实现必须在场，否则锁对着空文件恒绿
        let fs_src = std::fs::read_to_string(manifest_dir.join("src/host_api/fs.rs"))
            .expect("src/host_api/fs.rs 必须在场（交集 fs 实现 + FsUnitExecutor）");
        assert!(
            fs_src.contains("pub fn fs_read_dir") && fs_src.contains("pub fn fs_canonicalize")
                && fs_src.contains("pub fn fs_stat") && fs_src.contains("pub(crate) fn fs_read"),
            "src/host_api/fs.rs 必须仍含交集 fs 实现与提 pub 的三扩展函数实现本体（本锁正面锚点）"
        );
        let platform_src = std::fs::read_to_string(manifest_dir.join("src/host_api/platform.rs"))
            .expect("src/host_api/platform.rs 必须在场（交集 pick-* 实现）");
        assert!(
            platform_src.contains("pub(crate) fn platform_pick_files")
                && platform_src.contains("pub(crate) fn platform_pick_folder"),
            "src/host_api/platform.rs 必须仍含交集 pick-files / pick-folder（本锁正面锚点）"
        );
        let events_src = std::fs::read_to_string(manifest_dir.join("src/host_api/events.rs"))
            .expect("src/host_api/events.rs 必须在场（交集 emit 实现）");
        assert!(
            events_src.contains("pub(crate) fn emit_event"),
            "src/host_api/events.rs 必须仍含交集 emit_event（本锁正面锚点）"
        );
        // 反向：三个域文件 + system.rs 的代码行不得再现桌面扩展 interface 词汇
        // （跳注释行——模块文档正当地提到迁移史与落点，注释不该被判红）
        let system_src = std::fs::read_to_string(manifest_dir.join("src/system.rs"))
            .expect("src/system.rs 必须在场");
        for (file, source) in [
            ("src/host_api/fs.rs", fs_src.as_str()),
            ("src/host_api/platform.rs", platform_src.as_str()),
            ("src/host_api/events.rs", events_src.as_str()),
            ("src/system.rs", system_src.as_str()),
        ] {
            // 票 04：needle 补 `host_websocket_server` / `host_http_endpoint` /
            // `host_fs_mobile`——三个新拆分接口的 WIT impl 在能力域 crate /
            // 移动 fork（host-fs-mobile 归移动端），不得回流内核。
            for needle in [
                "host_fs_desktop",
                "host_platform_desktop",
                "host_events_desktop",
                "system::wsl",
                "host_websocket_server",
                "host_http_endpoint",
                "host_fs_mobile",
            ] {
                let hit = source
                    .lines()
                    .filter(|line| !line.trim_start().starts_with("//"))
                    .find(|line| line.contains(needle));
                assert!(
                    hit.is_none(),
                    "{file} 的代码行出现 `{needle}`（行：{hit:?}）：v36/v37 交集切片桌面扩展接口的 \
                     WIT impl 已迁宿主 `src-tauri/src/plugin/{{fs,platform,events}}.rs` / \
                     `src/system/wsl.rs` / 能力域 crate（host-websocket-server / \
                     host-http-endpoint，票 02 批次 06 + 票 04），不得回流内核"
                );
            }
        }
    }
}
