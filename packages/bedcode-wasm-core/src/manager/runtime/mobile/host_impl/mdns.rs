//! host-mdns v2 逻辑层 —— 移动端薄转发（双端共享 lib spec M3）
//!
//! 引擎机制（共享守护 / 双句柄表 / 属主仲裁 / 事件定向投递 / re-announce /
//! purge）全部在共享引擎 [`bedcode_discovery_engine`]（仓库根 `packages/`，
//! 零 WIT 形态，M1 落地）；本模块只保留两件事：
//!
//! 1. **5 条 host-mdns 原语薄转发**：函数签名与历史一致（component.rs 绑定 /
//!    host_impl 聚合入口不变），body 改为调引擎域函数；
//! 2. **移动端 `DiscoveryPorts` 适配器**（[`MobileDiscoveryPorts`]）：权限门
//!    （manifest `granted_permissions`）/ 总线发布 / 自播回显节点 ID / 宿主
//!    运行时任务派生——四个平台差异面的移动端实现。
//!
//! 守护单例真源：移动端宿主 `crate::mdns::engine` 已随 M3 删除，统一指向
//! 引擎的 `DAEMON`（Android 多播锁经 [`engine::set_daemon_init_hook`] 平台钩子
//! 装配，宿主 src-tauri 初始化期注册）。防回接锁：**本 crate 零
//! `ServiceDaemon::new()`**——任何新建守护都会打破「单守护」红线
//! （双守护同绑 5353 互抢多播包）。
//!
//! 行为契约（句柄表 / 属主隔离 / purge / topic 形状）由引擎自身单测覆盖
//! （`bedcode-discovery-engine` crate 测试），本模块只保薄转发语义断言。

use bedcode_discovery_engine::ports::{BoxedTask, DiscoveryPorts, DiscoveryTask};
use std::sync::Arc;

use super::super::WasmPluginState;
use crate::bus::MessageBus;
use crate::host_api::ports::HostEnginePorts;

// ==================== 移动端 DiscoveryPorts 适配器 ====================

/// 移动端端口适配：实现引擎 `DiscoveryPorts` 的四个平台差异面。
///
/// 按插件实例构建（`from_state`）：权限门读该插件 manifest
/// `granted_permissions`；事件线程 / 续期任务会克隆本端口并长期持有
/// （'static），故字段只放 Arc / 可克隆宿主句柄。`purge` / 无 state 上下文的
/// 登记走 [`minimal`]（bus/app 缺省，不发布、不拦回显）。
struct MobileDiscoveryPorts {
    /// 插件消息总线（事件定向投递出口；minimal 形态 None = 不发布）
    bus: Option<Arc<MessageBus>>,
    /// 宿主引擎端口（自播回显过滤读本机节点 ID）
    host_ports: Arc<dyn HostEnginePorts>,
    /// 宿主 AppHandle（节点 ID 查询入口；无头 / purge 形态 None）
    app: Option<Arc<tauri::AppHandle>>,
    /// 该插件实例是否已授权 `network:mdns`（构造时结算，权限门零延迟）
    permitted: bool,
}

impl MobileDiscoveryPorts {
    /// 从插件实例状态构建（宿主原语路径）；返回 trait 对象（引擎 API 要求）
    fn from_state(state: &WasmPluginState) -> Arc<dyn DiscoveryPorts> {
        Arc::new(Self {
            bus: Some(Arc::clone(&state.host_ctx.message_bus)),
            host_ports: Arc::clone(&state.host_ctx.ports),
            // 无头 / 测试形态 app_handle 为 None：自播回显不拦截（与历史 is_self_broadcast
            // 的 None 分支同口径）
            app: state.host_ctx.app_handle.clone(),
            permitted: state
                .granted_permissions
                .contains(bedcode_plugin_api_mobile::permission::PERMISSION_MDNS),
        })
    }

    /// 最小形态（purge 聚合入口）：无总线 / 无 app，不发布、不拦回显
    fn minimal(host_ports: &Arc<dyn HostEnginePorts>) -> Arc<dyn DiscoveryPorts> {
        Arc::new(Self {
            bus: None,
            host_ports: Arc::clone(host_ports),
            app: None,
            permitted: false,
        })
    }
}

/// 宿主运行时任务包装（tauri async_runtime JoinHandle → `DiscoveryTask`）
struct TauriSpawnedTask(tauri::async_runtime::JoinHandle<()>);

impl DiscoveryTask for TauriSpawnedTask {
    fn cancel(&self) {
        self.0.abort();
    }
}

impl DiscoveryPorts for MobileDiscoveryPorts {
    fn check_permission(&self, _plugin_id: &str, _api: &str) -> bool {
        // 权限仲裁走 manifest granted_permissions（与桌面 check_permission 同语义）；
        // 引擎在每次原语入口先问本门，denied 时原语直接拒绝（fail-closed）
        self.permitted
    }

