//! WASM 插件运行时
//!
//! 基于 wasmtime 的 WASM 模块加载、实例化、调用
//! 管理 Engine/Linker/Store/Instance 生命周期
//! 注册宿主 Host Functions 供 WASM 插件调用

use crate::db::Database;
use crate::plugin::fs_auth::{FsAuthChecker, FsOp};
use crate::plugin::permission::{PermissionManager, PERMISSION_STORAGE, PERMISSION_FS_READ, PERMISSION_FS_WRITE};
use crate::plugin::storage::PluginStorage;
use crate::plugin::wasm_host;
use crate::session::SessionManager;
use crate::system::config::AppConfig;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;
use wasmtime::{Engine, Instance, Linker, Memory, Module, Store};

/// WASM 插件运行时（全局共享）
///
/// Engine 和 Linker 是线程安全的可复用结构：
/// - Engine: WASM 编译器，全局单例
/// - Linker: Host function 注册表，所有插件实例共享
pub struct WasmRuntime {
    engine: Engine,
    linker: Linker<WasmPluginState>,
    /// 文件系统访问校验器
    fs_auth: Arc<FsAuthChecker>,
}

/// 单个 WASM 插件实例的状态
///
/// 每个插件实例化时创建独立的 Store<WasmPluginState>，
/// state 中包含插件 ID 和宿主上下文引用
pub struct WasmPluginState {
    /// 插件 ID（用于权限校验和数据隔离）
    plugin_id: String,
    /// 宿主上下文（注入宿主能力）
    host_ctx: Arc<WasmHostContext>,
}

