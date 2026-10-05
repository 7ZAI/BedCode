//! 能力注册表与系统组件装配（core-plugin-manager，wasm-core 票据 06）
//!
//! 能力模型：
//! - 能力名 = WIT host-* 接口名（如 `host-storage`），应用插件经 manifest
//!   `dependencies` 声明依赖；
//! - 每个能力恰有一个提供者（二选一装配）：宿主 Rust 原语（缺省，现状直连）
//!   或 WASM 系统组件实例（manifest `type: "system"` 的插件，激活时按组件
//!   实际导出的同形接口注册）；
//! - 应用插件的 import 调用经 Linker 进入宿主函数（host_impl/*），路由层
//!   查询本注册表：命中系统组件提供者则 host-side 转发到该组件实例的
//!   同形导出（组件间不共享内存，载荷在 WIT 类型边界序列化），否则走
//!   宿主原语（现状路径）。
//!
//! 故障自愈：系统组件调用 trap / 传输出错时，错误隔离为调用方的
//! `Err(string)` 返回（不扩散到应用插件实例），同时该能力回落为宿主原语
//! 提供者——其停用/trap 不造成能力**永久**缺失（能力随时可被重新激活的
//! 组件再次装配）。
//!
//! 术语：本文件沿用的「系统组件」= ADR 0032 的 **L1 基础服务**（`type: basic-service`，
//! 旧拼写 `system` 仍按别名解析）。**不**承诺「只停不删」：按 kind 拒绝卸载的守卫尚未
//! 实现（`uninstall_plugin` 只判「未启用」），要随第一个真实 L1 组件同批补
//! （ADR 0032 §6 L1 清单第 3 项）——故此处不得再写「只停不删」。
//!
//! 已知边界：能力提供链不成环（系统组件转发调用中再消费由另一系统组件
//! 提供的同能力会因实例互斥锁重入而等待；fixture 期不检测环，部署约定
//! 系统组件不消费自己提供的能力）。自调用（提供者 == 调用方）直接回落
//! 宿主原语，避免死锁。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use wasmtime::component::Instance;
use wasmtime::Store;

use super::runtime::WasmPluginState;
use crate::wasm_core::host_api::context::CapabilityTarget;

// ==================== 能力名与导出探测表 ====================

/// host-storage 能力（WIT `bedcode:plugin/host-storage`，装配框架首条路由能力）
pub(crate) const CAP_HOST_STORAGE: &str = "host-storage";

/// host-mdns 能力（WIT `bedcode:plugin/host-mdns`，票 09 从能力域 crate 扩表接入）
pub(crate) const CAP_HOST_MDNS: &str = "host-mdns";

/// auth-policy 能力（票 12 C3，desktop 独有——认证中心能力，双端偏离同 host-auth）：
/// 认证中心导出 `verify-device-token` 策略，宿主 server 中间件验签后取策略。
/// 与其他路由能力不同，本能力**不进注册表路由**（消费方是宿主中间件而非
/// 插件 import）——中间件按插件 ID 直查认证中心实例，见
/// `PluginHost::call_auth_policy`（票 06：并入统一 guest 门面，不再直锁实例）。
pub(crate) const CAP_AUTH_POLICY: &str = "auth-policy";

/// 能力接口导出函数名（`ItemName` 路径语法 `pkg:ns/iface.func`——组件的接口
/// 导出是「接口实例」嵌套形态，平名字符串 `iface#func` 无法被
/// `Instance::get_func` 的 str 查找命中，wasmtime 47 实证）
pub(crate) const EXPORT_STORAGE_GET: &str = "bedcode:plugin/host-storage.get";
pub(crate) const EXPORT_STORAGE_SET: &str = "bedcode:plugin/host-storage.set";
pub(crate) const EXPORT_STORAGE_DELETE: &str = "bedcode:plugin/host-storage.delete";

/// `host-mdns` 五条原语的导出函数名（同上 ItemName 语法）
pub(crate) const EXPORT_MDNS_BROWSE: &str = "bedcode:plugin/host-mdns.browse";
pub(crate) const EXPORT_MDNS_STOP_BROWSE: &str = "bedcode:plugin/host-mdns.stop-browse";
pub(crate) const EXPORT_MDNS_ADVERTISE: &str = "bedcode:plugin/host-mdns.advertise";
pub(crate) const EXPORT_MDNS_STOP_ADVERTISE: &str = "bedcode:plugin/host-mdns.stop-advertise";
pub(crate) const EXPORT_MDNS_IS_ADVERTISING: &str = "bedcode:plugin/host-mdns.is-advertising";

