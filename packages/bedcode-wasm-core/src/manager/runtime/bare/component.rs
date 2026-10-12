//! 组件模型壳（移动形态；票 05 绑定归位后：**内核零 WIT 绑定**）
//!
//! ## 本模块持有什么
//!
//! - linker 组装：**宿主自报能力模块**（`inventory`：core 12 域 + 移动独有 5 域，
//!   全部由移动宿主 adapter 自报，见 host-kit `registry` / `assembly`）+ WASI p2/p3
//!   （核心装配面，默认全开）；
//! - [`LoadedComponentPlugin`]：Store + Instance 所有权、导出调用骨架——
//!   `command` / `lifecycle` / `manifest` 走 `runtime::core_binding` 的 core 交集绑定
//!   （双端一份，绑契约真源 `wit/core.wit`），`abi.version` 走原生 `TypedFunc` 探测，
//!   `events.*` 走宿主注入的 [`GuestEventDispatcher`]（移动词汇 = on-bus-message /
//!   on-auth-success / on-disconnect / on-session-*，由移动宿主 `plugin/guest_events.rs`
//!   用自有绑定实现并注入）。
//!
//! ## 为什么内核不再绑定移动 world（票 05 / ADR 0022）
//!
//! `bindgen!` 旧形态把移动 world 的 12 个 core 域 `Host` impl 与 5 个移动独有域替身
//! 钉在核心里——契约事实（谁的 world、什么签名）住内核，与「内核零端差异」相悖。
//! 归位后：**绑定与域实现全部在移动宿主**（`src-tauri/src/plugin/{bindings,host_domains,
//! guest_events}.rs`），内核只剩「怎么驱动一个组件实例」的机制（见 ADR 0045/0047）。
//!
//! 移动 core 12 域没有复用桌面 `core_binding` 的实现：权限门不同源（移动 =
//! 实例级 `granted_permissions` 集）且插件私有库布局不同（移动
//! `plugins/<id 去点划>.db`）——详见 `plugin/host_domains/mod.rs` 头注。
//!
//! ## 与 core module 路径的差异（历史）
//!
//! - export 全部为必选：core ABI 中 on_message 等可选导出在组件契约中强制
//!   （组件 world 声明即契约，阶段 B SDK 无条件导出全部）
//! - log 不带 file/line 调用点（core ABI 经 ABI 传插件源码位置）
//! - 内存搬运由绑定层处理，无需 (ptr,len) 配对与 alloc/dealloc

use super::{StoreSpec, WasmHostContext, WasmPluginState};
use crate::manager::runtime::engine;
// guest 事件派发契约面（票 03 / ADR 0022）：`events` 导出双端词汇不同、且带
// `result` 返回（原生 TypedFunc 表达不了），调用契约由宿主实现并注入，本壳只转发。
use crate::manager::runtime::guest_events::GuestEventDispatcher;
use crate::AppError;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use wasmtime::component::{Component, Instance, Linker};
use wasmtime::Store;

// ==================== Component Linker 组装 ====================

/// 构建组件侧 Linker（装配面 = 宿主自报能力模块 + WASI）
///
/// 两段式（票 05 归位后内核不再有自有 import 域）：
/// 1. **宿主自报能力模块**：移动 world 的全部 import 接口（core 12 + 移动独有 5）
///    由移动宿主 adapter 经 `HostModule` + `inventory` 自报——先白名单双向校验
///    （收集结果必须等于宿主 `expect_host_module!` 的期望集，漏引/多引即红），
///    再按模块名字典序逐模块注册。未注册接口被组件 import 时实例化报
///    unknown import——缺接口即显性报错，不拖到调用期；
/// 2. **WASI p2/p3**（双端同源的核心装配面，用户裁决 2026-10-12；实现真源
///    [`crate::manager::runtime::wasi::add_to_linker`]）：当前移动产物是
///    wasm32-unknown-unknown（不 import WASI）⇒ 注册惰性无效；worker 类别
///    （ADR 0034 L3.b）产物改 wasip2/p3 即直接可用。
///
/// `wasi_cfg`：WASI 面开关（`CoreConfig::wasi`，**默认全开**）。
pub(crate) fn build_component_linker(
    engine: &wasmtime::Engine,
    wasi_cfg: &crate::config::WasiConfig,
) -> crate::Result<Linker<WasmPluginState>> {
    let mut linker = Linker::new(engine);
    let registry = bedcode_host_kit::ModuleRegistry::collected();
    registry
        .verify_whitelist(&bedcode_host_kit::expected_host_modules())
        .map_err(|e| AppError::Plugin(format!("host module whitelist mismatch: {e}")))?;
    registry
        .install_all(&mut linker)
        .map_err(|e| AppError::Plugin(format!("host module install failed: {e}")))?;
    crate::manager::runtime::wasi::add_to_linker(&mut linker, wasi_cfg)?;
    Ok(linker)
}

