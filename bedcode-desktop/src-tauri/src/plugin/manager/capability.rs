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
//! 提供者——系统组件「只停不删」，其停用/trap 不造成能力永久缺失。
//!
//! 已知边界：能力提供链不成环（系统组件转发调用中再消费由另一系统组件
//! 提供的同能力会因实例互斥锁重入而等待；fixture 期不检测环，部署约定
//! 系统组件不消费自己提供的能力）。自调用（提供者 == 调用方）直接回落
//! 宿主原语，避免死锁。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tokio::sync::Mutex;
use wasmtime::component::Instance;
use wasmtime::Store;

use super::wasm_runtime::{block_on_async, LoadedWasmPlugin, WasmHostContext, WasmPluginState};

// ==================== 能力名与导出探测表 ====================

/// host-storage 能力（WIT `bedcode:plugin/host-storage`，装配框架首条路由能力）
pub(crate) const CAP_HOST_STORAGE: &str = "host-storage";

/// auth-policy 能力（票 12 C3，desktop 独有——认证中心能力，双端偏离同 host-auth）：
/// 认证中心导出 `verify-device-token` 策略，宿主 server 中间件验签后取策略。
/// 与其他路由能力不同，本能力**不进注册表路由**（消费方是宿主中间件而非
/// 插件 import）——中间件按插件 ID 直查认证中心实例，见
/// `PluginHost::call_plugin_capability_export`。
pub(crate) const CAP_AUTH_POLICY: &str = "auth-policy";

/// 能力接口导出函数名（`ItemName` 路径语法 `pkg:ns/iface.func`——组件的接口
/// 导出是「接口实例」嵌套形态，平名字符串 `iface#func` 无法被
/// `Instance::get_func` 的 str 查找命中，wasmtime 47 实证）
const EXPORT_STORAGE_GET: &str = "bedcode:plugin/host-storage.get";
const EXPORT_STORAGE_SET: &str = "bedcode:plugin/host-storage.set";
const EXPORT_STORAGE_DELETE: &str = "bedcode:plugin/host-storage.delete";

/// 认证策略导出函数名（`auth-policy` 接口实例形态）
pub(crate) const EXPORT_AUTH_VERIFY_DEVICE_TOKEN: &str = "bedcode:plugin/auth-policy.verify-device-token";

/// 宿主原语能力清单（20 组 host-* WIT 接口，与 Linker 接线一一对应）
///
/// 注册表启动即全量登记为宿主原语提供者：能力对依赖检查恒可用，
/// 系统组件激活时可按名替换为 WASM 提供者。
const HOST_PRIMITIVE_CAPABILITIES: &[&str] = &[
    "host-storage",
    "host-database",
    "host-plugin-database",
    "host-terminal",
    "host-session",
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
];

/// 可路由能力表：能力名 → 该能力接口要求组件导出的全部函数
/// （导出存在性全命中才认定组件提供该能力）
///
/// 首条路由能力为 host-storage；后续能力随「宿主引擎逐个 WASM 化」立项
/// 逐组加入（每组需在 host_impl 对应模块接入转发分支）。
const ROUTABLE_CAPABILITIES: &[(&str, &[&str])] = &[(
    CAP_HOST_STORAGE,
    &[EXPORT_STORAGE_GET, EXPORT_STORAGE_SET, EXPORT_STORAGE_DELETE],
)];

/// 探测能力表：可路由能力 + **仅探测不路由**能力（票 12 `auth-policy`）
///
/// `probe_exported_capabilities` 以本表探测实例导出存在性——认证中心实例化时
/// `exported_capabilities()` 含 `auth-policy`，宿主 server 中间件据此确认策略
/// 导出就绪（`call_capability_export` 前探测）。仅探测不路由：SDK 默认实现使
/// 所有新 SDK 插件都导出该接口（默认拒绝），且消费方是宿主中间件而非插件
/// import——注册为路由提供者会让任意插件接管认证策略，语义错误。
const PROBE_CAPABILITIES: &[(&str, &[&str])] = &[
    (
        CAP_HOST_STORAGE,
        &[EXPORT_STORAGE_GET, EXPORT_STORAGE_SET, EXPORT_STORAGE_DELETE],
    ),
    (CAP_AUTH_POLICY, &[EXPORT_AUTH_VERIFY_DEVICE_TOKEN]),
];

// ==================== 能力注册表 ====================

