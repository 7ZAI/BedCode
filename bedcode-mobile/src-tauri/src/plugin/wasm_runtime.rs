//! WASM 插件运行时（移动端）
//!
//! 基于 wasmtime 的 WASM 模块加载、实例化、调用
//! 管理 Engine/Linker/Store/Instance 生命周期
//! 注册宿主 Host Functions 供 WASM 插件调用
//!
//! ABI v3（与 SDK `abi.rs` 对齐）：结果传递走 out_ptr（8 字节: ptr + len），
//! 参数/结果内存经 `__bedcode_allocate` / `__bedcode_deallocate` 配对回收；
//! 实例化时协商 `__bedcode_abi_version`，高于宿主支持版本拒绝加载；
//! 启动时 `verify_abi` 校验 Linker 注册与 `HOST_FN_SIGNATURES` 一致。
//!
//! 与桌面端差异：
//! - WasmHostContext 无 session_manager 和 permission
//! - 新增 host_notify（移动端系统通知）
//! - host_terminal_send 通过 WebSocket 转发到桌面端
//! - host_session_list/host_session_get 为空操作（保持 ABI 兼容）
//! - 新增 host_mark_plugin_error（插件生命周期失败上报，置 Error + 持久化未启用）

use crate::plugin::storage::PluginStorage;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use wasmtime::{
    Cache, CacheConfig, Config, Engine, Instance, Linker, Memory, Module, ResourceLimiter, Store,
};

// ==================== Resource Limits & Interruption ====================

/// 单次 wasm 导出调用允许消耗的燃料（指令数）——防失控/恶意插件无限执行
///
/// 用燃料（fuel）而非 epoch 墙钟窗口做看门狗：
/// - 燃料只计 guest 指令数，宿主调用阻塞期间（授权弹窗、目录扫描、网络）
///   guest 零消耗——慢宿主调用无论多久都不会被误杀；epoch 按墙钟计，
///   宿主阻塞期间照走，正是历史上误杀慢调用的根因
/// - 纯 guest 死循环持续烧燃料，必然耗尽被 trap（确定性，不受宿主负载影响）
/// - 每次导出调用前重置燃料（见 get_export_func/allocate_memory），预算只
///   约束单次调用内 guest 计算量，与宿主延迟彻底解耦
/// 64G 指令 ≈ 数十秒纯 guest 计算（wasm32 release 约 1-3G 指令/秒），
/// 覆盖大 JSON 解析等重活；死循环最迟烧完被 trap
const FUEL_PER_CALL: u64 = 64_000_000_000;
/// 单插件线性内存上限（字节）——防失控/恶意插件耗尽宿主内存
const MAX_PLUGIN_MEMORY_BYTES: usize = 256 * 1024 * 1024;
/// 单插件表元素上限
const MAX_PLUGIN_TABLE_ENTRIES: usize = 1_000_000;

/// WASM 插件运行时（全局共享）
///
/// Engine 和 Linker 是线程安全的可复用结构：
/// - Engine: WASM 编译器，全局单例
/// - Linker: Host function 注册表，所有插件实例共享
pub struct WasmRuntime {
    engine: Engine,
    linker: Linker<WasmPluginState>,
    /// Tokio 运行时句柄，供 Host Function 中 block_on 使用
    runtime_handle: tokio::runtime::Handle,
    /// AOT 编译产物（`.cwasm`）缓存目录（宿主 cache 目录，非插件目录）
    ///
    /// 插件目录可被安装方/插件自身写入，若把反序列化产物放回插件目录，
    /// 能写插件目录的攻击者可投放伪造产物触发宿主进程 UB
    /// （`Module::deserialize_file` 是 unsafe，假定数据可信）。
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
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
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

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
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
    /// Tauri AppHandle
    pub app_handle: Arc<tauri::AppHandle>,
    /// 文件系统访问校验器
    pub fs_auth: Arc<crate::plugin::fs_auth::FsAuthChecker>,
    /// 消息总线
    pub message_bus: Arc<crate::plugin::message_bus::MessageBus>,
    /// 插件状态上报回调（`host_mark_plugin_error` 触发）
    ///
    /// 由 PluginManager 注入：置 Error 状态 + 持久化未启用 + 前端通知
    pub status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync>,
}

