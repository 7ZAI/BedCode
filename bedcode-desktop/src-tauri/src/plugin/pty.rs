//! host-pty 宿主侧适配器（能力域端口 `PtyPorts` 的宿主实现 + 装配自报）
//!
//! 域机制（6 条原语 + 句柄表 + 配额表 + 退出事件 + 限频通知 + WIT 接线）在
//! `bedcode_pty_engine::plugin_binding`；本文件是**宿主侧的另一半**——wasm-core
//! 纯净性收口票 02 批次 02 整文件迁自 `bedcode-wasm-core/src/host_api/pty.rs`
//! （内核自此不依赖 `bedcode-pty-engine`，也不再出现它的任何路径）：
//!
//! 1. [`HostPtyPorts`] —— 能力域端口 `PtyPorts` 的宿主实现；
//! 2. [`PTY_PORTS_INSTALLER`] —— 装配回调（host-kit 自报表，内核装配链遍历调用）；
//! 3. `expect_host_module!` + 强制引用行 —— 白名单条目与链接保证（两者同处）。
//!
//! 回收面（停用回收 / 关停全量回收 / 在册计数）的真源在能力域（`registry`），宿主侧
//! 的调用点直接指过去（`system/lifecycle.rs` / `manager/host/activation.rs` 的钩子），
//! **不经本文件转发**（不留路径兼容垫片）。
//!
//! ## 哪些机制**刻意留在宿主**（AGENTS §5.1.3 允许的四类薄壳）
//!
//! - **权限门**（`pty:spawn` / `pty:io`）：复用 wasm-core 的 `host_api::check_permission`，
//!   同一份 PermissionManager、同一条拒绝 warn 路径；
//! - **消息总线**：`<owner>::pty:exit` / `<owner>::pty:output` 的命名空间门禁、有界
//!   队列与订阅方隔离是总线裁决，能力域只给 topic 与载荷；
//! - **配置快照**（终端默认行列 / 读缓冲 / 生命周期广播容量）：宿主配置真源在
//!   `AppConfig`，能力域不感知它；
//! - **同步↔异步桥与任务派生**：见 [`PtyPorts::block_on_any`] /
//!   [`PtyPorts::spawn_task`] 的实现注释。
//!
//! ## 端口句柄的形态（为什么存 `Arc<dyn HostPorts>`）
//!
//! host-kit 只能命名 [`HostPorts`]（机制内核不认识 `WasmHostContext`——那会把整个
//! 宿主拖进内核），而本适配器**必须**持宿主上下文。折中：字段存擦除后的
//! `Arc<dyn HostPorts>`，每次调用向下转型回 `WasmHostContext`
//! （[`downcast_host`]；类型不符即 panic —— 装配期编程错误，fail-visible，不静默
//! 降级）。`from_ctx` 让测试与同进程调用点直接给具体类型（零转型成本在构造侧）。

use std::any::Any;
use std::sync::Arc;

use bedcode_host_kit::ports::{downcast_host, HostPorts};
use bedcode_pty_engine::plugin_binding::ports::{BoxedBlocked, BoxedTask, PtyHostConfig, PtyPorts};
use bedcode_pty_engine::PtyEngineConfig;
use bedcode_wasm_core::host_api::check_permission;
use bedcode_wasm_core::host_api::context::WasmHostContext;
use bedcode_wasm_core::system::config::AppConfig;

// 能力模块白名单条目与强制引用行**必须同处**（本文件），两者不漂移——漏引用 ⇒
// 该 crate 的 inventory 自报静态不进最终二进制（装载期 missing 方向点名）；漏白名单
// ⇒ unlisted 方向点名。判据与纪律见 `bedcode_host_kit::assembly` 模块文档。
bedcode_host_kit::expect_host_module!(bedcode_pty_engine::plugin_binding::MODULE_NAME);
use bedcode_pty_engine as _;

/// 端口的宿主实现
///
/// 生产路径在**开机期**捕获宿主上下文（`PluginHost::new` 已建好），之后每条原语
/// 零查表地取权限门 / 消息总线 / 配置快照（转型见模块文档）。
pub struct HostPtyPorts {
    /// 宿主上下文（擦除形态；`ctx()` 向下转型取具体类型）
    ctx: Arc<dyn HostPorts>,
}

impl HostPtyPorts {
    /// 端口（生产 / 测试）：从宿主上下文构造
    pub fn from_ctx(ctx: Arc<WasmHostContext>) -> Self {
        // `Arc<WasmHostContext>` → `Arc<dyn HostPorts>`：上下文实现 HostPorts（机制内核的
        // 类型擦除通道），此处只做一次 unsize 转型
        Self { ctx }
    }

    /// 向下转型回具体宿主上下文（类型不符即 panic：装配期编程错误，见模块文档）
    fn ctx(&self) -> &WasmHostContext {
        downcast_host::<WasmHostContext>(self.ctx.as_ref())
    }
}

impl PtyPorts for HostPtyPorts {
    fn check_permission(&self, plugin_id: &str, permission: &str, api: &str) -> bool {
        // 权限门留宿主（AGENTS §5.1.3「安全闸门」——闸门不应可插拔）。复用
        // wasm-core 的 `host_api::check_permission`：同一份 PermissionManager、同一条
        // 拒绝 warn 路径（AGENTS §8 结构化字段），不另起一套判定。拿不到权限管理器时
        // 它返回 false（fail-safe），能力域据此回 `permission denied: …`。
        check_permission(self.ctx(), plugin_id, permission, api)
    }

    fn publish(&self, topic: &str, payload: serde_json::Value) {
        // sender 恒为 `host`（机制常量，能力域两处发布都同值）：总线负责命名空间
        // 门禁、每订阅者有界队列与投递任务派生（域侧注释的「零背压」即此保证）
        self.ctx().message_bus.publish(topic, "host", payload);
    }

