//! host-mdns 宿主侧适配器（能力域已迁出本模块）
//!
//! **本文件只剩三样东西**，原 795 行的 mDNS 实现已整体迁入
//! `bedcode-discovery-engine`（wasm-core-lib-split 票 03）：
//!
//! 1. [`HostDiscoveryPorts`] —— 能力域端口的宿主实现；
//! 2. [`install`] —— 开机期装配端口（供 PluginHost 装配链调用）；
//! 3. 一条「强制引用 + 白名单」所需的 re-export —— 见 `HOST_MODULE_NAME`。
//!
//! ## 方向倒置的终结
//!
//! 改造前 `server/ports_impl.rs` 反向调用
//! `crate::host_api::mdns::shared_daemon()`——宿主 server 的端口层
//! 依赖 wasm_core 的**插件绑定模块**（ADR 0022 裁剪线要消除的方向）。现在
//! `shared_daemon()` 直接取平台无关引擎 crate，该反向依赖消失。
//!
//! ## 为什么 adapter 是零大小类型
//!
//! 浏览器事件循环要长期持有一个 `'static` 端口（宿主上下文本身不满足
//! `'static`）。adapter 的每个方法都经全局取用（`AppContext::try_global()` /
//! `AppContext::try_global()` 拿宿主上下文），零字段 ⇒ `Arc<HostDiscoveryPorts>`
//! 分配是一次零字节分配。

use std::sync::Arc;

use bedcode_discovery_engine::{DiscoveryPorts, DiscoveryTask};
use bedcode_plugin_api::permission::PERMISSION_MDNS;

use crate::manager::capability;

/// 能力模块名（必须与 `bedcode-discovery-engine::DESC.name` 逐字一致）
///
/// 它同时是 `component.rs` 里 `HOST_MODULES` 白名单的键——两者不同即红。
pub const HOST_MODULE_NAME: &str = "discovery";

/// 端口的宿主实现（零大小：全部方法经全局取用）
pub struct HostDiscoveryPorts;

impl DiscoveryPorts for HostDiscoveryPorts {
    fn check_permission(&self, plugin_id: &str, api: &str) -> bool {
        // 权限门留宿主（AGENTS §5.1.3 四类薄壳之「安全闸门」——闸门不应可插拔）。
        // 复用既有 `host_api::check_permission`：同一份 PermissionManager、同一条
        // 拒绝 warn 路径（AGENTS §8 结构化字段），不另起一套判定。
        // 整核抽出 §4.4：经 crate 内宿主上下文注册表取 `WasmHostContext`（无头
        // /测试未装配 → None → fail-safe 拒绝，与既有 `AppContext::try_global()`
        // 语义逐字一致）。
        let Some(host_ctx) = crate::host_context_registry::get() else {
            return false;
        };
        super::check_permission(host_ctx.as_ref(), plugin_id, PERMISSION_MDNS, api)
    }

    fn local_node_id(&self) -> Option<String> {
        // `WasmHostContext::app_handle()` 返回 `Option<&AppHandle>`（无头环境为
        // None ⇒ 返回 None ⇒ 能力域按「无法比对即不拦自播」口径处理）；对等网络
        // 上下文经 §3.3 端口取（无头/测试未注入 → None，同口径）
        let host_ctx = crate::host_context_registry::get()?;
        let app = host_ctx.app_handle()?;
        let provider = host_ctx.peer_ctx_provider()?;
        let peer_ctx = provider(app);
        bedcode_server_peer_net::current_node_id(&peer_ctx)
    }

    fn publish(&self, topic: &str, payload: serde_json::Value) {
        // 无总线上下文静默跳过：与 peer 事件桥接同口径（能力域不应因宿主未起
        // 而 panic——事件丢弃即可，浏览句柄本身不受影响）
        if let Some(host_ctx) = crate::host_context_registry::get() {
            host_ctx.message_bus().publish(topic, "host", payload);
        }
    }

    fn spawn(&self, task: bedcode_discovery_engine::ports::BoxedTask) -> Arc<dyn DiscoveryTask> {
        Arc::new(HostDiscoveryTask {
            handle: tauri::async_runtime::spawn(task),
        })
    }

    // ==================== 能力路由（票 09 扩表） ====================
    //
    // 五条原语各问一次路由层：命中系统组件提供者就转发（组件侧再经 `host-mdns`
    // 同形导出），没命中返回 `None` 让能力域走本域引擎（见 [`route_via_capability`]）。
    fn forward_mdns_browse(&self, plugin_id: &str, service_type: &str) -> Option<Result<String, String>> {
        route_via_capability(|cap| capability::forward_mdns_browse(cap, plugin_id, service_type))
    }

    fn forward_mdns_stop_browse(&self, plugin_id: &str, browser_id: &str) -> Option<Result<bool, String>> {
        route_via_capability(|cap| capability::forward_mdns_stop_browse(cap, plugin_id, browser_id))
    }

    fn forward_mdns_advertise(&self, plugin_id: &str, config_json: &str) -> Option<Result<String, String>> {
        route_via_capability(|cap| capability::forward_mdns_advertise(cap, plugin_id, config_json))
    }

    fn forward_mdns_stop_advertise(&self, plugin_id: &str, advertise_id: &str) -> Option<Result<bool, String>> {
        route_via_capability(|cap| capability::forward_mdns_stop_advertise(cap, plugin_id, advertise_id))
    }

    fn forward_mdns_is_advertising(&self, plugin_id: &str, advertise_id: &str) -> Option<Result<bool, String>> {
        route_via_capability(|cap| capability::forward_mdns_is_advertising(cap, plugin_id, advertise_id))
    }
}

/// 经全局宿主上下文取能力路由作用域并转发
///
/// 本 adapter 是零大小类型（浏览事件循环要长期持有 `'static` 端口），故每次调用
/// 经 crate 内宿主上下文注册表（§4.4）取作用域——与同文件其余方法同款。
///
/// **未装配（无头 / 测试）时返回 `None`**：那是「本进程没有路由表」的真值，
/// 与「有路由表但该能力无提供者」同形，能力域两条路径的行为一致。
fn route_via_capability<T>(
    forward: impl FnOnce(&dyn crate::host_api::context::CapabilityScope) -> Option<T>,
) -> Option<T> {
    let host_ctx = crate::host_context_registry::get()?;
    forward(host_ctx.as_ref())
}

/// 后台任务句柄的宿主实现（`cancel` ⇒ `abort`，与原实现同款）
struct HostDiscoveryTask {
    handle: tauri::async_runtime::JoinHandle<()>,
}

impl DiscoveryTask for HostDiscoveryTask {
    fn cancel(&self) {
        self.handle.abort();
    }
}

/// 开机期装配能力域端口（幂等：重复装配被忽略，见 `install_ports`）
///
/// **必须早于任何插件激活**——浏览事件循环一开就会取端口，取不到直接 panic。
/// 与 `set_services` / `set_task_engine` 同属两阶段注入的装配链。
pub fn install() {
    bedcode_discovery_engine::install(HostDiscoveryPorts);
}

/// 回收某插件的全部浏览 + 广播句柄（插件停用/卸载时由 PluginHost 调用）
///
/// 只回收本人句柄；其余插件与宿主（owner=`host`）登记不受影响。
pub(crate) fn purge_for_plugin(plugin_id: &str) -> usize {
    bedcode_discovery_engine::engine::purge_for_plugin(plugin_id, &bedcode_discovery_engine::ports::ports())
}
