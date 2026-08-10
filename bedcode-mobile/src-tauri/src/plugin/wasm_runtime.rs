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
use crate::plugin::wasm_host;
use crate::state::get_connection_manager;
use crate::connection::request::TerminalRequest;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
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
fn aot_cache_key(path: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
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

        let cache_path = cache_dir.join(format!("{:016x}.cwasm", aot_cache_key(path)));

        // 产物存在且不旧于 wasm 源时尝试直接反序列化
        let cache_fresh = std::fs::metadata(path)
            .and_then(|w| w.modified())
            .ok()
            .zip(std::fs::metadata(&cache_path).and_then(|c| c.modified()).ok())
            .map(|(wasm_mtime, cache_mtime)| cache_mtime >= wasm_mtime)
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
    pub fn call_upload_hook(&mut self, meta_json: &str) -> crate::Result<String> {
        let (meta_ptr, meta_len) = self.write_string_to_memory(meta_json)?;
        let out_ptr =
            self.allocate_memory(bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE)?;

        let func = self
            .get_export_func(bedcode_plugin_api_mobile::abi::export::ON_UPLOAD_REQUEST)?;
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(meta_ptr as i32),
                wasmtime::Val::I32(meta_len as i32),
                wasmtime::Val::I32(out_ptr as i32),
            ],
            &mut [],
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_upload_request() call failed: {}", e))
        })?;

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

    /// 调用生命周期事件回调（可选导出，不存在则跳过）
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
        .func_wrap("bedcode", "host_fs_delete", host_fs_delete)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_fs_delete: {}", e)))?;

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

// ==================== Memory Helpers ====================

/// 从 WASM 线性内存读取字符串
fn read_wasm_string(caller: &mut wasmtime::Caller<'_, WasmPluginState>, ptr: u32, len: u32) -> Option<String> {
    if len == 0 {
        return Some(String::new());
    }
    let memory = caller.get_export("memory")?.into_memory()?;
    let data = memory.data(&caller);
    let start = ptr as usize;
    let end = start + len as usize;
    if end > data.len() {
        return None;
    }
    String::from_utf8(data[start..end].to_vec()).ok()
}

/// 将字符串写入 WASM 线性内存
fn write_wasm_string(
    caller: &mut wasmtime::Caller<'_, WasmPluginState>,
    s: &str,
) -> Option<(u32, u32)> {
    if s.is_empty() {
        return Some((0, 0));
    }

    let bytes = s.as_bytes();
    let len = bytes.len();

    let alloc_func = caller.get_export("__bedcode_allocate")?.into_func()?;
    let mut results = [wasmtime::Val::I32(0)];
    alloc_func.call(&mut *caller, &[wasmtime::Val::I32(len as i32)], &mut results).ok()?;
    let ptr = results[0].unwrap_i32() as u32;
    if ptr == 0 {
        return None;
    }

    let memory = caller.get_export("memory")?.into_memory()?;
    let data = memory.data_mut(caller);
    let start = ptr as usize;
    let end = start + len;
    if end > data.len() {
        return None;
    }
    data[start..end].copy_from_slice(bytes);

    Some((ptr, len as u32))
}

/// 检查插件是否拥有指定权限（host function 调用前校验）
///
/// 权限来自 manifest.permissions（实例化时注入 WasmPluginState）。
/// 校验失败返回 false，调用方记录日志并拒绝执行。
fn has_permission(caller: &wasmtime::Caller<'_, WasmPluginState>, permission: &str) -> bool {
    caller.data().granted_permissions.contains(permission)
}

/// 将 (ptr, len) 结果写入 WASM 线性内存的 out_ptr 位置（8 字节: ptr + len，小端序）
///
/// ABI v3：返回 (ptr, len) 的结果通过 out_ptr 输出参数传递，而非元组返回值
fn write_result_to_out_ptr(
    caller: &mut wasmtime::Caller<'_, WasmPluginState>,
    out_ptr: u32,
    ptr: u32,
    len: u32,
) -> bool {
    let memory = match caller.get_export("memory") {
        Some(e) => match e.into_memory() {
            Some(m) => m,
            None => return false,
        },
        None => return false,
    };
    let data = memory.data_mut(caller);
    let start = out_ptr as usize;
    let end = start + bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE;
    if end > data.len() {
        return false;
    }
    data[start..start + 4].copy_from_slice(&ptr.to_le_bytes());
    data[start + 4..end].copy_from_slice(&len.to_le_bytes());
    true
}

// ==================== Host Function Implementations ====================
//
// 移动端 Host Function 约定：
// - 敏感 host function 调用前按 manifest.permissions 校验（has_permission），
//   通用/插件自身状态类（emit_event / notify / log_* / mark_plugin_error）不校验
// - host_terminal_send 通过 WebSocket 转发到桌面端
// - host_notify 调用 tauri-plugin-notification
// - host_session_list/get 为空操作

// ==================== Host Call Panic Guard ====================

