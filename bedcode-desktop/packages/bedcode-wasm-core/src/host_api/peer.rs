//! host-peer 宿主侧适配器（能力域已迁出本模块）
//!
//! **本文件只剩两样东西**，原 727 行的 peer 绑定层已整体迁入
//! `bedcode_server_peer_net::plugin_binding`（wasm-core-lib-split 票 05）：
//!
//! 1. [`HostPeerPorts`] —— 能力域端口的宿主实现；
//! 2. [`install`] —— 开机期装配端口（供 PluginHost 装配链调用）。
//!
//! ## 票 05 的第二交付：反向耦合从 20 处降到 0
//!
//! 迁移前绑定层有 20 处 `crate::server::peer_net_cmds::peer_ctx(&app)`——
//! 「从宿主组装面取引擎状态」。那行同时把 `tauri::AppHandle` 与 managed state
//! 表拖进能力域（正是 `dependency_direction_lock` 禁的 `crate::server::` /
//! `tauri::` 两种形态）。现在能力域只问 [`PeerPorts::peer_ctx`] 要「已装配好的
//! [`PeerCtx`]」，**装配动作留在这里**——本 crate 的 `peer_net_cmds` 仍是唯一
//! 认识 `AppHandle` 的引擎装配点（ADR 0022 §5.1.3「引擎实现」薄壳），但那是
//! **本文件**的依赖，不是能力域的。
//!
//! ## 判定顺序是被断言的行为（勿调换）
//!
//! 属主判定刻意排在 [`PeerPorts::peer_ctx`] 之前：非属主的一次 `close` 试探
//! 在无头 / 引擎未就绪的环境下也必须得到同一个答案（属主拒绝），否则「先报
//! headless」会把越权探测的结果随机化。迁移前的顺序与文案逐字保留。

use std::any::Any;
use std::sync::Arc;

use bedcode_server_peer_net::plugin_binding::ports::{BoxedBlocked, PeerPorts};
use bedcode_server_peer_net::PeerCtx;

use crate::host_api::context::WasmHostContext;

/// 能力模块名（必须与 `bedcode_server_peer_net::plugin_binding::DESC.name` 逐字一致）
///
/// 它同时是 `component.rs` 里 `HOST_MODULES` 白名单的键——两者不同即红。
pub const HOST_MODULE_NAME: &str = "peer-net";

/// 端口的宿主实现
///
/// 生产路径在**开机期**捕获 `Arc<WasmHostContext>`（`PluginHost::new` 已建好），
/// 之后每条原语两次取端口：一次权限门、一次引擎上下文。
pub struct HostPeerPorts {
    /// 宿主上下文（权限门 + 引擎上下文的取法都在它身上）
    ctx: Arc<WasmHostContext>,
}

impl HostPeerPorts {
    /// 端口（生产）：从宿主上下文取权限管理器与 `AppHandle`
    pub fn from_ctx(ctx: Arc<WasmHostContext>) -> Self {
        Self { ctx }
    }
}

impl PeerPorts for HostPeerPorts {
    fn check_permission(&self, plugin_id: &str, permission: &str, api: &str) -> bool {
        // 权限门留宿主（AGENTS §5.1.3 四类薄壳之「安全闸门」——闸门不应可插拔）。
        // 复用既有 `host_api::check_permission`：同一份 PermissionManager、同一条
        // 拒绝 warn 路径（AGENTS §8 结构化字段），不另起一套判定。
        super::check_permission(self.ctx.as_ref(), plugin_id, permission, api)
    }

    fn peer_ctx(&self) -> Result<Arc<PeerCtx>, String> {
        // 引擎上下文的**装配动作**留宿主（唯一认识 AppHandle 的引擎装配点）；
        // 无头口径与文案由能力域定义，本处只汇报「取不到」。整核抽出 §3.3：
        // 对等网络上下文经 `PeerCtxProvider` 端口取（lib 注入 `peer_net_cmds::
        // peer_ctx`），未注入（无头/测试）与无 app_handle 同报 `HEADLESS_UNAVAILABLE`。
        let app = self
            .ctx
            .app_handle()
            .ok_or_else(|| bedcode_server_peer_net::plugin_binding::ports::HEADLESS_UNAVAILABLE.to_string())?;
        let provider = self
            .ctx
            .peer_ctx_provider()
            .ok_or_else(|| bedcode_server_peer_net::plugin_binding::ports::HEADLESS_UNAVAILABLE.to_string())?;
        Ok(provider(app))
    }

    fn block_on_any(&self, fut: BoxedBlocked) -> Box<dyn Any + Send> {
        // 复用宿主那份唯一的同步↔异步桥（ambient runtime + actix current_thread
        // 自锁规避都是实测产物，能力域不得复制第二份，见 ports 模块文档）
        crate::runtime_util::block_on_async(fut)
    }
}

/// 开机期装配能力域端口（幂等：重复装配被忽略，见 `install_ports`）
///
/// **两处登记**（各登一份等价对象，行为一致）：
/// - **进程级** [`bedcode_server_peer_net::plugin_binding::install_ports`]：供插件实例
///   之外的非实例路径使用；
/// - **实例级** [`WasmHostContext::set_domain_ports`]：供插件实例经
///   [`bedcode_host_kit::ports::HostPorts::domain_ports`] 取回，使权限判定落在
///   **本实例的**权限管理器上（多上下文场景下不会读到别人的权限库）。
///
/// **必须早于任何插件激活**——guest 一调 `host-peer` 原语就取端口，取不到
/// 直接 panic（fail-visible）。与 `set_services` / `set_task_engine` / `ws::install`
/// 同属两阶段注入的装配链。
pub fn install(ctx: Arc<WasmHostContext>) {
    let ports: Arc<dyn PeerPorts> = Arc::new(HostPeerPorts::from_ctx(Arc::clone(&ctx)));
    ctx.set_domain_ports(
        bedcode_server_peer_net::plugin_binding::DOMAIN,
        Arc::new(Arc::clone(&ports)) as Arc<dyn std::any::Any + Send + Sync>,
    );
    bedcode_server_peer_net::plugin_binding::install_ports(Arc::clone(&ports));
}
