//! host-websocket 宿主侧适配器（能力域端口 `WsPorts` 的宿主实现 + 装配自报）
//!
//! 域机制（15 条 `host-websocket` 原语 + 出站连接 / 服务端端点 / 在线客户端三面）在
//! `bedcode-server-websocket`；本文件是**宿主侧的另一半**——wasm-core 纯净性收口票 02
//! 批次 03 整文件迁自 `bedcode-wasm-core/src/host_api/ws.rs`。
//!
//! 四件同处（与 pty / mdns / peer 样板一致）：端口实现 / `install` 装配自报 / 白名单声明 +
//! 强制引用行 / 装配器静态。
//!
//! ## 两种构造（与 pty/mdns/peer 不同的地方）
//!
//! - [`HostWsPorts::from_ctx`] / [`HostWsPorts::from_erased`]：**完整端口**（权限门走
//!   宿主 `PermissionManager`）——生产路径，开机装配用；
//! - [`HostWsPorts::from_bus`]：**帧投递专用窄端口**（只绑总线、无权限管理器 ⇒ 权限门
//!   恒拒，fail-safe）——供「只拿到总线」的调用点使用（`bus::HostBusPort` 的帧回灌路径
//!   与端点登记）。这条构造是**总线侧 plumbing**，随本 adapter 一并住宿主。
//!
//! ## 哪些机制**刻意留在宿主**（AGENTS §5.1.3 允许的薄壳）
//!
//! - **权限门**：复用 wasm-core 的 `host_api::check_permission`（同一份 PermissionManager、
//!   同一条拒绝 warn 路径）；
//! - **消息总线**：`publish` / 帧回灌的订阅方隔离与投递任务派生都是总线裁决；
//! - **同步↔异步桥**：`runtime_util::block_on_async`（ambient runtime + actix
//!   current_thread 自锁规避都是实测产物，能力域不得复制第二份）。
//!
//! ## 端口句柄的形态
//!
//! 完整端口持擦除后的 `Option<Arc<dyn HostPorts>>`（host-kit 只能命名它），权限判定时
//! [`downcast_host`] 还原 `WasmHostContext`；类型不符即 panic（装配期编程错误，
//! fail-visible）。与 pty / mdns / peer 同形。

use std::any::Any;
use std::sync::Arc;

use bedcode_host_kit::ports::{downcast_host, HostPorts};
use bedcode_server_base::ports::BusPort;
use bedcode_server_websocket::plugin_binding::ports::{
    BoxedBlocked, FrameDispatch, WsFrameTarget, WsPorts,
};

use bedcode_wasm_core::bus::{HostBusPort, MessageBus, WsFrameDispatch};
use bedcode_wasm_core::host_api::check_permission;
use bedcode_wasm_core::host_api::context::WasmHostContext;

// 能力模块白名单条目与强制引用行**必须同处**（本文件），两者不漂移——漏引用 ⇒
// 该 crate 的 inventory 自报静态不进最终二进制（装载期 missing 方向点名）；漏白名单
// ⇒ unlisted 方向点名。判据与纪律见 `bedcode_host_kit::assembly` 模块文档。
bedcode_host_kit::expect_host_module!(bedcode_server_websocket::plugin_binding::MODULE_NAME);
use bedcode_server_websocket as _;

/// 端口的宿主实现
pub struct HostWsPorts {
    /// 插件消息总线（状态事件发布 + `events-ws` 帧投递的 dispatcher 来源）
    bus: Arc<MessageBus>,
    /// 宿主上下文（擦除形态；权限判定时向下转型；`from_bus` 构造下为 `None` ⇒ 权限门恒拒）
    ctx: Option<Arc<dyn HostPorts>>,
}

impl HostWsPorts {
    /// 完整端口（测试 / 同进程调用点）：从**具体**宿主上下文构造（零转型成本在构造侧）
    pub fn from_ctx(ctx: Arc<WasmHostContext>) -> Self {
        let bus = ctx.message_bus().clone();
        Self { bus, ctx: Some(ctx) }
    }

    /// 完整端口（装配路径）：从**擦除后的**宿主上下文构造
    ///
    /// 装配回调只拿得到 `Arc<dyn HostPorts>`（host-kit 的类型擦除通道），故总线在构造期
    /// 向下转型取一次，权限判定留给 [`HostWsPorts::check_permission`] 每次调用转型。
    pub fn from_erased(host: Arc<dyn HostPorts>) -> Self {
        let bus = downcast_host::<WasmHostContext>(host.as_ref())
            .message_bus()
            .clone();
        Self { bus, ctx: Some(host) }
    }