/// 在 wasmtime host function 内执行阻塞宿主调用并捕获 panic
///
/// wasmtime host function 经 extern "C" ABI 进入，panic 越过该边界是 UB
/// （release 下 panic=unwind 时 catch_unwind 生效，但 C ABI 边界自身不展开）。
/// host fn 内的 block_in_place / Handle::current() / 锁 unwrap 等异常会 panic，
/// 统一在此截获：记录 error 日志（含插件 ID 与调用名），返回 fallback 让调用方
/// 按失败语义继续 —— WASM 插件侧已有结构化错误处理（任务置 Failed 推送到前端），
/// 插件业务 panic 不再拖垮整个应用。
fn guarded_host_call<T>(
    plugin_id: &str,
    host_fn: &'static str,
    fallback: T,
    f: impl FnOnce() -> T,
) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(panic_err) => {
            let msg = panic_err
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| panic_err.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic payload".to_string());
            tracing::error!(
                plugin_id = %plugin_id,
                host_fn = host_fn,
                error = %msg,
                "host function panicked; swallowed and returning fallback (plugin survives)"
            );
            fallback
        }
    }
}

fn host_storage_get(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE) {
        tracing::warn!(plugin_id = %plugin_id, "host_storage_get: permission denied (storage)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let key = match read_wasm_string(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_get: failed to read key");
            return -1;
        }
    };

    let storage = host_ctx.storage.clone();
    let handle = caller.data().runtime_handle.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_storage_get",
        Err(crate::AppError::Internal("host_storage_get panicked".to_string())),
        || tokio::task::block_in_place(|| handle.block_on(storage.get(&plugin_id, &key))),
    );

    match result {
        Ok(Some(value)) => {
            let json_str = match serde_json::to_string(&value) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, plugin_id = %plugin_id, "host_storage_get: JSON serialization failed");
                    return -1;
                }
            };
            match write_wasm_string(&mut caller, &json_str) {
                Some((ptr, len)) => {
                    if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                        0
                    } else {
                        -1
                    }
                }
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_storage_get: failed to write result to WASM memory");
                    -1
                }
            }
        }
        Ok(None) => {
            let _ = write_result_to_out_ptr(&mut caller, out_ptr, 0, 0);
            0
        }
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, key = %key, "host_storage_get: storage error");
            -1
        }
    }
}

fn host_storage_set(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
    val_ptr: u32,
    val_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE) {
        tracing::warn!(plugin_id = %plugin_id, "host_storage_set: permission denied (storage)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let key = match read_wasm_string(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_set: failed to read key");
            return -1;
        }
    };

    let val_str = match read_wasm_string(&mut caller, val_ptr, val_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_set: failed to read value");
            return -1;
        }
    };

    let json_value: serde_json::Value = match serde_json::from_str(&val_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_storage_set: invalid JSON value");
            return -1;
        }
    };

    let storage = host_ctx.storage.clone();
    let handle = caller.data().runtime_handle.clone();
    match guarded_host_call(
        &plugin_id,
        "host_storage_set",
        Err(crate::AppError::Internal("host_storage_set panicked".to_string())),
        || tokio::task::block_in_place(|| handle.block_on(storage.set(&plugin_id, &key, json_value))),
    ) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_storage_set: storage error");
            -1
        }
    }
}

fn host_storage_delete(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE) {
        tracing::warn!(plugin_id = %plugin_id, "host_storage_delete: permission denied (storage)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let key = match read_wasm_string(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_delete: failed to read key");
            return -1;
        }
    };

    let storage = host_ctx.storage.clone();
    let handle = caller.data().runtime_handle.clone();
    match guarded_host_call(
        &plugin_id,
        "host_storage_delete",
        Err(crate::AppError::Internal("host_storage_delete panicked".to_string())),
        || tokio::task::block_in_place(|| handle.block_on(storage.delete(&plugin_id, &key))),
    ) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_storage_delete: storage error");
            -1
        }
    }
}

fn host_db_execute(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE) {
        tracing::warn!(plugin_id = %plugin_id, "host_db_execute: permission denied (storage)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let sql = match read_wasm_string(&mut caller, sql_ptr, sql_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_db_execute: failed to read SQL");
            return -1;
        }
    };

    // 表名前缀校验
    if let Err(e) = wasm_host::validate_sql_table_prefix(&plugin_id, &sql) {
        tracing::error!(error = %e, plugin_id = %plugin_id, "host_db_execute: table name validation failed");
        return -1;
    }

    let db = host_ctx.db.clone();
    // 作用域收窄 guard 生命周期：避免尾部表达式临时值悬垂（db 先于 guard drop）
    // poison 容忍：host fn 内 panic 被 guarded_host_call 截获后锁会中毒，不能连锁 panic
    let affected = {
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());
        match conn.execute(&sql, []) {
            Ok(affected) => affected as i32,
            Err(e) => {
                tracing::error!(error = %e, plugin_id = %plugin_id, "host_db_execute: SQL execution failed");
                -1
            }
        }
    };
    affected
}

fn host_db_query(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE) {
        tracing::warn!(plugin_id = %plugin_id, "host_db_query: permission denied (storage)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let sql = match read_wasm_string(&mut caller, sql_ptr, sql_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_db_query: failed to read SQL");
            return -1;
        }
    };

    if let Err(e) = wasm_host::validate_sql_table_prefix(&plugin_id, &sql) {
        tracing::error!(error = %e, plugin_id = %plugin_id, "host_db_query: table name validation failed");
        return -1;
    }

    let db = host_ctx.db.clone();
    let query_result: Result<serde_json::Value, String> = (|| {
        // poison 容忍：host fn 内 panic 被截获后锁会中毒，不能连锁 panic
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("prepare: {}", e))?;

        let column_count = stmt.column_count();
        let column_names: Vec<String> = (0..column_count)
            .map(|i| {
                stmt.column_name(i)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| format!("col{}", i))
            })
            .collect();

        let rows: Vec<serde_json::Map<String, serde_json::Value>> = stmt
            .query_map([], |row| {
                let mut map = serde_json::Map::new();
                for (i, col_name) in column_names.iter().enumerate() {
                    let value = wasm_host::column_to_json(row, i);
                    map.insert(col_name.clone(), value);
                }
                Ok(map)
            })
            .map_err(|e| format!("query_map: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(serde_json::Value::Array(
            rows.into_iter()
                .map(serde_json::Value::Object)
                .collect(),
        ))
    })();

    match query_result {
        Ok(value) => {
            let json_str = match serde_json::to_string(&value) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, plugin_id = %plugin_id, "host_db_query: JSON serialization failed");
                    return -1;
                }
            };
            match write_wasm_string(&mut caller, &json_str) {
                Some((ptr, len)) => {
                    if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                        0
                    } else {
                        -1
                    }
                }
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_db_query: failed to write result to WASM memory");
                    -1
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_db_query: SQL query failed");
            -1
        }
    }
}