/// 认证策略导出函数名（`auth-policy` 接口实例形态）
pub(crate) const EXPORT_AUTH_VERIFY_DEVICE_TOKEN: &str = "bedcode:plugin/auth-policy.verify-device-token";

/// 路由方法前缀（[`ROUTABLE_CAPABILITIES`] 第二项）
///
/// 一个可路由能力的三层方法族统一用 `<PREFIX>` 开头，**跨三层同名**：
///
/// 1. 能力域端口（消费方声明，能力域 crate 内）：
///    `SqlitePorts::forward_storage_*` / `DiscoveryPorts::forward_mdns_*`；
/// 2. 本模块的转发函数：`forward_storage_get` / `forward_mdns_browse`；
/// 3. 提供者窄端口 [`CapabilityTarget`]（声明形状，无 `forward_` 前缀）：
///    `storage_get` / `mdns_browse`。
///
/// **为什么把前缀写进表里**：这三层是「一个能力一组函数」的闭表，靠约定维护
/// 就是靠自觉；前缀入表后，闭表锁可机械比对「表 ↔ 三层方法」（见
/// [`tests::routable_capabilities_and_forward_methods_stay_in_sync`]），漏改任一层即红。
pub(crate) const FORWARD_STORAGE: &str = "storage";
pub(crate) const FORWARD_MDNS: &str = "mdns";

/// 宿主原语能力清单（**21 组** host-* WIT 接口，与 Linker 接线一一对应）
///
/// 注册表启动即全量登记为宿主原语提供者：能力对依赖检查恒可用，
/// 系统组件激活时可按名替换为 WASM 提供者。
///
/// 计数口径（票 10 复核）：v26 清单为 23 组，本票删 **2 组**——`host-session`
/// （会话原语域整 interface 退役）与 `host-terminal`（「宿主替插件往交互终端注入
/// 按键」的最后一处业务面入口：零消费者，且实现 100% 依赖会话域）——故为 **21 组**：
/// 进程 3（host-pty / host-process / host-task）+ 网络 5（host-http / host-websocket /
/// host-mdns / host-peer / host-connection）+ 存储 4（host-database / host-plugin-database /
/// host-storage / host-fs）+ 宿主面 7（host-events / host-config / host-log /
/// host-timer / host-app / host-platform / host-crypto）+ 互调 2
/// （host-bus / host-api-call）。**只减不加**：本票不新增任何组。
const HOST_PRIMITIVE_CAPABILITIES: &[&str] = &[
    "host-storage",
    "host-database",
    "host-plugin-database",
    "host-connection",
    "host-events",
    "host-http",
    "host-fs",
    "host-config",
    "host-log",
    "host-bus",
    "host-api-call",
    "host-peer",
    "host-mdns",
    "host-platform",
    "host-timer",
    "host-process",
    "host-app",
    "host-websocket",
    "host-pty",
    "host-task",
    "host-crypto",
];

/// 可路由能力表（**闭表**）：`(能力名, 路由方法前缀, 该能力接口要求组件导出的全部函数)`
///
/// 三个分量的关系是强制的，缺一即错：
/// - 能力名 = 依赖检查用的能力标识（必须是 [`HOST_PRIMITIVE_CAPABILITIES`] 里的一项）
/// - 路由方法前缀 = 三层路由方法族的共同命名前缀（见 [`FORWARD_STORAGE`]）
/// - 导出函数表 = 组件同形导出的**全部**函数（**全命中**才认定组件提供该能力，
///   少一个即视为未提供）
///
/// 新增一组能力 = 同时改四层（本表 · [`CapabilityTarget`] · 能力域端口 ·
/// `GuestOp`），漏改任一层由
/// [`tests::routable_capabilities_and_forward_methods_stay_in_sync`] 点名。
///
/// 现状：`host-storage`（装配框架首条路由能力）+ `host-mdns`（票 09 随能力域
/// crate 化扩表）。其余已迁出的能力域（peer / websocket / http / database）
/// **有意不入表**：它们的真源按调用方 `plugin_id` 分区，而转发链路当前只传参数
/// 不传调用方身份，路由过去会串命名空间——扩表须先修身份传递
/// （`.scratch/2026-10-04-wasm-core-lib-split/issues/10-capability-forward-caller-identity.md`）。
const ROUTABLE_CAPABILITIES: &[(&str, &str, &[&str])] = &[
    (
        CAP_HOST_STORAGE,
        FORWARD_STORAGE,
        &[EXPORT_STORAGE_GET, EXPORT_STORAGE_SET, EXPORT_STORAGE_DELETE],
    ),
    (
        CAP_HOST_MDNS,
        FORWARD_MDNS,
        &[
            EXPORT_MDNS_BROWSE,
            EXPORT_MDNS_STOP_BROWSE,
            EXPORT_MDNS_ADVERTISE,
            EXPORT_MDNS_STOP_ADVERTISE,
            EXPORT_MDNS_IS_ADVERTISING,
        ],
    ),
];