// ==================== WasmRuntime 组件路径 ====================

impl super::WasmRuntime {
    /// 从文件编译 WASM 组件（带 AOT 缓存）
    ///
    /// 机制本体在 [`engine`]（双端唯一一份：缓存键以 `c` 前缀 + wasm 路径 hash
    /// 命名、宿主 cache 目录（非插件目录）、原子写回、deserialize 失败降级重编译）。
    /// 本方法只把形态面（Engine 句柄 + 宿主 cache 目录）传进去。
    /// 宿主 PluginLoader 消费（组件编译 + AOT 缓存）
    pub fn compile_component_from_file(&self, path: &Path) -> crate::Result<Component> {
        engine::compile_component_from_file(&self.engine, self.aot_cache_dir.as_deref(), path)
    }

    /// 实例化 WASM 组件
    ///
    /// 防护：ResourceLimiter、实例化前燃料注入（覆盖静态构造器）、
    /// `abi.version()` 协商（> ABI_VERSION 拒绝）。
    /// 宿主 PluginLoader 消费（实例化 + ABI 协商）
    pub fn instantiate_component(
        &self,
        component: &Component,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
        granted_permissions: HashSet<String>,
    ) -> crate::Result<LoadedComponentPlugin> {
        // 统一 state（host-kit）：权限集随实例注入 per-plugin 上下文（Arc 克隆共享
        // 机制句柄）；StoreSpec 取默认资源上限 + 开燃料看门狗
        let host: Arc<dyn bedcode_host_kit::ports::HostPorts> =
            Arc::new(host_ctx.for_plugin(granted_permissions));
        let spec = StoreSpec {
            limits: bedcode_host_kit::limits::StoreLimits::default(),
            fuel_enabled: true,
            metrics: Arc::new(bedcode_host_kit::metrics::PluginMetrics::default()),
        };
        // WASI 上下文：移动形态无 manifest 预打开声明面（`wasiPreopenDirs` 是桌面
        // worker 通道，ADR 0034）⇒ 空上下文；`CoreConfig::wasi` 的开/关体现在 linker
        // 注册面（build_component_linker）。worker 接线时此处改 `wasi::build_ctx`。
        let state = WasmPluginState::new(
            plugin_id.to_string(),
            host,
            crate::manager::runtime::wasi::empty_ctx(),
            spec,
        );
        // Store 装配（limiter 接线 + 首调用燃料注入）走引擎模块的唯一一份；
        // 移动形态插件是 wasm32-unknown-unknown（无 WASI import），实例化走同步入口
        let mut store = engine::new_store(&self.engine, state)?;

        let instance = engine::instantiate(
            &mut store,
            &self.linker,
            component,
            engine::InstantiateMode::Sync,
        )
        .map_err(|e| {
            AppError::Plugin(format!(
                "Failed to instantiate WASM component for plugin '{}': {}",
                plugin_id, e
            ))
        })?;

        LoadedComponentPlugin::verify_abi(&mut store, &instance)?;

        // 实例创建日志：与 LoadedComponentPlugin::drop 的死亡日志成对，
        // 构成实例生命周期观测（plugin_id 键控，实现见 engine）
        engine::log_instance_created(plugin_id);

        Ok(LoadedComponentPlugin {
            plugin_id: plugin_id.to_string(),
            instance,
            store,
            created_at: std::time::Instant::now(),
            // 派发器快照（与桌面壳同款）：实例化时的注入值决定本实例后续事件投递
            guest_events: self
                .guest_events
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        })
    }
}

// ==================== 组件插件实例 ====================

