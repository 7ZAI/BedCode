//! 配置模块（core-config）
//!
//! wasmtime Engine 构建参数与 Store 资源上限的单一配置面。
//!
//! 分层加载：编译期默认（[`defaults`]，与历史生产常量一致）< 配置文件
//! （应用配置目录 `wasm-core.json`，未知字段容忍）< 运行时覆盖
//! （[`crate::wasm_core::manager::runtime::WasmRuntime::set_config`]，
//! 只影响覆盖后新建立的 Store——Engine 参数在构建期固化）。
//!
//! 单插件资源覆盖的「请求 + 安全上限钳制」在票据 04（安全模块）落地；
//! 本模块提供钳制机制 [`StoreLimits::clamped_within`]。

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
/// 钳制由安全模块负责（[`crate::wasm_core::security::SecurityFramework::resolve_store_limits`]），
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