/// 宿主上下文（注入到 WasmPluginState）
///
/// 持有宿主子系统引用，Host Functions 通过此上下文访问宿主能力
pub struct WasmHostContext {
    db: Arc<Mutex<Database>>,
    /// 插件独立数据库池 — 每插件一个独立 .db 文件和连接
    plugin_dbs: Arc<Mutex<HashMap<String, Arc<Mutex<Database>>>>>,
    storage: Arc<PluginStorage>,
    session_manager: Arc<SessionManager>,
    app_handle: Arc<tauri::AppHandle>,
    permission: Arc<PermissionManager>,
    fs_auth: Arc<FsAuthChecker>,
    message_bus: Arc<crate::plugin::message_bus::MessageBus>,
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

impl WasmRuntime {
    /// 创建 WASM 运行时
    ///
    /// 初始化 Engine、Linker，注册所有 Host Functions
    pub fn new(
        db: Arc<Mutex<Database>>,
        storage: Arc<PluginStorage>,
        session_manager: Arc<SessionManager>,
        app_handle: Arc<tauri::AppHandle>,
        permission: Arc<PermissionManager>,
    ) -> crate::Result<Self> {
        let engine = Engine::default();
        let mut linker = Linker::new(&engine);

        // 注册所有 Host Functions 到 "bedcode" 命名空间
        register_host_functions(&mut linker)?;

        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), app_handle.clone()));

        Ok(Self { engine, linker, fs_auth })
    }

    /// 从字节流编译 WASM 模块
    pub fn compile_module(&self, bytes: &[u8]) -> crate::Result<Module> {
        Module::from_binary(&self.engine, bytes).map_err(|e| {
            crate::AppError::Plugin(format!("Failed to compile WASM module: {}", e))
        })
    }

    /// 从文件编译 WASM 模块
    pub fn compile_module_from_file(&self, path: &Path) -> crate::Result<Module> {
        Module::from_file(&self.engine, path).map_err(|e| {
            crate::AppError::Plugin(format!(
                "Failed to compile WASM module from '{}': {}",
                path.display(),
                e
            ))
        })
    }

    /// 获取文件系统访问校验器引用
    pub fn fs_auth(&self) -> &Arc<FsAuthChecker> {
        &self.fs_auth
    }

    /// 实例化 WASM 模块
    ///
    /// 创建 Store + WasmPluginState，通过 Linker 实例化模块
    pub fn instantiate(
        &self,
        module: &Module,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
    ) -> crate::Result<LoadedWasmPlugin> {
        let state = WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx,
        };
        let mut store = Store::new(&self.engine, state);

        let instance = self
            .linker
            .instantiate(&mut store, module)
            .map_err(|e| {
                crate::AppError::Plugin(format!(
                    "Failed to instantiate WASM module for plugin '{}': {}",
                    plugin_id, e
                ))
            })?;

        // 获取线性内存引用（Host Functions 读写插件内存需要）
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| {
                crate::AppError::Plugin(format!(
                    "WASM module for plugin '{}' has no exported 'memory'",
                    plugin_id
                ))
            })?;

        Ok(LoadedWasmPlugin {
            instance,
            store,
            memory,
        })
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
    ///
    /// 传入 (name_ptr, name_len, args_ptr, args_len)，返回 (result_ptr, result_len)
    /// 宿主通过线性内存读写字符串
    pub fn invoke_command(
        &mut self,
        command_name: &str,
        args_json: &str,
    ) -> crate::Result<String> {
        let (name_ptr, name_len) = self.write_string_to_memory(command_name)?;
        let (args_ptr, args_len) = self.write_string_to_memory(args_json)?;

        let func = self.get_export_func("__bedcode_invoke_command")?;
        let mut results = [wasmtime::Val::I32(0), wasmtime::Val::I32(0)];
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(name_ptr as i32),
                wasmtime::Val::I32(name_len as i32),
                wasmtime::Val::I32(args_ptr as i32),
                wasmtime::Val::I32(args_len as i32),
            ],
            &mut results,
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM invoke_command() call failed: {}", e))
        })?;

        let ptr = results[0].unwrap_i32() as u32;
        let len = results[1].unwrap_i32() as u32;

        self.read_string_from_memory(ptr, len)
    }

    /// 调用插件的 on_terminal_input 导出函数
    pub fn on_terminal_input(
        &mut self,
        session_id: &str,
        text: &str,
    ) -> crate::Result<Option<String>> {
        let (sid_ptr, sid_len) = self.write_string_to_memory(session_id)?;
        let (text_ptr, text_len) = self.write_string_to_memory(text)?;

        let func = self.get_export_func("__bedcode_on_terminal_input")?;
        let mut results = [wasmtime::Val::I32(0), wasmtime::Val::I32(0)];
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(sid_ptr as i32),
                wasmtime::Val::I32(sid_len as i32),
                wasmtime::Val::I32(text_ptr as i32),
                wasmtime::Val::I32(text_len as i32),
            ],
            &mut results,
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_terminal_input() call failed: {}", e))
        })?;

        let ptr = results[0].unwrap_i32() as u32;
        let len = results[1].unwrap_i32() as u32;

        if ptr == 0 && len == 0 {
            Ok(None)
        } else {
            self.read_string_from_memory(ptr, len).map(Some)
        }
    }

    /// 调用插件的 on_terminal_output 导出函数
    pub fn on_terminal_output(
        &mut self,
        session_id: &str,
        data: &str,
    ) -> crate::Result<Option<String>> {
        let (sid_ptr, sid_len) = self.write_string_to_memory(session_id)?;
        let (data_ptr, data_len) = self.write_string_to_memory(data)?;

        let func = self.get_export_func("__bedcode_on_terminal_output")?;
        let mut results = [wasmtime::Val::I32(0), wasmtime::Val::I32(0)];
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(sid_ptr as i32),
                wasmtime::Val::I32(sid_len as i32),
                wasmtime::Val::I32(data_ptr as i32),
                wasmtime::Val::I32(data_len as i32),
            ],
            &mut results,
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_terminal_output() call failed: {}", e))
        })?;

        let ptr = results[0].unwrap_i32() as u32;
        let len = results[1].unwrap_i32() as u32;

        if ptr == 0 && len == 0 {
            Ok(None)
        } else {
            self.read_string_from_memory(ptr, len).map(Some)
        }
    }

    /// 调用插件的 on_startup 导出函数（可选）
    pub fn on_startup(&mut self) -> crate::Result<()> {
        if let Ok(func) = self.get_export_func("__bedcode_on_startup") {
            func.call(&mut self.store, &[], &mut []).map_err(|e| {
                crate::AppError::Plugin(format!("WASM on_startup() call failed: {}", e))
            })?;
        }
        Ok(())
    }

    /// 调用插件的 on_shutdown 导出函数（可选）
    pub fn on_shutdown(&mut self) -> crate::Result<()> {
        if let Ok(func) = self.get_export_func("__bedcode_on_shutdown") {
            func.call(&mut self.store, &[], &mut []).map_err(|e| {
                crate::AppError::Plugin(format!("WASM on_shutdown() call failed: {}", e))
            })?;
        }
        Ok(())
    }

    /// 调用插件的消息总线消息接收导出函数（可选）
    pub fn on_message(
        &mut self,
        topic: &str,
        sender: &str,
        payload: &serde_json::Value,
    ) -> crate::Result<()> {
        // 可选导出：如果插件未导出 __bedcode_on_message，跳过
        let Ok(func) = self.get_export_func("__bedcode_on_message") else {
            return Ok(());
        };

        let (topic_ptr, topic_len) = self.write_string_to_memory(topic)?;
        let (sender_ptr, sender_len) = self.write_string_to_memory(sender)?;
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
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
            ],
            &mut results,
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_message() call failed: {}", e))
        })?;

        let status = results[0].unwrap_i32();
        if status != 0 {
            tracing::warn!("WASM on_message() returned non-zero status: {}", status);
        }
        Ok(())
    }

    /// 获取插件的 manifest JSON
    pub fn get_manifest(&mut self) -> crate::Result<String> {
        let func = self.get_export_func("__bedcode_manifest")?;
        let mut results = [wasmtime::Val::I32(0), wasmtime::Val::I32(0)];
        func.call(&mut self.store, &[], &mut results).map_err(|e| {
            crate::AppError::Plugin(format!("WASM manifest() call failed: {}", e))
        })?;

        let ptr = results[0].unwrap_i32() as u32;
        let len = results[1].unwrap_i32() as u32;

        self.read_string_from_memory(ptr, len)
    }

    // ==================== Memory Helpers ====================

    /// 获取导出函数
    fn get_export_func(
        &mut self,
        name: &str,
    ) -> crate::Result<wasmtime::Func> {
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
    ///
    /// 通过插件的 __bedcode_allocate 导出函数分配内存，
    /// 然后写入字符串字节，返回 (ptr, len)
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
}