/// 已加载的 WASM 插件
///
/// 持有 Instance、Store 和 Memory 引用
/// Store 必须与 Instance 一起持有，否则 Instance 的导出函数无法调用
pub struct LoadedWasmPlugin {
    instance: Instance,
    store: Store<WasmPluginState>,
    memory: Memory,
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
    /// 初始化 Engine、Linker，注册所有 Host Functions
    /// 必须在 Tokio 运行时上下文中调用（需要 Handle 供 Host Function 使用）
    ///
    /// `aot_cache_dir`：AOT 编译产物缓存目录（宿主 cache 目录），
    /// None 时禁用文件级 AOT 缓存（退化为纯编译）
    pub fn new(aot_cache_dir: Option<PathBuf>) -> crate::Result<Self> {
        let runtime_handle = tokio::runtime::Handle::current();

        let mut config = Config::new();
        // 燃料看门狗：guest 指令计数耗尽即 trap（宿主调用阻塞不消耗，见 FUEL_PER_CALL）
        config.consume_fuel(true);
        // 编译缓存：跨进程复用已编译产物（初始化失败降级为不缓存，不阻断运行时）
        match Cache::new(CacheConfig::new()) {
            Ok(cache) => {
                config.cache(Some(cache));
            }
            Err(e) => {
                tracing::warn!(error = %e, "WASM compile cache disabled");
            }
        }
        let engine = Engine::new(&config).map_err(|e| {
            crate::AppError::Plugin(format!("Failed to initialize WASM engine: {}", e))
        })?;
        let mut linker = Linker::new(&engine);

        register_host_functions(&mut linker)?;

        Ok(Self { engine, linker, runtime_handle, aot_cache_dir })
    }

    /// 从字节流编译 WASM 模块
    pub fn compile_module(&self, bytes: &[u8]) -> crate::Result<Module> {
        Module::from_binary(&self.engine, bytes).map_err(|e| {
            crate::AppError::Plugin(format!("Failed to compile WASM module: {}", e))
        })
    }

    /// 从文件编译 WASM 模块（带 AOT 缓存）
    ///
    /// 优先加载宿主 cache 目录中的 `.cwasm` 编译产物（wasm 源未变时跳过编译）；
    /// 产物缺失/过期/与当前 Engine 不兼容（版本或特性变化）时重新编译并写回。
    ///
    /// 缓存文件以 wasm 路径 hash 命名，位于宿主 cache 目录而非插件目录：
    /// 插件目录对安装方/插件可写，反序列化产物放在那里可被投毒
    /// （`Module::deserialize_file` 是 unsafe，假定数据可信）。
    pub fn compile_module_from_file(&self, path: &Path) -> crate::Result<Module> {
        // 无 AOT 缓存目录时退化为纯编译
        let Some(cache_dir) = &self.aot_cache_dir else {
            return Module::from_file(&self.engine, path).map_err(|e| {
                crate::AppError::Plugin(format!(
                    "Failed to compile WASM module from '{}': {}",
                    path.display(),
                    e
                ))
            });
        };

        // 源码大小计入缓存 key（内容变化但 mtime 未更新的场景：大小变化必然换 key）；
        // 新鲜度主判据为 mtime——同大小同 mtime 的编辑无法探测（无成本方案），
        // 但旧产物是合法编译代码不会崩溃，仅行为漂移，属可接受残留
        let wasm_md = std::fs::metadata(path).ok();
        let cache_path = cache_dir.join(format!(
            "{:016x}.cwasm",
            aot_cache_key(path, wasm_md.as_ref().map(|md| md.len()).unwrap_or(0))
        ));

        let cache_fresh = wasm_md
            .and_then(|w| w.modified().ok())
            .zip(
                std::fs::metadata(&cache_path)
                    .ok()
                    .and_then(|c| c.modified().ok()),
            )
            .map(|(wm, cm)| cm >= wm)
            .unwrap_or(false);

        if cache_fresh {
            // unsafe：产物为本机自写缓存；Engine 版本/特性不匹配时 deserialize 失败，
            // 回退到完整编译路径
            if let Ok(module) = unsafe { Module::deserialize_file(&self.engine, &cache_path) } {
                tracing::debug!(
                    path = %cache_path.display(),
                    "Loaded WASM module from AOT cache"
                );
                return Ok(module);
            }
        }

        let module = Module::from_file(&self.engine, path).map_err(|e| {
            crate::AppError::Plugin(format!(
                "Failed to compile WASM module from '{}': {}",
                path.display(),
                e
            ))
        })?;

        // 写回 AOT 缓存：先写临时文件再 rename（原子替换，避免崩溃留半截产物）；
        // 失败不阻断加载（下次启动重新编译）
        match module.serialize() {
            Ok(bytes) => {
                // 目录可能尚未创建（无头/测试路径注入时），写前确保存在
                if let Err(e) = std::fs::create_dir_all(cache_dir) {
                    tracing::warn!(
                        path = %cache_dir.display(),
                        error = %e,
                        "Failed to create AOT cache dir, will recompile next time"
                    );
                    return Ok(module);
                }
                let tmp_path = cache_path.with_extension("cwasm.tmp");
                let write_result = std::fs::write(&tmp_path, &bytes)
                    .and_then(|_| std::fs::rename(&tmp_path, &cache_path));
                if let Err(e) = write_result {
                    tracing::warn!(
                        path = %cache_path.display(),
                        error = %e,
                        "Failed to write AOT cache, will recompile next time"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to serialize module for AOT cache");
            }
        }

        Ok(module)
    }

    /// 实例化 WASM 模块
    ///
    /// 创建 Store + WasmPluginState，通过 Linker 实例化模块
    pub fn instantiate(
        &self,
        module: &Module,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
        granted_permissions: std::collections::HashSet<String>,
    ) -> crate::Result<LoadedWasmPlugin> {
        let state = WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx,
            runtime_handle: self.runtime_handle.clone(),
            granted_permissions,
        };
        let mut store = Store::new(&self.engine, state);

        // 注册资源限制（内存/表超限拒绝增长）并配置燃料看门狗（wasm 死循环烧完燃料 trap）
        store.limiter(|state| state as &mut dyn ResourceLimiter);
        // 实例化可能执行 guest 代码（静态构造器等），先注入单次调用燃料
        store.set_fuel(FUEL_PER_CALL).map_err(|e| {
            crate::AppError::Plugin(format!(
                "Failed to set fuel for plugin '{}': {}",
                plugin_id, e
            ))
        })?;

        let instance = self
            .linker
            .instantiate(&mut store, module)
            .map_err(|e| {
                crate::AppError::Plugin(format!(
                    "Failed to instantiate WASM module for plugin '{}': {}",
                    plugin_id, e
                ))
            })?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| {
                crate::AppError::Plugin(format!(
                    "WASM module for plugin '{}' has no exported 'memory'",
                    plugin_id
                ))
            })?;

        // ABI 版本协商：插件要求的版本高于宿主支持时拒绝加载，避免静默契约漂移
        if let Some(func) = instance.get_func(&mut store, bedcode_plugin_api_mobile::abi::export::ABI_VERSION) {
            let mut results = [wasmtime::Val::I32(0)];
            func.call(&mut store, &[], &mut results).map_err(|e| {
                crate::AppError::Plugin(format!(
                    "Failed to read ABI version from plugin '{}': {}",
                    plugin_id, e
                ))
            })?;
            let plugin_abi = results[0].unwrap_i32() as u32;
            if plugin_abi > bedcode_plugin_api_mobile::abi::ABI_VERSION {
                return Err(crate::AppError::Plugin(format!(
                    "Plugin '{}' requires ABI v{}, but host supports v{}",
                    plugin_id,
                    plugin_abi,
                    bedcode_plugin_api_mobile::abi::ABI_VERSION
                )));
            }
        }

        Ok(LoadedWasmPlugin {
            instance,
            store,
            memory,
        })
    }

