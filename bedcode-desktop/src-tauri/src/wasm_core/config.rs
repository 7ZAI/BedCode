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

/// 编译期默认值——与 wasm_runtime 历史生产常量逐一相等（对照测试锁定），
/// 也是单插件覆盖的硬上限（任何覆盖不得突破）
pub(crate) mod defaults {
    /// 单次 wasm 导出调用允许消耗的燃料（指令数）——防失控/恶意插件无限执行
    ///
    /// 用燃料（fuel）而非 epoch 墙钟窗口做看门狗：
    /// - 燃料只计 guest 指令数，宿主调用阻塞期间（授权弹窗、目录扫描、网络）
    ///   guest 零消耗——慢宿主调用无论多久都不会被误杀；epoch 按墙钟计，
    ///   宿主阻塞期间照走，正是历史上误杀慢调用的根因
    /// - 纯 guest 死循环持续烧燃料，必然耗尽被 trap（确定性，不受宿主负载影响）
    /// - 每次导出调用前重置燃料，预算只约束单次调用内 guest 计算量，
    ///   与宿主延迟彻底解耦
    ///   64G 指令 ≈ 数十秒纯 guest 计算（wasm32 release 约 1-3G 指令/秒），
    ///   覆盖大 JSON 解析等重活；死循环最迟烧完被 trap
    pub(crate) const FUEL_PER_CALL: u64 = 64_000_000_000;
    /// 插件调试模式（`BEDCODE_PLUGIN_DEBUG=1`）下燃料预算放大倍率
    ///
    /// debug profile 的 wasm 产物不做优化，指令数与体积相对 release 成倍膨胀
    /// （典型 10-30 倍），同一逻辑在 debug 产物下烧燃料更快；若不放大，正常
    /// 插件调用可能被燃料看门狗误判为失控 trap。取 32 倍覆盖 debug 膨胀上界
    /// 并留余量；仅 [`plugin_debug_mode`] 为真时生效
    pub(crate) const FUEL_DEBUG_MULTIPLIER: u64 = 32;
    /// 单插件线性内存上限（字节）——防失控/恶意插件耗尽宿主内存
    ///
    /// 双重身份：既是资源限制器的增长拒绝线，也是 Engine 层
    /// `memory_reservation` 预留量的估算依据（估算的最大线性内存）：实例化时按此值
    /// 一次性预留虚拟地址空间，guest 内存增长全程落在预留内（零系统调用、基址不搬移），
    /// 触及上限前已被 limiter 拒绝。预留只占虚拟地址空间，物理内存仍按实际触碰页提交。
    /// 两处必须严格一致：预留小于上限会让合法增长退化为搬移路径，
    /// 大于上限则白白放大 VA 占用
    pub(crate) const MAX_PLUGIN_MEMORY_BYTES: usize = 256 * 1024 * 1024;
    /// 单插件表元素上限
    pub(crate) const MAX_PLUGIN_TABLE_ENTRIES: usize = 1_000_000;
    /// Wasm 执行栈深度上限（字节）——guest 深度递归超限即确定性栈溢出 trap
    ///
    /// 防递归打穿真实线程栈导致进程 abort。宿主函数栈帧不计入此预算但计入真实
    /// 线程栈，故该值必须显著小于调用方线程栈余量（tokio blocking / std 线程
    /// 默认 2MiB）。与 wasmtime 默认一致（512KiB），显式钉死防止上游默认漂移
    pub(crate) const MAX_WASM_STACK_BYTES: usize = 512 * 1024;
    /// 单 Store 核心实例数上限
    ///
    /// 组件实例化会为 wit-component 嵌入的 adapter module 派生额外核心实例
    /// （正常插件 1-2 个），留余量的同时封顶防滥用；超限实例化直接报错
    pub(crate) const MAX_PLUGIN_INSTANCES_PER_STORE: usize = 8;
    /// 单 Store 线性内存数量上限
    ///
    /// 每个线性内存独立预留 VA（上限 × ~288MiB 含 guard），多内存声明会线性
    /// 放大虚拟地址空间占用，WASI preview2 插件正常仅 1 个内存
    pub(crate) const MAX_PLUGIN_MEMORIES_PER_STORE: usize = 4;
    /// 单 Store 表数量上限
    pub(crate) const MAX_PLUGIN_TABLES_PER_STORE: usize = 16;
    /// WASM 内部调用栈 backtrace 最大帧数
    ///
    /// trap（panic/栈溢出/燃料耗尽/内存越界）错误串携带插件内部函数调用链
    /// （names section 函数名，release 构建即有），随 AppError::Plugin 进
    /// error.log 与插件 Degraded 状态。wasmtime 47 的 backtrace 在 default
    /// features 内（零编译成本），显式钉死 32 帧防止上游默认（20 帧）漂移
    pub(crate) const WASM_BACKTRACE_MAX_FRAMES: u32 = 32;
}

/// 插件调试模式是否开启（dev 构建下读 `BEDCODE_PLUGIN_DEBUG`，非空即开）
///
/// 仅 `cfg!(debug_assertions)` 生效：release 构建忽略该变量（调试产物不会
/// 出现在 release 场景，见 `scripts/plugin-build.js` 与各插件 `build.js`）。
/// 调试模式是会话态开关，不新增持久化配置项。
pub(crate) fn plugin_debug_mode() -> bool {
    cfg!(debug_assertions)
        && std::env::var("BEDCODE_PLUGIN_DEBUG")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
}

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