impl WasmHostContext {
    /// 创建宿主上下文
    pub fn new(
        db: Arc<Mutex<Database>>,
        plugin_dbs: Arc<Mutex<HashMap<String, Arc<Mutex<Database>>>>>,
        storage: Arc<PluginStorage>,
        session_manager: Arc<SessionManager>,
        app_handle: Arc<tauri::AppHandle>,
        permission: Arc<PermissionManager>,
        fs_auth: Arc<FsAuthChecker>,
        message_bus: Arc<crate::plugin::message_bus::MessageBus>,
    ) -> Self {
        Self {
            db,
            plugin_dbs,
            storage,
            session_manager,
            app_handle,
            permission,
            fs_auth,
            message_bus,
        }
    }

    /// 获取消息总线引用
    pub fn message_bus(&self) -> &Arc<crate::plugin::message_bus::MessageBus> {
        &self.message_bus
    }

    /// 获取或懒加载插件独立数据库
    ///
    /// 首次调用时创建目录 + 打开/创建 plugin.db + 缓存连接
    /// 后续调用直接返回缓存的连接
    pub async fn get_or_create_plugin_db(&self, plugin_id: &str) -> crate::Result<Arc<Mutex<Database>>> {
        // 快速路径：已缓存
        {
            let dbs = self.plugin_dbs.lock().await;
            if let Some(db) = dbs.get(plugin_id) {
                return Ok(db.clone());
            }
        }

        // 慢路径：创建数据库
        let app_data_dir = self.app_handle.path().app_data_dir()
            .map_err(|e| crate::AppError::Plugin(format!("Failed to get app data dir: {}", e)))?;
        let plugin_dir = app_data_dir.join("plugins").join(plugin_id);

        // 创建插件数据目录
        if !plugin_dir.exists() {
            std::fs::create_dir_all(&plugin_dir)
                .map_err(|e| crate::AppError::Plugin(format!(
                    "Failed to create plugin data dir '{}': {}",
                    plugin_dir.display(), e
                )))?;
        }

        let db_path = plugin_dir.join("plugin.db");
        let db = Database::new(&db_path)?;

        // 缓存连接
        let db_arc = Arc::new(Mutex::new(db));
        {
            let mut dbs = self.plugin_dbs.lock().await;
            // 双重检查：另一个线程可能已插入
            if let Some(existing) = dbs.get(plugin_id) {
                return Ok(existing.clone());
            }
            dbs.insert(plugin_id.to_string(), db_arc.clone());
        }

        tracing::info!(plugin_id = %plugin_id, path = %db_path.display(), "Plugin database created/opened");
        Ok(db_arc)
    }
}

// ==================== Host Function Registration ====================

/// 注册所有 Host Functions 到 Linker
///
/// 所有函数注册在 "bedcode" 命名空间下，WASM 插件通过
/// `import "bedcode" "func_name"` 调用
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

    // 插件独立数据库
    linker
        .func_wrap("bedcode", "host_plugin_db_execute", host_plugin_db_execute)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_plugin_db_execute: {}", e)))?;

    linker
        .func_wrap("bedcode", "host_plugin_db_query", host_plugin_db_query)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_plugin_db_query: {}", e)))?;

    // 终端
    linker
        .func_wrap("bedcode", "host_terminal_send", host_terminal_send)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_terminal_send: {}", e)))?;

    // 会话
    linker
        .func_wrap("bedcode", "host_session_list", host_session_list)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_session_list: {}", e)))?;

    linker
        .func_wrap("bedcode", "host_session_get", host_session_get)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_session_get: {}", e)))?;

    // 事件
    linker
        .func_wrap("bedcode", "host_emit_event", host_emit_event)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_emit_event: {}", e)))?;

    // 广播同步事件（移动端同步通道）
    linker
        .func_wrap("bedcode", "host_broadcast_sync", host_broadcast_sync)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_broadcast_sync: {}", e)))?;

    // HTTP 代理
    linker
        .func_wrap("bedcode", "host_http_fetch", host_http_fetch)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_http_fetch: {}", e)))?;

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

    // 通知（发送 Tauri 事件到前端 toast 显示）
    linker
        .func_wrap("bedcode", "host_notify", host_notify)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_notify: {}", e)))?;

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

    // 配置读取
    linker
        .func_wrap("bedcode", "host_config_get", host_config_get)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_config_get: {}", e)))?;

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

    // 会话生命周期事件订阅
    linker
        .func_wrap("bedcode", "host_session_lifecycle_subscribe", host_session_lifecycle_subscribe)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to register host_session_lifecycle_subscribe: {}", e)))?;

    Ok(())
}