/// 终端：发送输入（通过 WebSocket 转发到桌面端）
fn host_terminal_send(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sid_ptr: u32,
    sid_len: u32,
    data_ptr: u32,
    data_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_TERMINAL_INPUT) {
        tracing::warn!(plugin_id = %plugin_id, "host_terminal_send: permission denied (terminal:input)");
        return -1;
    }

    let session_id = match read_wasm_string(&mut caller, sid_ptr, sid_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_terminal_send: failed to read session_id");
            return -1;
        }
    };

    let data = match read_wasm_string(&mut caller, data_ptr, data_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_terminal_send: failed to read data");
            return -1;
        }
    };

    // 通过 ConnectionManager WebSocket 转发到桌面端
    let conn = get_connection_manager();
    let message = TerminalRequest::input(&session_id, &data, None);
    let handle = caller.data().runtime_handle.clone();

    match guarded_host_call(
        &plugin_id,
        "host_terminal_send",
        Err(crate::AppError::Internal("host_terminal_send panicked".to_string())),
        || tokio::task::block_in_place(|| handle.block_on(conn.send(&message))),
    ) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, session_id = %session_id, "host_terminal_send: WebSocket send failed");
            -1
        }
    }
}

/// 事件：向前端发送
fn host_emit_event(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    name_ptr: u32,
    name_len: u32,
    payload_ptr: u32,
    payload_len: u32,
) {
    let event_name = match read_wasm_string(&mut caller, name_ptr, name_len) {
        Some(s) => s,
        None => {
            tracing::error!("host_emit_event: failed to read event_name");
            return;
        }
    };

    let payload_str = match read_wasm_string(&mut caller, payload_ptr, payload_len) {
        Some(s) => s,
        None => {
            tracing::error!(event = %event_name, "host_emit_event: failed to read payload");
            return;
        }
    };

    let json_payload: serde_json::Value = match serde_json::from_str(&payload_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, event = %event_name, "host_emit_event: invalid JSON payload, using raw string");
            serde_json::Value::String(payload_str)
        }
    };

    let host_ctx = caller.data().host_ctx.clone();
    if let Err(e) = host_ctx.app_handle.emit(&event_name, json_payload) {
        tracing::error!(error = %e, event = %event_name, "host_emit_event: emit failed");
    }
}

/// HTTP 代理：发起 HTTP 请求
fn host_http_fetch(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    req_ptr: u32,
    req_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_NETWORK_HTTP) {
        tracing::warn!(plugin_id = %plugin_id, "host_http_fetch: permission denied (network:http)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let request_json = match read_wasm_string(&mut caller, req_ptr, req_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_http_fetch: failed to read request JSON");
            return -1;
        }
    };

    let request: serde_json::Value = match serde_json::from_str(&request_json) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_http_fetch: invalid request JSON");
            return -1;
        }
    };

    let is_stream = request.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    if is_stream {
        let stream_id = uuid::Uuid::new_v4().to_string();
        let stream_event = request
            .get("streamEvent")
            .and_then(|v| v.as_str())
            .unwrap_or(&stream_id)
            .to_string();

        let app_handle = host_ctx.app_handle.clone();
        let plugin_id_clone = plugin_id.clone();
        let stream_event_clone = stream_event.clone();

        tokio::spawn(async move {
            if let Err(e) = wasm_host::execute_streaming_http(
                &request,
                &app_handle,
                &stream_event_clone,
                &plugin_id_clone,
            )
            .await
            {
                tracing::error!(
                    error = %e,
                    plugin_id = %plugin_id_clone,
                    "Streaming HTTP request failed"
                );
                let _ = app_handle.emit(
                    &stream_event_clone,
                    serde_json::json!({ "error": e.to_string(), "done": true }),
                );
            }
        });

        let result_json = serde_json::json!({
            "streamId": stream_id,
            "streamEvent": stream_event,
        });
        let result_str = serde_json::to_string(&result_json).unwrap_or_default();
        match write_wasm_string(&mut caller, &result_str) {
            Some((ptr, len)) => {
                if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                    0
                } else {
                    -1
                }
            }
            None => -1,
        }
    } else {
        let handle = caller.data().runtime_handle.clone();
        match guarded_host_call(
            &plugin_id,
            "host_http_fetch",
            Err(anyhow::anyhow!("host_http_fetch panicked")),
            || tokio::task::block_in_place(|| {
                handle.block_on(wasm_host::execute_http_request(&request))
            }),
        ) {
            Ok(response) => {
                let result_str = match serde_json::to_string(&response) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(error = %e, plugin_id = %plugin_id, "host_http_fetch: response serialization failed");
                        return -1;
                    }
                };
                match write_wasm_string(&mut caller, &result_str) {
                    Some((ptr, len)) => {
                        if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                            0
                        } else {
                            -1
                        }
                    }
                    None => {
                        tracing::error!(plugin_id = %plugin_id, "host_http_fetch: failed to write result to WASM memory");
                        -1
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, plugin_id = %plugin_id, "host_http_fetch: HTTP request failed");
                -1
            }
        }
    }
}