/// Store 资源上限（单插件维度，实例化时快照注入）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoreLimits {
    /// 单次导出调用燃料预算（debug 模式按 [`Self::fuel_debug_multiplier`] 放大）
    pub fuel_per_call: u64,
    /// 插件调试模式燃料放大倍率
    pub fuel_debug_multiplier: u64,
    /// 单插件线性内存上限（字节）
    pub max_memory_bytes: usize,
    /// 单插件表元素上限
    pub max_table_entries: usize,
    /// Wasm 执行栈深度上限（字节）——Engine 构建参数，此处仅为覆盖快照同构保留
    pub max_wasm_stack_bytes: usize,
    /// 单 Store 核心实例数上限
    pub max_instances: usize,
    /// 单 Store 线性内存数量上限
    pub max_memories: usize,
    /// 单 Store 表数量上限
    pub max_tables: usize,
}

impl StoreLimits {
    /// 单次导出调用的燃料预算（debug 模式下按倍率放大）
    ///
    /// 所有燃料注入点（实例化 / ABI 协商 / 每次导出调用前）统一走此方法，
    /// 避免调试模式与非调试模式语义分叉
    pub(crate) fn fuel_budget(&self) -> u64 {
        self.fuel_budget_for(plugin_debug_mode())
    }

    /// 燃料预算的纯逻辑形态：按 `debug_mode` 决定是否应用放大倍率
    ///
    /// 独立于 [`plugin_debug_mode`]（环境变量 + 构建形态）以便单测直接覆盖
    /// 倍率分支，不依赖进程级环境变量（测试并行安全）
    pub(crate) fn fuel_budget_for(&self, debug_mode: bool) -> u64 {
        if debug_mode {
            self.fuel_per_call * self.fuel_debug_multiplier
        } else {
            self.fuel_per_call
        }
    }

    /// 应用插件 manifest 的资源覆盖请求：`None` 字段继承内核配置
    ///
    /// 仅做「请求合并」，不做仲裁——越界请求的钳制由安全模块负责
    /// （[`crate::wasm_core::security::SecurityFramework::resolve_store_limits`]），
    /// 保证「谁声明谁合并、谁仲裁谁钳制」的职责边界。`max_wasm_stack_bytes`
    /// 不在可请求面（Engine 构建期参数，见字段注释），恒继承配置。
    pub(crate) fn apply_overrides(&self, request: &ResourceOverrides) -> StoreLimits {
        StoreLimits {
            fuel_per_call: request.fuel_per_call.unwrap_or(self.fuel_per_call),
            // debug 放大倍率不在请求面（全局调试开关参数），恒继承配置
            fuel_debug_multiplier: self.fuel_debug_multiplier,
            max_memory_bytes: request.max_memory_bytes.unwrap_or(self.max_memory_bytes),
            max_table_entries: request.max_table_entries.unwrap_or(self.max_table_entries),
            max_wasm_stack_bytes: self.max_wasm_stack_bytes,
            max_instances: request.max_instances.unwrap_or(self.max_instances),
            max_memories: request.max_memories.unwrap_or(self.max_memories),
            max_tables: request.max_tables.unwrap_or(self.max_tables),
        }
    }

    /// 在硬上限内取覆盖值（逐字段 min）——单插件覆盖的安全钳制机制
    ///
    /// `ceiling` 通常为编译期默认（[`StoreLimits::default`]）：插件可请求
    /// 更紧的限制（自我约束），放宽请求被钳制回硬上限
    pub fn clamped_within(&self, ceiling: &StoreLimits) -> StoreLimits {
        StoreLimits {
            fuel_per_call: self.fuel_per_call.min(ceiling.fuel_per_call),
            fuel_debug_multiplier: self.fuel_debug_multiplier.min(ceiling.fuel_debug_multiplier),
            max_memory_bytes: self.max_memory_bytes.min(ceiling.max_memory_bytes),
            max_table_entries: self.max_table_entries.min(ceiling.max_table_entries),
            max_wasm_stack_bytes: self.max_wasm_stack_bytes.min(ceiling.max_wasm_stack_bytes),
            max_instances: self.max_instances.min(ceiling.max_instances),
            max_memories: self.max_memories.min(ceiling.max_memories),
            max_tables: self.max_tables.min(ceiling.max_tables),
        }
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

impl Default for StoreLimits {
    fn default() -> Self {
        Self {
            fuel_per_call: defaults::FUEL_PER_CALL,
            fuel_debug_multiplier: defaults::FUEL_DEBUG_MULTIPLIER,
            max_memory_bytes: defaults::MAX_PLUGIN_MEMORY_BYTES,
            max_table_entries: defaults::MAX_PLUGIN_TABLE_ENTRIES,
            max_wasm_stack_bytes: defaults::MAX_WASM_STACK_BYTES,
            max_instances: defaults::MAX_PLUGIN_INSTANCES_PER_STORE,
            max_memories: defaults::MAX_PLUGIN_MEMORIES_PER_STORE,
            max_tables: defaults::MAX_PLUGIN_TABLES_PER_STORE,
        }
    }
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            engine: EngineConfig::default(),
            store: StoreLimits::default(),
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
        if self.store.max_memory_bytes == 0 {
            return Err("store.max_memory_bytes 必须 > 0".to_string());
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
        let merged = cfg.apply_overrides(&request);
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