// ==================== Host Function Implementations ====================
//
// 所有 Host Function 签名约定：
// - 字符串参数以 (ptr, len) 对传递，指向 WASM 线性内存
// - 返回 (ptr, len) 对的函数通过 out_ptr 输出参数写入（8 字节: ptr + len）
// - 其他函数用 i32 状态码返回
// - 宿主通过 Caller 访问 WasmPluginState 获取 plugin_id 和宿主能力

/// 将 (ptr, len) 结果写入 WASM 线性内存中的 out_ptr 位置（8 字节: ptr:u32 + len:u32）
///
/// 返回 0 表示成功，-1 表示写入失败
fn write_result_to_out_ptr(
    caller: &mut wasmtime::Caller<'_, WasmPluginState>,
    out_ptr: u32,
    ptr: u32,
    len: u32,
) -> i32 {
    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return -1,
    };
    let data = memory.data_mut(caller);
    let start = out_ptr as usize;
    let end = start + 8;
    if end > data.len() {
        return -1;
    }
    data[start..start + 4].copy_from_slice(&ptr.to_le_bytes());
    data[start + 4..end].copy_from_slice(&len.to_le_bytes());
    0
}

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

/// 将字符串写入 WASM 线性内存，返回 (ptr, len)
///
/// 通过插件的 __bedcode_allocate 导出函数分配内存
fn write_wasm_string(
    caller: &mut wasmtime::Caller<'_, WasmPluginState>,
    s: &str,
) -> Option<(u32, u32)> {
    if s.is_empty() {
        return Some((0, 0));
    }

    let bytes = s.as_bytes();
    let len = bytes.len();

    // 调用插件的内存分配器
    let alloc_func = caller.get_export("__bedcode_allocate")?.into_func()?;
    let mut results = [wasmtime::Val::I32(0)];
    alloc_func.call(&mut *caller, &[wasmtime::Val::I32(len as i32)], &mut results).ok()?;
    let ptr = results[0].unwrap_i32() as u32;
    if ptr == 0 {
        return None;
    }

    // 重新获取 memory 引用（alloc_func.call 消费了 caller 的借用）
    // 使用 caller 的 AsContextMut 实现直接访问内存
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

/// 存储：获取值
///
/// 参数：(key_ptr, key_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
fn host_storage_get(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let key = match read_wasm_string(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_get: failed to read key from WASM memory");
            return -1;
        }
    };

    // 权限校验
    if !host_ctx.permission.check(&plugin_id, PERMISSION_STORAGE) {
        tracing::error!(
            plugin_id = %plugin_id,
            permission = "storage",
            "host_storage_get: permission denied"
        );
        return -1;
    }

    let storage = host_ctx.storage.clone();
    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(storage.get(&plugin_id, &key))
    });

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
                Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_storage_get: failed to write result to WASM memory");
                    -1
                }
            }
        }
        Ok(None) => write_result_to_out_ptr(&mut caller, out_ptr, 0, 0),
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, key = %key, "host_storage_get: storage error");
            -1
        }
    }
}

/// 存储：设置值
///
/// 参数：(key_ptr, key_len, val_ptr, val_len)
/// 返回：0 成功，-1 失败
fn host_storage_set(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
    val_ptr: u32,
    val_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
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
            tracing::error!(plugin_id = %plugin_id, key = %key, "host_storage_set: failed to read value");
            return -1;
        }
    };

    let json_value: serde_json::Value = match serde_json::from_str(&val_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, key = %key, "host_storage_set: invalid JSON value");
            return -1;
        }
    };

    if !host_ctx.permission.check(&plugin_id, PERMISSION_STORAGE) {
        tracing::error!(plugin_id = %plugin_id, permission = "storage", "host_storage_set: permission denied");
        return -1;
    }

    let storage = host_ctx.storage.clone();
    match tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(storage.set(&plugin_id, &key, json_value))
    }) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, key = %key, "host_storage_set: storage error");
            -1
        }
    }
}

