//! 配置模块（core-config）
//!
//! wasmtime Engine 构建参数与 Store 资源上限的单一配置面。
//!
//! 分层加载：编译期默认（[`defaults`]，与历史生产常量一致）< 配置文件
//! （应用配置目录 `wasm-core.json`，未知字段容忍）< 运行时覆盖
//! （[`crate::manager::runtime::WasmRuntime::set_config`]，
//! 只影响覆盖后新建立的 Store——Engine 参数在构建期固化）。
//!
//! 单插件资源覆盖的「请求 + 安全上限钳制」在票据 04（安全模块）落地；
//! 本模块提供钳制机制 [`StoreLimits::clamped_within`]。
//!
//! wasmtime 定制面（票 wasmtime-engine-config A 面）：写死与走默认的 knob 全部
//! 归入 [`EngineConfig::tuning`]（强类型 + serde + 校验），配置面够不着的原语
//! 经逃生舱 [`crate::manager::runtime::EngineCustomizer`]；优先级链见
//! `manager/runtime.rs` 的 `build_engine_config`。

use bedcode_plugin_api::ResourceOverrides;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ==================== 编译期默认值 ====================

/// 编译期默认值（转发）：真源在 `bedcode-host-kit`（wasm-core-lib-split 票 03 搬迁）。
///
/// 可见性由 kit 的 `pub mod` 收窄为 `pub(crate)`，使既有 `config::defaults::X`
/// 引用路径逐字不变——本仓库无根 workspace，改路径需连带 fixture 与
/// `EngineConfig` 推导处同步，收益不足以换取一次搬迁噪音。
pub(crate) use bedcode_host_kit::limits::defaults;

/// 插件调试模式是否开启（转发）：真源在 `bedcode-host-kit`
/// （wasm-core-lib-split 票 03 搬迁）。见 kit 内 `plugin_debug_mode` 的语义注释。
pub(crate) use bedcode_host_kit::limits::plugin_debug_mode;

// ==================== 配置结构 ====================

/// 内核配置（wasm-core）：Engine 构建参数 + Store 资源上限
///
/// serde 容器级 `default`：配置文件缺省字段回落到 [`defaults`]；
/// 未知字段容忍（serde 默认行为），配置面可向前演进。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CoreConfig {
    /// Engine 构建参数（Engine 构建后固化，运行时覆盖不影响已建 Engine）
    pub engine: EngineConfig,
    /// Store 资源上限（每次建立 Store 时读取快照，运行时覆盖即时生效）
    pub store: StoreLimits,
    /// 插件实例调用模型（票 06 P1 灰度开关；实例级快照——实例创建/重建时读一次，
    /// 已运行实例不随开关热切，见 [`CallModel`]）
    pub call_model: CallModel,
}

/// 插件实例调用模型（`.scratch/2026-09-26-plugin-concurrency-model/` 票 06 P1）
///
/// - [`CallModel::Mutex`]：现状——每实例一把 `Arc<Mutex<LoadedWasmPlugin>>`，
///   调用 = 抢锁 + `spawn_blocking` + `block_on_async`（锁持有到 guest 返回）
/// - [`CallModel::EventLoop`]：新——每实例一个常驻事件循环属主任务（唯一持
///   `&mut Store`），调用投递队列 + oneshot 结算
///
/// 语义：开关是**实例级快照**（建实例时读一次并存入装配条目）——Store 与实例
/// 绑定，热切会开出第二个 Store 入口（破坏 I1）。切换只影响此后重建 / 新激活的
/// 实例；`rebuild_wasm_instance` 按当前配置重建，即「reload 即切换」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CallModel {
    /// 每实例一把互斥锁（默认：P1 验收前的回退窗口）
    #[default]
    Mutex,
    /// 每实例一个事件循环属主任务
    EventLoop,
}

impl CallModel {
    /// 诊断/日志用的稳定名字（与配置文件取值一致）
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Mutex => "mutex",
            Self::EventLoop => "event-loop",
        }
    }
}

/// Engine 构建参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    /// 燃料看门狗开关：guest 指令计数耗尽即 trap（宿主调用阻塞不消耗）
    pub consume_fuel: bool,
    /// WASM backtrace 最大帧数（见 [`defaults::WASM_BACKTRACE_MAX_FRAMES`]）
    pub wasm_backtrace_max_frames: u32,
    /// 线性内存预留量（字节）——估算的最大线性内存，须 >= store.max_memory_bytes
    /// （见 [`defaults::MAX_PLUGIN_MEMORY_BYTES`] 双重身份说明）
    pub memory_reservation_bytes: u64,
    /// 编译缓存开关：跨进程复用已编译产物（初始化失败降级为不缓存，不阻断运行时）
    pub compile_cache: bool,
    /// wasmtime 定制项：此前写死或走默认的引擎参数（[`EngineTuning`]）
    pub tuning: EngineTuning,
}