    fn local_node_id(&self) -> Option<String> {
        // 自播回显过滤：browse 会收到本机自己的广播（TXT `id` == 节点 ID）。
        // 无 app 句柄（无头 / 测试）或节点未启动时返回 None ⇒ 引擎不拦截
        let app = self.app.as_ref()?;
        self.host_ports.current_node_id(app.as_ref())
    }

    fn publish(&self, topic: &str, payload: serde_json::Value) {
        // 总线订阅方隔离属宿主门禁；sender 恒 "host"（总线过滤「发布者==订阅者」，
        // 用插件 id 会把自己的事件一起滤掉，与历史 publish_mdns 语义一致）
        if let Some(bus) = &self.bus {
            bus.publish(topic, "host", payload);
        }
    }

    fn spawn(&self, task: BoxedTask) -> Arc<dyn DiscoveryTask> {
        // 宿主运行时派生（替代历史 std::thread + 阻塞 recv：事件循环 / 续期任务
        // 统一挂 tauri async_runtime，顺带消灭线程阻塞范式）
        Arc::new(TauriSpawnedTask(tauri::async_runtime::spawn(task)))
    }

    // 能力路由：移动端无 core-plugin-manager → 恒 None（契约保留，语义 =
    // 无提供者走引擎，逐字与桌面「无提供者落引擎」一致）
    fn forward_mdns_browse(
        &self,
        _plugin_id: &str,
        _service_type: &str,
    ) -> Option<Result<String, String>> {
        None
    }

    fn forward_mdns_stop_browse(
        &self,
        _plugin_id: &str,
        _browser_id: &str,
    ) -> Option<Result<bool, String>> {
        None
    }

    fn forward_mdns_advertise(
        &self,
        _plugin_id: &str,
        _config_json: &str,
    ) -> Option<Result<String, String>> {
        None
    }

    fn forward_mdns_stop_advertise(
        &self,
        _plugin_id: &str,
        _advertise_id: &str,
    ) -> Option<Result<bool, String>> {
        None
    }

    fn forward_mdns_is_advertising(
        &self,
        _plugin_id: &str,
        _advertise_id: &str,
    ) -> Option<Result<bool, String>> {
        None
    }
}

// ==================== 5 条原语薄转发（签名与历史一致） ====================

/// 浏览某服务类型：引擎铸造 browser-id（`mdnsbr-<uuid>`）并经本模块端口启动
/// 事件定向投递循环（topic 最新格式 `<owner>::mdns:found|lost`，M3 统一）
pub(crate) fn mdns_browse(state: &WasmPluginState, service_type: &str) -> Result<String, String> {
    let ports = MobileDiscoveryPorts::from_state(state);
    bedcode_discovery_engine::engine::browse(&ports, &state.plugin_id, service_type)
}

/// 停止浏览并回收句柄：引擎权限门 + 属主校验后退订；事件循环随 channel 断开退出
pub(crate) fn mdns_stop_browse(state: &WasmPluginState, browser_id: &str) -> Result<bool, String> {
    let ports = MobileDiscoveryPorts::from_state(state);
    bedcode_discovery_engine::engine::stop_browse(&ports, &state.plugin_id, browser_id)
}

/// 广播某服务类型（v2 新增）：引擎解析 config-json（纯引擎参数透传）→ 共享守护
/// 注册 + 周期续期 + 句柄登记（owner = 调用插件）
pub(crate) fn mdns_advertise(state: &WasmPluginState, config_json: &str) -> Result<String, String> {
    let ports = MobileDiscoveryPorts::from_state(state);
    bedcode_discovery_engine::engine::advertise(&ports, &state.plugin_id, config_json)
}

/// 停止广播并回收句柄：引擎权限门 + 属主校验 → 注销 → 回收续期任务
pub(crate) fn mdns_stop_advertise(
    state: &WasmPluginState,
    advertise_id: &str,
) -> Result<bool, String> {
    let ports = MobileDiscoveryPorts::from_state(state);
    bedcode_discovery_engine::engine::stop_advertise(&ports, &state.plugin_id, advertise_id)
}

/// 查询广播状态（返回是否存在该句柄）：引擎权限门 + 属主校验（跨插件拒绝）
pub(crate) fn mdns_is_advertising(
    state: &WasmPluginState,
    advertise_id: &str,
) -> Result<bool, String> {
    let ports = MobileDiscoveryPorts::from_state(state);
    bedcode_discovery_engine::engine::is_advertising(&ports, &state.plugin_id, advertise_id)
}

// ==================== 宿主身份广播登记（owner=host） ====================