/// 存储：删除值
///
/// 参数：(key_ptr, key_len)
/// 返回：0 成功，-1 失败
fn host_storage_delete(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let key = match read_wasm_string(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_delete: failed to read key");
            return -1;
        }
    };

    if !host_ctx.permission.check(&plugin_id, PERMISSION_STORAGE) {
        tracing::error!(plugin_id = %plugin_id, permission = "storage", "host_storage_delete: permission denied");
        return -1;
    }

    let storage = host_ctx.storage.clone();
    match tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(storage.delete(&plugin_id, &key))
    }) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, key = %key, "host_storage_delete: storage error");
            -1
        }
    }
}

/// 数据库：执行 SQL
///
/// 参数：(sql_ptr, sql_len)
/// 返回：受影响行数（>= 0），负数表示错误
fn host_db_execute(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let sql = match read_wasm_string(&mut caller, sql_ptr, sql_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_db_execute: failed to read SQL");
            return -1;
        }
    };

    if !host_ctx.permission.check(&plugin_id, PERMISSION_STORAGE) {
        tracing::error!(plugin_id = %plugin_id, permission = "storage", "host_db_execute: permission denied");
        return -1;
    }

    // 表名前缀校验
    if let Err(e) = wasm_host::validate_sql_table_prefix(&plugin_id, &sql) {
        tracing::error!(error = %e, plugin_id = %plugin_id, "host_db_execute: table name validation failed");
        return -1;
    }

    let db = host_ctx.db.clone();
    match tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let db = db.lock().await;
            db.conn().execute(&sql, []).map_err(|e| e.to_string())
        })
    }) {
        Ok(affected) => affected as i32,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, sql = %sql, "host_db_execute: SQL execution failed");
            -1
        }
    }
}

/// 数据库：查询 SQL
///
/// 参数：(sql_ptr, sql_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
fn host_db_query(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let sql = match read_wasm_string(&mut caller, sql_ptr, sql_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_db_query: failed to read SQL");
            return -1;
        }
    };

    if !host_ctx.permission.check(&plugin_id, PERMISSION_STORAGE) {
        tracing::error!(plugin_id = %plugin_id, permission = "storage", "host_db_query: permission denied");
        return -1;
    }

    if let Err(e) = wasm_host::validate_sql_table_prefix(&plugin_id, &sql) {
        tracing::error!(error = %e, plugin_id = %plugin_id, "host_db_query: table name validation failed");
        return -1;
    }

    let db = host_ctx.db.clone();
    let query_result: Result<serde_json::Value, String> = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let db = db.lock().await;
            let conn = db.conn();

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
        })
    });

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
                Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_db_query: failed to write result to WASM memory");
                    -1
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, sql = %sql, "host_db_query: SQL query failed");
            -1
        }
    }
}

/// 插件独立数据库：执行 SQL
///
/// 参数：(sql_ptr, sql_len)
/// 返回：受影响行数（>= 0），负数表示错误
///
/// 与 host_db_execute 的区别：
/// - 使用插件独立数据库连接（无全局 Mutex 竞争）
/// - 无表名前缀校验（整个数据库都是插件的）
fn host_plugin_db_execute(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let sql = match read_wasm_string(&mut caller, sql_ptr, sql_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_plugin_db_execute: failed to read SQL");
            return -1;
        }
    };

    if !host_ctx.permission.check(&plugin_id, PERMISSION_STORAGE) {
        tracing::error!(plugin_id = %plugin_id, permission = "storage", "host_plugin_db_execute: permission denied");
        return -1;
    }

    let result: Result<i32, String> = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let db_arc = host_ctx.get_or_create_plugin_db(&plugin_id).await
                .map_err(|e| e.to_string())?;
            let db = db_arc.lock().await;
            db.conn().execute(&sql, []).map(|n| n as i32).map_err(|e| e.to_string())
        })
    });

    match result {
        Ok(affected) => affected as i32,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, sql = %sql, "host_plugin_db_execute: SQL execution failed");
            -1
        }
    }
}

/// 插件独立数据库：查询 SQL
///
/// 参数：(sql_ptr, sql_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
///
/// 与 host_db_query 的区别：
/// - 使用插件独立数据库连接（无全局 Mutex 竞争）
/// - 无表名前缀校验（整个数据库都是插件的）
fn host_plugin_db_query(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let sql = match read_wasm_string(&mut caller, sql_ptr, sql_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_plugin_db_query: failed to read SQL");
            return -1;
        }
    };

    if !host_ctx.permission.check(&plugin_id, PERMISSION_STORAGE) {
        tracing::error!(plugin_id = %plugin_id, permission = "storage", "host_plugin_db_query: permission denied");
        return -1;
    }

    let query_result: Result<serde_json::Value, String> = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let db_arc = host_ctx.get_or_create_plugin_db(&plugin_id).await
                .map_err(|e| e.to_string())?;
            let db = db_arc.lock().await;
            let conn = db.conn();

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
        })
    });

    match query_result {
        Ok(value) => {
            let json_str = match serde_json::to_string(&value) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, plugin_id = %plugin_id, "host_plugin_db_query: JSON serialization failed");
                    return -1;
                }
            };
            match write_wasm_string(&mut caller, &json_str) {
                Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_plugin_db_query: failed to write result to WASM memory");
                    -1
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, sql = %sql, "host_plugin_db_query: SQL query failed");
            -1
        }
    }
}