/// backtrace 细节级别（wasmtime `WasmBacktraceDetails` 的配置面镜像）
///
/// 不直接用 wasmtime 的类型：它是 serde 无 derive 的普通 enum，接进配置文件
/// 就要在本面自建词汇（取值 kebab-case，与 `call_model` 同风格）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BacktraceDetails {
    /// 无条件解析调试信息（不依赖环境变量）
    Enable,
    /// 关闭细节解析：trap 错误串只剩函数名栈
    Disable,
    /// 条件解析：读 `WASMTIME_BACKTRACE_DETAILS` 环境变量（生产默认；插件调试
    /// 模式由宿主置该变量，见 [`EngineTuning::resolve`]）
    #[default]
    Environment,
}

/// Cranelift 代码生成优化等级（wasmtime `OptLevel` 的配置面镜像）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OptLevel {
    /// 不优化：编译最快、运行最慢（调试插件 / CI 冷编译场景）
    None,
    /// 优化速度（wasmtime 默认）
    Speed,
    /// 优化速度与产物大小（内存受限设备 / AOT 缓存体积敏感场景）
    SpeedAndSize,
}

/// 引擎级性能剖析器（wasmtime `ProfilingStrategy` 的配置面镜像）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profiling {
    /// 不开剖析器（默认）
    None,
    /// perf-map 文件格式（Linux `perf`）
    PerfMap,
    /// jitdump 文件格式（Linux `perf`）
    JitDump,
    /// VTune ittapi
    VTune,
}

/// wasmtime 引擎定制项（此前写死 / 从未被触碰的 knob 全部归入此节）
///
/// **两组语义，刻意不同**：
///
/// 1. **显式钉死组**（[`Self::component_model_async`] / [`Self::backtrace_details`] /
///    [`Self::memory_may_move`]）：内核原本就在每次构建 Engine 时显式设置它们，
///    默认值逐字等于历史字面量（零行为变更），且**恒调用** wasmtime API——不继承
///    上游默认，防止锁版升级时默认值漂移。
/// 2. **跟随默认组**（其余全部 `Option`）：内核从未触碰这些 knob，`None` 表示
///    **不调用该 API**，逐字继承 wasmtime 默认。这样把「可覆盖」开放出去的同时，
///    不把上游默认值变成我们的隐性依赖——升级（ADR 0019 双端锁版）时不会出现
///    「没人配过却行为变了」。
///
/// 不含 `target` / `allocation_strategy(pooling)`：两者改地址空间布局与 target
/// 假设，不属机制中立项，不进配置文件；宿主仍可经逃生舱自担。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineTuning {
    /// 组件模型异步支持（wasip3 插件基线；关掉会让随包插件全部实例化失败，
    /// 故 [`CoreConfig::validate`] 硬拒 false）
    pub component_model_async: bool,
    /// trap 错误串的调试信息细节级别
    pub backtrace_details: BacktraceDetails,
    /// 线性内存是否允许搬移（`true` = 放弃「预留即硬顶、基址恒定」的优化，
    /// 增长退化为搬移路径）
    pub memory_may_move: bool,

    /// 为 JIT 产物生成 DWARF 调试信息（`None` = 不调用，跟随 wasmtime 默认 false）
    pub debug_info: Option<bool>,
    /// 生成原生栈展开信息（宿主栈回溯；`None` = 跟随 wasmtime 默认）
    pub native_unwind_info: Option<bool>,
    /// 多线程编译（`None` = 跟随 wasmtime 默认开启）
    pub parallel_compilation: Option<bool>,
    /// 代码生成优化等级（`None` = 跟随 wasmtime 默认 `speed`）
    pub opt_level: Option<OptLevel>,
    /// 引擎级剖析器（`None` = 不开）
    pub profiling: Option<Profiling>,
    /// 引擎级 epoch 墙钟中断（`None` = 跟随 wasmtime 默认关闭；**注意内核看门狗
    /// 用 fuel**：开了也没人 `set_epoch_deadline`，不产生任何墙钟效果，构建期 warn）
    pub epoch_interruption: Option<bool>,
    /// 组件模型的 error-context 扩展（`None` = 跟随 wasmtime 默认）
    pub component_model_error_context: Option<bool>,
    /// Wasm 线程提案（`None` = 跟随 wasmtime 默认）
    pub wasm_threads: Option<bool>,
    /// Wasm SIMD 提案（`None` = 跟随 wasmtime 默认）
    pub wasm_simd: Option<bool>,
    /// Wasm bulk-memory 提案（`None` = 跟随 wasmtime 默认）
    pub wasm_bulk_memory: Option<bool>,
    /// Wasm 多线性内存提案（`None` = 跟随 wasmtime 默认）
    pub wasm_multi_memory: Option<bool>,
    /// Wasm tail-call 提案（`None` = 跟随 wasmtime 默认）
    pub wasm_tail_call: Option<bool>,
    /// Wasm 引用类型提案（`None` = 跟随 wasmtime 默认）
    pub wasm_reference_types: Option<bool>,
    /// Wasm 64 位线性内存提案（`None` = 跟随 wasmtime 默认）
    pub wasm_memory64: Option<bool>,
}