/// 登记宿主（peer-net 引擎）节点身份广播：注册动作由引擎经共享守护完成
/// （TXT/ServiceInfo 由引擎构造，本模块零业务拼装）；本函数只做 ADVERTISERS
/// 句柄登记（owner=host）。续期归引擎 re-announce 循环，登记行不挂续期任务
pub fn register_host_service(service_type: &str, fullname: &str) -> Result<String, String> {
    bedcode_discovery_engine::engine::register_host_service(service_type, fullname)
}

/// 注销宿主身份广播登记（节点停机时调用）：只移除 owner=host 的登记行，
/// 实际 unregister 由引擎 DiscoveryDaemon.stop() 按全名完成。幂等
pub fn stop_host_service(advertise_id: &str) -> Result<bool, String> {
    bedcode_discovery_engine::engine::stop_host_service(advertise_id)
}

// ==================== 双表回收 ====================

/// 回收指定插件的全部浏览 + 广播句柄（插件停用/卸载时经 host_impl 聚合入口
/// 调用）。只回收本人句柄；其余插件与宿主（owner=host）登记不受影响
pub(crate) fn purge_for_plugin(ports: &Arc<dyn HostEnginePorts>, plugin_id: &str) -> usize {
    let mobile_ports = MobileDiscoveryPorts::minimal(ports);
    bedcode_discovery_engine::engine::purge_for_plugin(plugin_id, &mobile_ports)
}

// ==================== Tests（薄转发语义，零运行时 / 零真实守护依赖） ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::MessageBus;
    use crate::manager::runtime::WasmHostContext;
    use crate::security::fs_auth::FsAuthChecker;
    use crate::storage::PluginStorage;
    use std::collections::HashSet;

    /// 无头插件状态：权限集按 mdns 授权开关构建。deny 路径在引擎权限门处
    /// 短路（零守护 / 零 spawn），其余行为契约由引擎 crate 单测覆盖
    fn state(plugin_id: &str, mdns_granted: bool) -> WasmPluginState {
        let db = Arc::new(std::sync::Mutex::new(crate::db::Database::from_connection(
            rusqlite::Connection::open_in_memory().expect("open in-memory db"),
        )));
        let storage = PluginStorage::test_storage();
        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), None, Vec::new()));
        let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(|_, _| {});
        let mut granted_permissions = HashSet::new();
        if mdns_granted {
            granted_permissions.insert(
                bedcode_plugin_api_mobile::permission::PERMISSION_MDNS.to_string(),
            );
        }
        WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx: Arc::new(WasmHostContext::new_headless(
                db,
                storage,
                None,
                fs_auth,
                Arc::new(MessageBus::new()),
                status_reporter,
            )),
            runtime_handle: tokio::runtime::Handle::current(),
            granted_permissions,
            on_message_binary: None,
        }
    }

    /// 权限门 fail-closed：未授权（granted_permissions 缺 network:mdns）时
    /// 全部浏览/广播原语在引擎权限门处直接拒绝——零守护触网（薄转发关键面）
    #[tokio::test]
    async fn mdns_denied_without_permission_short_circuits_before_daemon() {
        let s = state("deny-a", false);
        for result in [
            super::mdns_browse(&s, "_test._tcp.local."),
            super::mdns_advertise(&s, r#"{"serviceType":"_t._cp.local.","port":1}"#),
        ] {
            assert_eq!(result, Err("permission denied: mdns".to_string()));
        }
        for result in [
            super::mdns_stop_advertise(&s, "mdnsad-x"),
            super::mdns_is_advertising(&s, "mdnsad-x"),
        ] {
            assert_eq!(result, Err("permission denied: mdns".to_string()));
        }
    }

    /// 宿主身份登记纯表操作（引擎 NullTask 占位，不 spawn）：登记 → 跨 owner
    /// 拒绝 → 注销幂等
    #[tokio::test]
    async fn host_service_registration_owner_arbitration() {
        let host_reg = super::register_host_service("_co._cp.local.", "h._co._cp.local.")
            .expect("host registration books handle");
        // 非 owner（插件）注销 host 登记 → 拒绝（属主仲裁，引擎表内判定）
        let s = state("intruder-plugin", true);
        assert_eq!(
            super::mdns_stop_advertise(&s, &host_reg),
            Err("not owner of mdns handle".to_string())
        );
        // host 注销幂等移除
        assert!(super::stop_host_service(&host_reg).expect("host registration removable"));
        assert!(!super::stop_host_service(&host_reg).expect("idempotent false"));
    }

    /// purge 空表零副作用（minimal 端口，不发布 / 不拦回显 / 不触守护）
    #[tokio::test]
    async fn purge_empty_table_is_noop() {
        let ports: Arc<dyn HostEnginePorts> = Arc::new(crate::test_support::MockPorts::new());
        assert_eq!(super::purge_for_plugin(&ports, "ghost-plugin"), 0);
    }
}