/// 终端：发送输入
///
/// 参数：(session_id_ptr, session_id_len, data_ptr, data_len)
/// 返回：0 成功，-1 失败
fn host_terminal_send(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sid_ptr: u32,
    sid_len: u32,
    data_ptr: u32,
    data_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

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
            tracing::error!(plugin_id = %plugin_id, session_id = %session_id, "host_terminal_send: failed to read data");
            return -1;
        }
    };

    if !host_ctx.permission.check(&plugin_id, "terminal:input") {
        tracing::error!(plugin_id = %plugin_id, permission = "terminal:input", "host_terminal_send: permission denied");
        return -1;
    }

    let sm = host_ctx.session_manager.clone();
    match tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(sm.write_input(&session_id, &data))
    }) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, session_id = %session_id, "host_terminal_send: write failed");
            -1
        }
    }
}

/// 会话：列出所有会话
///
/// 参数：(out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
fn host_session_list(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    out_ptr: u32,
) -> i32 {
    let host_ctx = caller.data().host_ctx.clone();
    let sm = host_ctx.session_manager.clone();

    let sessions = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(sm.list_sessions())
    });

    match serde_json::to_string(&sessions) {
        Ok(json) => match write_wasm_string(&mut caller, &json) {
            Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
            None => {
                tracing::error!("host_session_list: failed to write result to WASM memory");
                -1
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "host_session_list: serialization failed");
            -1
        }
    }
}

/// 会话：获取单个会话
///
/// 参数：(session_id_ptr, session_id_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
fn host_session_get(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sid_ptr: u32,
    sid_len: u32,
    out_ptr: u32,
) -> i32 {
    let session_id = match read_wasm_string(&mut caller, sid_ptr, sid_len) {
        Some(s) => s,
        None => {
            tracing::error!("host_session_get: failed to read session_id");
            return -1;
        }
    };

    let host_ctx = caller.data().host_ctx.clone();
    let sm = host_ctx.session_manager.clone();

    match tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(sm.get_session(&session_id))
    }) {
        Some(info) => match serde_json::to_string(&info) {
            Ok(json) => match write_wasm_string(&mut caller, &json) {
                Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
                None => {
                    tracing::error!(session_id = %session_id, "host_session_get: failed to write result to WASM memory");
                    -1
                }
            },
            Err(e) => {
                tracing::error!(error = %e, session_id = %session_id, "host_session_get: serialization failed");
                -1
            }
        },
        None => write_result_to_out_ptr(&mut caller, out_ptr, 0, 0),
    }
}

/// 事件：向前端发送
///
/// 参数：(event_name_ptr, event_name_len, payload_ptr, payload_len)
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