    /// 校验 Linker 实际注册的 host functions 与 SDK ABI 签名表一致
    ///
    /// 契约单一事实来源：`bedcode_plugin_api_mobile::abi::HOST_FN_SIGNATURES`。
    /// 任何名称/参数数/返回值数漂移在启动期即暴露，而非运行时静默失败。
    /// 由 PluginManager 在注入 WasmHostContext 后调用。
    pub fn verify_abi(&self, host_ctx: Arc<WasmHostContext>) -> crate::Result<()> {
        use bedcode_plugin_api_mobile::abi::{HOST_FN_SIGNATURES, NAMESPACE};

        let state = WasmPluginState {
            plugin_id: "__abi_verify__".to_string(),
            host_ctx,
            runtime_handle: self.runtime_handle.clone(),
            granted_permissions: std::collections::HashSet::new(),
        };
        let mut store = Store::new(&self.engine, state);

        for (name, arg_count, result_count) in HOST_FN_SIGNATURES {
            let ext = self.linker.get(&mut store, NAMESPACE, name).map_err(|e| {
                crate::AppError::Plugin(format!(
                    "ABI contract violation: failed to resolve host function '{}': {}",
                    name, e
                ))
            })?;
            let func = ext.into_func().ok_or_else(|| {
                crate::AppError::Plugin(format!(
                    "ABI contract violation: host function '{}' is not a function",
                    name
                ))
            })?;
            let ty = func.ty(&store);
            if ty.params().len() != *arg_count || ty.results().len() != *result_count {
                return Err(crate::AppError::Plugin(format!(
                    "ABI contract violation: host function '{}' signature mismatch (expected {}/{} args/results, got {}/{})",
                    name, arg_count, result_count, ty.params().len(), ty.results().len()
                )));
            }
        }
        Ok(())
    }
}

impl LoadedWasmPlugin {
    /// 调用插件的 activate 导出函数
    pub fn activate(&mut self) -> crate::Result<i32> {
        let func = self.get_export_func("__bedcode_activate")?;
        let mut results = [wasmtime::Val::I32(0)];
        func.call(&mut self.store, &[], &mut results).map_err(|e| {
            crate::AppError::Plugin(format!("WASM activate() call failed: {}", e))
        })?;
        Ok(results[0].unwrap_i32())
    }

    /// 调用插件的 deactivate 导出函数
    pub fn deactivate(&mut self) -> crate::Result<i32> {
        let func = self.get_export_func("__bedcode_deactivate")?;
        let mut results = [wasmtime::Val::I32(0)];
        func.call(&mut self.store, &[], &mut results).map_err(|e| {
            crate::AppError::Plugin(format!("WASM deactivate() call failed: {}", e))
        })?;
        Ok(results[0].unwrap_i32())
    }