/// 通知：移动端特有，调用 tauri-plugin-notification
fn host_notify(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    title_ptr: u32,
    title_len: u32,
    body_ptr: u32,
    body_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let title = match read_wasm_string(&mut caller, title_ptr, title_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_notify: failed to read title");
            return -1;
        }
    };

    let body = match read_wasm_string(&mut caller, body_ptr, body_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_notify: failed to read body");
            return -1;
        }
    };

    use tauri_plugin_notification::NotificationExt;
    match host_ctx.app_handle.notification().builder()
        .title(&title)
        .body(&body)
        .show()
    {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_notify: notification failed");
            -1
        }
    }
}

/// 会话列表：移动端空操作，保持 ABI 兼容
fn host_session_list_noop(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    out_ptr: u32,
) -> i32 {
    match write_wasm_string(&mut caller, "[]") {
        Some((ptr, len)) => {
            if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                0
            } else {
                -1
            }
        }
        None => -1,
    }
}

/// 会话获取：移动端空操作，保持 ABI 兼容
fn host_session_get_noop(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    _sid_ptr: u32,
    _sid_len: u32,
    out_ptr: u32,
) -> i32 {
    let _ = write_result_to_out_ptr(&mut caller, out_ptr, 0, 0);
    0
}

// ==================== Logging ====================

fn host_log_info(mut caller: wasmtime::Caller<'_, WasmPluginState>, msg_ptr: u32, msg_len: u32) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::info!("[plugin:{}] {}", plugin_id, message);
}

fn host_log_debug(mut caller: wasmtime::Caller<'_, WasmPluginState>, msg_ptr: u32, msg_len: u32) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::debug!("[plugin:{}] {}", plugin_id, message);
}

fn host_log_warn(mut caller: wasmtime::Caller<'_, WasmPluginState>, msg_ptr: u32, msg_len: u32) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::warn!("[plugin:{}] {}", plugin_id, message);
}

fn host_log_error(mut caller: wasmtime::Caller<'_, WasmPluginState>, msg_ptr: u32, msg_len: u32) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::error!("[plugin:{}] {}", plugin_id, message);
}

// ==================== File System Host Functions ====================

/// 文件系统：读取文件
fn host_fs_read(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_READ) {
        tracing::warn!(plugin_id = %plugin_id, "host_fs_read: permission denied (fs:read)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let path = match read_wasm_string(&mut caller, path_ptr, path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_read: failed to read path");
            return -1;
        }
    };

    // 访问校验
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&plugin_id, "host_fs_read", false, || {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(fs_auth.check(&plugin_id, &path, crate::plugin::fs_auth::FsOp::Read))
        })
    });
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "host_fs_read: access denied by fs_auth");
        return -1;
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => match write_wasm_string(&mut caller, &content) {
            Some((ptr, len)) => {
                if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                    0
                } else {
                    -1
                }
            }
            None => {
                tracing::error!(plugin_id = %plugin_id, path = %path, "host_fs_read: failed to write result to WASM memory");
                -1
            }
        },
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_read: file read failed");
            -1
        }
    }
}

/// 文件系统：写入文件
fn host_fs_write(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
    data_ptr: u32,
    data_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE) {
        tracing::warn!(plugin_id = %plugin_id, "host_fs_write: permission denied (fs:write)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let path = match read_wasm_string(&mut caller, path_ptr, path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_write: failed to read path");
            return -1;
        }
    };

    let data = match read_wasm_string(&mut caller, data_ptr, data_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, path = %path, "host_fs_write: failed to read data");
            return -1;
        }
    };

    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&plugin_id, "host_fs_write", false, || {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(fs_auth.check(&plugin_id, &path, crate::plugin::fs_auth::FsOp::Write))
        })
    });
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "host_fs_write: access denied by fs_auth");
        return -1;
    }

    // 自动创建父目录
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_write: failed to create parent directory");
                return -1;
            }
        }
    }

    match std::fs::write(&path, &data) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_write: file write failed");
            -1
        }
    }
}

/// 文件系统：复制文件
fn host_fs_copy(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    src_ptr: u32,
    src_len: u32,
    dst_ptr: u32,
    dst_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_READ)
        || !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE)
    {
        tracing::warn!(plugin_id = %plugin_id, "host_fs_copy: permission denied (fs:read+fs:write)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let src = match read_wasm_string(&mut caller, src_ptr, src_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_copy: failed to read src path");
            return -1;
        }
    };

    let dst = match read_wasm_string(&mut caller, dst_ptr, dst_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_copy: failed to read dst path");
            return -1;
        }
    };

    // 复制需要读+写权限
    let fs_auth = host_ctx.fs_auth.clone();
    let plugin_id_clone = plugin_id.clone();
    let src_clone = src.clone();
    let dst_clone = dst.clone();
    let allowed = guarded_host_call(&plugin_id, "host_fs_copy", false, || {
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let read_ok = fs_auth.check(&plugin_id_clone, &src_clone, crate::plugin::fs_auth::FsOp::Read).await;
                if !read_ok { return false; }
                fs_auth.check(&plugin_id_clone, &dst_clone, crate::plugin::fs_auth::FsOp::Write).await
            })
        })
    });
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, src = %src, dst = %dst, "host_fs_copy: access denied by fs_auth");
        return -1;
    }

    // 自动创建目标父目录
    if let Some(parent) = std::path::Path::new(&dst).parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!(error = %e, plugin_id = %plugin_id, dst = %dst, "host_fs_copy: failed to create parent directory");
                return -1;
            }
        }
    }

    match std::fs::copy(&src, &dst) {
        Ok(_) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, src = %src, dst = %dst, "host_fs_copy: file copy failed");
            -1
        }
    }
}