/// 广播同步事件到所有客户端（移动端同步通道）
///
/// 插件通过此函数将状态变更推送到 DesktopSyncEvent 广播通道，
/// 由 SyncEventHandler 转发给所有已认证的 WebSocket 客户端（移动端）。
///
/// payload 格式：
/// ```json
/// { "type": "TaskStatusChanged", "session_id": "...", "task_status": "in_progress", "task_reason": "...", "task_questions": [...] }
/// { "type": "SessionModeChanged", "session_id": "...", "auto_approve": true }
/// ```
fn host_broadcast_sync(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    payload_ptr: u32,
    payload_len: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();

    let payload_str = match read_wasm_string(&mut caller, payload_ptr, payload_len) {
        Some(s) => s,
        None => {
            tracing::error!("[plugin:{}] host_broadcast_sync: failed to read payload", plugin_id);
            return;
        }
    };

    let payload: serde_json::Value = match serde_json::from_str(&payload_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "[plugin:{}] host_broadcast_sync: invalid JSON payload", plugin_id);
            return;
        }
    };

    let event_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");

    use crate::events::DesktopSyncEvent;

    let sync_event = match event_type {
        "TaskStatusChanged" => {
            let session_id = payload.get("session_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let task_status = payload.get("task_status").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let task_reason = payload.get("task_reason").and_then(|v| v.as_str()).map(|s| s.to_string());
            let task_questions = payload.get("task_questions")
                .and_then(|v| serde_json::from_value::<Vec<crate::enums::PluginQuestion>>(v.clone()).ok());
            DesktopSyncEvent::TaskStatusChanged {
                session_id,
                task_status,
                task_reason,
                task_questions,
            }
        }
        "SessionModeChanged" => {
            let session_id = payload.get("session_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let auto_approve = payload.get("auto_approve").and_then(|v| v.as_bool()).unwrap_or(false);
            DesktopSyncEvent::SessionModeChanged {
                session_id,
                auto_approve,
            }
        }
        _ => {
            tracing::warn!("[plugin:{}] host_broadcast_sync: unknown event type: {}", plugin_id, event_type);
            return;
        }
    };

    let ctx = crate::system::app_context::AppContext::global();
    let sync_tx = ctx.sync_tx();
    if let Err(e) = sync_tx.send(sync_event) {
        tracing::error!(error = %e, "[plugin:{}] host_broadcast_sync: broadcast failed", plugin_id);
    }
}

/// HTTP 代理：发起 HTTP 请求
///
/// 参数：(request_json_ptr, request_json_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
///
/// request_json 格式：
/// ```json
/// {
///   "method": "POST",
///   "url": "https://api.example.com/v1/chat",
///   "headers": { "Authorization": "Bearer xxx", "Content-Type": "application/json" },
///   "body": "{...}",
///   "stream": true,
///   "streamEvent": "ai-chatbox:stream:xxx"
/// }
/// ```
///
/// 流式模式：宿主 spawn tokio 任务执行 HTTP 请求，逐 chunk 通过 emit_event 推送，
/// http_fetch 立即返回 stream_id
/// 非流式模式：block_on 执行，返回完整响应
fn host_http_fetch(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    req_ptr: u32,
    req_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let request_json = match read_wasm_string(&mut caller, req_ptr, req_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_http_fetch: failed to read request JSON");
            return -1;
        }
    };

    // 解析请求
    let request: serde_json::Value = match serde_json::from_str(&request_json) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_http_fetch: invalid request JSON");
            return -1;
        }
    };

    let is_stream = request.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    if is_stream {
        // 流式模式：spawn 后台任务，立即返回 stream_id
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
                    stream_event = %stream_event_clone,
                    "Streaming HTTP request failed"
                );
                // 发送错误事件通知插件
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
            Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
            None => -1,
        }
    } else {
        // 非流式模式：同步执行 HTTP 请求
        match tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(wasm_host::execute_http_request(&request))
        }) {
            Ok(response) => {
                let result_str = match serde_json::to_string(&response) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(error = %e, plugin_id = %plugin_id, "host_http_fetch: response serialization failed");
                        return -1;
                    }
                };
                match write_wasm_string(&mut caller, &result_str) {
                    Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
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

/// 日志：info 级别
fn host_log_info(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::info!("[plugin:{}] {}", plugin_id, message);
}

/// 日志：debug 级别
fn host_log_debug(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::debug!("[plugin:{}] {}", plugin_id, message);
}

/// 日志：warn 级别
fn host_log_warn(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::warn!("[plugin:{}] {}", plugin_id, message);
}

/// 日志：error 级别
fn host_log_error(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let message = read_wasm_string(&mut caller, msg_ptr, msg_len).unwrap_or_default();
    tracing::error!("[plugin:{}] {}", plugin_id, message);
}

/// 通知：通过 Tauri 事件发送到前端 toast
///
/// 参数：(title_ptr, title_len, body_ptr, body_len)
/// 返回：0 成功，-1 失败
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
            tracing::error!(plugin_id = %plugin_id, title = %title, "host_notify: failed to read body");
            return -1;
        }
    };

    match host_ctx.app_handle.emit("plugin:notify", serde_json::json!({
        "plugin_id": plugin_id,
        "title": title,
        "body": body,
    })) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_notify: emit failed");
            -1
        }
    }
}

// ==================== File System Host Functions ====================