/// 仅探测不路由的能力（探测面 = 可路由 ∪ 本表）
///
/// `probe_exported_capabilities` 探测两张表的并集（**可路由 ⊆ 可探测**由构造保证，
/// 不再靠两份表各写一遍——旧形态两份表都列了 `host-storage`，加能力时极易只改一处）。
///
/// 本表的能力**不进注册表路由**：`auth-policy` 的消费方是宿主中间件而非插件
/// import，注册为路由提供者会让任意插件接管认证策略，语义错误。
const PROBE_ONLY_CAPABILITIES: &[(&str, &[&str])] = &[(CAP_AUTH_POLICY, &[EXPORT_AUTH_VERIFY_DEVICE_TOKEN])];

// ==================== 能力注册表 ====================

/// 能力提供者（二选一装配）
enum CapabilityProvider {
    /// 宿主 Rust 原语（host_impl/* 现状直连实现）
    HostPrimitive,
    /// WASM 系统组件实例（host-side 转发目标；窄端口，不知道调用模型）
    SystemComponent {
        plugin_id: String,
        target: Arc<dyn CapabilityTarget>,
    },
}

/// 能力注册表：能力名 → 提供者
///
/// std RwLock：临界区仅 map 读/写（不跨 await 持锁），wasm host 同步
/// 调用栈内可直接读（宿主函数非 async）。
pub struct CapabilityRegistry {
    providers: RwLock<HashMap<String, CapabilityProvider>>,
}

impl CapabilityRegistry {
    /// 创建并预登记全部宿主原语能力
    pub fn new() -> Self {
        let providers = HOST_PRIMITIVE_CAPABILITIES
            .iter()
            .map(|name| (name.to_string(), CapabilityProvider::HostPrimitive))
            .collect();
        Self {
            providers: RwLock::new(providers),
        }
    }

    /// 能力是否已有提供者（依赖检查用；未知能力名 = 无提供者）
    pub fn is_available(&self, name: &str) -> bool {
        let providers = self.providers.read().unwrap_or_else(|e| e.into_inner());
        providers.contains_key(name)
    }

    /// 返回依赖清单中未装配的能力名（依赖缺失报错用）
    pub fn missing(&self, dependencies: &[String]) -> Vec<String> {
        let providers = self.providers.read().unwrap_or_else(|e| e.into_inner());
        dependencies
            .iter()
            .filter(|dep| !providers.contains_key(dep.as_str()))
            .cloned()
            .collect()
    }

    /// 注册系统组件为能力提供者（替换宿主原语，二选一装配）
    ///
    /// 仅可路由能力可注册；未知/未接入路由的能力名拒绝并告警
    /// （注册了也无人转发，属于部署错误）。
    pub fn register_system_component(
        &self,
        name: &str,
        plugin_id: &str,
        target: Arc<dyn CapabilityTarget>,
    ) -> crate::Result<()> {
        if !is_routable(name) {
            return Err(crate::AppError::Plugin(format!(
                "capability {} is not routable (cannot register system component provider)",
                name
            )));
        }
        let mut providers = self.providers.write().unwrap_or_else(|e| e.into_inner());
        tracing::info!(
            capability = %name,
            plugin_id = %plugin_id,
            "[CapabilityRegistry] 能力切换为系统组件提供者（host-side 转发）"
        );
        providers.insert(
            name.to_string(),
            CapabilityProvider::SystemComponent {
                plugin_id: plugin_id.to_string(),
                target,
            },
        );
        Ok(())
    }

    /// 能力回落宿主原语（仅当前提供者确为该插件时生效）
    ///
    /// 用于系统组件停用/trap 自愈；条件判断防重建竞态误撤新实例的注册。
    pub fn revert_to_host(&self, name: &str, plugin_id: &str) {
        let mut providers = self.providers.write().unwrap_or_else(|e| e.into_inner());
        let dominated = matches!(
            providers.get(name),
            Some(CapabilityProvider::SystemComponent { plugin_id: pid, .. }) if pid == plugin_id
        );
        if dominated {
            tracing::info!(
                capability = %name,
                plugin_id = %plugin_id,
                "[CapabilityRegistry] 能力回落为宿主原语提供者"
            );
            providers.insert(name.to_string(), CapabilityProvider::HostPrimitive);
        }
    }