    /// 帧投递专用窄端口：只绑定总线，无权限管理器（权限门恒拒，fail-safe）
    ///
    /// 供「只拿到总线」的调用点使用：`bus::HostBusPort` 的帧回灌路径与内核端点登记。
    pub fn from_bus(bus: Arc<MessageBus>) -> Self {
        Self { bus, ctx: None }
    }
}

impl WsPorts for HostWsPorts {
    fn check_permission(&self, plugin_id: &str, permission: &str, api: &str) -> bool {
        // 权限门留宿主（AGENTS §5.1.3「安全闸门」——闸门不应可插拔）。复用 wasm-core 的
        // `host_api::check_permission`：同一份 PermissionManager、同一条拒绝 warn 路径。
        let Some(host) = self.ctx.as_ref() else {
            // 无权限管理器上下文（帧投递专用窄端口）⇒ 拒绝。拿不到权限管理器就不放行
            //（fail-safe，与既有 check_permission 的无上下文分支同向）。
            return false;
        };
        check_permission(
            downcast_host::<WasmHostContext>(host.as_ref()),
            plugin_id,
            permission,
            api,
        )
    }

    fn publish(&self, topic: &str, payload: serde_json::Value) {
        // sender 恒为 `"host"`（与迁移前 `publish_ws` 逐字一致：宿主是事件源，
        // 订阅侧按精确 topic 分发）
        self.bus.publish(topic, "host", payload);
    }

    fn bus_port(&self) -> Arc<dyn BusPort> {
        // 端点登记的总线端口：帧投递侧用**绑定到本总线**的窄端口（无权限管理器）
        Arc::new(HostBusPort::new(
            Arc::clone(&self.bus),
            Arc::new(Self::from_bus(Arc::clone(&self.bus))),
        ))
    }

    fn dispatch_frame(
        &self,
        plugin_id: &str,
        target: WsFrameTarget<'_>,
        kind: &str,
        payload: Vec<u8>,
    ) -> FrameDispatch {
        // `events-ws` 帧回灌不经 topic 订阅，直接按属主寻址投给插件实例
        //（`MessageBus::dispatcher` 的唯一非总线消费者）。
        let Some(dispatcher) = block_on_dispatcher(&self.bus) else {
            return FrameDispatch::Unavailable;
        };
        let frame = match target {
            WsFrameTarget::Client(handle) => WsFrameDispatch::Client {
                handle: handle.to_string(),
                kind: kind.to_string(),
                payload,
            },
            WsFrameTarget::EndpointClient { endpoint_id, client_id } => WsFrameDispatch::EndpointClient {
                endpoint_id: endpoint_id.to_string(),
                client_id: client_id.to_string(),
                kind: kind.to_string(),
                payload,
            },
        };
        match dispatcher.dispatch_ws_frame(plugin_id, &frame) {
            Ok(true) => FrameDispatch::Delivered,
            Ok(false) => FrameDispatch::NotExported,
            Err(e) => FrameDispatch::Failed(e.to_string()),
        }
    }

    fn block_on_any(&self, fut: BoxedBlocked) -> Box<dyn Any + Send> {
        // 复用宿主那份唯一的同步↔异步桥（见模块文档第三条）
        bedcode_wasm_core::runtime_util::block_on_async(fut)
    }
}

/// 取当前注入的投递器（未注入 = 两阶段初始化中间态 → `None`，不 panic）
///
/// `MessageBus::dispatcher` 是 `async` 方法（`RwLock` 读），而能力域的 `dispatch_frame`
/// 是同步端口方法 ⇒ 走宿主同一个桥读一次。
fn block_on_dispatcher(
    bus: &Arc<MessageBus>,
) -> Option<Arc<dyn bedcode_wasm_core::bus::MessageDispatcher>> {
    bedcode_wasm_core::runtime_util::block_on_async(async move { bus.dispatcher().await })
}

