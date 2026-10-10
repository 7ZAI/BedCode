//! 插件实例状态（机制面）：`Store<WasmPluginState>` 的数据体
//!
//! ## 归属说明
//!
//! 本类型住在 `bedcode-host-kit` 而非宿主 bin crate，**不是风格选择而是结构必需**：
//! `bedcode::plugin::host_*::add_to_linker::<S, D>` 的 `S` 是单态类型参数，能力 crate
//! 必须能命名它（否则要么退回宿主侧逐接口硬编码，要么与宿主构成 Cargo 环）。
//!
//! 字段只有**机制属性**：实例身份、WASI 上下文、资源上限快照、指标句柄、可选导出
//! 探测句柄。宿主子系统引用一律经 [`HostPorts`]（[`crate::ports`]）——**本结构不
//! 持有任何产品语义**（AGENTS §5.1 B1/B3）。

use std::sync::Arc;

use wasmtime::component::TypedFunc;
#[cfg(feature = "wasi-store")]
use wasmtime::component::ResourceTable;

use crate::limits::StoreLimits;
use crate::metrics::PluginMetrics;
use crate::ports::HostPorts;

/// 可选导出 `events-binary#on-message-binary` 的探测句柄类型
///
/// （插件 ID, topic, 二进制载荷）→ ()
pub type OnMessageBinaryExport = TypedFunc<(String, String, Vec<u8>), ()>;

/// 可选导出 `events-ws#on-message`（客户端域帧回调）的探测句柄类型
pub type OnWsMessageExport = TypedFunc<(String, String, Vec<u8>), ()>;

/// 可选导出 `events-ws#on-client-message`（服务端域帧回调）的探测句柄类型
///
/// （插件 ID, 广播者 ID, 目标 topic, 载荷）→ ()
pub type OnWsClientMessageExport = TypedFunc<(String, String, String, Vec<u8>), ()>;

/// 可选导出 `events-task#on-task-event`（宿主并发任务进度/终态回调）的探测句柄类型
pub type OnTaskEventExport = TypedFunc<(String,), ()>;

/// 实例化一个插件 Store 所需的全部配置与埋点句柄
///
/// 由宿主 `core-config` × `core-monitor` 交汇产出；本 crate 只收结构不收来源。
#[derive(Clone)]
pub struct StoreSpec {
    /// Store 资源上限快照
    pub limits: StoreLimits,
    /// 燃料看门狗是否开启（Engine 级开关的投影；关闭时 set_fuel/get_fuel 不可用）
    pub fuel_enabled: bool,
    /// 插件指标句柄（埋点入口）
    pub metrics: Arc<PluginMetrics>,
}

/// 单个 WASM 插件实例的状态
///
/// 每个插件实例化时创建独立的 `Store<WasmPluginState>`。
pub struct WasmPluginState {
    /// 插件 ID（用于权限校验和数据隔离）
    pub plugin_id: String,
    /// 宿主上下文（注入宿主能力；见 [`HostPorts`]）
    pub host: Arc<dyn HostPorts>,
    /// WASI 预览 3 上下文（预打开目录由宿主装配面解析；
    /// 未开启自身文件访问的插件为空上下文，不干扰 host 文件域路径）。
    /// 仅 `wasi-store` 形态（桌面宿主；票 06 批次 02 Store 装配端口化）
    #[cfg(feature = "wasi-store")]
    pub wasi_ctx: wasmtime_wasi::WasiCtx,
    /// WASI 资源表（文件句柄 / 流等，随每个插件实例独立生命周期）。仅 `wasi-store`
    #[cfg(feature = "wasi-store")]
    pub wasi_table: ResourceTable,
    /// Store 资源上限快照（实例化时自内核配置读取）
    pub limits: StoreLimits,
    /// 燃料看门狗是否开启（见 [`StoreSpec::fuel_enabled`]）
    pub fuel_enabled: bool,
    /// 插件指标句柄（埋点入口）
    pub metrics: Arc<PluginMetrics>,
    /// 可选导出 `events-binary#on-message-binary` 的动态探测句柄。
    /// 旧产物不导出该函数 → None，二进制消息对其按「格式不匹配」
    /// 拒绝（总线侧过滤，不会到达本字段为 None 的实例）
    pub on_message_binary: Option<OnMessageBinaryExport>,
    /// 可选导出 `events-ws#on-message`（客户端域帧回调）的探测句柄。
    /// 未导出 → None：宿主按降级语义处理（消息帧丢弃 + 首次 warn + 计数，
    /// 不缓存），状态事件仍经消息总线照常投递
    pub on_ws_message: Option<OnWsMessageExport>,
    /// 可选导出 `events-ws#on-client-message`（服务端域帧回调）的探测句柄
    pub on_ws_client_message: Option<OnWsClientMessageExport>,
    /// 可选导出 `events-task#on-task-event`（宿主并发任务进度/终态回调）的
    /// 探测句柄。未导出 → None：宿主按降级语义处理（事件丢弃 + 首次 warn +
    /// 计数，不缓存），离线查询原语自愈
    pub on_task_event: Option<OnTaskEventExport>,
}

