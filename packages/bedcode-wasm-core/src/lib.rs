//! WASM 内核（bedcode-wasm-core）——插件系统微内核整核（可复用 crate）
//!
//! 桌面端 `wasm_core` 整核抽出（.scratch/2026-10-06-wasm-core-whole-crate/spec.md）：
//! 插件核心机制（`manager` / `security` / `host_api` / `bus` / `config` / `monitor` /
//! `permission` / `runtime_util` / `intercall` / `storage`）+ 引擎面（`db` /
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
/// 异步桥基础设施：`manager` / `host_api` / `security` 共用的中立层，
/// 自身不依赖任何 wasm_core 兄弟模块（票 01）
pub mod runtime_util;
pub mod security;
/// 插件存储中立层（原 manager/storage.rs 下沉，票 03）：`security` / `host_api` /
/// `manager` 皆可引用，自身只依赖 `crate::db`
pub mod storage;
/// 引擎级配置 / 文件定位 / 进程创建（lib 的 `system.rs` 组合根经垫片零改动引用）
pub mod system;
/// 连接身份 + 测试夹具（lib 集成测试经垫片消费；认证桥接 / 会话窄转发已回宿主 lib，票 05）
pub mod utils;
/// 测试基建（常编译公开，lib 集成测试消费；来源见模块头注释）
pub mod test_support;

// ==================== Facade 再导出 ====================
// 外部消费方（Tauri 命令层、system、peer 等）只经 facade 引用，
// 不感知模块内部结构

pub use bus::{BusMessageHandler, MessageBus};
pub use manager::host;
pub use manager::host::api_bridge;
pub use manager::host::PluginHost;
// 引擎定制面（wasmtime-engine-config A+B）：第三方宿主写 `EngineCustomizer`
// 钩子时需要命名 `wasmtime::Config`，经此再导出即可，不必自己加 wasmtime
// 依赖（ADR 0019 双端锁版：版本只在本 crate 锁一处）
pub use manager::runtime::{EngineCustomizer, EngineSetup};
#[cfg(debug_assertions)]
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
             bedcode-pty-engine；本 crate 只保留 `host_api::pty` 端口 adapter）"
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
}