    /// 调用插件的 invoke_command 导出函数
    pub fn invoke_command(
        &mut self,
        command_name: &str,
        args_json: &str,
    ) -> crate::Result<String> {
        let (name_ptr, name_len) = self.write_string_to_memory(command_name)?;
        let (args_ptr, args_len) = self.write_string_to_memory(args_json)?;
        let out_ptr = self.allocate_memory(bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE)?;

        let func = self.get_export_func(bedcode_plugin_api_mobile::abi::export::INVOKE_COMMAND)?;
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(name_ptr as i32),
                wasmtime::Val::I32(name_len as i32),
                wasmtime::Val::I32(args_ptr as i32),
                wasmtime::Val::I32(args_len as i32),
                wasmtime::Val::I32(out_ptr as i32),
            ],
            &mut [],
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM invoke_command() call failed: {}", e))
        })?;

        let (ptr, len) = self.read_result_from_out_ptr(out_ptr)?;
        let result = self.read_string_from_memory(ptr, len);
        // 读取完毕，回收插件分配的结果缓冲区与 out_ptr 本身，防止线性内存单调增长
        self.dealloc_plugin_memory(ptr, len);
        self.dealloc_plugin_memory(out_ptr, bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE as u32);
        result
    }

    /// 调用插件的 on_terminal_input 导出函数
    pub fn on_terminal_input(
        &mut self,
        session_id: &str,
        text: &str,
    ) -> crate::Result<Option<String>> {
        let (sid_ptr, sid_len) = self.write_string_to_memory(session_id)?;
        let (text_ptr, text_len) = self.write_string_to_memory(text)?;
        let out_ptr = self.allocate_memory(bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE)?;

        let func = self.get_export_func(bedcode_plugin_api_mobile::abi::export::ON_TERMINAL_INPUT)?;
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(sid_ptr as i32),
                wasmtime::Val::I32(sid_len as i32),
                wasmtime::Val::I32(text_ptr as i32),
                wasmtime::Val::I32(text_len as i32),
                wasmtime::Val::I32(out_ptr as i32),
            ],
            &mut [],
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_terminal_input() call failed: {}", e))
        })?;

        let (ptr, len) = self.read_result_from_out_ptr(out_ptr)?;
        let result = if ptr == 0 && len == 0 {
            Ok(None)
        } else {
            self.read_string_from_memory(ptr, len).map(Some)
        };
        self.dealloc_plugin_memory(ptr, len);
        self.dealloc_plugin_memory(out_ptr, bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE as u32);
        result
    }

    /// 调用插件的 on_terminal_output 导出函数
    pub fn on_terminal_output(
        &mut self,
        session_id: &str,
        data: &str,
    ) -> crate::Result<Option<String>> {
        let (sid_ptr, sid_len) = self.write_string_to_memory(session_id)?;
        let (data_ptr, data_len) = self.write_string_to_memory(data)?;
        let out_ptr = self.allocate_memory(bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE)?;

        let func = self.get_export_func(bedcode_plugin_api_mobile::abi::export::ON_TERMINAL_OUTPUT)?;
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(sid_ptr as i32),
                wasmtime::Val::I32(sid_len as i32),
                wasmtime::Val::I32(data_ptr as i32),
                wasmtime::Val::I32(data_len as i32),
                wasmtime::Val::I32(out_ptr as i32),
            ],
            &mut [],
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_terminal_output() call failed: {}", e))
        })?;

        let (ptr, len) = self.read_result_from_out_ptr(out_ptr)?;
        let result = if ptr == 0 && len == 0 {
            Ok(None)
        } else {
            self.read_string_from_memory(ptr, len).map(Some)
        };
        self.dealloc_plugin_memory(ptr, len);
        self.dealloc_plugin_memory(out_ptr, bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE as u32);
        result
    }

    /// 获取插件的 manifest JSON
    pub fn get_manifest(&mut self) -> crate::Result<String> {
        let out_ptr = self.allocate_memory(bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE)?;

        let func = self.get_export_func(bedcode_plugin_api_mobile::abi::export::MANIFEST)?;
        func.call(&mut self.store, &[wasmtime::Val::I32(out_ptr as i32)], &mut [])
            .map_err(|e| {
                crate::AppError::Plugin(format!("WASM manifest() call failed: {}", e))
            })?;

        let (ptr, len) = self.read_result_from_out_ptr(out_ptr)?;
        let result = self.read_string_from_memory(ptr, len);
        self.dealloc_plugin_memory(ptr, len);
        self.dealloc_plugin_memory(out_ptr, bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE as u32);
        result
    }

    /// 调用上传策略钩子导出（ABI v4，可选导出；不存在返回 Err，调用方 fail-closed）
    ///
    /// 入参 meta_json 为 UploadRequestMeta JSON，返回插件写入 out_ptr 的决定 JSON
    pub fn call_upload_hook(&mut self, meta_json: &str) -> crate::Result<String> {        let (meta_ptr, meta_len) = self.write_string_to_memory(meta_json)?;
        let out_ptr =
            self.allocate_memory(bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE)?;

        let func = self
            .get_export_func(bedcode_plugin_api_mobile::abi::export::ON_UPLOAD_REQUEST)?;
        // 导出签名 `__bedcode_on_upload_request(meta_ptr, meta_len, out_ptr) -> i32`
        // （见 SDK wasm.rs PLUGIN_EXPORT_SIGNATURES 中 (ON_UPLOAD_REQUEST, 3, 1)）。
        // 返回 i32 为状态码（SDK 当前实现恒返 0；拒绝语义由写入 out_ptr 的决定
        // JSON 表达，返回值仅供宿主侧诊断）。必须提供 1 个返回 slot 接这个 i32，
        // 否则 wasmtime 报 "expected 1 results, got 0"——该错曾被上层误提为
        // “call_upload_hook 返 None → upload hook unavailable” fail-closed 拒绝。
        // 这里只接返回值，不阻断 out_ptr 决定的读取（与 SDK 当前语义一致）。
        let mut ret = [wasmtime::Val::I32(0)];
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(meta_ptr as i32),
                wasmtime::Val::I32(meta_len as i32),
                wasmtime::Val::I32(out_ptr as i32),
            ],
            &mut ret,
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_upload_request() call failed: {}", e))
        })?;
        if ret[0].i32().unwrap_or(0) != 0 {
            tracing::warn!(
                status = ret[0].i32().unwrap_or(0),
                "WASM on_upload_request() returned non-zero status (decision JSON still read from out_ptr)"
            );
        }

        let (ptr, len) = self.read_result_from_out_ptr(out_ptr)?;
        let result = self.read_string_from_memory(ptr, len)?;
        // 回收插件分配的决定缓冲区与 out_ptr 本身，防止线性内存单调增长
        self.dealloc_plugin_memory(ptr, len);
        self.dealloc_plugin_memory(
            out_ptr,
            bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE as u32,
        );
        Ok(result)
    }

    /// 调用批量传输请求钩子导出（ABI v6，可选导出；不存在返回 Err，调用方 fail-closed）
    ///
    /// 与 [`call_upload_hook`](Self::call_upload_hook) 完全同构：入参 meta_json 为
    /// TransferRequestMeta JSON，返回插件写入 out_ptr 的决定 JSON
    pub fn call_transfer_request(&mut self, meta_json: &str) -> crate::Result<String> {
        let (meta_ptr, meta_len) = self.write_string_to_memory(meta_json)?;
        let out_ptr =
            self.allocate_memory(bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE)?;

        let func = self
            .get_export_func(bedcode_plugin_api_mobile::abi::export::ON_TRANSFER_REQUEST)?;
        // 导出签名 `__bedcode_on_transfer_request(meta_ptr, meta_len, out_ptr) -> i32`
        //（见 SDK wasm.rs PLUGIN_EXPORT_SIGNATURES 中 (ON_TRANSFER_REQUEST, 3, 1)）。
        // 返回 i32 为状态码（SDK 当前实现恒返 0；拒绝语义由写入 out_ptr 的决定
        // JSON 表达，返回值仅供宿主侧诊断）——必须提供 1 个返回 slot，
        // 否则 wasmtime 报 "expected 1 results, got 0"（同 call_upload_hook 坑）
        let mut ret = [wasmtime::Val::I32(0)];
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(meta_ptr as i32),
                wasmtime::Val::I32(meta_len as i32),
                wasmtime::Val::I32(out_ptr as i32),
            ],
            &mut ret,
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_transfer_request() call failed: {}", e))
        })?;
        if ret[0].i32().unwrap_or(0) != 0 {
            tracing::warn!(
                status = ret[0].i32().unwrap_or(0),
                "WASM on_transfer_request() returned non-zero status (decision JSON still read from out_ptr)"
            );
        }

        let (ptr, len) = self.read_result_from_out_ptr(out_ptr)?;
        let result = self.read_string_from_memory(ptr, len)?;
        // 回收插件分配的决定缓冲区与 out_ptr 本身，防止线性内存单调增长
        self.dealloc_plugin_memory(ptr, len);
        self.dealloc_plugin_memory(
            out_ptr,
            bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE as u32,
        );
        Ok(result)
    }
    pub fn call_lifecycle_event(&mut self, event: &crate::plugin::types::PluginLifecycleEvent) -> crate::Result<()> {
        use crate::plugin::types::PluginLifecycleEvent;

        let export_name = event.wasm_export_name();

        // 检查导出函数是否存在（可选导出）
        let func = match self.instance.get_func(&mut self.store, export_name) {
            Some(f) => f,
            None => return Ok(()),
        };

        match event {
            PluginLifecycleEvent::AppStartup
            | PluginLifecycleEvent::AppShutdown
            | PluginLifecycleEvent::AuthSuccess => {
                func.call(&mut self.store, &[], &mut [])
                    .map_err(|e| crate::AppError::Plugin(format!(
                        "WASM {} call failed: {}", export_name, e
                    )))?;
            }
            PluginLifecycleEvent::Disconnect { reason } => {
                let (ptr, len) = self.write_string_to_memory(reason)?;
                func.call(&mut self.store, &[
                    wasmtime::Val::I32(ptr as i32),
                    wasmtime::Val::I32(len as i32),
                ], &mut [])
                    .map_err(|e| crate::AppError::Plugin(format!(
                        "WASM {} call failed: {}", export_name, e
                    )))?;
            }
            PluginLifecycleEvent::SessionCreated { session_id }
            | PluginLifecycleEvent::SessionStopped { session_id } => {
                let (ptr, len) = self.write_string_to_memory(session_id)?;
                func.call(&mut self.store, &[
                    wasmtime::Val::I32(ptr as i32),
                    wasmtime::Val::I32(len as i32),
                ], &mut [])
                    .map_err(|e| crate::AppError::Plugin(format!(
                        "WASM {} call failed: {}", export_name, e
                    )))?;
            }
            PluginLifecycleEvent::TerminalInput { session_id, data }
            | PluginLifecycleEvent::TerminalOutput { session_id, data } => {
                let (sid_ptr, sid_len) = self.write_string_to_memory(session_id)?;
                let (data_ptr, data_len) = self.write_string_to_memory(data)?;
                let out_ptr = self.allocate_memory(bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE)?;
                func.call(&mut self.store, &[
                    wasmtime::Val::I32(sid_ptr as i32),
                    wasmtime::Val::I32(sid_len as i32),
                    wasmtime::Val::I32(data_ptr as i32),
                    wasmtime::Val::I32(data_len as i32),
                    wasmtime::Val::I32(out_ptr as i32),
                ], &mut [])
                    .map_err(|e| crate::AppError::Plugin(format!(
                        "WASM {} call failed: {}", export_name, e
                    )))?;
                let (ptr, len) = self.read_result_from_out_ptr(out_ptr)?;
                self.dealloc_plugin_memory(ptr, len);
                self.dealloc_plugin_memory(out_ptr, bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE as u32);
            }
        }
        Ok(())
    }

    /// 调用插件的 on_bus_message 回调（可选导出）
    pub fn on_bus_message(&mut self, msg: &bedcode_plugin_api_mobile::BusMessage) -> crate::Result<()> {
        let func = match self.instance.get_func(&mut self.store, "__bedcode_on_bus_message") {
            Some(f) => f,
            None => return Ok(()), // 可选导出，不存在则跳过
        };

        let (topic_ptr, topic_len) = self.write_string_to_memory(&msg.topic)?;
        let (sender_ptr, sender_len) = self.write_string_to_memory(&msg.sender)?;
        let payload_str = serde_json::to_string(&msg.payload).unwrap_or_default();
        let (payload_ptr, payload_len) = self.write_string_to_memory(&payload_str)?;

        let mut results = [wasmtime::Val::I32(0)];
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(topic_ptr as i32),
                wasmtime::Val::I32(topic_len as i32),
                wasmtime::Val::I32(sender_ptr as i32),
                wasmtime::Val::I32(sender_len as i32),
                wasmtime::Val::I32(payload_ptr as i32),
                wasmtime::Val::I32(payload_len as i32),
                wasmtime::Val::I64(msg.timestamp as i64),
            ],
            &mut results,
        )
        .map_err(|e| crate::AppError::Plugin(format!(
            "WASM __bedcode_on_bus_message call failed: {}", e
        )))?;

        Ok(())
    }

    // ==================== Memory Helpers ====================

    fn get_export_func(&mut self, name: &str) -> crate::Result<wasmtime::Func> {
        // 重置燃料预算：单次调用预算，宿主调用阻塞不消耗燃料（见 FUEL_PER_CALL）
        self.store.set_fuel(FUEL_PER_CALL).map_err(|e| {
            crate::AppError::Plugin(format!("Failed to refill fuel: {}", e))
        })?;
        self.instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| {
                crate::AppError::Plugin(format!(
                    "WASM module missing required export '{}'",
                    name
                ))
            })
    }

    /// 将字符串写入插件线性内存
    fn write_string_to_memory(&mut self, s: &str) -> crate::Result<(u32, u32)> {
        if s.is_empty() {
            return Ok((0, 0));
        }

        let bytes = s.as_bytes();
        let len = bytes.len();

        let alloc_func = self
            .instance
            .get_func(&mut self.store, "__bedcode_allocate")
            .ok_or_else(|| {
                crate::AppError::Plugin(
                    "WASM module missing required export '__bedcode_allocate'".to_string(),
                )
            })?;

        let mut alloc_results = [wasmtime::Val::I32(0)];
        alloc_func
            .call(&mut self.store, &[wasmtime::Val::I32(len as i32)], &mut alloc_results)
            .map_err(|e| {
                crate::AppError::Plugin(format!("WASM allocate() call failed: {}", e))
            })?;

        let ptr = alloc_results[0].unwrap_i32() as u32;
        if ptr == 0 {
            return Err(crate::AppError::Plugin(
                "WASM allocate() returned null pointer".to_string(),
            ));
        }

        let memory = self.memory;
        let memory_data = memory.data_mut(&mut self.store);
        let start = ptr as usize;
        let end = start + len;
        if end > memory_data.len() {
            return Err(crate::AppError::Plugin(format!(
                "WASM allocate() returned ptr {} + len {} exceeds memory size {}",
                ptr,
                len,
                memory_data.len()
            )));
        }
        memory_data[start..end].copy_from_slice(bytes);

        Ok((ptr, len as u32))
    }

    /// 从插件线性内存读取字符串
    fn read_string_from_memory(&self, ptr: u32, len: u32) -> crate::Result<String> {
        if ptr == 0 && len == 0 {
            return Ok(String::new());
        }
        if len == 0 {
            return Ok(String::new());
        }

        let memory = self.memory;
        let memory_data = memory.data(&self.store);
        let start = ptr as usize;
        let end = start + len as usize;

        if end > memory_data.len() {
            return Err(crate::AppError::Plugin(format!(
                "WASM read_string: ptr {} + len {} exceeds memory size {}",
                ptr,
                len,
                memory_data.len()
            )));
        }

        let bytes = &memory_data[start..end];
        String::from_utf8(bytes.to_vec()).map_err(|e| {
            crate::AppError::Plugin(format!("WASM read_string: invalid UTF-8: {}", e))
        })
    }

    /// 通过插件的 `__bedcode_allocate` 导出函数分配线性内存（供 out_ptr 使用）
    fn allocate_memory(&mut self, size: usize) -> crate::Result<u32> {
        let alloc_func = self
            .instance
            .get_func(&mut self.store, bedcode_plugin_api_mobile::abi::export::ALLOCATE)
            .ok_or_else(|| {
                crate::AppError::Plugin(
                    "WASM module missing required export '__bedcode_allocate'".to_string(),
                )
            })?;
        let mut alloc_results = [wasmtime::Val::I32(0)];
        // 重置燃料预算：guest 分配器也是 guest 代码，死循环同样会被 fuel trap
        self.store.set_fuel(FUEL_PER_CALL).map_err(|e| {
            crate::AppError::Plugin(format!("Failed to refill fuel for allocate: {}", e))
        })?;
        alloc_func
            .call(&mut self.store, &[wasmtime::Val::I32(size as i32)], &mut alloc_results)
            .map_err(|e| crate::AppError::Plugin(format!("WASM allocate() call failed: {}", e)))?;
        let ptr = alloc_results[0].unwrap_i32() as u32;
        if ptr == 0 {
            return Err(crate::AppError::Plugin(
                "WASM allocate() returned null pointer".to_string(),
            ));
        }
        Ok(ptr)
    }

    /// 从 out_ptr（8 字节: ptr + len）读取结果对
    fn read_result_from_out_ptr(&self, out_ptr: u32) -> crate::Result<(u32, u32)> {
        let memory_data = self.memory.data(&self.store);
        let start = out_ptr as usize;
        let end = start + bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE;
        if end > memory_data.len() {
            return Err(crate::AppError::Plugin(format!(
                "WASM read_result: out_ptr {} + {} exceeds memory size {}",
                out_ptr,
                bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE,
                memory_data.len()
            )));
        }
        let mut ptr_bytes = [0u8; 4];
        let mut len_bytes = [0u8; 4];
        ptr_bytes.copy_from_slice(&memory_data[start..start + 4]);
        len_bytes.copy_from_slice(&memory_data[start + 4..end]);
        Ok((u32::from_le_bytes(ptr_bytes), u32::from_le_bytes(len_bytes)))
    }

    /// 回收插件线性内存中由 `__bedcode_allocate` / `wasm_alloc_string` 分配的缓冲区
    ///
    /// 旧插件未导出 `__bedcode_deallocate` 时跳过回收，退化 v1 行为
    fn dealloc_plugin_memory(&mut self, ptr: u32, len: u32) {
        if ptr == 0 || len == 0 {
            return;
        }
        let Some(func) = self
            .instance
            .get_func(&mut self.store, bedcode_plugin_api_mobile::abi::export::DEALLOCATE)
        else {
            return;
        };
        // 重置燃料预算：guest 回收函数也是 guest 代码，死循环同样会被 fuel trap
        if let Err(e) = self.store.set_fuel(FUEL_PER_CALL) {
            tracing::warn!(error = %e, "Failed to refill fuel for deallocate");
        }
        let _ = func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(ptr as i32),
                wasmtime::Val::I32(len as i32),
            ],
            &mut [],
        );
    }
}