impl WasmPluginState {
    /// 构建插件状态（`wasi_ctx` 由调用方按插件配置构建；`wasi-store` 形态）
    #[cfg(feature = "wasi-store")]
    pub fn new(
        plugin_id: String,
        host: Arc<dyn HostPorts>,
        wasi_ctx: wasmtime_wasi::WasiCtx,
        spec: StoreSpec,
    ) -> Self {
        Self {
            plugin_id,
            host,
            wasi_ctx,
            wasi_table: ResourceTable::new(),
            limits: spec.limits,
            fuel_enabled: spec.fuel_enabled,
            metrics: spec.metrics,
            // 可选导出在实例化后动态探测（verify_abi 内写入）
            on_message_binary: None,
            on_ws_message: None,
            on_ws_client_message: None,
            on_task_event: None,
        }
    }

    /// 构建插件状态（无 WASI 形态：移动宿主 / 无头第三方宿主，票 06 批次 02）
    #[cfg(not(feature = "wasi-store"))]
    pub fn new(plugin_id: String, host: Arc<dyn HostPorts>, spec: StoreSpec) -> Self {
        Self {
            plugin_id,
            host,
            limits: spec.limits,
            fuel_enabled: spec.fuel_enabled,
            metrics: spec.metrics,
            // 可选导出在实例化后动态探测（verify_abi 内写入）
            on_message_binary: None,
            on_ws_message: None,
            on_ws_client_message: None,
            on_task_event: None,
        }
    }
}

/// WASI 预览 3 视图：`add_to_linker_sync` / `add_to_linker` 通过此 trait 访问每个
/// 插件实例的 `WasiCtx` + `ResourceTable`（linker 共享、ctx 每实例）。
/// 仅 `wasi-store` 形态（桌面装配的 p2/p3 linker 需要；移动形态无 WASI linker）
#[cfg(feature = "wasi-store")]
impl wasmtime_wasi::WasiView for WasmPluginState {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.wasi_table,
        }
    }
}

/// 插件实例资源限制器
///
/// 直接借用 Store 状态（`Store::limiter` 的闭包返回本状态的可变引用），
/// 限制单插件线性内存与表大小，防止失控/恶意插件耗尽宿主内存。
impl wasmtime::ResourceLimiter for WasmPluginState {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.limits.max_memory_bytes {
            tracing::warn!(
                plugin_id = %self.plugin_id,
                desired_bytes = desired,
                max_bytes = self.limits.max_memory_bytes,
                "WASM memory growth denied by resource limiter"
            );
            Ok(false)
        } else {
            // 指标记账：当前值/峰值（纯原子操作，不进日志）
            self.metrics.record_memory_growth(desired);
            Ok(true)
        }
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.limits.max_table_entries {
            tracing::warn!(
                plugin_id = %self.plugin_id,
                desired_entries = desired,
                max_entries = self.limits.max_table_entries,
                "WASM table growth denied by resource limiter"
            );
            Ok(false)
        } else {
            Ok(true)
        }
    }
}
