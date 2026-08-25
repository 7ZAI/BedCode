//! WASM 插件运行时（移动端）
//!
//! 基于 wasmtime 组件模型（Component Model）的 WASM 插件加载、实例化、调用：
//! - 契约单一事实来源：SDK `packages/plugin-sdk-mobile/rust/wit/bedcode.wit`
//!   （移动端独立 WIT，11 import / 8 export，wasmtime 47 自带 bindgen! 宏）
//! - 加载/实例化/调用实现在子模块 component（wasm_runtime/component.rs）：
//!   Host trait 接线（11 组）、LoadedComponentPlugin 业务方法
//! - 本文件管理 Engine/Linker/Store 生命周期、资源限制与 AOT 缓存
//!
//! 安全机制（09 清理后原样保留）：
//! - 燃料看门狗：单次导出调用预算，宿主调用阻塞不消耗燃料
//! - ResourceLimiter：单插件线性内存 256MB / 表 1M 条，
//!   实例/内存/表数量封顶（防多内存声明放大 VA 占用）
//! - 线性内存预留即硬顶：`memory_reservation` 按估算的最大线性内存
//!   （= limiter 上限）预留 VA + `memory_may_move(false)` 杜绝搬移；
//!   栈深 512KiB 上限防递归打穿真实线程栈
//! - granted_permissions 校验（manifest.permissions，host 调用前检查）
//! - `abi.version()` 协商：插件版本高于宿主支持版本拒绝加载（fail-closed）
//! - AOT `.cwasm` 缓存：宿主 cache 目录（非插件目录，防产物投毒），
//!   组件产物统一 `c` 前缀命名（与桌面端一致）
//!
//! 与桌面端差异：
//! - WasmHostContext 无 session_manager 和 permission
//! - 新增 host_notify（移动端系统通知）
//! - host_terminal_send 通过 WebSocket 转发到桌面端
//! - 无 session/plugin-database/params/api-call/timer/process 接口
//! - 新增 host_mark_plugin_error（插件生命周期失败上报，置 Error + 持久化未启用）

use crate::plugin::storage::PluginStorage;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use wasmtime::component::Linker;
use wasmtime::{Cache, CacheConfig, Config, Engine, ResourceLimiter};

// ==================== Component Model 路径 ====================
//
// 组件形态加载/校验/调用全部在子模块 component（wasm_runtime/component.rs）：
// - bindgen! 绑定（wasmtime 47 自带宏）、11 组 Host trait 接线（ticket 02/03）
// - LoadedComponentPlugin 业务方法（ticket 03）
// - 自研 ABI core 路径（`__bedcode_*` 导出、(ptr,len) 内存搬运、签名表校验）
//   已在 ticket 09 删除，本文件为组件单路径
pub(crate) mod component;

pub(crate) use component::LoadedComponentPlugin;

// ==================== Resource Limits & Interruption ====================

/// 单次 wasm 导出调用允许消耗的燃料（指令数）——防失控/恶意插件无限执行
///
/// 用燃料（fuel）而非 epoch 墙钟窗口做看门狗：
/// - 燃料只计 guest 指令数，宿主调用阻塞期间（授权弹窗、目录扫描、网络）
///   guest 零消耗——慢宿主调用无论多久都不会被误杀；epoch 按墙钟计，
///   宿主阻塞期间照走，正是历史上误杀慢调用的根因
/// - 纯 guest 死循环持续烧燃料，必然耗尽被 trap（确定性，不受宿主负载影响）
/// - 每次导出调用前重置燃料（见 exports/instantiate_component），预算只
///   约束单次调用内 guest 计算量，与宿主延迟彻底解耦
/// 64G 指令 ≈ 数十秒纯 guest 计算（wasm32 release 约 1-3G 指令/秒），
/// 覆盖大 JSON 解析等重活；死循环最迟烧完被 trap
const FUEL_PER_CALL: u64 = 64_000_000_000;
/// 单插件线性内存上限（字节）——防失控/恶意插件耗尽宿主内存
///
/// 双重身份：既是 [`ResourceLimiter`] 的增长拒绝线，也是 Engine 层
/// `memory_reservation` 预留量的估算依据（估算的最大线性内存）：实例化时按此值
/// 一次性预留虚拟地址空间，guest 内存增长全程落在预留内（零系统调用、基址不搬移），
/// 触及上限前已被 limiter 拒绝。预留只占虚拟地址空间，物理内存仍按实际触碰页提交。
/// 两处必须严格一致：预留小于上限会让合法增长退化为搬移路径，
/// 大于上限则白白放大 VA 占用。移动端构建目标均为 64 位
/// （arm64-v8a / x86_64），VA 充裕，与桌面端同值
const MAX_PLUGIN_MEMORY_BYTES: usize = 256 * 1024 * 1024;
/// 单插件表元素上限
const MAX_PLUGIN_TABLE_ENTRIES: usize = 1_000_000;
/// Wasm 执行栈深度上限（字节）——guest 深度递归超限即确定性栈溢出 trap
///
/// 防递归打穿真实线程栈导致进程 abort。宿主函数栈帧不计入此预算但计入真实
/// 线程栈，故该值必须显著小于调用方线程栈余量（Android JNI 调用线程 /
/// tokio blocking 线程 ≥ 1-2MiB）。与 wasmtime 默认一致（512KiB），
/// 显式钉死防止上游默认漂移
const MAX_WASM_STACK_BYTES: usize = 512 * 1024;
/// 单 Store 核心实例数上限
///
/// 组件实例化会为 wit-component 嵌入的 adapter module 派生额外核心实例
/// （正常插件 1-2 个），留余量的同时封顶防滥用；超限实例化直接报错
const MAX_PLUGIN_INSTANCES_PER_STORE: usize = 8;
/// 单 Store 线性内存数量上限
///
/// 每个线性内存独立预留 VA（上限 × ~288MiB 含 guard），移动端 VA 相对宝贵，
/// 多内存声明会线性放大占用；WASI preview2 插件正常仅 1 个内存
const MAX_PLUGIN_MEMORIES_PER_STORE: usize = 4;
/// 单 Store 表数量上限——adapter module 自带间接调用表，正常插件个位数
const MAX_PLUGIN_TABLES_PER_STORE: usize = 16;