/// 能力提供者（二选一装配）
enum CapabilityProvider {
    /// 宿主 Rust 原语（host_impl/* 现状直连实现）
    HostPrimitive,
    /// WASM 系统组件实例（host-side 转发目标）
    SystemComponent {
        plugin_id: String,
        instance: Arc<Mutex<LoadedWasmPlugin>>,
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
        instance: Arc<Mutex<LoadedWasmPlugin>>,
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
                instance,
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
    /// 返回（提供者插件 ID, 实例句柄）
    ///
    /// 自调用返回 None（走宿主原语，避免实例互斥锁重入死锁）。
    fn system_component_instance(
        &self,
        name: &str,
        caller_plugin_id: &str,
    ) -> Option<(String, Arc<Mutex<LoadedWasmPlugin>>)> {
        let providers = self.providers.read().unwrap_or_else(|e| e.into_inner());
        match providers.get(name) {
            Some(CapabilityProvider::SystemComponent { plugin_id, instance }) if plugin_id != caller_plugin_id => {
                Some((plugin_id.clone(), instance.clone()))
            }
            _ => None,
        }
    }

    /// 提供者形态（测试断言用）："host" / "system:<plugin_id>" / "none"
    #[cfg(test)]
    pub(crate) fn provider_kind(&self, name: &str) -> String {
        let providers = self.providers.read().unwrap_or_else(|e| e.into_inner());
        match providers.get(name) {
            Some(CapabilityProvider::HostPrimitive) => "host".to_string(),
            Some(CapabilityProvider::SystemComponent { plugin_id, .. }) => format!("system:{}", plugin_id),
            None => "none".to_string(),
        }
    }
}

/// 能力是否可路由（系统组件可接管）
pub(crate) fn is_routable(name: &str) -> bool {
    ROUTABLE_CAPABILITIES.iter().any(|(cap, _)| *cap == name)
}

/// 探测组件实例导出的可路由能力（实例化时调用，全函数命中才算提供）
///
/// 以 [`PROBE_CAPABILITIES`]（可路由 + 仅探测）为准——仅探测能力（如
/// `auth-policy`）供宿主导航消费方直查，不进入注册表路由。
pub(crate) fn probe_exported_capabilities(instance: &Instance, store: &mut Store<WasmPluginState>) -> Vec<String> {
    PROBE_CAPABILITIES
        .iter()
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
    host_ctx: &WasmHostContext,
    caller_plugin_id: &str,
    key: &str,
) -> Option<Result<Option<String>, String>> {
    let (provider_id, instance) = host_ctx
        .capabilities()
        .system_component_instance(CAP_HOST_STORAGE, caller_plugin_id)?;
    let key = key.to_string();
    let result = block_on_async(async move {
        let mut guard = instance.lock().await;
        guard.call_capability_export::<(String,), (Result<Option<String>, String>,)>(EXPORT_STORAGE_GET, (key,))
    });
    Some(unwrap_forward_result(CAP_HOST_STORAGE, &provider_id, host_ctx, result))
}

/// `host-storage.set` 路由（语义同 [`forward_storage_get`]）
pub(crate) fn forward_storage_set(
    host_ctx: &WasmHostContext,
    caller_plugin_id: &str,
    key: &str,
    value: &str,
) -> Option<Result<(), String>> {
    let (provider_id, instance) = host_ctx
        .capabilities()
        .system_component_instance(CAP_HOST_STORAGE, caller_plugin_id)?;
    let key = key.to_string();
    let value = value.to_string();
    let result = block_on_async(async move {
        let mut guard = instance.lock().await;
        guard.call_capability_export::<(String, String), (Result<(), String>,)>(EXPORT_STORAGE_SET, (key, value))
    });
    Some(unwrap_forward_result(CAP_HOST_STORAGE, &provider_id, host_ctx, result))
}

/// `host-storage.delete` 路由（语义同 [`forward_storage_get`]）
pub(crate) fn forward_storage_delete(
    host_ctx: &WasmHostContext,
    caller_plugin_id: &str,
    key: &str,
) -> Option<Result<(), String>> {
    let (provider_id, instance) = host_ctx
        .capabilities()
        .system_component_instance(CAP_HOST_STORAGE, caller_plugin_id)?;
    let key = key.to_string();
    let result = block_on_async(async move {
        let mut guard = instance.lock().await;
        guard.call_capability_export::<(String,), (Result<(), String>,)>(EXPORT_STORAGE_DELETE, (key,))
    });
    Some(unwrap_forward_result(CAP_HOST_STORAGE, &provider_id, host_ctx, result))
}

/// 转发结果解包：guest 返回的 `Err(string)`（WIT result 内层）原样透传；
/// trap/传输出错（外层 Err）隔离为调用方错误结果 + 能力回落宿主原语
///（trap 隔离不扩散 + 自愈）
fn unwrap_forward_result<T>(
    capability: &str,
    provider_id: &str,
    host_ctx: &WasmHostContext,
    result: crate::Result<(Result<T, String>,)>,
) -> Result<T, String> {
    match result {
        Ok((r,)) => r,
        Err(e) => {
            tracing::error!(
                capability = %capability,
                plugin_id = %provider_id,
                error = %e,
                "[CapabilityRegistry] 系统组件能力调用失败（trap/传输错误已隔离），能力回落宿主原语"
            );
            host_ctx.capabilities().revert_to_host(capability, provider_id);
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
        // 票 12：auth-policy 仅探测不路由（消费方是宿主中间件，注册为路由提供者
        // 会让任意插件接管认证策略）——register 必须拒绝
        assert!(!is_routable(CAP_AUTH_POLICY));
        assert!(!is_routable("host-bus"));
        assert!(!is_routable("nonexistent-cap"));
    }

    #[test]
    fn revert_to_host_only_when_provider_matches() {
        // 条件回落：提供者不是指定插件时不动现注册（防重建竞态误撤新实例）
        let registry = CapabilityRegistry::new();
        registry.revert_to_host(CAP_HOST_STORAGE, "com.test.sys");
        assert_eq!(registry.provider_kind(CAP_HOST_STORAGE), "host");
    }
}