    /// 撤销某系统组件的全部能力提供（停用/重建前清理）
    pub fn revert_all_from(&self, plugin_id: &str) {
        let names: Vec<String> = {
            let providers = self.providers.read().unwrap_or_else(|e| e.into_inner());
            providers
                .iter()
                .filter_map(|(name, p)| match p {
                    CapabilityProvider::SystemComponent { plugin_id: pid, .. } if pid == plugin_id => {
                        Some(name.clone())
                    }
                    _ => None,
                })
                .collect()
        };
        for name in names {
            self.revert_to_host(&name, plugin_id);
        }
    }

    /// 查询路由目标：能力当前由系统组件提供且调用方非提供者自身时，
    /// 返回（提供者插件 ID, 转发目标）
    ///
    /// 自调用返回 None（走宿主原语，避免实例重入自锁）。
    fn system_component_instance(
        &self,
        name: &str,
        caller_plugin_id: &str,
    ) -> Option<(String, Arc<dyn CapabilityTarget>)> {
        let providers = self.providers.read().unwrap_or_else(|e| e.into_inner());
        match providers.get(name) {
            Some(CapabilityProvider::SystemComponent { plugin_id, target }) if plugin_id != caller_plugin_id => {
                Some((plugin_id.clone(), target.clone()))
            }
            _ => None,
        }
    }

    /// 提供者形态（测试断言用）："host" / "system:<plugin_id>" / "none"
    // 无 cfg(test)：host_api::context::CapabilityProvider trait impl（非 test 构建
    // 编译）委托它，不能随测试门控（票 04）
    pub(crate) fn provider_kind(&self, name: &str) -> String {
        let providers = self.providers.read().unwrap_or_else(|e| e.into_inner());
        match providers.get(name) {
            Some(CapabilityProvider::HostPrimitive) => "host".to_string(),
            Some(CapabilityProvider::SystemComponent { plugin_id, .. }) => format!("system:{}", plugin_id),
            None => "none".to_string(),
        }
    }
}

/// 测试用能力注册表（host_api 测试构造宿主上下文用；host_api 不命名具体类型，票 04）
#[cfg(test)]
pub(crate) fn test_registry() -> Arc<CapabilityRegistry> {
    Arc::new(CapabilityRegistry::new())
}

/// host_api 侧能力消费端口实现（票 04 ISP 化）：`host_api::context::CapabilityProvider`
/// 由本注册表实现——宿主上下文持有 `Arc<dyn CapabilityProvider>`，host_api 只经 trait
/// 消费能力路由（`forward_*` 与访问器），不接触具体类型；manager 侧直连具体类型
/// （方向合法）。
impl crate::wasm_core::host_api::context::CapabilityProvider for CapabilityRegistry {
    fn is_available(&self, name: &str) -> bool {
        CapabilityRegistry::is_available(self, name)
    }
    fn missing(&self, dependencies: &[String]) -> Vec<String> {
        CapabilityRegistry::missing(self, dependencies)
    }
    fn register_system_component(
        &self,
        name: &str,
        plugin_id: &str,
        target: Arc<dyn CapabilityTarget>,
    ) -> crate::Result<()> {
        CapabilityRegistry::register_system_component(self, name, plugin_id, target)
    }
    fn revert_to_host(&self, name: &str, plugin_id: &str) {
        CapabilityRegistry::revert_to_host(self, name, plugin_id)
    }
    fn revert_all_from(&self, plugin_id: &str) {
        CapabilityRegistry::revert_all_from(self, plugin_id)
    }
    fn system_component_instance(
        &self,
        name: &str,
        caller_plugin_id: &str,
    ) -> Option<(String, Arc<dyn CapabilityTarget>)> {
        CapabilityRegistry::system_component_instance(self, name, caller_plugin_id)
    }
    fn provider_kind(&self, name: &str) -> String {
        CapabilityRegistry::provider_kind(self, name)
    }
}

/// 能力是否可路由（系统组件可接管）
pub(crate) fn is_routable(name: &str) -> bool {
    ROUTABLE_CAPABILITIES.iter().any(|(cap, _, _)| *cap == name)
}