/// 文件系统：检查文件是否存在
///
/// 返回：1 存在，0 不存在，-1 错误
fn host_fs_exists(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_READ) {
        tracing::warn!(plugin_id = %plugin_id, "host_fs_exists: permission denied (fs:read)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let path = match read_wasm_string(&mut caller, path_ptr, path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_exists: failed to read path");
            return -1;
        }
    };

    // 访问校验
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&plugin_id, "host_fs_exists", false, || {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                fs_auth.check(&plugin_id, &path, crate::plugin::fs_auth::FsOp::Read),
            )
        })
    });
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "host_fs_exists: access denied by fs_auth");
        return -1;
    }

    if std::path::Path::new(&path).exists() { 1 } else { 0 }
}

/// 文件系统：删除文件
///
/// 返回：0 成功（文件不存在也视为成功），-1 失败。
/// Android 平台经 Kotlin FileDeletePlugin 删除（分区存储兼容）；
/// 非 Android 平台（桌面 dev 场景）直接 std::fs。
fn host_fs_delete(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE) {
        tracing::warn!(plugin_id = %plugin_id, "host_fs_delete: permission denied (fs:write)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let path = match read_wasm_string(&mut caller, path_ptr, path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_delete: failed to read path");
            return -1;
        }
    };

    // 访问校验
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&plugin_id, "host_fs_delete", false, || {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(fs_auth.check(&plugin_id, &path, crate::plugin::fs_auth::FsOp::Write))
        })
    });
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "host_fs_delete: access denied by fs_auth");
        return -1;
    }

    // 幂等：不存在视为成功（与桌面端 host_fs_delete 语义一致）
    if !std::path::Path::new(&path).exists() {
        return 0;
    }

    #[cfg(target_os = "android")]
    {
        let path_clone = path.clone();
        let result = guarded_host_call(
            &plugin_id,
            "host_fs_delete(android)",
            Err(crate::AppError::Internal("host_fs_delete(android) panicked".to_string())),
            || {
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(crate::plugin::android_plugins::delete_file(&path_clone))
                })
            },
        );
        return match result {
            Ok(()) => 0,
            Err(e) => {
                tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_delete: android delete failed");
                -1
            }
        };
    }

    #[cfg(not(target_os = "android"))]
    {
        match std::fs::remove_file(&path) {
            Ok(()) => 0,
            Err(e) => {
                tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_delete: file delete failed");
                -1
            }
        }
    }
}

// ==================== Message Bus Host Functions ====================

/// 消息总线：发布消息
fn host_bus_publish(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    topic_ptr: u32,
    topic_len: u32,
    payload_ptr: u32,
    payload_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_BUS) {
        tracing::warn!(plugin_id = %plugin_id, "host_bus_publish: permission denied (bus)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let topic = match read_wasm_string(&mut caller, topic_ptr, topic_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_bus_publish: failed to read topic");
            return -1;
        }
    };

    let payload_str = match read_wasm_string(&mut caller, payload_ptr, payload_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, topic = %topic, "host_bus_publish: failed to read payload");
            return -1;
        }
    };

    let payload: serde_json::Value = match serde_json::from_str(&payload_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, plugin_id = %plugin_id, topic = %topic, "host_bus_publish: invalid JSON payload, using raw string");
            serde_json::Value::String(payload_str)
        }
    };

    host_ctx.message_bus.publish(&topic, &plugin_id, payload);
    0
}

/// 消息总线：订阅 topic
fn host_bus_subscribe(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    topic_ptr: u32,
    topic_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_BUS) {
        tracing::warn!(plugin_id = %plugin_id, "host_bus_subscribe: permission denied (bus)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let topic = match read_wasm_string(&mut caller, topic_ptr, topic_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_bus_subscribe: failed to read topic");
            return -1;
        }
    };

    let bus = host_ctx.message_bus.clone();
    let handle = caller.data().runtime_handle.clone();
    guarded_host_call(&plugin_id, "host_bus_subscribe", (), || {
        tokio::task::block_in_place(|| handle.block_on(bus.subscribe_wasm(&plugin_id, &topic)))
    });
    0
}

/// 消息总线：取消订阅
fn host_bus_unsubscribe(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    topic_ptr: u32,
    topic_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_BUS) {
        tracing::warn!(plugin_id = %plugin_id, "host_bus_unsubscribe: permission denied (bus)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let topic = match read_wasm_string(&mut caller, topic_ptr, topic_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_bus_unsubscribe: failed to read topic");
            return -1;
        }
    };

    let bus = host_ctx.message_bus.clone();
    let handle = caller.data().runtime_handle.clone();
    guarded_host_call(&plugin_id, "host_bus_unsubscribe", (), || {
        tokio::task::block_in_place(|| handle.block_on(bus.unsubscribe(&plugin_id, &topic)))
    });
    0
}