/// 文件系统：读取文件
///
/// 参数：(path_ptr, path_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
fn host_fs_read(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let path = match read_wasm_string(&mut caller, path_ptr, path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_read: failed to read path");
            return -1;
        }
    };

    // 权限校验
    if !host_ctx.permission.check(&plugin_id, PERMISSION_FS_READ) {
        tracing::error!(plugin_id = %plugin_id, path = %path, "host_fs_read: permission denied");
        return -1;
    }

    // 访问校验（三层策略）
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(fs_auth.check(&plugin_id, &path, FsOp::Read))
    });
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "host_fs_read: access denied by fs_auth");
        return -1;
    }

    // 执行文件读取
    match std::fs::read_to_string(&path) {
        Ok(content) => match write_wasm_string(&mut caller, &content) {
            Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
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
///
/// 参数：(path_ptr, path_len, data_ptr, data_len)
/// 返回：0 成功，-1 失败
fn host_fs_write(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
    data_ptr: u32,
    data_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
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

    // 权限校验
    if !host_ctx.permission.check(&plugin_id, PERMISSION_FS_WRITE) {
        tracing::error!(plugin_id = %plugin_id, path = %path, "host_fs_write: permission denied");
        return -1;
    }

    // 访问校验
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(fs_auth.check(&plugin_id, &path, FsOp::Write))
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
///
/// 参数：(src_ptr, src_len, dst_ptr, dst_len)
/// 返回：0 成功，-1 失败
fn host_fs_copy(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    src_ptr: u32,
    src_len: u32,
    dst_ptr: u32,
    dst_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
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
    if !host_ctx.permission.check(&plugin_id, PERMISSION_FS_READ) {
        tracing::error!(plugin_id = %plugin_id, "host_fs_copy: fs:read permission denied");
        return -1;
    }
    if !host_ctx.permission.check(&plugin_id, PERMISSION_FS_WRITE) {
        tracing::error!(plugin_id = %plugin_id, "host_fs_copy: fs:write permission denied");
        return -1;
    }

    // 访问校验（源文件读、目标文件写）
    let fs_auth = host_ctx.fs_auth.clone();
    let plugin_id_clone = plugin_id.clone();
    let src_clone = src.clone();
    let dst_clone = dst.clone();
    let allowed = tokio::task::block_in_place(|| {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let read_ok = fs_auth.check(&plugin_id_clone, &src_clone, FsOp::Read).await;
            if !read_ok {
                return false;
            }
            fs_auth.check(&plugin_id_clone, &dst_clone, FsOp::Write).await
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

// ==================== Config Host Functions ====================

/// 配置白名单 key 列表
const CONFIG_WHITELIST: &[&str] = &["plugin.token", "network.port", "home_dir"];

/// 配置：读取宿主配置项
///
/// 参数：(key_ptr, key_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
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

    // 白名单校验
    if !CONFIG_WHITELIST.contains(&key.as_str()) {
        tracing::warn!(plugin_id = %plugin_id, key = %key, "host_config_get: key not in whitelist");
        return -1;
    }

    let value = match key.as_str() {
        "plugin.token" => {
            let config = AppConfig::global();
            config.plugin.token.clone()
        }
        "network.port" => {
            let config = AppConfig::global();
            config.network.port.to_string()
        }
        "home_dir" => {
            match dirs::home_dir() {
                Some(dir) => dir.to_string_lossy().to_string(),
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_config_get: home_dir not available");
                    return -1;
                }
            }
        }
        _ => return -1,
    };

    match write_wasm_string(&mut caller, &value) {
        Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
        None => {
            tracing::error!(plugin_id = %plugin_id, key = %key, "host_config_get: failed to write result to WASM memory");
            -1
        }
    }
}

// ==================== Message Bus Host Functions ====================

/// 消息总线：发布消息
///
/// 参数：(topic_ptr, topic_len, payload_ptr, payload_len)
/// 返回：0 成功，-1 失败
fn host_bus_publish(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    topic_ptr: u32,
    topic_len: u32,
    payload_ptr: u32,
    payload_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
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
            tracing::error!(error = %e, plugin_id = %plugin_id, topic = %topic, "host_bus_publish: invalid JSON payload");
            return -1;
        }
    };

    let bus = host_ctx.message_bus.clone();
    bus.publish(&topic, &plugin_id, payload);
    0
}

/// 消息总线：订阅 topic
///
/// 参数：(topic_ptr, topic_len)
/// 返回：0 成功，-1 失败
fn host_bus_subscribe(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    topic_ptr: u32,
    topic_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let topic = match read_wasm_string(&mut caller, topic_ptr, topic_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_bus_subscribe: failed to read topic");
            return -1;
        }
    };

    let bus = host_ctx.message_bus.clone();
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(bus.subscribe_wasm(&plugin_id, &topic))
    });
    0
}

/// 消息总线：取消订阅
///
/// 参数：(topic_ptr, topic_len)
/// 返回：0 成功，-1 失败
fn host_bus_unsubscribe(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    topic_ptr: u32,
    topic_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let topic = match read_wasm_string(&mut caller, topic_ptr, topic_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_bus_unsubscribe: failed to read topic");
            return -1;
        }
    };

    let bus = host_ctx.message_bus.clone();
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(bus.unsubscribe(&plugin_id, &topic))
    });
    0
}

// ==================== Session Lifecycle Host Function ====================

/// 会话生命周期：订阅生命周期事件
///
/// 插件调用后，当会话创建/停止时会通过 on_message 回调接收事件
/// 参数：无（自动根据调用者的 plugin_id 注册）
/// 返回：0 成功，-1 失败
fn host_session_lifecycle_subscribe(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();

    let ctx = crate::system::app_context::AppContext::global();
    let plugin_host = ctx.plugin_host();
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(plugin_host.subscribe_session_lifecycle(&plugin_id))
    });
    0
}