/// 探测组件实例导出的可路由能力（实例化时调用，全函数命中才算提供）
///
/// 以「可路由 + 仅探测」两张表的**并集**为准——仅探测能力（如 `auth-policy`）
/// 供宿主导航消费方直查，不进入注册表路由。可路由能力恒在探测面内（构造保证），
/// 否则宿主会在实例化期认定「本组件提供 X」，激活时却因不可路由被注册表拒绝。
pub(crate) fn probe_exported_capabilities(instance: &Instance, store: &mut Store<WasmPluginState>) -> Vec<String> {
    ROUTABLE_CAPABILITIES
        .iter()
        .map(|(cap, _, exports)| (*cap, *exports))
        .chain(PROBE_ONLY_CAPABILITIES.iter().copied())
        .filter(|(_, exports)| {
            exports.iter().all(|name| {
                // ItemName 路径语法（见 EXPORT_* 注释）；解析失败即探测未命中
                match name.parse::<wasmtime::component::wit_parser::ItemName>() {
                    Ok(item) => instance.get_func(&mut *store, &item).is_some(),
                    Err(_) => false,
                }
            })
        })
        .map(|(name, _)| name.to_string())
        .collect()
}

// ==================== host-storage 转发（host_impl/storage.rs 调用） ====================

/// `host-storage.get` 路由：系统组件提供时转发并返回结果；宿主原语提供时
/// 返回 None（调用方走现状路径）
pub(crate) fn forward_storage_get(
    cap: &dyn crate::wasm_core::host_api::context::CapabilityScope,
    caller_plugin_id: &str,
    key: &str,
) -> Option<Result<Option<String>, String>> {
    let (provider_id, target) = cap
        .capabilities()
        .system_component_instance(CAP_HOST_STORAGE, caller_plugin_id)?;
    let result = target.storage_get(key);
    Some(unwrap_forward_result(CAP_HOST_STORAGE, &provider_id, cap, result))
}

/// `host-storage.set` 路由（语义同 [`forward_storage_get`]）
pub(crate) fn forward_storage_set(
    cap: &dyn crate::wasm_core::host_api::context::CapabilityScope,
    caller_plugin_id: &str,
    key: &str,
    value: &str,
) -> Option<Result<(), String>> {
    let (provider_id, target) = cap
        .capabilities()
        .system_component_instance(CAP_HOST_STORAGE, caller_plugin_id)?;
    let result = target.storage_set(key, value);
    Some(unwrap_forward_result(CAP_HOST_STORAGE, &provider_id, cap, result))
}

/// `host-storage.delete` 路由（语义同 [`forward_storage_get`]）
pub(crate) fn forward_storage_delete(
    cap: &dyn crate::wasm_core::host_api::context::CapabilityScope,
    caller_plugin_id: &str,
    key: &str,
) -> Option<Result<(), String>> {
    let (provider_id, target) = cap
        .capabilities()
        .system_component_instance(CAP_HOST_STORAGE, caller_plugin_id)?;
    let result = target.storage_delete(key);
    Some(unwrap_forward_result(CAP_HOST_STORAGE, &provider_id, cap, result))
}

// ==================== host-mdns 转发（discovery 域能力端口调用） ====================
//
// 五条原语与 `host-storage` 同形：系统组件提供该能力时转发到它的同形导出，
// 否则返回 `None` 由能力域走本域引擎。**句柄类返回值（`browse` / `advertise`
// 的字符串 id）在提供者侧属于提供者自己的命名空间** —— 见
// `issues/10-capability-forward-caller-identity.md`（身份传递修复前，本族方法
// 只可能在「单调用方」场景下语义正确）。

/// `host-mdns.browse` 路由
pub(crate) fn forward_mdns_browse(
    cap: &dyn crate::wasm_core::host_api::context::CapabilityScope,
    caller_plugin_id: &str,
    service_type: &str,
) -> Option<Result<String, String>> {
    let (provider_id, target) = cap
        .capabilities()
        .system_component_instance(CAP_HOST_MDNS, caller_plugin_id)?;
    let result = target.mdns_browse(service_type);
    Some(unwrap_forward_result(CAP_HOST_MDNS, &provider_id, cap, result))
}