/// 插件状态上报：标记插件为错误状态
///
/// 插件自检失败（如 API 配置无效）时调用。宿主置 Error 状态、
/// 持久化启用状态为 false，并通知前端。
fn host_mark_plugin_error(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let msg = match read_wasm_string(&mut caller, msg_ptr, msg_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_mark_plugin_error: failed to read message");
            return;
        }
    };

    (host_ctx.status_reporter)(&plugin_id, &msg);
}

// ==================== File Service & Transfer Host Functions（ABI v4） ====================
//
// 内网文件传输插件规格阶段 2：文件服务挂载注册 + 传输引擎。
// 与桌面端 host_functions/file_service.rs + transfer.rs 同语义（移动端独立实现）。

/// 文件服务：挂载
///
/// 参数：(opts_ptr, opts_len, out_ptr) — opts 为 MountOptions JSON
/// 返回：0 成功（MountResult JSON 写入 out_ptr），-1 失败（权限/fs 授权/参数错误）
fn host_filesrv_mount(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    opts_ptr: u32,
    opts_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_mount: permission denied (fileservice)");
        return -1;
    }

    let opts_str = match read_wasm_string(&mut caller, opts_ptr, opts_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_mount: failed to read options");
            return -1;
        }
    };

    let options: bedcode_plugin_api_mobile::MountOptions = match serde_json::from_str(&opts_str) {
        Ok(o) => o,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_filesrv_mount: invalid MountOptions JSON");
            return -1;
        }
    };

    let fs = crate::state::get_file_service();
    let mount_path = options.mount_path.clone();
    let handle = caller.data().runtime_handle.clone();
    let mount_result = guarded_host_call(
        &plugin_id,
        "host_filesrv_mount",
        Err(crate::AppError::Internal("host_filesrv_mount panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                handle.block_on(fs.registry.mount(
                    &plugin_id,
                    options,
                    crate::file_service::registry::HookTarget::Wasm,
                ))
            })
        },
    );

    match mount_result {
        Ok(entry) => {
            let result = bedcode_plugin_api_mobile::MountResult {
                mount_path: entry.mount_path.clone(),
                // 移动端无 /api 前缀：/{plugin_id}/{mount}/**
                base_path: format!("/{}/{}", entry.plugin_id, entry.mount_path),
            };
            let result_json = match serde_json::to_string(&result) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, plugin_id = %plugin_id, "host_filesrv_mount: serialize MountResult failed");
                    return -1;
                }
            };

            // 首个挂载会启动 HTTP 服务；挂载变更后立即公告（异步，不阻塞 WASM 调用；
            // 错误边界包装：announce/ensure_started panic 不致 release 构建闪退）
            crate::system::error_boundary::spawn_with_error_boundary(
                "filesrv_wasm_after_mount",
                async move {
                    fs.after_mount_changed().await;
                },
            );

            match write_wasm_string(&mut caller, &result_json) {
                Some((ptr, len)) => {
                    if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                        0
                    } else {
                        -1
                    }
                }
                None => {
                    tracing::error!(plugin_id = %plugin_id, mount = %mount_path, "host_filesrv_mount: failed to write result");
                    -1
                }
            }
        }
        Err(e) => {
            tracing::warn!(plugin_id = %plugin_id, error = %e, "host_filesrv_mount: mount failed");
            -1
        }
    }
}

/// 文件服务：卸载挂载点
///
/// 参数：(mp_ptr, mp_len)
/// 返回：0 成功，-1 失败（权限/挂载不存在）
fn host_filesrv_unmount(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    mp_ptr: u32,
    mp_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_unmount: permission denied (fileservice)");
        return -1;
    }

    let mount_path = match read_wasm_string(&mut caller, mp_ptr, mp_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_unmount: failed to read mount path");
            return -1;
        }
    };

    let fs = crate::state::get_file_service();
    let handle = caller.data().runtime_handle.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_filesrv_unmount",
        Err(crate::AppError::Internal("host_filesrv_unmount panicked".to_string())),
        || tokio::task::block_in_place(|| handle.block_on(fs.registry.unmount(&plugin_id, &mount_path))),
    );

    match result {
        Ok(()) => {
            // 末个挂载摘除时停服务 + Withdraw，否则重新公告
            crate::system::error_boundary::spawn_with_error_boundary(
                "filesrv_wasm_after_unmount",
                async move {
                    fs.after_unmount().await;
                },
            );
            0
        }
        Err(e) => {
            tracing::warn!(plugin_id = %plugin_id, mount = %mount_path, error = %e, "host_filesrv_unmount: unmount failed");
            -1
        }
    }
}