    fn config(&self) -> PtyHostConfig {
        // 每次 spawn 现取，不缓存：宿主配置可运行期变更（`AppConfig` 的既有语义）
        let global = AppConfig::global();
        PtyHostConfig {
            default_cols: global.terminal.default_cols,
            default_rows: global.terminal.default_rows,
            engine: PtyEngineConfig {
                lifecycle_capacity: global.channels.lifecycle_capacity,
                read_buffer_size: global.terminal.read_buffer_size,
            },
        }
    }

    fn block_on_any<'a>(&self, fut: BoxedBlocked<'a>) -> Box<dyn Any + Send> {
        // 复用宿主那份唯一的同步↔异步桥（ambient runtime + actix current_thread
        // 自锁规避都是实测产物，能力域不得复制第二份，见 ports 模块文档）
        bedcode_wasm_core::runtime_util::block_on_async(fut)
    }

    fn spawn_task(&self, name: &'static str, task: BoxedTask) {
        // 显式派生到 ambient runtime：WASI 预打开模式下插件调用跑在**无 runtime
        // handle 的阻塞线程**上，那里 `tokio::spawn` 立即 panic 并污染 wasmtime Store
        bedcode_wasm_core::system::error_boundary::spawn_with_error_boundary_on(
            &bedcode_wasm_core::runtime_util::ambient_handle(),
            name,
            task,
        );
    }
}

/// 装配回调（host-kit 自报）：开机期装本域端口
///
/// **两处登记**（各登一份等价对象，行为一致）：
/// - **实例级** `WasmHostContext::set_domain_ports`：供插件实例经
///   `bedcode_host_kit::ports::HostPorts::domain_ports` 取回，使权限判定落在**本实例
///   的** PermissionManager 上（多上下文场景下不会读到别人的授权）；
/// - **进程级** `bedcode_pty_engine::plugin_binding::install_ports`：供插件实例
///   之外的非实例路径（停用回收转发 / 关停全量回收）使用。
///
/// **必须早于任何插件激活**——内核装配链（`install_capability_domain_ports`，生产经
/// `PluginHost::new`、无头测试经 `setup_wasm_runtime`）遍历自报表时调用；guest 一调
/// `host-pty` 原语就取端口，取不到直接 panic（fail-visible）。
fn install(host: Arc<dyn HostPorts>) {
    let ctx = downcast_host::<WasmHostContext>(host.as_ref());
    // 端口对象持一份引用（`ctx` 的向下转型借用仍在用 host，故不移动它）
    let ports: Arc<dyn PtyPorts> = Arc::new(HostPtyPorts { ctx: Arc::clone(&host) });
    ctx.set_domain_ports(
        bedcode_pty_engine::plugin_binding::DOMAIN,
        Arc::new(Arc::clone(&ports)) as Arc<dyn Any + Send + Sync>,
    );
    bedcode_pty_engine::plugin_binding::install_ports(ports);
}

/// 端口装配器静态（与白名单条目 / 强制引用行同处；内核装配链遍历本自报表）
pub static PTY_PORTS_INSTALLER: bedcode_host_kit::DomainPortsInstaller = bedcode_host_kit::DomainPortsInstaller {
    name: bedcode_pty_engine::plugin_binding::MODULE_NAME,
    install,
};

bedcode_host_kit::submit_domain_ports_installer!(PTY_PORTS_INSTALLER);

#[cfg(test)]
mod tests {
    use super::*;

    /// 白名单声明、接口路径、权限位三件与能力域描述符逐字一致
    ///
    /// 为什么在宿主断言而不是只在能力域自证：白名单条目在**宿主**（本文件），
    /// 接口路径与权限位是装载期一致性核对的输入，两处字面量必须相等。
    /// `expected_host_modules()` 含本模块名即证明 `expect_host_module!` 行仍在且被收集
    /// （漏了 ⇒ 装载期 unlisted 点名，但那时已在运行期，本用例让它在单测就红）。
    #[test]
    fn host_module_declaration_matches_capability_domain_desc() {
        let module_name = bedcode_pty_engine::plugin_binding::MODULE_NAME;
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&module_name),
            "能力模块白名单缺 {module_name}（expect_host_module! 行被删/未收集）"
        );
        assert_eq!(module_name, "pty", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            bedcode_pty_engine::plugin_binding::MODULE_INTERFACES,
            &["bedcode:plugin/host-pty"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(
            bedcode_pty_engine::plugin_binding::MODULE_PERMISSIONS,
            &["pty:spawn", "pty:io"],
            "权限位必须与 `bedcode.wit` / SDK 权限表逐字一致（装载期一致性核对用）"
        );
    }

    /// 配置快照逐字取自宿主配置真源（引擎不再读 AppConfig，快照错了行为会静默偏移）
    #[test]
    fn config_snapshot_mirrors_host_app_config() {
        // 夹具与生产同一条装配路径（顺便覆盖：本装配器已在 setup 期被内核装配链调用）
        let (_, host_ctx) = bedcode_wasm_core::test_support::setup_wasm_runtime();
        let global = AppConfig::global();
        let ports = HostPtyPorts::from_ctx(host_ctx);
        let snapshot = ports.config();
        assert_eq!(snapshot.default_cols, global.terminal.default_cols);
        assert_eq!(snapshot.default_rows, global.terminal.default_rows);
        assert_eq!(snapshot.engine.lifecycle_capacity, global.channels.lifecycle_capacity);
        assert_eq!(snapshot.engine.read_buffer_size, global.terminal.read_buffer_size);
    }
}