/// `host-mdns.stop-browse` 路由（语义同 [`forward_mdns_browse`]）
pub(crate) fn forward_mdns_stop_browse(
    cap: &dyn crate::wasm_core::host_api::context::CapabilityScope,
    caller_plugin_id: &str,
    browser_id: &str,
) -> Option<Result<bool, String>> {
    let (provider_id, target) = cap
        .capabilities()
        .system_component_instance(CAP_HOST_MDNS, caller_plugin_id)?;
    let result = target.mdns_stop_browse(browser_id);
    Some(unwrap_forward_result(CAP_HOST_MDNS, &provider_id, cap, result))
}

/// `host-mdns.advertise` 路由（语义同 [`forward_mdns_browse`]）
pub(crate) fn forward_mdns_advertise(
    cap: &dyn crate::wasm_core::host_api::context::CapabilityScope,
    caller_plugin_id: &str,
    config_json: &str,
) -> Option<Result<String, String>> {
    let (provider_id, target) = cap
        .capabilities()
        .system_component_instance(CAP_HOST_MDNS, caller_plugin_id)?;
    let result = target.mdns_advertise(config_json);
    Some(unwrap_forward_result(CAP_HOST_MDNS, &provider_id, cap, result))
}

/// `host-mdns.stop-advertise` 路由（语义同 [`forward_mdns_browse`]）
pub(crate) fn forward_mdns_stop_advertise(
    cap: &dyn crate::wasm_core::host_api::context::CapabilityScope,
    caller_plugin_id: &str,
    advertise_id: &str,
) -> Option<Result<bool, String>> {
    let (provider_id, target) = cap
        .capabilities()
        .system_component_instance(CAP_HOST_MDNS, caller_plugin_id)?;
    let result = target.mdns_stop_advertise(advertise_id);
    Some(unwrap_forward_result(CAP_HOST_MDNS, &provider_id, cap, result))
}

/// `host-mdns.is-advertising` 路由（语义同 [`forward_mdns_browse`]）
pub(crate) fn forward_mdns_is_advertising(
    cap: &dyn crate::wasm_core::host_api::context::CapabilityScope,
    caller_plugin_id: &str,
    advertise_id: &str,
) -> Option<Result<bool, String>> {
    let (provider_id, target) = cap
        .capabilities()
        .system_component_instance(CAP_HOST_MDNS, caller_plugin_id)?;
    let result = target.mdns_is_advertising(advertise_id);
    Some(unwrap_forward_result(CAP_HOST_MDNS, &provider_id, cap, result))
}

