//! host-pty 宿主侧适配器（能力域已迁出本模块）
//!
//! **本文件只剩两样东西**，原 611 行的 pty 域实现（6 条原语 + 句柄表 + 配额表 +
//! 退出事件 + 限频通知）与 337 行的输出通知装饰器已整体迁入
//! `bedcode_pty_engine::plugin_binding`（pty-capability-domain 票 D1/D2：WIT 接线随
//! 域机制一并迁出内核，边界走窄端口 trait——同 http / ws / peer-net / mdns 四域）：
//!
//! 1. [`HostPtyPorts`] —— 能力域端口 `PtyPorts` 的宿主实现；
//! 2. [`install`] —— 开机期装配端口（供 `PluginHost` 装配链调用）。
//!
//! 回收面（停用回收 / 关停全量回收 / 在册计数）的真源在能力域（`registry`），宿主侧
//! 的调用点直接指过去，**不经本文件转发**（不留路径兼容垫片）。
//!
//! ## 哪些机制**刻意留在宿主**（AGENTS §5.1.3 允许的四类薄壳）
//!
//! - **权限门**（`pty:spawn` / `pty:io`）：复用既有 `host_api::check_permission`，
//!   同一份 PermissionManager、同一条拒绝 warn 路径；
//! - **消息总线**：`<owner>::pty:exit` / `<owner>::pty:output` 的命名空间门禁、有界
//!   队列与订阅方隔离是总线裁决，能力域只给 topic 与载荷；
//! - **配置快照**（终端默认行列 / 读缓冲 / 生命周期广播容量）：宿主配置真源在
//!   `AppConfig`，能力域不感知它；
//! - **同步↔异步桥与任务派生**：见 [`PtyPorts::block_on_any`] /
//!   [`PtyPorts::spawn_task`] 的实现注释。

use std::any::Any;
use std::sync::Arc;

use bedcode_pty_engine::plugin_binding::ports::{BoxedBlocked, BoxedTask, PtyHostConfig, PtyPorts};
use bedcode_pty_engine::PtyEngineConfig;

use crate::host_api::context::WasmHostContext;
use crate::system::config::AppConfig;

/// 能力模块名（必须与 `bedcode_pty_engine::plugin_binding::DESC.name` 逐字一致）
///
/// 它同时是 `component.rs` 里 `HOST_MODULES` 白名单的键——两者不同即红。
pub const HOST_MODULE_NAME: &str = "pty";

/// 端口的宿主实现
///
/// 生产路径在**开机期**捕获 `Arc<WasmHostContext>`（`PluginHost::new` 已建好），
/// 之后每条原语零查表地取权限门 / 消息总线 / 配置快照。
pub struct HostPtyPorts {
    /// 宿主上下文（权限门、消息总线都在它身上；配置读全局 `AppConfig`）
    ctx: Arc<WasmHostContext>,
}

impl HostPtyPorts {
    /// 端口（生产）：从宿主上下文取权限管理器与消息总线
    pub fn from_ctx(ctx: Arc<WasmHostContext>) -> Self {
        Self { ctx }
    }
}

impl PtyPorts for HostPtyPorts {
    fn check_permission(&self, plugin_id: &str, permission: &str, api: &str) -> bool {
        // 权限门留宿主（AGENTS §5.1.3「安全闸门」——闸门不应可插拔）。复用既有
        // `host_api::check_permission`：同一份 PermissionManager、同一条拒绝 warn
        // 路径（AGENTS §8 结构化字段），不另起一套判定。拿不到权限管理器时它返回
        // false（fail-safe），能力域据此回 `permission denied: …`。
        super::check_permission(self.ctx.as_ref(), plugin_id, permission, api)
    }

    fn publish(&self, topic: &str, payload: serde_json::Value) {
        // sender 恒为 `host`（机制常量，能力域两处发布都同值）：总线负责命名空间
        // 门禁、每订阅者有界队列与投递任务派生（域侧注释的「零背压」即此保证）
        self.ctx.message_bus.publish(topic, "host", payload);
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
        crate::runtime_util::block_on_async(fut)
    }

    fn spawn_task(&self, name: &'static str, task: BoxedTask) {
        // 显式派生到 ambient runtime：WASI 预打开模式下插件调用跑在**无 runtime
        // handle 的阻塞线程**上，那里 `tokio::spawn` 立即 panic 并污染 wasmtime Store
        crate::system::error_boundary::spawn_with_error_boundary_on(
            &crate::runtime_util::ambient_handle(),
            name,
            task,
        );
    }
}

/// 开机期装配能力域端口（幂等：重复装配被忽略，见 `install_ports`）
///
/// **两处登记**（各登一份等价对象，行为一致）：
/// - **进程级** [`bedcode_pty_engine::plugin_binding::install_ports`]：供插件实例
///   之外的非实例路径（停用回收转发 / 关停全量回收）使用；
/// - **实例级** [`WasmHostContext::set_domain_ports`]：供插件实例经
///   `bedcode_host_kit::ports::HostPorts::domain_ports` 取回，使权限判定落在**本实例
///   的** PermissionManager 上（多上下文场景下不会读到别人的授权）。
///
/// **必须早于任何插件激活**——guest 一调 `host-pty` 原语就取端口，取不到直接 panic
/// （fail-visible）。与 `set_services` / `set_task_engine` / `http::install` 同属
/// 两阶段注入的装配链。
pub fn install(ctx: Arc<WasmHostContext>) {
    let ports: Arc<dyn PtyPorts> = Arc::new(HostPtyPorts::from_ctx(Arc::clone(&ctx)));
    ctx.set_domain_ports(
        bedcode_pty_engine::plugin_binding::DOMAIN,
        Arc::new(Arc::clone(&ports)) as Arc<dyn std::any::Any + Send + Sync>,
    );
    bedcode_pty_engine::plugin_binding::install_ports(Arc::clone(&ports));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 能力模块名与能力域描述符逐字一致（白名单键错即 guest 实例化期缺 import）
    ///
    /// 为什么在本 crate 断言而不是只在能力域自证：白名单键在**宿主**的
    /// `HOST_MODULES` 里，两处字面量必须相等，而能力域无法看见宿主的白名单。
    #[test]
    fn host_module_name_matches_capability_domain_desc() {
        assert_eq!(
            HOST_MODULE_NAME,
            bedcode_pty_engine::plugin_binding::MODULE_NAME
        );
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
        let global = AppConfig::global();
        let ports = HostPtyPorts::from_ctx(crate::host_api::tests::build_host_ctx());
        let snapshot = ports.config();
        assert_eq!(snapshot.default_cols, global.terminal.default_cols);
        assert_eq!(snapshot.default_rows, global.terminal.default_rows);
        assert_eq!(
            snapshot.engine.lifecycle_capacity,
            global.channels.lifecycle_capacity
        );
        assert_eq!(
            snapshot.engine.read_buffer_size,
            global.terminal.read_buffer_size
        );
    }
}