/// 已加载的组件形态 WASM 插件
///
/// 持有 component Instance + Store；导出调用走 core 交集绑定（command / lifecycle /
/// manifest）与原生探测（abi / 可选导出），`events` 走宿主注入的派发器。
/// Store 必须与 Instance 一起持有，否则导出函数无法调用。
pub struct LoadedComponentPlugin {
    plugin_id: String,
    instance: wasmtime::component::Instance,
    store: Store<WasmPluginState>,
    /// 实例创建时刻（Drop 日志计算存活时长）
    created_at: std::time::Instant,
    /// 移动 guest 事件派发器（票 05 / ADR 0022：契约面由宿主实现并注入）
    guest_events: Arc<dyn GuestEventDispatcher>,
}

impl Drop for LoadedComponentPlugin {
    /// 实例死亡日志：Store 被 drop（停用 / 热重载替换 / 应用退出 / 异常清理）时记录，
    /// 与创建日志（`instantiate_component`）成对，构成实例生命周期观测。
    /// Drop 内无锁操作，tracing 安全。日志实现见 [`engine::log_instance_dropped`]。
    fn drop(&mut self) {
        engine::log_instance_dropped(&self.plugin_id, self.created_at.elapsed());
    }
}

#[allow(dead_code)] // 宿主消费面（PluginManager/PluginLoader 调用业务方法）
impl LoadedComponentPlugin {
    /// ABI 版本协商（插件 `abi.version()` 高于宿主支持版本时拒绝加载）
    ///
    /// - `abi.version()` 语义与 `bedcode_plugin_api_mobile::abi::ABI_VERSION` 一致
    /// - 移动端 WIT 无 `abi.form()`（项目未发布、一次性切割，无 core 共存形态）
    ///
    /// `abi.version` 走原生 `TypedFunc` 探测（`core_binding::abi_version`）：`abi`
    /// 虽在 core.wit，却不在 `world core` 的导出表内（各端 cap 追加）——绑进内核
    /// 等于让内核「对谁的 abi」表态。原生探测零绑定零端差异。
    fn verify_abi(store: &mut Store<WasmPluginState>, instance: &Instance) -> crate::Result<()> {
        // 本路径不经 exports()（实例化后立即校验），独立重置燃料；预算口径与
        // 桌面形态同源（`StoreLimits::fuel_budget`，debug 构建含放大倍率）
        engine::inject_call_fuel(store)?;
        let version = crate::manager::runtime::core_binding::abi_version(store, instance)?
            .ok_or_else(|| {
                AppError::Plugin(
                    "WASM component missing required export `abi.version` \
                     （该产物由其他插件协议（ABI）的插件 SDK 构建，请用当前 SDK 重建插件产物）"
                        .to_string(),
                )
            })?;
        if version > bedcode_plugin_api_mobile::abi::ABI_VERSION {
            return Err(AppError::Plugin(format!(
                "Plugin requires ABI v{} but host supports v{} — please upgrade BedCode",
                version,
                bedcode_plugin_api_mobile::abi::ABI_VERSION
            )));
        }

        // v9：动态探测可选导出 events-binary#on-message-binary。该接口不声明进
        // plugin world（旧插件必选导出会因缺失而实例化失败），此处按名探测，
        // 缺失容忍为 None——旧插件只收 JSON，二进制消息由总线按格式不匹配拒绝。
        //
        // **已知缺陷（票 05 §7 记录，未在本票修）**：这里沿用迁入前的**平名**
        // 探测串（`iface#func`）。组件接口导出是嵌套实例形态，平名查找恒不命中
        // （桌面侧早已改为 ItemName 点号 `iface.func` 并实证）——即本探测当前恒为
        // `None`，二进制投递在本端实际不可达。修它 = 行为变更（二进制消息开始真的
        // 投进插件），按「迁移保持行为」纪律留给后续专项（T-06 之后）。
        let on_message_binary = instance
            .get_typed_func::<(String, String, Vec<u8>), ()>(
                &mut *store,
                "bedcode:plugin/events-binary#on-message-binary",
            )
            .ok();
        store.data_mut().on_message_binary = on_message_binary;
        Ok(())
    }

    /// 获取 **core 交集** 导出绑定（command / lifecycle / manifest）
    ///
    /// 顺带重置燃料预算（单次调用预算 + 上一区间消耗记账，实现见
    /// [`engine::refill_call_fuel`]；宿主调用阻塞不消耗燃料）。
    /// cap 导出不在此：`events` 走宿主派发器、可选导出走原生探测。
    fn exports(&mut self) -> crate::Result<crate::manager::runtime::core_binding::CoreExports> {
        engine::refill_call_fuel(&mut self.store)?;
        crate::manager::runtime::core_binding::exports(&mut self.store, &self.instance)
    }