impl Default for EngineTuning {
    fn default() -> Self {
        Self {
            // 显式钉死组：逐字等于 runtime.rs 构建 Engine 时的历史字面量
            component_model_async: true,
            backtrace_details: BacktraceDetails::default(),
            memory_may_move: false,
            // 跟随默认组：一律不调用 wasmtime API
            debug_info: None,
            native_unwind_info: None,
            parallel_compilation: None,
            opt_level: None,
            profiling: None,
            epoch_interruption: None,
            component_model_error_context: None,
            wasm_threads: None,
            wasm_simd: None,
            wasm_bulk_memory: None,
            wasm_multi_memory: None,
            wasm_tail_call: None,
            wasm_reference_types: None,
            wasm_memory64: None,
        }
    }
}

impl EngineTuning {
    /// 解析出实际要施加到 `wasmtime::Config` 的定制项（纯函数，可单测）
    ///
    /// 唯一推导规则（[`Self::debug_info`]）：**未显式配置且处于插件调试模式**
    /// 时按 `Some(true)` 处理——让既有注释（`runtime.rs` WASMTIME_BACKTRACE_DETAILS
    /// 段）与既有冒烟测试 `test_debug_mode_trap_includes_line_info` 的意图真正成立：
    /// wasmtime 默认 `debug_info = false` 不产 DWARF，只置环境变量拿不到 `file:line`。
    /// 显式配置永不被推导覆盖（含显式 `false`）。
    pub fn resolve(&self, plugin_debug_mode: bool) -> Self {
        let mut resolved = self.clone();
        if resolved.debug_info.is_none() && plugin_debug_mode {
            resolved.debug_info = Some(true);
        }
        resolved
    }
}

/// Store 资源上限（转发）：真源在 `bedcode-host-kit`（wasm-core-lib-split 票 03 搬迁）。
///
/// 为什么整型搬走、覆盖合并留在本模块：上限值对象是**机制**——ResourceLimiter 与
/// 燃料注入点直接消费它，且它随 `WasmPluginState` 一起住在机制内核里；而
/// 「把插件 manifest 的资源覆盖请求合并进来」是**配置面**，输入是插件 SDK 的
/// manifest 类型，机制内核不应认识。
pub use bedcode_host_kit::limits::StoreLimits;

/// 应用插件 manifest 的资源覆盖请求：`None` 字段继承内核配置
///
/// 仅做「请求合并」+ **钳制到自身配置值**，不做跨上限仲裁——越界请求的最终
/// 钳制由安全模块负责（[`crate::security::SecurityFramework::resolve_store_limits`]），
/// 保证「谁声明谁合并、谁仲裁谁钳制」的职责边界。`max_wasm_stack_bytes`
/// 不在可请求面（Engine 构建期参数，见字段注释），恒继承配置。
///
/// **钳制到自身（R-19）**：放宽请求（大于当前配置）被钳回配置值——历史上
/// 这里只做合并、钳制全交给 framework 记得调用，任何直接调用方都会拿到
/// 超限 StoreLimits（违背 `reservation >= max_memory`）。钳制到 base 后无论
/// 谁调用本函数，结果都不会突破当前配置。
///
/// **显式 `0` 视为无效覆盖（R-20）**：请求值 0（如 `max_memory_bytes: 0`）
/// 不是合法资源限制（实例化期会得到不透明 wasmtime 错误），回落继承配置值
/// 并 warn 记录（防静默）；「收紧到 0」没有真实语义，不需要被支持。
///
/// 形态说明：本函数是原 `StoreLimits::apply_overrides` 固有方法的等价物，仅因
/// `StoreLimits` 已随机制内核搬出本模块（不能再给外来类型写 inherent impl）
/// 而降为自由函数。调用点：`security::framework::resolve_store_limits` 与本模块单测。
pub(crate) fn apply_store_overrides(base: &StoreLimits, request: &ResourceOverrides) -> StoreLimits {
    StoreLimits {
        fuel_per_call: nonzero_or_inherit("fuel_per_call", request.fuel_per_call, base.fuel_per_call),
        // debug 放大倍率不在请求面（全局调试开关参数），恒继承配置
        fuel_debug_multiplier: base.fuel_debug_multiplier,
        max_memory_bytes: nonzero_or_inherit("max_memory_bytes", request.max_memory_bytes, base.max_memory_bytes),
        max_table_entries: nonzero_or_inherit("max_table_entries", request.max_table_entries, base.max_table_entries),
        max_wasm_stack_bytes: base.max_wasm_stack_bytes,
        max_instances: nonzero_or_inherit("max_instances", request.max_instances, base.max_instances),
        max_memories: nonzero_or_inherit("max_memories", request.max_memories, base.max_memories),
        max_tables: nonzero_or_inherit("max_tables", request.max_tables, base.max_tables),
    }
}