/// 装配回调（host-kit 自报）：开机期装本域端口
///
/// **两处登记**（各登一份等价对象，行为一致）：
/// - **实例级** `WasmHostContext::set_domain_ports`：供插件实例经
///   `bedcode_host_kit::ports::HostPorts::domain_ports` 取回，使端口与**本实例的**
///   权限管理器 / 消息总线绑定（多上下文场景下不会读到别人的总线）；
/// - **进程级** `bedcode_server_websocket::plugin_binding::install_ports`：供插件实例
///   之外的非实例路径（停用回收转发）使用。
///
/// **必须早于任何插件激活**（激活发生在 `PluginHost::new` 内部）——内核装配链
/// （`install_capability_domain_ports`）遍历自报表时调用；guest 一调 `host-websocket`
/// 原语就取端口，取不到直接 panic（fail-visible）。
fn install(host: Arc<dyn HostPorts>) {
    let ctx = downcast_host::<WasmHostContext>(host.as_ref());
    // 端口对象持一份引用（`ctx` 的向下转型借用仍在用 host，故不移动它）
    let ports: Arc<dyn WsPorts> = Arc::new(HostWsPorts::from_erased(Arc::clone(&host)));
    ctx.set_domain_ports(
        bedcode_server_websocket::plugin_binding::DOMAIN,
        Arc::new(Arc::clone(&ports)) as Arc<dyn Any + Send + Sync>,
    );
    bedcode_server_websocket::plugin_binding::install_ports(ports);
}

/// 端口装配器静态（与白名单条目 / 强制引用行同处；内核装配链遍历本自报表）
pub static WS_PORTS_INSTALLER: bedcode_host_kit::DomainPortsInstaller =
    bedcode_host_kit::DomainPortsInstaller {
        name: bedcode_server_websocket::plugin_binding::MODULE_NAME,
        install,
    };

bedcode_host_kit::submit_domain_ports_installer!(WS_PORTS_INSTALLER);

/// 回收某插件的全部 WS 资源（显式回收入口 / 测试用）
///
/// 转发到能力域（出站连接 + 入站端点 + 在线客户端，一次调用两侧一并回收）。
/// **停用回收的常规路径已走自报钩子**（`bedcode-server-websocket::plugin_binding::HOOKS`
/// 的 `on_plugin_purge`，票 02 批次 03）。
pub fn purge_for_plugin(plugin_id: &str) -> usize {
    bedcode_server_websocket::plugin_binding::purge_for_plugin(
        plugin_id,
        &bedcode_server_websocket::plugin_binding::ports::ports(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 白名单声明、接口路径、权限位三件与能力域描述符逐字一致
    #[test]
    fn host_module_declaration_matches_capability_domain_desc() {
        let module_name = bedcode_server_websocket::plugin_binding::MODULE_NAME;
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&module_name),
            "能力模块白名单缺 {module_name}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(module_name, "websocket", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            bedcode_server_websocket::plugin_binding::MODULE_INTERFACES,
            &[
                "bedcode:plugin/host-websocket",
                "bedcode:plugin/host-websocket-server"
            ],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配；票 04 拆出服务端域）"
        );
        assert_eq!(
            bedcode_server_websocket::plugin_binding::MODULE_PERMISSIONS,
            &["ws:client", "ws:server"],
            "权限位必须与 `bedcode.wit` / SDK 权限表逐字一致"
        );
    }

    /// 装配器静态被内核装配链遍历到（自报 → 装 → 取回，三段闭环）
    #[test]
    fn submitted_installer_is_wired_into_the_boot_chain() {
        let (_, _ctx) = bedcode_wasm_core::test_support::setup_wasm_runtime();
        // 取回端口即证明安装过（内部 panic 文案会点名「host must call install_ports」）
        let ports = bedcode_server_websocket::plugin_binding::ports::ports();
        // 点位：窄端口的权限门恒拒（无权限管理器——fail-safe，不静默放行）
        let narrow = HostWsPorts::from_bus(Arc::new(bedcode_wasm_core::bus::MessageBus::new()));
        assert!(
            !narrow.check_permission("com.bedcode.never-granted", "ws:client", "host_websocket.test"),
            "帧投递窄端口没有权限管理器 ⇒ 权限门必须恒拒"
        );
        // 完整端口（本夹具的实例级端口）同样对未授权插件 fail-closed
        assert!(
            !ports.check_permission("com.bedcode.never-granted", "ws:client", "host_websocket.test"),
            "未授权插件必须被权限门拒绝（复用宿主 check_permission 的 fail-safe 分支）"
        );
    }
}