    /// 调用事件导出前的共同准备：燃料续费（事件回调同样消耗 guest 燃料）
    fn refill_call_fuel(&mut self) -> crate::Result<()> {
        engine::refill_call_fuel(&mut self.store)
    }

    // ==================== 业务方法 ====================

    /// 调用插件的 activate 导出
    pub fn activate(&mut self) -> crate::Result<i32> {
        let exports = self.exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        match crate::runtime_util::block_on_async(async {
            lifecycle.call_activate(&mut self.store).await
        }) {
            Ok(Ok(())) => Ok(0),
            Ok(Err(msg)) => Err(AppError::Plugin(format!("WASM activate() failed: {}", msg))),
            Err(e) => Err(engine::trapped(&self.store, "activate", e)),
        }
    }

    /// 调用插件的 deactivate 导出
    pub fn deactivate(&mut self) -> crate::Result<i32> {
        let exports = self.exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        match crate::runtime_util::block_on_async(async {
            lifecycle.call_deactivate(&mut self.store).await
        }) {
            Ok(Ok(())) => Ok(0),
            Ok(Err(msg)) => Err(AppError::Plugin(format!(
                "WASM deactivate() failed: {}",
                msg
            ))),
            Err(e) => Err(engine::trapped(&self.store, "deactivate", e)),
        }
    }

    /// 调用插件的 invoke_command 导出
    pub fn invoke_command(&mut self, command_name: &str, args_json: &str) -> crate::Result<String> {
        let exports = self.exports()?;
        let cmd = exports.bedcode_plugin_command();
        crate::runtime_util::block_on_async(async {
            cmd.call_invoke(&mut self.store, command_name, args_json).await
        })
        .map_err(|e| engine::trapped(&self.store, "invoke_command", e))
    }