/// WASM 插件运行时（全局共享）
///
/// Engine 和 Linker 是线程安全的可复用结构：
/// - Engine: WASM 编译器，全局单例
/// - Linker: 组件 import 接口注册表，所有插件实例共享（见 component 子模块）
pub struct WasmRuntime {
    engine: Engine,
    /// 组件模型 Linker（组件 import 接口注册表，见 component 子模块）
    linker: Linker<WasmPluginState>,
    /// Tokio 运行时句柄，供 Host Function 中 block_on 使用
    runtime_handle: tokio::runtime::Handle,
    /// AOT 编译产物（`.cwasm`）缓存目录（宿主 cache 目录，非插件目录）
    ///
    /// 插件目录可被安装方/插件自身写入，若把反序列化产物放回插件目录，
    /// 能写插件目录的攻击者可投放伪造产物触发宿主进程 UB
    /// （`Component::deserialize_file` 是 unsafe，假定数据可信）。
    aot_cache_dir: Option<PathBuf>,
}

/// 单个 WASM 插件实例的状态
///
/// 每个插件实例化时创建独立的 Store<WasmPluginState>，
/// state 中包含插件 ID、宿主上下文引用和 Tokio 运行时句柄
pub struct WasmPluginState {
    /// 插件 ID（用于数据隔离）
    plugin_id: String,
    /// 宿主上下文
    host_ctx: Arc<WasmHostContext>,
    /// Tokio 运行时句柄（供 Host Function block_on 使用）
    runtime_handle: tokio::runtime::Handle,
    /// 插件已授予权限（来自 manifest.permissions，host function 调用前校验）
    granted_permissions: std::collections::HashSet<String>,
}

