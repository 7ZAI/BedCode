//! WASM 插件运行时（移动端）
//!
//! 基于 wasmtime 的 WASM 模块加载、实例化、调用
//! 管理 Engine/Linker/Store/Instance 生命周期
//! 注册宿主 Host Functions 供 WASM 插件调用
//!
//! 与桌面端差异：
//! - WasmHostContext 无 session_manager 和 permission
//! - 新增 host_notify（移动端系统通知）
//! - host_terminal_send 通过 WebSocket 转发到桌面端
//! - host_session_list/host_session_get 为空操作（保持 ABI 兼容）

use crate::plugin::storage::PluginStorage;
use crate::plugin::wasm_host;
use crate::state::get_connection_manager;
use crate::connection::request::TerminalRequest;
use std::path::Path;
use std::sync::Arc;
use tauri::Emitter;
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
}

/// 单个 WASM 插件实例的状态
///
/// 每个插件实例化时创建独立的 Store<WasmPluginState>，
/// state 中包含插件 ID 和宿主上下文引用
pub struct WasmPluginState {
    /// 插件 ID（用于数据隔离）
    plugin_id: String,
    /// 宿主上下文
    host_ctx: Arc<WasmHostContext>,
}

/// 宿主上下文（注入到 WasmPluginState）
///
/// 移动端无 SessionManager 和 PermissionManager
pub struct WasmHostContext {
    /// 数据库（移动端直接使用 rusqlite::Connection）
    db: Arc<Mutex<rusqlite::Connection>>,
    /// 插件 KV 存储
    storage: Arc<PluginStorage>,
    /// Tauri AppHandle
    app_handle: Arc<tauri::AppHandle>,
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
        db: Arc<Mutex<rusqlite::Connection>>,
        storage: Arc<PluginStorage>,
        app_handle: Arc<tauri::AppHandle>,
    ) -> crate::Result<Self> {
        let engine = Engine::default();
        let mut linker = Linker::new(&engine);

        register_host_functions(&mut linker)?;

        Ok(Self { engine, linker })
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

    fn get_export_func(&mut self, name: &str) -> crate::Result<wasmtime::Func> {
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
}

impl WasmHostContext {
    /// 创建宿主上下文
    pub fn new(
        db: Arc<Mutex<rusqlite::Connection>>,
        storage: Arc<PluginStorage>,
        app_handle: Arc<tauri::AppHandle>,
    ) -> Self {
        Self {
            db,
            storage,
            app_handle,
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

// ==================== Host Function Implementations ====================
//
// 移动端 Host Function 约定：
// - 无权限校验（移动端无 PermissionManager）
// - host_terminal_send 通过 WebSocket 转发到桌面端
// - host_notify 调用 tauri-plugin-notification
// - host_session_list/get 为空操作

fn host_storage_get(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
) -> (u32, u32) {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let key = match read_wasm_string(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_get: failed to read key");
            return (0, 0);
        }
    };

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
                    return (0, 0);
                }
            };
            match write_wasm_string(&mut caller, &json_str) {
                Some((ptr, len)) => (ptr, len),
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_storage_get: failed to write result to WASM memory");
                    (0, 0)
                }
            }
        }
        Ok(None) => (0, 0),
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, key = %key, "host_storage_get: storage error");
            (0, 0)
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
    match tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(storage.set(&plugin_id, &key, json_value))
    }) {
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
    let host_ctx = caller.data().host_ctx.clone();

    let key = match read_wasm_string(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_storage_delete: failed to read key");
            return -1;
        }
    };

    let storage = host_ctx.storage.clone();
    match tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(storage.delete(&plugin_id, &key))
    }) {
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
    match tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let conn = db.lock().await;
            conn.execute(&sql, []).map_err(|e| e.to_string())
        })
    }) {
        Ok(affected) => affected as i32,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_db_execute: SQL execution failed");
            -1
        }
    }
}

fn host_db_query(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
) -> (u32, u32) {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let sql = match read_wasm_string(&mut caller, sql_ptr, sql_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_db_query: failed to read SQL");
            return (0, 0);
        }
    };

    if let Err(e) = wasm_host::validate_sql_table_prefix(&plugin_id, &sql) {
        tracing::error!(error = %e, plugin_id = %plugin_id, "host_db_query: table name validation failed");
        return (0, 0);
    }

    let db = host_ctx.db.clone();
    let query_result: Result<serde_json::Value, String> = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let conn = db.lock().await;

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
                    return (0, 0);
                }
            };
            match write_wasm_string(&mut caller, &json_str) {
                Some((ptr, len)) => (ptr, len),
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_db_query: failed to write result to WASM memory");
                    (0, 0)
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_db_query: SQL query failed");
            (0, 0)
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

    match tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(conn.send(&message))
    }) {
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
) -> (u32, u32) {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let request_json = match read_wasm_string(&mut caller, req_ptr, req_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_http_fetch: failed to read request JSON");
            return (0, 0);
        }
    };

    let request: serde_json::Value = match serde_json::from_str(&request_json) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_http_fetch: invalid request JSON");
            return (0, 0);
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
            Some((ptr, len)) => (ptr, len),
            None => (0, 0),
        }
    } else {
        match tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(wasm_host::execute_http_request(&request))
        }) {
            Ok(response) => {
                let result_str = match serde_json::to_string(&response) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(error = %e, plugin_id = %plugin_id, "host_http_fetch: response serialization failed");
                        return (0, 0);
                    }
                };
                match write_wasm_string(&mut caller, &result_str) {
                    Some((ptr, len)) => (ptr, len),
                    None => {
                        tracing::error!(plugin_id = %plugin_id, "host_http_fetch: failed to write result to WASM memory");
                        (0, 0)
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, plugin_id = %plugin_id, "host_http_fetch: HTTP request failed");
                (0, 0)
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
) -> (u32, u32) {
    match write_wasm_string(&mut caller, "[]") {
        Some((ptr, len)) => (ptr, len),
        None => (0, 0),
    }
}

/// 会话获取：移动端空操作，保持 ABI 兼容
fn host_session_get_noop(
    _caller: wasmtime::Caller<'_, WasmPluginState>,
    _sid_ptr: u32,
    _sid_len: u32,
) -> (u32, u32) {
    (0, 0)
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