    /// 调用插件的 on_startup 导出
    ///
    /// WIT `result<_, string>`：内层 Err 为插件显式失败（启动初始化未完成），
    /// 映射为 `crate::Result` 错误以驱动宿主 Degraded 状态机。
    pub fn on_startup(&mut self) -> crate::Result<()> {
        let exports = self.exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        match crate::runtime_util::block_on_async(async {
            lifecycle.call_on_startup(&mut self.store).await
        }) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(msg)) => Err(AppError::Plugin(format!(
                "WASM on_startup() failed: {}",
                msg
            ))),
            Err(e) => Err(engine::trapped(&self.store, "on_startup", e)),
        }
    }

    /// 调用插件的 on_shutdown 导出
    pub fn on_shutdown(&mut self) -> crate::Result<()> {
        let exports = self.exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        match crate::runtime_util::block_on_async(async {
            lifecycle.call_on_shutdown(&mut self.store).await
        }) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(msg)) => Err(AppError::Plugin(format!(
                "WASM on_shutdown() failed: {}",
                msg
            ))),
            Err(e) => Err(engine::trapped(&self.store, "on_shutdown", e)),
        }
    }

    /// 调用插件的消息总线消息接收导出（移动端 WIT events.on-bus-message）
    ///
    /// 派发委托给宿主注入的 [`GuestEventDispatcher`]（票 05）：本壳不持有 `events`
    /// 绑定，只把 (Store, Instance) 与消息交给移动宿主实现。移动 WIT 的
    /// `on-bus-message(topic, payload)` 无 sender 参数——统一签名里的 sender 由
    /// 实现方自行决定是否消费（移动实现不消费，见 `plugin/guest_events.rs`）。
    pub fn on_bus_message(
        &mut self,
        msg: &bedcode_plugin_api_mobile::BusMessage,
    ) -> crate::Result<()> {
        let payload_str = serde_json::to_string(&msg.payload).unwrap_or_default();
        self.refill_call_fuel()?;
        let instance = self.instance;
        self.guest_events
            .on_message(&mut self.store, instance, &msg.topic, &msg.sender, &payload_str)
    }

    /// 调用插件的消息总线二进制消息接收导出（v9，可选导出动态探测）
    ///
    /// 仅当实例在实例化时探测到 `events-binary#on-message-binary` 导出才可调用；
    /// 总线侧已按订阅者格式偏好过滤，正常不会对无导出的实例发起本调用，
    /// 此处防御性拒绝（旧插件二进制 → 格式不匹配拒绝的语义由总线保证）。
    ///
    /// 注：探测串形态缺陷（恒 None，见 [`Self::verify_abi`] 注释）使本方法当前
    /// 恒返回「无导出」——行为与迁入前一致，修复属后续专项。
    pub fn on_message_binary(
        &mut self,
        topic: &str,
        sender: &str,
        payload: &[u8],
    ) -> crate::Result<()> {
        // 燃料续费：本方法不经 exports()（直接持探测句柄调用），必须自带续费
        self.refill_call_fuel()?;
        // TypedFunc 先 clone 再调用，避免与 &mut self.store 的借用冲突
        let Some(func) = self.store.data().on_message_binary.clone() else {
            return Err(AppError::Plugin(format!(
                "WASM plugin has no events-binary export (v9 required)"
            )));
        };
        // 无返回值（观察型回调）：guest 内部失败经 host-log 记录；此处仅 trap 上抛
        crate::runtime_util::block_on_async(async {
            func.call_async(
                &mut self.store,
                (topic.to_string(), sender.to_string(), payload.to_vec()),
            )
            .await
        })
        .map_err(|e| engine::trapped(&self.store, "on_message_binary", e))
    }

    /// 获取插件的 manifest JSON
    ///
    /// pub（票 05）：宿主集成测试直接驱动真实产物读 manifest（api 清单比对），
    /// 与桌面壳 `get_manifest` 同口径。
    pub fn get_manifest(&mut self) -> crate::Result<String> {
        let exports = self.exports()?;
        let manifest = exports.bedcode_plugin_manifest();
        crate::runtime_util::block_on_async(async { manifest.call_get(&mut self.store).await })
            .map_err(|e| engine::trapped(&self.store, "manifest", e))
    }

    /// 调用插件的生命周期事件导出（移动端 PluginLifecycleEvent 枚举 → 导出映射）
    ///
    /// 映射表：AppStartup/AppShutdown → `lifecycle.on-startup/on-shutdown`（core 绑定）；
    /// AuthSuccess → `events.on-auth-success`；Disconnect → `events.on-disconnect`；
    /// SessionCreated/SessionStopped → `events.on-session-created/on-session-stopped`
    /// （后四者是移动独有导出，经宿主注入的派发器投递）。
    pub fn call_lifecycle_event(
        &mut self,
        event: &crate::manager::types::PluginLifecycleEvent,
    ) -> crate::Result<()> {
        use crate::manager::types::PluginLifecycleEvent;
        // 事件导出同样消耗 guest 燃料：派发前续费（与 on_bus_message 同款；
        // AppStartup/AppShutdown 走 lifecycle 导出，其续费在 exports() 内）
        match event {
            PluginLifecycleEvent::AppStartup => self.on_startup(),
            PluginLifecycleEvent::AppShutdown => self.on_shutdown(),
            PluginLifecycleEvent::AuthSuccess => {
                self.refill_call_fuel()?;
                let instance = self.instance;
                self.guest_events.on_auth_success(&mut self.store, instance)
            }
            PluginLifecycleEvent::Disconnect { reason } => {
                self.refill_call_fuel()?;
                let instance = self.instance;
                self.guest_events
                    .on_disconnect(&mut self.store, instance, reason)
            }
            PluginLifecycleEvent::SessionCreated { session_id } => {
                self.refill_call_fuel()?;
                let instance = self.instance;
                self.guest_events
                    .on_session_created(&mut self.store, instance, session_id)
            }
            PluginLifecycleEvent::SessionStopped { session_id } => {
                self.refill_call_fuel()?;
                let instance = self.instance;
                self.guest_events
                    .on_session_stopped(&mut self.store, instance, session_id)
            }
        }
    }

    /// 测试访问器：直接获取 Store / Instance（燃料断言与耗尽 trap 测试用）
    ///
    /// 非 `#[cfg(test)]` 门控的原因：宿主集成测试链的是**不带 cfg(test)** 的
    /// 本 crate——燃料 / 摘要断言经 `crate::test_support` 的转发面消费（生产跨 crate
    /// 仍不可达：pub(crate)）。
    pub(crate) fn raw_store(
        &mut self,
    ) -> (&mut Store<WasmPluginState>, &wasmtime::component::Instance) {
        (&mut self.store, &self.instance)
    }
}