/// 覆盖请求字段的取值助手：
/// - `Some(0)` 无效（R-20）：回落继承并 warn（防实例化期不透明错误）；
/// - `Some(v)` 且 v > 当前配置：钳回当前配置（R-19，放宽请求不得突破当前配置，
///   直接调用方也拿不到超限值）；
/// - 其余取请求值；`None` 继承
fn nonzero_or_inherit<T>(field: &'static str, request: Option<T>, current: T) -> T
where
    T: Copy + PartialEq + PartialOrd + From<u8>,
{
    match request {
        Some(v) if v == T::from(0) => {
            tracing::warn!(
                field = %field,
                "Plugin resource override is 0 (not a valid limit), inheriting kernel config value"
            );
            current
        }
        Some(v) if v > current => {
            tracing::warn!(
                field = %field,
                "Plugin resource override exceeds kernel config, clamped to config value"
            );
            current
        }
        Some(v) => v,
        None => current,
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            consume_fuel: true,
            wasm_backtrace_max_frames: defaults::WASM_BACKTRACE_MAX_FRAMES,
            memory_reservation_bytes: defaults::MAX_PLUGIN_MEMORY_BYTES as u64,
            compile_cache: true,
            tuning: EngineTuning::default(),
        }
    }
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            engine: EngineConfig::default(),
            store: StoreLimits::default(),
            // 默认 mutex：P1（票 06）验收通过前保持现状行为（回退窗口），
            // 见 spec §6 灰度说明
            call_model: CallModel::default(),
        }
    }
}

impl CoreConfig {
    /// 默认配置文件名（应用配置目录下）
    pub const FILE_NAME: &'static str = "wasm-core.json";