impl WasmHostContext {
    /// 创建宿主上下文
    pub fn new(
        db: Arc<Mutex<rusqlite::Connection>>,
        storage: Arc<PluginStorage>,
        app_handle: Arc<tauri::AppHandle>,
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

// ==================== Host Function Registration ====================

/// 注册所有 Host Functions 到 Linker
fn register_host_functions(linker: &mut Linker<WasmPluginState>) -> crate::Result<()> {
    // 存储
    linker
        .func_wrap("bedcode", "host_storage_get", host_storage_get)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_storage_get: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_storage_set", host_storage_set)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_storage_set: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_storage_delete", host_storage_delete)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_storage_delete: {}", e)))?;

    // 数据库
    linker
        .func_wrap("bedcode", "host_db_execute", host_db_execute)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_db_execute: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_db_query", host_db_query)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_db_query: {}", e)))?;

    // 终端
    linker
        .func_wrap("bedcode", "host_terminal_send", host_terminal_send)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_terminal_send: {}", e)))?;

    // 事件
    linker
        .func_wrap("bedcode", "host_emit_event", host_emit_event)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_emit_event: {}", e)))?;

    // HTTP 代理
    linker
        .func_wrap("bedcode", "host_http_fetch", host_http_fetch)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_http_fetch: {}", e)))?;

    // 通知（移动端特有）
    linker
        .func_wrap("bedcode", "host_notify", host_notify)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_notify: {}", e)))?;

    // 日志
    linker
        .func_wrap("bedcode", "host_log_info", host_log_info)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_log_info: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_log_debug", host_log_debug)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_log_debug: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_log_warn", host_log_warn)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_log_warn: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_log_error", host_log_error)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_log_error: {}", e)))?;

    // 会话（移动端空操作，保持 ABI 兼容）
    linker
        .func_wrap("bedcode", "host_session_list", host_session_list_noop)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_session_list: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_session_get", host_session_get_noop)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_session_get: {}", e)))?;

    // 文件系统
    linker
        .func_wrap("bedcode", "host_fs_read", host_fs_read)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_fs_read: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_fs_write", host_fs_write)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_fs_write: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_fs_copy", host_fs_copy)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_fs_copy: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_fs_exists", host_fs_exists)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_fs_exists: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_fs_request_auth", host_fs_request_auth)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_fs_request_auth: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_fs_delete", host_fs_delete)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_fs_delete: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_fs_write_media_downloads", host_fs_write_media_downloads)
        .map_err(|e| {
            crate::AppError::Plugin(format!(
                "Failed to register host_fs_write_media_downloads: {}",
                e
            ))
        })?;
    linker
        .func_wrap("bedcode", "host_fs_save_to_document", host_fs_save_to_document)
        .map_err(|e| {
            crate::AppError::Plugin(format!(
                "Failed to register host_fs_save_to_document: {}",
                e
            ))
        })?;

    // 消息总线
    linker
        .func_wrap("bedcode", "host_bus_publish", host_bus_publish)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_bus_publish: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_bus_subscribe", host_bus_subscribe)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_bus_subscribe: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_bus_unsubscribe", host_bus_unsubscribe)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_bus_unsubscribe: {}", e)))?;

    // 插件状态上报
    linker
        .func_wrap("bedcode", "host_mark_plugin_error", host_mark_plugin_error)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_mark_plugin_error: {}", e)))?;

    // 文件服务（ABI v4，内网文件传输插件规格阶段 2）
    linker
        .func_wrap("bedcode", "host_filesrv_mount", host_filesrv_mount)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_filesrv_mount: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_filesrv_unmount", host_filesrv_unmount)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_filesrv_unmount: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_filesrv_update_roots", host_filesrv_update_roots)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_filesrv_update_roots: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_filesrv_get_peer", host_filesrv_get_peer)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_filesrv_get_peer: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_filesrv_query_peer", host_filesrv_query_peer)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_filesrv_query_peer: {}", e)))?;

    // 批量传输批准（ABI v6）
    linker
        .func_wrap("bedcode", "host_filesrv_approve_transfer", host_filesrv_approve_transfer)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_filesrv_approve_transfer: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_filesrv_reject_transfer", host_filesrv_reject_transfer)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_filesrv_reject_transfer: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_filesrv_set_approval_timeout", host_filesrv_set_approval_timeout)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_filesrv_set_approval_timeout: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_filesrv_cancel_receiving", host_filesrv_cancel_receiving)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_filesrv_cancel_receiving: {}", e)))?;

    // 传输引擎（ABI v4）
    linker
        .func_wrap("bedcode", "host_transfer_start", host_transfer_start)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_transfer_start: {}", e)))?;
    linker
        .func_wrap("bedcode", "host_transfer_cancel", host_transfer_cancel)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_transfer_cancel: {}", e)))?;

    // 配置读取（ABI v5）
    linker
        .func_wrap("bedcode", "host_config_get", host_config_get)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_config_get: {}", e)))?;

    Ok(())
}

mod host_impl;
use host_impl::*;