/// 插件实例资源限制器
///
/// 直接借用 Store 状态（`Store::limiter` 的闭包返回本状态的可变引用），
/// 限制单插件线性内存与表大小，防止失控/恶意插件耗尽宿主内存。
impl ResourceLimiter for WasmPluginState {
    fn memory_growing(&mut self, _current: usize, desired: usize, _maximum: Option<usize>) -> wasmtime::Result<bool> {
        if desired > MAX_PLUGIN_MEMORY_BYTES {
            tracing::warn!(
                plugin_id = %self.plugin_id,
                desired_bytes = desired,
                max_bytes = MAX_PLUGIN_MEMORY_BYTES,
                "WASM memory growth denied by resource limiter"
            );
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn table_growing(&mut self, _current: usize, desired: usize, _maximum: Option<usize>) -> wasmtime::Result<bool> {
        if desired > MAX_PLUGIN_TABLE_ENTRIES {
            tracing::warn!(
                plugin_id = %self.plugin_id,
                desired_entries = desired,
                max_entries = MAX_PLUGIN_TABLE_ENTRIES,
                "WASM table growth denied by resource limiter"
            );
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn instances(&self) -> usize {
        MAX_PLUGIN_INSTANCES_PER_STORE
    }

    fn memories(&self) -> usize {
        MAX_PLUGIN_MEMORIES_PER_STORE
    }

    fn tables(&self) -> usize {
        MAX_PLUGIN_TABLES_PER_STORE
    }
}

/// 宿主上下文（注入到 WasmPluginState）
///
/// 移动端无 SessionManager 和 PermissionManager
pub struct WasmHostContext {
    /// 数据库（移动端直接使用 rusqlite::Connection）
    ///
    /// std Mutex：host fn 为同步上下文，SQL 执行亦为同步操作，
    /// 无需经 tokio 锁 + block_in_place/block_on 绕行
    pub db: Arc<Mutex<rusqlite::Connection>>,
    /// 插件 KV 存储
    pub storage: Arc<PluginStorage>,
    /// Tauri AppHandle（None 时无头/测试上下文，依赖前端事件的能力降级）
    pub app_handle: Option<Arc<tauri::AppHandle>>,
    /// 文件系统访问校验器
    pub fs_auth: Arc<crate::plugin::fs_auth::FsAuthChecker>,
    /// 消息总线
    pub message_bus: Arc<crate::plugin::message_bus::MessageBus>,
    /// 插件状态上报回调（`host_mark_plugin_error` 触发）
    ///
    /// 由 PluginManager 注入：置 Error 状态 + 持久化未启用 + 前端通知
    pub status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync>,
}

/// 根据 wasm 路径生成 AOT 缓存文件名（稳定 hash，避免路径字符/长度问题）
/// 产物 key：路径 + 源码大小双因子哈希
///
/// 源码大小进入 key：解压器保留旧 mtime 时，仅 mtime 比较发现不了内容
/// 变更；大小变化必然换 key → 缓存 miss → 重新编译。产物自身长度与源码
/// 长度无固定关系，不能作为新鲜度因子（比较会恒不等、永久禁用缓存）
fn aot_cache_key(path: &Path, source_len: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    source_len.hash(&mut hasher);
    hasher.finish()
}

impl WasmRuntime {
    /// 创建 WASM 运行时
    ///
    /// 初始化 Engine、组件 Linker（注册全部 Host import 接口），
    /// 必须在 Tokio 运行时上下文中调用（需要 Handle 供 Host Function 使用）
    ///
    /// `aot_cache_dir`：AOT 编译产物缓存目录（宿主 cache 目录），
    /// None 时禁用文件级 AOT 缓存（退化为纯编译）
    pub fn new(aot_cache_dir: Option<PathBuf>) -> crate::Result<Self> {
        let runtime_handle = tokio::runtime::Handle::current();

        let mut config = Config::new();
        // 燃料看门狗：guest 指令计数耗尽即 trap（宿主调用阻塞不消耗，见 FUEL_PER_CALL）
        config.consume_fuel(true);
        // 线性内存预留 = 估算的最大线性内存（与 limiter 上限严格一致，见
        // MAX_PLUGIN_MEMORY_BYTES）：实例化时一次性预留 256MiB 虚拟地址空间，
        // 增长零系统调用、基址恒定；相比 64-bit 默认（4GiB 预留 + 32MiB guard/
        // 内存）大幅降低 VA 占用。GC 堆未显式配置时沿用同值（wasmtime 语义：
        // gc_heap_* 缺省继承 memory_* 配置）
        config.memory_reservation(MAX_PLUGIN_MEMORY_BYTES as u64);
        // 预留即硬顶：初始分配与增长超出预留前均被 limiter 拒绝（memory_growing
        // 在物理分配前调用），内存永不搬移；编译器可静态假设基址不变做优化，
        // 同时杜绝任何路径触发重定位
        config.memory_may_move(false);
        // Wasm 执行栈深度上限：深度递归在 wasm 侧确定性栈溢出 trap，
        // 而非打穿真实线程栈导致进程 abort（见 MAX_WASM_STACK_BYTES）
        config.max_wasm_stack(MAX_WASM_STACK_BYTES);
        // 编译缓存：跨进程复用已编译产物（初始化失败降级为不缓存，不阻断运行时）
        match Cache::new(CacheConfig::new()) {
            Ok(cache) => {
                config.cache(Some(cache));
            }
            Err(e) => {
                tracing::warn!(error = %e, "WASM compile cache disabled");
            }
        }
        let engine = Engine::new(&config)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to initialize WASM engine: {}", e)))?;

        let linker = component::build_component_linker(&engine)?;

        Ok(Self {
            engine,
            linker,
            runtime_handle,
            aot_cache_dir,
        })
    }

    /// 测试访问器：Engine 引用（组件编译与实例化必须同一 Engine 实例，
    /// 跨 Engine 实例化 wasmtime 直接拒绝）
    #[cfg(test)]
    pub(crate) fn engine(&self) -> &Engine {
        &self.engine
    }
}

impl WasmHostContext {
    /// 创建宿主上下文
    ///
    /// `app_handle` 为 None 时（无头/测试上下文）依赖前端事件的宿主能力降级
    /// （对齐桌面端形态，见其 wasm_runtime.rs 同名字段注释）
    pub fn new(
        db: Arc<Mutex<rusqlite::Connection>>,
        storage: Arc<PluginStorage>,
        app_handle: Option<Arc<tauri::AppHandle>>,
        fs_auth: Arc<crate::plugin::fs_auth::FsAuthChecker>,
        message_bus: Arc<crate::plugin::message_bus::MessageBus>,
        status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync>,
    ) -> Self {
        Self {
            db,
            storage,
            app_handle,
            fs_auth,
            message_bus,
            status_reporter,
        }
    }
}

// ==================== Host 能力实现层 ====================
//
// 业务逻辑（值传递、权限校验、block_on 执行）在 host_impl/，
// 组件路径的 Host trait impl（wasm_runtime/component.rs）直接调用。
// core 形态的 func_wrap 胶水（Caller + (ptr,len) 内存搬运）已在 09 清理。
mod host_impl;
