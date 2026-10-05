//! host-websocket 宿主侧适配器（能力域已迁出本模块）
//!
//! **本文件只剩三样东西**，原 1126 行的 WS 实现已整体迁入
//! `bedcode_server_websocket::plugin_binding`（wasm-core-lib-split 票 04）：
//!
//! 1. [`HostWsPorts`] —— 能力域端口的宿主实现；
//! 2. [`install`] —— 开机期装配端口（供 PluginHost 装配链调用）；
//! 3. 回收转发 —— 插件停用时 [`purge_for_plugin`] 只碰本人。
//!
//! ## 为什么 adapter 不持有 `MessageBus` 以外的任何宿主状态
//!
//! 出站读任务要活到连接关闭之后，必须长期持有一份 `'static` 端口。生产路径的
//! adapter 在**开机期**捕获 `Arc<WasmHostContext>`（`PluginHost::new` 已建好），
//! 之后每帧零查表——这与迁移前「每次调用从 `host_ctx` 取总线」等价，且不再有
//! 「实例 ctx 与全局总线分叉」的可能（两者本来就是同一份）。
//!
//! [`from_bus`] 是**帧投递专用**的窄构造：端点登记（`HostBusPort`）只需要一个绑定到
//! 指定总线的端口视图（它没有权限管理器上下文），拿不到权限管理器时权限门恒拒
//! （fail-safe，与 [`check_permission`] 的无上下文分支同向）。

use std::any::Any;
use std::sync::Arc;

use bedcode_server_base::ports::BusPort;
use bedcode_server_websocket::plugin_binding::ports::{BoxedBlocked, FrameDispatch, WsFrameTarget, WsPorts};

use crate::wasm_core::bus::{MessageBus, WsFrameDispatch};
use crate::wasm_core::host_api::context::WasmHostContext;

/// 能力模块名（必须与 `bedcode_server_websocket::plugin_binding::DESC.name` 逐字一致）
///
/// 它同时是 `component.rs` 里 `HOST_MODULES` 白名单的键——两者不同即红。
pub const HOST_MODULE_NAME: &str = "websocket";

/// 端口的宿主实现
///
/// 两种构造：
/// - [`HostWsPorts::from_ctx`]：完整实现（权限门走宿主 `PermissionManager`）——
///   **生产路径**，开机装配用；
/// - [`HostWsPorts::from_bus`]：帧投递专用窄实现（无权限管理器 ⇒ 权限门恒拒）——
///   供「只拿到总线」的调用点（`HostBusPort` 为插件端点登记挂的总线端口）使用。
pub struct HostWsPorts {
    /// 插件消息总线（状态事件发布 + `events-ws` 帧投递的 dispatcher 来源）
    bus: Arc<MessageBus>,
    /// 宿主上下文（权限门；`from_bus` 构造下为 `None` ⇒ 权限门恒拒）
    ctx: Option<Arc<WasmHostContext>>,
}

impl HostWsPorts {
    /// 完整端口（生产）：权限门与总线都取自宿主上下文
    pub fn from_ctx(ctx: Arc<WasmHostContext>) -> Self {
        let bus = ctx.message_bus().clone();
        Self { bus, ctx: Some(ctx) }
    }

    /// 帧投递专用窄端口：只绑定总线，无权限管理器（权限门恒拒，fail-safe）
    pub fn from_bus(bus: Arc<MessageBus>) -> Self {
        Self { bus, ctx: None }
    }
}

impl WsPorts for HostWsPorts {
    fn check_permission(&self, plugin_id: &str, permission: &str, api: &str) -> bool {
        // 权限门留宿主（AGENTS §5.1.3 四类薄壳之「安全闸门」——闸门不应可插拔）。
        // 复用既有 `host_api::check_permission`：同一份 PermissionManager、同一条
        // 拒绝 warn 路径（AGENTS §8 结构化字段），不另起一套判定。
        let Some(ctx) = self.ctx.as_ref() else {
            // 无权限管理器上下文（帧投递专用窄端口）⇒ 拒绝。拿不到权限管理器就不放行
            //（fail-safe，与既有 check_permission 的无上下文分支同向）。
            return false;
        };
        super::check_permission(ctx.as_ref(), plugin_id, permission, api)
    }

    fn publish(&self, topic: &str, payload: serde_json::Value) {
        // sender 恒为 `"host"`（与迁移前 `publish_ws` 逐字一致：宿主是事件源，
        // 订阅侧按精确 topic 分发）
        self.bus.publish(topic, "host", payload);
    }

    fn bus_port(&self) -> Arc<dyn BusPort> {
        Arc::new(crate::server::ports_impl::HostBusPort::new(Arc::clone(&self.bus)))
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
        // 复用宿主那份唯一的同步↔异步桥（ambient runtime + actix current_thread
        // 自锁规避都是实测产物，能力域不得复制第二份，见 ports 模块文档）
        crate::wasm_core::runtime_util::block_on_async(fut)
    }
}

/// 取当前注入的投递器（未注入 = 两阶段初始化中间态 → `None`，不 panic）
///
/// `MessageBus::dispatcher` 是 `async` 方法（`RwLock` 读），而能力域的
/// `dispatch_frame` 是同步端口方法 ⇒ 走宿主同一个桥读一次。
fn block_on_dispatcher(bus: &Arc<MessageBus>) -> Option<Arc<dyn crate::wasm_core::bus::MessageDispatcher>> {
    crate::wasm_core::runtime_util::block_on_async(async move { bus.dispatcher().await })
}

/// 开机期装配能力域端口（幂等：重复装配被忽略，见 `install_ports`）
///
/// **两处登记**（各登一份等价对象，行为一致）：
/// - **进程级** [`bedcode_server_websocket::plugin_binding::install_ports`]：供插件实例
///   之外的非实例路径（停用回收转发）使用；
/// - **实例级** [`WasmHostContext::set_domain_ports`]：供插件实例经
///   [`bedcode_host_kit::ports::HostPorts::domain_ports`] 取回，使端口与**本实例的**
///   权限管理器 / 消息总线绑定（多上下文场景下不会读到别人的总线）。
///
/// **必须早于任何插件激活**——guest 一调 `host-websocket` 原语就取端口，取不到
/// 直接 panic（fail-visible）。与 `set_services` / `set_task_engine` /
/// `mdns::install` 同属两阶段注入的装配链。
pub fn install(ctx: Arc<WasmHostContext>) {
    let ports: Arc<dyn WsPorts> = Arc::new(HostWsPorts::from_ctx(Arc::clone(&ctx)));
    ctx.set_domain_ports(
        bedcode_server_websocket::plugin_binding::DOMAIN,
        Arc::new(Arc::clone(&ports)) as Arc<dyn std::any::Any + Send + Sync>,
    );
    bedcode_server_websocket::plugin_binding::install_ports(Arc::clone(&ports));
}

/// 回收某插件的全部 WS 资源（插件停用/卸载时由 PluginHost 调用）
///
/// 转发到能力域（出站连接 + 入站端点 + 在线客户端，一次调用两侧一并回收）。
pub(crate) fn purge_for_plugin(plugin_id: &str) -> usize {
    bedcode_server_websocket::plugin_binding::purge_for_plugin(
        plugin_id,
        &bedcode_server_websocket::plugin_binding::ports::ports(),
    )
}