/// 文件服务：更新挂载点允许目录根（roots 为 JSON 数组字符串）
///
/// 参数：(mp_ptr, mp_len, roots_ptr, roots_len)
/// 返回：0 成功，-1 失败（权限/挂载不存在/fs 授权/参数错误）
fn host_filesrv_update_roots(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    mp_ptr: u32,
    mp_len: u32,
    roots_ptr: u32,
    roots_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_update_roots: permission denied (fileservice)");
        return -1;
    }

    let mount_path = match read_wasm_string(&mut caller, mp_ptr, mp_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_update_roots: failed to read mount path");
            return -1;
        }
    };
    let roots_str = match read_wasm_string(&mut caller, roots_ptr, roots_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_update_roots: failed to read roots");
            return -1;
        }
    };
    let roots: Vec<String> = match serde_json::from_str(&roots_str) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_filesrv_update_roots: invalid roots JSON");
            return -1;
        }
    };

    let fs = crate::state::get_file_service();
    let handle = caller.data().runtime_handle.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_filesrv_update_roots",
        Err(crate::AppError::Internal("host_filesrv_update_roots panicked".to_string())),
        || tokio::task::block_in_place(|| handle.block_on(fs.registry.update_roots(&plugin_id, &mount_path, roots))),
    );

    match result {
        Ok(()) => {
            // 目录变更即时生效：重新公告（挂载集合未变，公告幂等）
            crate::system::error_boundary::spawn_with_error_boundary(
                "filesrv_wasm_after_update_roots",
                async move {
                    fs.after_mount_changed().await;
                },
            );
            0
        }
        Err(e) => {
            tracing::warn!(plugin_id = %plugin_id, mount = %mount_path, error = %e, "host_filesrv_update_roots: update failed");
            -1
        }
    }
}

/// 文件服务：获取对端文件服务信息
///
/// 参数：(peer_ptr, peer_len, out_ptr)
/// 返回：0 成功（PeerFileService JSON 写入 out_ptr；(0,0) 表示未公告），-1 失败
fn host_filesrv_get_peer(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    peer_ptr: u32,
    peer_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_get_peer: permission denied (fileservice)");
        return -1;
    }

    let peer_id = match read_wasm_string(&mut caller, peer_ptr, peer_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_get_peer: failed to read peer id");
            return -1;
        }
    };

    let fs = crate::state::get_file_service();
    let handle = caller.data().runtime_handle.clone();
    let peer = guarded_host_call(&plugin_id, "host_filesrv_get_peer", None, || {
        tokio::task::block_in_place(|| handle.block_on(fs.registry.get_peer(&peer_id)))
    });

    let Some(peer) = peer else {
        // 未公告：out_ptr 写 (0,0)，插件侧 SDK 映射为 Ok(None)
        return if write_result_to_out_ptr(&mut caller, out_ptr, 0, 0) { 0 } else { -1 };
    };

    let json = match serde_json::to_string(&peer) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, peer_id = %peer_id, "host_filesrv_get_peer: serialize failed");
            return -1;
        }
    };
    match write_wasm_string(&mut caller, &json) {
        Some((ptr, len)) => {
            if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                0
            } else {
                -1
            }
        }
        None => {
            tracing::error!(plugin_id = %plugin_id, peer_id = %peer_id, "host_filesrv_get_peer: failed to write result");
            -1
        }
    }
}

/// 文件服务：主动询问对端状态（经 WS 控制面发送 Query）
///
/// 参数：(peer_ptr, peer_len) — 单连接场景忽略 peer_id，向当前连接发送；
/// 对端回复 Announce/Withdraw 后由注册表推送 `filesrv:peer_changed`。
/// 返回：0 成功（已发送），-1 失败（权限/未连接/发送失败）
fn host_filesrv_query_peer(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    peer_ptr: u32,
    peer_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_query_peer: permission denied (fileservice)");
        return -1;
    }

    let peer_id = match read_wasm_string(&mut caller, peer_ptr, peer_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_query_peer: failed to read peer id");
            return -1;
        }
    };

    let conn = crate::state::get_connection_manager();
    let handle = caller.data().runtime_handle.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_filesrv_query_peer",
        Err(crate::AppError::WebSocket("host_filesrv_query_peer panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                handle.block_on(async {
                    if !conn.is_connected().await {
                        return Err(crate::AppError::WebSocket("not connected".to_string()));
                    }
                    conn.send(&crate::model::message::Message::file_service(
                        crate::enums::file_service::FileServicePayload::Query {},
                    ))
                    .await
                })
            })
        },
    );

    match result {
        Ok(_) => {
            tracing::debug!(plugin_id = %plugin_id, peer_id = %peer_id, "file service query sent");
            0
        }
        Err(e) => {
            tracing::warn!(plugin_id = %plugin_id, error = %e, "host_filesrv_query_peer: send failed");
            -1
        }
    }
}

/// 传输引擎：启动传输任务
///
/// 参数：(req_ptr, req_len, out_ptr) — req 为 TransferRequest JSON
/// 返回：0 成功（task_id 写入 out_ptr），-1 失败（权限/fs 授权/参数错误）
fn host_transfer_start(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    req_ptr: u32,
    req_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_TRANSFER) {
        tracing::warn!(plugin_id = %plugin_id, "host_transfer_start: permission denied (transfer)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let req_str = match read_wasm_string(&mut caller, req_ptr, req_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_transfer_start: failed to read request");
            return -1;
        }
    };

    let request: bedcode_plugin_api_mobile::TransferRequest = match serde_json::from_str(&req_str) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_transfer_start: invalid TransferRequest JSON");
            return -1;
        }
    };

    // 本地路径 fs 授权：下载 = 写授权，上传 = 读授权
    // （panic guard：授权流程异常不崩溃，fail-closed 拒绝并回报插件）
    let handle = caller.data().runtime_handle.clone();
    let authorized = guarded_host_call(&plugin_id, "host_transfer_start", false, || {
        tokio::task::block_in_place(|| {
            handle.block_on(crate::plugin::transfer::check_local_path_authorized(
                &plugin_id, &request,
            ))
        })
    });
    if !authorized {
        tracing::error!(
            plugin_id = %plugin_id,
            local_path = %request.local_path,
            "host_transfer_start: local path not authorized by user"
        );
        return -1;
    }

    let task_id = crate::plugin::transfer::spawn_transfer(
        request,
        host_ctx.app_handle.clone(),
        host_ctx.message_bus.clone(),
    );

    match write_wasm_string(&mut caller, &task_id) {
        Some((ptr, len)) => {
            if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                0
            } else {
                -1
            }
        }
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_transfer_start: failed to write task_id");
            -1
        }
    }
}