/// 转发结果解包：guest 返回的 `Err(string)`（WIT result 内层）原样透传；
/// **传输层失败**（trap / 超时 / 属主已停 / 应答形态不符）隔离为调用方错误结果 +
/// 能力回落宿主原语（trap 隔离不扩散 + 自愈 + 环依赖有界失败）
fn unwrap_forward_result<T>(
    capability: &str,
    provider_id: &str,
    cap: &dyn crate::wasm_core::host_api::context::CapabilityScope,
    result: Result<Result<T, String>, String>,
) -> Result<T, String> {
    match result {
        Ok(inner) => inner,
        Err(e) => {
            tracing::error!(
                capability = %capability,
                plugin_id = %provider_id,
                error = %e,
                "[CapabilityRegistry] 系统组件能力调用失败（传输错误已隔离，含超时兜底），能力回落宿主原语"
            );
            cap.capabilities().revert_to_host(capability, provider_id);
            Err(format!("system component capability call failed: {}", e))
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_pre_registers_all_host_primitives() {
        let registry = CapabilityRegistry::new();
        for cap in HOST_PRIMITIVE_CAPABILITIES {
            assert!(registry.is_available(cap), "宿主原语能力应预登记: {}", cap);
            assert_eq!(registry.provider_kind(cap), "host");
        }
        assert!(!registry.is_available("nonexistent-cap"));
    }

    #[test]
    fn missing_reports_unknown_capability_names() {
        let registry = CapabilityRegistry::new();
        let missing = registry.missing(&[
            "host-storage".to_string(),
            "no-such-cap".to_string(),
            "another-missing".to_string(),
        ]);
        assert_eq!(missing, vec!["no-such-cap", "another-missing"]);
        assert!(registry.missing(&[]).is_empty());
    }

    #[test]
    fn register_rejects_non_routable_capability() {
        // 可路由性门禁：仅 ROUTABLE_CAPABILITIES 中的能力可被系统组件接管；
        // 未知/未接入路由的能力注册即拒绝（注册了也无人转发，属部署错误）。
        // （实例构造需真实 Store，门禁判定独立于实例，由 is_routable 单测覆盖）
        assert!(is_routable(CAP_HOST_STORAGE));
        // 票 09：随能力域 crate 化扩表，host-mdns 同为可路由能力
        assert!(is_routable(CAP_HOST_MDNS));
        // 票 12：auth-policy 仅探测不路由（消费方是宿主中间件，注册为路由提供者
        // 会让任意插件接管认证策略）——register 必须拒绝
        assert!(!is_routable(CAP_AUTH_POLICY));
        assert!(!is_routable("host-bus"));
        assert!(!is_routable("nonexistent-cap"));
    }

    // ==================== 闭表锁（票 09） ====================

    /// 每个可路由能力声明的端口位置（相对 `src-tauri/`）
    ///
    /// **为什么表里带路径**：端口由各能力自己声明，宿主无法经类型系统反查「某能力的
    /// 转发方法声明在哪」。路径入表后，端口文件改名 / 搬目录 / 删方法，本锁立刻红
    /// （而不是等到某个能力路由用例失败）。
    ///
    /// ADR 0036 后两种归属并存：`host-storage` 的端口留在宿主内
    /// （`host_api/sqlite_ports.rs`），`host-mdns` 的端口仍在能力域 crate
    /// （`bedcode-discovery-engine/src/ports.rs`）——闭表锁按各自真实落点登记。
    const ROUTED_CAPABILITY_PORT_SOURCES: &[(&str, &str)] = &[
        (FORWARD_STORAGE, "src/wasm_core/host_api/sqlite_ports.rs"),
        (FORWARD_MDNS, "../packages/bedcode-discovery-engine/src/ports.rs"),
    ];

    /// 抽出 `CapabilityTarget` trait 块内声明的方法名
    ///
    /// 闭表的一端是「声明」而非「调用」，Rust 无反射 ⇒ 读源码文本（与
    /// `instance_call_model_test` 的结构锁同手法）。块边界取 trait 行到首个列零的
    /// `}`（本文件顶层项的收尾括号都在列零）。
    fn capability_target_methods(ctx_src: &str) -> Vec<String> {
        let start = ctx_src
            .find("pub trait CapabilityTarget")
            .expect("CapabilityTarget trait must exist in context.rs");
        let block = &ctx_src[start..];
        let end = block.find("\n}\n").expect("CapabilityTarget trait must be closed");
        block[..end]
            .lines()
            .filter_map(|line| line.trim().strip_prefix("fn "))
            .filter_map(|rest| rest.split('(').next())
            .map(|name| name.to_string())
            .collect()
    }

    /// 统计文本里 `fn <prefix>_` 形态的声明数（能力域端口的转发方法族）
    fn count_forward_decls(src: &str, prefix: &str) -> usize {
        src.lines()
            .filter(|line| {
                line.trim()
                    .strip_prefix("fn ")
                    .map(|rest| rest.starts_with(&format!("forward_{prefix}_")))
                    .unwrap_or(false)
            })
            .count()
    }

    /// 路由表与三层路由方法**同表同步**（闭表锁，双向）
    ///
    /// 「一个能力一组函数」在本仓库是约定，而约定不加锁等于不存在：新增一组
    /// 能力时最容易漏掉的正是「能力域端口声明了 `forward_x_*`、但本模块没有对应
    /// 转发函数」（或反之）——那时注册表会认为能力可路由，实际调用永远返回
    /// `None`，能力**静默**退回宿主原语，没有任何报错。
    ///
    /// 四条判据：
    /// 1. 表里每个能力名都在 [`HOST_PRIMITIVE_CAPABILITIES`]（否则依赖检查永远报缺）；
    /// 2. 表里每组能力都有导出函数，且导出路径全部可被 `ItemName` 解析
    ///    （拼错 ⇒ 探测永远不命中 ⇒ 能力静默不可路由）；
    /// 3. `CapabilityTarget` 的方法族 ⊇ 表里每个能力前缀，且**不多不少**
    ///    （多出来的是没人转发的方法，少的是调不到的声明）；
    /// 4. 每个能力域端口里的 `forward_<prefix>_*` 方法数 == 该能力的导出函数数
    ///    （一条导出对应一条转发方法，少一条即那条原语永远走宿主原语）。
    #[test]
    fn routable_capabilities_and_forward_methods_stay_in_sync() {
        assert!(
            !ROUTABLE_CAPABILITIES.is_empty(),
            "routable capability table must not be empty (host-storage is the baseline)"
        );

        for (cap, prefix, exports) in ROUTABLE_CAPABILITIES {
            // 1. 能力名必须是注册表已登记的宿主原语能力
            assert!(
                HOST_PRIMITIVE_CAPABILITIES.contains(cap),
                "routable capability '{cap}' is not a registered host primitive capability: {HOST_PRIMITIVE_CAPABILITIES:?}"
            );
            assert!(
                !prefix.is_empty(),
                "routable capability '{cap}' declares an empty forward prefix"
            );
            // 2. 导出函数表非空且每条都能解析（探测面据此逐条命中）
            assert!(
                !exports.is_empty(),
                "routable capability '{cap}' declares no export (a provider could never satisfy it)"
            );
            for export in *exports {
                export
                    .parse::<wasmtime::component::wit_parser::ItemName>()
                    .unwrap_or_else(|e| panic!("export '{export}' of '{cap}' is not a valid ItemName: {e}"));
            }
            // 4. 能力域端口的转发方法族与导出数一一对应
            let (_, port_src) = ROUTED_CAPABILITY_PORT_SOURCES
                .iter()
                .find(|(p, _)| p == prefix)
                .unwrap_or_else(|| {
                    panic!(
                        "forward prefix '{prefix}' (capability '{cap}') has no entry in \
                         ROUTED_CAPABILITY_PORT_SOURCES"
                    )
                });
            let port_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(port_src);
            let port_text = std::fs::read_to_string(&port_path)
                .unwrap_or_else(|e| panic!("read capability domain ports {}: {e}", port_path.display()));
            let decls = count_forward_decls(&port_text, prefix);
            assert_eq!(
                decls,
                exports.len(),
                "capability '{cap}': capability domain ports {} declare {decls} \
                 `forward_{prefix}_*` methods but the routable table lists {} exports",
                port_path.display(),
                exports.len()
            );
        }

        // 3. 提供者窄端口的方法族与表**逐项对应**（不多不少）
        let ctx_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/wasm_core/host_api/context.rs");
        let ctx_src = std::fs::read_to_string(&ctx_path).unwrap_or_else(|e| panic!("read {}: {e}", ctx_path.display()));
        let methods = capability_target_methods(&ctx_src);
        for (_, prefix, exports) in ROUTABLE_CAPABILITIES {
            let marker = format!("{prefix}_");
            let declared: Vec<&String> = methods.iter().filter(|m| m.starts_with(&marker)).collect();
            assert_eq!(
                declared.len(),
                exports.len(),
                "CapabilityTarget declares {declared:?} for forward prefix '{prefix}' but the \
                 routable table lists {} exports — one method per exported function, no more",
                exports.len()
            );
        }
        for method in &methods {
            let owner = ROUTABLE_CAPABILITIES
                .iter()
                .find(|(_, prefix, _)| method.starts_with(&format!("{prefix}_")));
            assert!(
                owner.is_some(),
                "CapabilityTarget method '{method}' belongs to no routable capability (closed table) — \
                 add the capability to ROUTABLE_CAPABILITIES or remove the method"
            );
        }
    }

    /// 路由表本身的三条不变量（表形态漂移的自检）
    #[test]
    fn routable_table_entries_are_well_formed() {
        // 能力名不重复（重复项会让 `is_routable` 与探测表语义含糊）
        let mut names: Vec<&str> = ROUTABLE_CAPABILITIES.iter().map(|(cap, _, _)| *cap).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate capability in routable table: {names:?}");

        // 前缀不重复：两个能力共用一个前缀 ⇒ 方法族混在一起，闭表锁的前缀判据失效
        let mut prefixes: Vec<&str> = ROUTABLE_CAPABILITIES.iter().map(|(_, prefix, _)| *prefix).collect();
        let before = prefixes.len();
        prefixes.sort_unstable();
        prefixes.dedup();
        assert_eq!(
            prefixes.len(),
            before,
            "two routable capabilities share one forward prefix: {prefixes:?}"
        );

        // 仅探测能力不得与可路由能力同名（否则「仅探测」的语义说明失真）
        for (cap, _) in PROBE_ONLY_CAPABILITIES {
            assert!(
                !is_routable(cap),
                "'{cap}' is listed as probe-only but is also routable"
            );
        }
    }

    #[test]
    fn revert_to_host_only_when_provider_matches() {
        // 条件回落：提供者不是指定插件时不动现注册（防重建竞态误撤新实例）
        let registry = CapabilityRegistry::new();
        registry.revert_to_host(CAP_HOST_STORAGE, "com.test.sys");
        assert_eq!(registry.provider_kind(CAP_HOST_STORAGE), "host");
    }
}