    /// 从 JSON 文件加载配置；文件不存在返回默认配置
    ///
    /// 解析失败/校验失败返回带操作上下文的错误（调用方决定降级策略——
    /// 生产启动路径记录 warn 并回落默认，见 `WasmRuntime::new`）
    pub fn load_from(path: &Path) -> crate::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| crate::AppError::Config(format!("读取内核配置文件 '{}' 失败: {}", path.display(), e)))?;
        let cfg: Self = serde_json::from_str(&text)
            .map_err(|e| crate::AppError::Config(format!("解析内核配置文件 '{}' 失败: {}", path.display(), e)))?;
        cfg.validate()
            .map_err(|e| crate::AppError::Config(format!("内核配置文件 '{}' 非法: {}", path.display(), e)))?;
        Ok(cfg)
    }

    /// 合法性校验（跨字段约束一并在此）
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.store.fuel_per_call == 0 {
            return Err("store.fuel_per_call 必须 > 0（为 0 会让任何插件调用立即 trap）".to_string());
        }
        if self.store.fuel_debug_multiplier == 0 {
            return Err("store.fuel_debug_multiplier 必须 > 0".to_string());
        }
        // R-03：fuel × 倍率的乘积必须不溢出——debug 模式下 `fuel_budget_for`
        // 会饱和，但配置面不该把「超产品预算」放进来（加载期就拒掉）
        if self
            .store
            .fuel_per_call
            .checked_mul(self.store.fuel_debug_multiplier)
            .is_none()
        {
            return Err("store.fuel_per_call × fuel_debug_multiplier 溢出 u64（debug 模式预算无法表示）".to_string());
        }
        if self.store.max_memory_bytes == 0 {
            return Err("store.max_memory_bytes 必须 > 0".to_string());
        }
        // R-11：兄弟字段同样显式拒绝零值（坏配置不该拖到 Engine/Store 构造期
        // 才报不透明错误）
        if self.store.max_table_entries == 0 {
            return Err("store.max_table_entries 必须 > 0".to_string());
        }
        if self.store.max_wasm_stack_bytes == 0 {
            return Err("store.max_wasm_stack_bytes 必须 > 0（0 会让任意深度的 wasm 递归立即栈溢出）".to_string());
        }
        if self.store.max_instances == 0 || self.store.max_memories == 0 || self.store.max_tables == 0 {
            return Err("store.max_instances/max_memories/max_tables 必须 > 0（为 0 组件无法实例化）".to_string());
        }
        if self.engine.wasm_backtrace_max_frames == 0 {
            return Err("engine.wasm_backtrace_max_frames 必须 > 0".to_string());
        }
        // D5：插件基线是 wasip3 + 组件模型异步（wasmtime-wasi p3），关掉会让
        // 随包插件全部实例化失败——灰度开关写错不得静默生效
        if !self.engine.tuning.component_model_async {
            return Err(
                "engine.tuning.component_model_async 必须为 true（BedCode 插件基线为 wasip3 + 组件模型异步；\
                 关掉会让随包插件全部实例化失败）"
                    .to_string(),
            );
        }
        if self.engine.memory_reservation_bytes < self.store.max_memory_bytes as u64 {
            return Err(format!(
                "engine.memory_reservation_bytes（{}）必须 >= store.max_memory_bytes（{}）——\
                 预留小于上限会让合法内存增长退化为搬移/失败路径",
                self.engine.memory_reservation_bytes, self.store.max_memory_bytes
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认值与历史生产常量逐一相等（搬迁/重构的回归锚点）
    #[test]
    fn defaults_match_legacy_constants() {
        let cfg = CoreConfig::default();
        assert_eq!(cfg.store.fuel_per_call, 64_000_000_000);
        assert_eq!(cfg.store.fuel_debug_multiplier, 32);
        assert_eq!(cfg.store.max_memory_bytes, 256 * 1024 * 1024);
        assert_eq!(cfg.store.max_table_entries, 1_000_000);
        assert_eq!(cfg.store.max_wasm_stack_bytes, 512 * 1024);
        assert_eq!(cfg.store.max_instances, 8);
        assert_eq!(cfg.store.max_memories, 4);
        assert_eq!(cfg.store.max_tables, 16);
        assert!(cfg.engine.consume_fuel);
        assert_eq!(cfg.engine.wasm_backtrace_max_frames, 32);
        assert_eq!(cfg.engine.memory_reservation_bytes, 256 * 1024 * 1024);
        assert!(cfg.engine.compile_cache);
        cfg.validate().expect("默认配置必须合法");
    }

    /// A 面回归锚点：`EngineTuning` 默认值逐字等于写死前的历史行为——
    /// 显式钉死组 = 三个写死字面量，跟随默认组 = 一律 `None`（不调用 wasmtime API）。
    /// 任何一个默认值漂移都会让「开放配置」变成「静默行为变更」
    #[test]
    fn tuning_defaults_match_current_hardcoded_behavior() {
        let tuning = EngineTuning::default();
        // 显式钉死组（runtime.rs 构建 Engine 时的历史字面量）
        assert!(tuning.component_model_async);
        assert_eq!(tuning.backtrace_details, BacktraceDetails::Environment);
        assert!(!tuning.memory_may_move, "内存预留即硬顶：基址恒定、不允许搬移");
        // 跟随默认组：一律不调用 wasmtime API（否则锁版升级会带进行为变更）
        assert_eq!(tuning.debug_info, None);
        assert_eq!(tuning.native_unwind_info, None);
        assert_eq!(tuning.parallel_compilation, None);
        assert_eq!(tuning.opt_level, None);
        assert_eq!(tuning.profiling, None);
        assert_eq!(tuning.epoch_interruption, None);
        assert_eq!(tuning.component_model_error_context, None);
        assert_eq!(tuning.wasm_threads, None);
        assert_eq!(tuning.wasm_simd, None);
        assert_eq!(tuning.wasm_bulk_memory, None);
        assert_eq!(tuning.wasm_multi_memory, None);
        assert_eq!(tuning.wasm_tail_call, None);
        assert_eq!(tuning.wasm_reference_types, None);
        assert_eq!(tuning.wasm_memory64, None);
    }

    /// `debug_info` 推导规则四分支：显式值优先 / 调试模式推导 / 非调试保持 `None` /
    /// 显式 `false` 不被推导翻回（显式配置永不被覆盖）
    #[test]
    fn tuning_resolve_derives_debug_info_only_for_debug_mode() {
        let base = EngineTuning::default();
        // 非调试模式：不推导（逐字继承 wasmtime 默认 = 不产 DWARF）
        assert_eq!(base.resolve(false).debug_info, None);
        // 调试模式：推导为 true，让 WASMTIME_BACKTRACE_DETAILS 真能拿到 file:line
        assert_eq!(base.resolve(true).debug_info, Some(true));
        // 显式配置优先于推导
        let explicit_true = EngineTuning {
            debug_info: Some(true),
            ..base.clone()
        };
        assert_eq!(explicit_true.resolve(false).debug_info, Some(true));
        let explicit_false = EngineTuning {
            debug_info: Some(false),
            ..base.clone()
        };
        assert_eq!(
            explicit_false.resolve(true).debug_info,
            Some(false),
            "显式 false 不得被调试模式推导翻回（显式配置永不被覆盖）"
        );
        // resolve 不改原值（纯函数）
        assert_eq!(base.debug_info, None);
        // 其余字段逐字透传
        let tuned = EngineTuning {
            opt_level: Some(OptLevel::None),
            profiling: Some(Profiling::JitDump),
            ..base
        };
        let resolved = tuned.resolve(true);
        assert_eq!(resolved.opt_level, Some(OptLevel::None));
        assert_eq!(resolved.profiling, Some(Profiling::JitDump));
        assert!(resolved.component_model_async);
    }

    /// 配置文件接线：`wasm-core.json` 的 `engine.tuning` 节可解析（枚举按 kebab-case），
    /// 未知取值必须报错并带文件路径 + 操作上下文（不得静默回落）
    #[test]
    fn tuning_loads_from_config_file_and_rejects_invalid_enum() {
        let dir = std::env::temp_dir().join(format!("wasm-core-cfg-tuning-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wasm-core.json");

        std::fs::write(
            &path,
            r#"{"engine":{"tuning":{"component_model_async":true,"backtrace_details":"disable",
                "memory_may_move":true,"debug_info":true,"opt_level":"speed-and-size",
                "profiling":"jit-dump","parallel_compilation":false}}}"#,
        )
        .unwrap();
        let cfg = CoreConfig::load_from(&path).expect("合法 tuning 配置应加载成功");
        assert_eq!(cfg.engine.tuning.backtrace_details, BacktraceDetails::Disable);
        assert!(cfg.engine.tuning.memory_may_move);
        assert_eq!(cfg.engine.tuning.debug_info, Some(true));
        assert_eq!(cfg.engine.tuning.opt_level, Some(OptLevel::SpeedAndSize));
        assert_eq!(cfg.engine.tuning.profiling, Some(Profiling::JitDump));
        assert_eq!(cfg.engine.tuning.parallel_compilation, Some(false));

        // 缺省字段回落默认（不因新增字段破坏旧配置文件）
        std::fs::write(&path, r#"{"engine":{"tuning":{"debug_info":true}}}"#).unwrap();
        let cfg = CoreConfig::load_from(&path).expect("只写一个字段应回落其余默认");
        assert_eq!(cfg.engine.tuning.debug_info, Some(true));
        assert!(cfg.engine.tuning.component_model_async, "未写字段须回落默认");
        assert_eq!(cfg.engine.tuning.backtrace_details, BacktraceDetails::Environment);

        // 非法枚举取值：报错须带文件路径 + 操作上下文（不留「以为生效了」的静默）
        std::fs::write(&path, r#"{"engine":{"tuning":{"opt_level":"turbo"}}}"#).unwrap();
        let err = CoreConfig::load_from(&path).expect_err("未知枚举取值必须报错");
        let msg = format!("{err}");
        assert!(msg.contains("解析内核配置文件"), "错误须带操作上下文: {msg}");
        assert!(msg.contains("wasm-core.json"), "错误须带文件路径: {msg}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// D5：`component_model_async=false` 硬拒（关掉会让随包插件全部实例化失败）
    #[test]
    fn validate_rejects_component_model_async_disabled() {
        let mut cfg = CoreConfig::default();
        cfg.engine.tuning.component_model_async = false;
        let err = cfg.validate().expect_err("关掉组件异步必须被拒");
        assert!(err.contains("component_model_async"), "错误须指明字段: {err}");
        assert!(err.contains("wasip3"), "错误须说明后果: {err}");
    }

    /// 票 06 P1：调用模型开关——默认 mutex（回退窗口）；配置文件按 kebab-case
    /// 取值覆盖；非法取值必须报错（灰度开关写错不得静默回落）
    #[test]
    fn call_model_defaults_to_mutex_and_parses_from_config_file() {
        assert_eq!(CoreConfig::default().call_model, CallModel::Mutex);

        let dir = std::env::temp_dir().join(format!("wasm-core-cfg-model-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wasm-core.json");

        std::fs::write(&path, r#"{"call_model":"event-loop"}"#).unwrap();
        let cfg = CoreConfig::load_from(&path).expect("event-loop 取值应可加载");
        assert_eq!(cfg.call_model, CallModel::EventLoop);
        assert_eq!(cfg.call_model.as_str(), "event-loop");

        std::fs::write(&path, r#"{"call_model":"nope"}"#).unwrap();
        let err = CoreConfig::load_from(&path).expect_err("未知取值必须报错");
        assert!(format!("{err}").contains("wasm-core.json"), "错误须带文件路径: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_file_returns_default() {
        let cfg = CoreConfig::load_from(Path::new("/nonexistent/wasm-core.json")).expect("缺失文件应回落默认");
        assert_eq!(cfg, CoreConfig::default());
    }

    #[test]
    fn load_partial_file_fills_defaults_and_tolerates_unknown_fields() {
        let dir = std::env::temp_dir().join(format!("wasm-core-cfg-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wasm-core.json");
        // 只覆盖一个字段 + 携带未知字段
        std::fs::write(&path, r#"{"store":{"fuel_per_call":1024},"future_field":{"x":1}}"#).unwrap();
        let cfg = CoreConfig::load_from(&path).expect("部分字段 + 未知字段应加载成功");
        assert_eq!(cfg.store.fuel_per_call, 1024);
        assert_eq!(cfg.store.max_memory_bytes, defaults::MAX_PLUGIN_MEMORY_BYTES);
        assert_eq!(cfg.engine, EngineConfig::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_invalid_json_reports_context() {
        let dir = std::env::temp_dir().join(format!("wasm-core-cfg-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wasm-core.json");
        std::fs::write(&path, "{ not json").unwrap();
        let err = CoreConfig::load_from(&path).expect_err("非法 JSON 必须报错");
        let msg = format!("{err}");
        assert!(msg.contains("解析内核配置文件"), "错误须带操作上下文: {msg}");
        assert!(msg.contains("wasm-core.json"), "错误须带文件路径: {msg}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 票据 02 验收：配置文件含非法值（fuel=0）→ 加载报错且带操作上下文；
    /// 同一文件里的合法字段不掩盖错误（整体拒绝，不静默回落）
    #[test]
    fn load_invalid_value_reports_context() {
        let dir = std::env::temp_dir().join(format!("wasm-core-cfg-invalid-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wasm-core.json");
        std::fs::write(&path, r#"{"store":{"fuel_per_call":0}}"#).unwrap();
        let err = CoreConfig::load_from(&path).expect_err("非法值配置必须报错");
        let msg = format!("{err}");
        assert!(msg.contains("内核配置文件"), "错误须带操作上下文: {msg}");
        assert!(msg.contains("非法"), "错误须标明校验失败: {msg}");
        assert!(msg.contains("fuel_per_call"), "错误须指明非法字段: {msg}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 票据 02 验收：调试模式燃料放大倍率可配置——debug 模式按倍率放大、
    /// 非 debug 原样返回（纯逻辑分支，不依赖进程环境变量）
    #[test]
    fn fuel_budget_for_applies_debug_multiplier() {
        let limits = StoreLimits::default();
        assert_eq!(limits.fuel_budget_for(false), limits.fuel_per_call, "非 debug 不得放大");
        assert_eq!(
            limits.fuel_budget_for(true),
            limits.fuel_per_call * limits.fuel_debug_multiplier,
            "debug 模式须按倍率放大（防 debug 产物被燃料看门狗误杀）"
        );
        // 边界：倍率为 1（合法最小值，validate 允许）时与非 debug 等价
        let no_op = StoreLimits {
            fuel_debug_multiplier: 1,
            ..StoreLimits::default()
        };
        assert_eq!(no_op.fuel_budget_for(true), no_op.fuel_per_call);
    }

    /// 票据 07：插件覆盖请求的合并语义——`Some` 字段取请求值、`None` 字段继承配置；
    /// `max_wasm_stack_bytes` 不在请求面（Engine 构建期参数）恒继承
    #[test]
    fn apply_overrides_takes_request_and_inherits_unset_fields() {
        let cfg = StoreLimits::default();
        let request = ResourceOverrides {
            max_memory_bytes: Some(1024),
            fuel_per_call: Some(42),
            ..ResourceOverrides::default()
        };
        let merged = apply_store_overrides(&cfg, &request);
        assert_eq!(merged.max_memory_bytes, 1024, "请求字段须生效");
        assert_eq!(merged.fuel_per_call, 42);
        assert_eq!(merged.max_tables, cfg.max_tables, "未请求字段须继承配置");
        assert_eq!(merged.max_memories, cfg.max_memories);
        assert_eq!(
            merged.max_wasm_stack_bytes, cfg.max_wasm_stack_bytes,
            "栈深为 Engine 参数，恒继承配置"
        );
    }

    #[test]
    fn validate_rejects_zero_fuel_and_reservation_below_memory_cap() {
        let mut cfg = CoreConfig::default();
        cfg.store.fuel_per_call = 0;
        assert!(cfg.validate().unwrap_err().contains("fuel_per_call"));

        let mut cfg = CoreConfig::default();
        cfg.engine.memory_reservation_bytes = (cfg.store.max_memory_bytes - 1) as u64;
        assert!(cfg.validate().unwrap_err().contains("memory_reservation"));
    }

    /// R-11：与兄弟字段同口径的零值拒绝——max_table_entries / max_wasm_stack_bytes
    /// 为 0 也必须在 validate 期报错（不拖到 Engine/Store 构造期的不透明错误）
    #[test]
    fn validate_rejects_zero_table_entries_and_wasm_stack() {
        let mut cfg = CoreConfig::default();
        cfg.store.max_table_entries = 0;
        assert!(cfg.validate().unwrap_err().contains("max_table_entries"));

        let mut cfg = CoreConfig::default();
        cfg.store.max_wasm_stack_bytes = 0;
        assert!(cfg.validate().unwrap_err().contains("max_wasm_stack_bytes"));
    }

    /// R-03：燃料预算 × 放大倍率溢出时 saturating 到 u64::MAX（防 debug panic /
    /// release 回卷成伪看门狗预算）；校验面拒绝溢出的配置组合
    #[test]
    fn fuel_budget_saturates_and_validate_rejects_overflowing_config() {
        let limits = StoreLimits {
            fuel_per_call: u64::MAX / 2 + 1,
            fuel_debug_multiplier: u64::MAX,
            ..StoreLimits::default()
        };
        // 乘法溢出 → 饱和而非回卷
        assert_eq!(limits.fuel_budget_for(true), u64::MAX);
        assert_eq!(limits.fuel_budget_for(false), limits.fuel_per_call, "非 debug 不过倍率");

        // 配置面拒绝超产品预算（加载期就判非法）
        let mut cfg = CoreConfig::default();
        cfg.store.fuel_per_call = u64::MAX / 2 + 1;
        cfg.store.fuel_debug_multiplier = u64::MAX;
        assert!(
            cfg.validate().unwrap_err().contains("溢出"),
            "乘积溢出配置必须在 validate 期被拒: {:?}",
            cfg.validate()
        );
    }

    /// R-19（直接调用面就不该拿到超限值）：`apply_overrides` 自身钳制到当前配置，
    /// 不依赖 framework 记得 clamp——放宽请求直接被钳回，收紧请求保留
    #[test]
    fn apply_overrides_clamps_to_self_without_framework() {
        let cfg = StoreLimits::default();
        let relaxed = ResourceOverrides {
            max_memory_bytes: Some(cfg.max_memory_bytes * 2),
            fuel_per_call: Some(cfg.fuel_per_call * 2),
            max_tables: Some(cfg.max_tables * 2),
            ..ResourceOverrides::default()
        };
        let merged = apply_store_overrides(&cfg, &relaxed);
        assert_eq!(merged.max_memory_bytes, cfg.max_memory_bytes, "放宽请求须被钳回配置值");
        assert_eq!(merged.fuel_per_call, cfg.fuel_per_call);
        assert_eq!(merged.max_tables, cfg.max_tables);

        let tightened = ResourceOverrides {
            max_memory_bytes: Some(1024),
            ..ResourceOverrides::default()
        };
        assert_eq!(
            apply_store_overrides(&cfg, &tightened).max_memory_bytes,
            1024,
            "收紧请求须保留"
        );
    }

    /// R-20：显式 `Some(0)` 不是合法资源限制，回落继承配置值（防实例化期不透明
    /// wasmtime 错误）——覆盖请求面与 None 语义一致
    #[test]
    fn apply_overrides_treats_explicit_zero_as_inherit() {
        let cfg = StoreLimits::default();
        let zeros = ResourceOverrides {
            max_memory_bytes: Some(0),
            fuel_per_call: Some(0),
            max_tables: Some(0),
            ..ResourceOverrides::default()
        };
        let merged = apply_store_overrides(&cfg, &zeros);
        assert_eq!(merged.max_memory_bytes, cfg.max_memory_bytes, "Some(0) 必须继承配置值");
        assert_eq!(merged.fuel_per_call, cfg.fuel_per_call);
        assert_eq!(merged.max_tables, cfg.max_tables);
    }

    #[test]
    fn clamped_within_takes_field_wise_min() {
        let ceiling = StoreLimits::default();
        let request = StoreLimits {
            fuel_per_call: ceiling.fuel_per_call * 2, // 放宽请求 → 钳回
            max_memory_bytes: 1024,                   // 收紧请求 → 保留
            ..StoreLimits::default()
        };
        let clamped = request.clamped_within(&ceiling);
        assert_eq!(clamped.fuel_per_call, ceiling.fuel_per_call);
        assert_eq!(clamped.max_memory_bytes, 1024);
        assert_eq!(clamped.max_tables, ceiling.max_tables);
    }
}