/// 传输引擎：取消传输任务
///
/// 参数：(task_ptr, task_len)
/// 返回：0 成功；任务不存在（已完成/未知）也返回 0（幂等），记录 debug 日志
fn host_transfer_cancel(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    task_ptr: u32,
    task_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_TRANSFER) {
        tracing::warn!(plugin_id = %plugin_id, "host_transfer_cancel: permission denied (transfer)");
        return -1;
    }

    let task_id = match read_wasm_string(&mut caller, task_ptr, task_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_transfer_cancel: failed to read task id");
            return -1;
        }
    };

    let handle = caller.data().runtime_handle.clone();
    let cancelled = guarded_host_call(&plugin_id, "host_transfer_cancel", false, || {
        tokio::task::block_in_place(|| handle.block_on(crate::plugin::transfer::cancel_transfer(&task_id)))
    });
    if cancelled {
        tracing::info!(plugin_id = %plugin_id, task_id = %task_id, "transfer cancel requested");
    } else {
        tracing::debug!(
            plugin_id = %plugin_id,
            task_id = %task_id,
            "host_transfer_cancel: task not active (already finished or unknown)"
        );
    }
    0
}

// ==================== Config Host Function（ABI v5） ====================

/// 配置：读取宿主配置项
///
/// 参数：(key_ptr, key_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
///
/// 白名单 = SDK `ConfigKey` 枚举本身：`from_str` 过滤非法 key，
/// value match 穷尽所有变体 —— 新增配置项时编译器强制补实现，
/// 结构性杜绝"白名单声明了但实现缺失"的漂移
fn host_config_get(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();

    let key = match read_wasm_string(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_config_get: failed to read key");
            return -1;
        }
    };

    // 白名单校验：仅接受 ConfigKey 枚举覆盖的 key
    let Some(config_key) = bedcode_plugin_api_mobile::ConfigKey::from_str(&key) else {
        tracing::warn!(plugin_id = %plugin_id, key = %key, "host_config_get: key not in whitelist");
        return -1;
    };

    // 穷尽 match：新增 ConfigKey 变体必须在此补实现（编译错误兜底）
    let value = match config_key {
        bedcode_plugin_api_mobile::ConfigKey::AppDownloadsDir => {
            match resolve_downloads_dir(&caller) {
                Some(path) => path,
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_config_get: downloads dir not available");
                    return -1;
                }
            }
        }
    };

    match write_wasm_string(&mut caller, &value) {
        Some((ptr, len)) => {
            if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                0
            } else {
                -1
            }
        }
        None => {
            tracing::error!(plugin_id = %plugin_id, key = %key, "host_config_get: failed to write result to WASM memory");
            -1
        }
    }
}

/// 解析下载目录路径
///
/// 策略（按优先级）：
/// 1. Kotlin 桥：`getExternalFilesDir(DIRECTORY_DOWNLOADS)`（外部私有目录，免权限）
/// 2. 兜底：`app_data_dir()/Downloads`（内部存储，注释说明局限）
/// 目录不存在时惰性创建
fn resolve_downloads_dir(caller: &wasmtime::Caller<'_, WasmPluginState>) -> Option<String> {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();
    let handle = caller.data().runtime_handle.clone();

    // 首选：Kotlin 桥获取外部私有下载目录
    let external_path = guarded_host_call(&plugin_id, "resolve_downloads_dir(external)", None, || {
        tokio::task::block_in_place(|| {
            handle.block_on(crate::plugin::android_plugins::get_external_downloads_dir())
        })
    });

    let path = if let Some(ext_path) = external_path {
        tracing::debug!(path = %ext_path, "resolve_downloads_dir: using external private downloads dir");
        std::path::PathBuf::from(ext_path)
    } else {
        // 兜底：app_data_dir()/Downloads（内部存储目录，文件管理器不可见；
        // 外部存储不可用时的降级方案）
        let fallback = guarded_host_call(&plugin_id, "resolve_downloads_dir(fallback)", None, || {
            tokio::task::block_in_place(|| {
                handle.block_on(async {
                    host_ctx.app_handle.path().app_data_dir().ok()
                })
            })
        });
        match fallback {
            Some(data_dir) => {
                let path = data_dir.join("Downloads");
                tracing::debug!(path = %path.display(), "resolve_downloads_dir: using app_data_dir/Downloads fallback");
                path
            }
            None => {
                tracing::error!("resolve_downloads_dir: neither external storage nor app_data_dir available");
                return None;
            }
        }
    };

    // 惰性创建目录
    if !path.exists() {
        if let Err(e) = std::fs::create_dir_all(&path) {
            tracing::error!(error = %e, path = %path.display(), "resolve_downloads_dir: failed to create directory");
            return None;
        }
        tracing::info!(path = %path.display(), "resolve_downloads_dir: created downloads directory");
    }

    Some(path.to_string_lossy().to_string())
}
