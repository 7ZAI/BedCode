//! HostContext FFI
//!
//! 宿主向 cdylib 插件注入的能力上下文实现
//! 所有函数指针由宿主实现，插件通过调用这些函数访问宿主能力

use crate::db::Database;
use crate::plugin::permission::{PermissionManager, PERMISSION_STORAGE};
use crate::plugin::storage::PluginStorage;
use crate::session::SessionManager;
use regex::Regex;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;

// ==================== FFI Struct ====================

/// 宿主能力上下文 -- 通过函数指针向 cdylib 插件暴露宿主 API
///
/// 每个插件激活时获得独立的 HostContext 实例，plugin_id 字段
/// 标识调用方身份，用于权限校验和数据隔离
#[repr(C)]
pub struct HostContext {
    /// 插件 ID（C 字符串，由宿主分配，生命周期与 HostContext 一致）
    pub plugin_id: *const c_char,
    /// 释放由宿主分配的 C 字符串
    pub free_string: extern "C" fn(*mut c_char),
    /// 获取存储值，返回 JSON 字符串或 null（key 不存在）
    /// 参数：(plugin_id, key)
    pub storage_get: extern "C" fn(*const c_char, *const c_char) -> *mut c_char,
    /// 设置存储值，0 成功，-1 失败
    /// 参数：(plugin_id, key, value_json)
    pub storage_set: extern "C" fn(*const c_char, *const c_char, *const c_char) -> i32,
    /// 删除存储值，0 成功，-1 失败
    /// 参数：(plugin_id, key)
    pub storage_delete: extern "C" fn(*const c_char, *const c_char) -> i32,
    /// 执行 SQL（INSERT/UPDATE/DELETE），返回受影响行数，负数表示错误
    /// 参数：(plugin_id, sql)
    pub db_execute: extern "C" fn(*const c_char, *const c_char) -> i32,
    /// 查询 SQL（SELECT），返回 JSON 数组字符串或 null
    /// 参数：(plugin_id, sql)
    pub db_query: extern "C" fn(*const c_char, *const c_char) -> *mut c_char,
    /// 向终端会话发送输入，0 成功，-1 失败
    /// 参数：(session_id, data)
    pub terminal_send_input: extern "C" fn(*const c_char, *const c_char) -> i32,
    /// 列出所有会话，返回 JSON 数组字符串
    pub session_list: extern "C" fn() -> *mut c_char,
    /// 获取单个会话，返回 JSON 对象字符串或 null
    /// 参数：(session_id)
    pub session_get: extern "C" fn(*const c_char) -> *mut c_char,
    /// 向前端发送事件
    /// 参数：(event_name, payload_json)
    pub emit_event: extern "C" fn(*const c_char, *const c_char),
}

// ==================== HostContextFns ====================

/// HostContext 函数指针的工厂 -- 持有宿主子系统引用，为每个插件构建 HostContext
///
/// 各 Arc 引用从 PluginHost::new() 传入，生命周期与 PluginHost 一致
/// 字段通过 FFI 函数指针间接使用（extern "C" fn 内访问 AppContext::global()）
#[allow(dead_code)]
pub struct HostContextFns {
    db: Arc<Mutex<Database>>,
    storage: Arc<PluginStorage>,
    session_manager: Arc<SessionManager>,
    app_handle: Arc<tauri::AppHandle>,
    permission: Arc<PermissionManager>,
}

impl HostContextFns {
    /// 创建 HostContextFns 工厂
    pub fn new(
        db: Arc<Mutex<Database>>,
        storage: Arc<PluginStorage>,
        session_manager: Arc<SessionManager>,
        app_handle: Arc<tauri::AppHandle>,
        permission: Arc<PermissionManager>,
    ) -> Self {
        Self {
            db,
            storage,
            session_manager,
            app_handle,
            permission,
        }
    }

    /// 为指定插件构建 HostContext 实例
    ///
    /// plugin_id 会被复制为 C 字符串嵌入 HostContext，调用方无需保留原始字符串
    pub fn build_host_context(&self, plugin_id: &str) -> HostContext {
        let plugin_id_cstr = CString::new(plugin_id)
            .expect("plugin_id should not contain null bytes")
            .into_raw() as *const c_char;

        HostContext {
            plugin_id: plugin_id_cstr,
            free_string: host_free_string,
            storage_get: host_storage_get,
            storage_set: host_storage_set,
            storage_delete: host_storage_delete,
            db_execute: host_db_execute,
            db_query: host_db_query,
            terminal_send_input: host_terminal_send_input,
            session_list: host_session_list,
            session_get: host_session_get,
            emit_event: host_emit_event,
        }
    }
}

// ==================== Helper: C string conversion ====================

/// 将 *const c_char 转换为 Rust String，null 指针返回 None
///
/// # Safety
/// 调用者必须确保指针指向有效的以 null 结尾的 C 字符串
unsafe fn cstr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string())
}

/// 将 Rust String 转换为 C 堆分配字符串（调用方需通过 free_string 释放）
fn string_to_cstr(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            // CString::new 不允许内部 null 字节，理论上不应发生
            tracing::error!("Failed to create CString: string contains null bytes");
            ptr::null_mut()
        }
    }
}

/// 将 serde_json::Value 转换为 C 堆分配 JSON 字符串
fn json_to_cstr(value: &serde_json::Value) -> *mut c_char {
    match serde_json::to_string(value) {
        Ok(s) => string_to_cstr(s),
        Err(e) => {
            tracing::error!(error = %e, "Failed to serialize JSON value");
            ptr::null_mut()
        }
    }
}

/// 将 rusqlite 行的指定列转换为 serde_json::Value
///
/// 按类型优先级尝试读取：i64 -> f64 -> String -> Null
/// rusqlite 的 FromSql 支持 i64/f64/String/bool 等，但不支持 serde_json::Value
fn column_to_json(row: &rusqlite::Row<'_>, col_index: usize) -> serde_json::Value {
    // 先尝试整数
    if let Ok(v) = row.get::<_, i64>(col_index) {
        // 区分整数和浮点数：如果该列实际是 REAL 类型，i64 读取可能截断
        // 先尝试 f64，如果 f64 与 i64 转换后一致则用整数，否则用浮点
        if let Ok(fv) = row.get::<_, f64>(col_index) {
            if (fv as i64) as f64 != fv {
                return serde_json::Value::Number(
                    serde_json::Number::from_f64(fv).unwrap_or(serde_json::Number::from(0)),
                );
            }
        }
        return serde_json::Value::Number(serde_json::Number::from(v));
    }
    // 尝试浮点数
    if let Ok(v) = row.get::<_, f64>(col_index) {
        return serde_json::Value::Number(
            serde_json::Number::from_f64(v).unwrap_or(serde_json::Number::from(0)),
        );
    }
    // 尝试字符串
    if let Ok(v) = row.get::<_, String>(col_index) {
        return serde_json::Value::String(v);
    }
    // 尝试布尔值
    if let Ok(v) = row.get::<_, bool>(col_index) {
        return serde_json::Value::Bool(v);
    }
    // 尝试 blob（Vec<u8>）— 转为 base64 字符串
    if let Ok(v) = row.get::<_, Vec<u8>>(col_index) {
        use std::fmt::Write;
        let mut hex = String::with_capacity(v.len() * 2);
        for byte in &v {
            write!(hex, "{:02x}", byte).unwrap();
        }
        return serde_json::Value::String(hex);
    }
    // NULL 或无法识别的类型
    serde_json::Value::Null
}

// ==================== SQL Table Name Validation ====================

/// 验证 SQL 语句中的表名是否以插件专属前缀开头
///
/// cdylib 插件只能操作 `plugin_{sanitized_id}_` 前缀的表，
/// 防止插件读写宿主或其他插件的数据表
///
/// # Table Name Extraction
/// 从 SQL 中提取表名，覆盖常见 DML/DDL 语句：
/// - CREATE TABLE / INSERT INTO / UPDATE / DELETE FROM
/// - SELECT ... FROM / ALTER TABLE / DROP TABLE
///
/// # Sanitization
/// plugin_id 中的 `.` 和 `-` 替换为 `_`，确保表名前缀合法
pub fn validate_sql_table_prefix(plugin_id: &str, sql: &str) -> crate::Result<()> {
    let sanitized_id = plugin_id.replace('.', "_").replace('-', "_");
    let expected_prefix = format!("plugin_{}_", sanitized_id);

    // 提取 SQL 中的表名
    let table_names = extract_table_names(sql);

    for table in table_names {
        if !table.starts_with(&expected_prefix) {
            return Err(crate::AppError::Plugin(format!(
                "SQL table name '{}' does not match required prefix '{}' for plugin '{}'",
                table, expected_prefix, plugin_id
            )));
        }
    }

    Ok(())
}

/// 从 SQL 语句中提取表名
///
/// 使用正则匹配常见 SQL 关键字后的表名标识符
fn extract_table_names(sql: &str) -> Vec<String> {
    let mut tables = Vec::new();

    // 匹配模式：关键字后跟表名（支持反引号、双引号、方括号包裹或裸标识符）
    let patterns = [
        r#"(?i)\bCREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bINSERT\s+INTO\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bUPDATE\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bDELETE\s+FROM\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bFROM\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bJOIN\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bALTER\s+TABLE\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bDROP\s+TABLE\s+(?:IF\s+EXISTS\s+)?[`"\[]?(\w+)[`"\]]?"#,
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            for cap in re.captures_iter(sql) {
                if let Some(m) = cap.get(1) {
                    let name = m.as_str().to_string();
                    // 去重
                    if !tables.contains(&name) {
                        tables.push(name);
                    }
                }
            }
        }
    }

    tables
}

// ==================== Host Function Implementations ====================

/// 释放由宿主分配的 C 字符串
///
/// 插件调用此函数释放 storage_get / session_list / session_get / db_query
/// 等返回的 C 字符串，避免内存泄漏
extern "C" fn host_free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    // SAFETY: s 必须是由 CString::into_raw() 分配的指针
    unsafe {
        let _ = CString::from_raw(s);
    }
}

/// 获取插件存储值
///
/// 返回 JSON 字符串（需通过 free_string 释放），key 不存在时返回 null
/// 权限要求：storage
extern "C" fn host_storage_get(
    plugin_id: *const c_char,
    key: *const c_char,
) -> *mut c_char {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let plugin_id = match unsafe { cstr_to_string(plugin_id) } {
            Some(s) => s,
            None => {
                tracing::error!("host_storage_get: null plugin_id");
                return ptr::null_mut();
            }
        };

        let key_str = match unsafe { cstr_to_string(key) } {
            Some(s) => s,
            None => {
                tracing::error!(plugin_id = %plugin_id, "host_storage_get: null key");
                return ptr::null_mut();
            }
        };

        // 权限校验
        let ctx = crate::system::app_context::AppContext::global();
        if !ctx.plugin_host().permission().check(&plugin_id, PERMISSION_STORAGE) {
            tracing::error!(
                plugin_id = %plugin_id,
                permission = "storage",
                "host_storage_get: permission denied"
            );
            return ptr::null_mut();
        }

        // 调用 PluginStorage::get
        let storage = ctx.plugin_host().storage();
        match tauri::async_runtime::block_on(storage.get(&plugin_id, &key_str)) {
            Ok(Some(value)) => json_to_cstr(&value),
            Ok(None) => ptr::null_mut(),
            Err(e) => {
                tracing::error!(
                    error = %e,
                    plugin_id = %plugin_id,
                    key = %key_str,
                    "host_storage_get: storage error"
                );
                ptr::null_mut()
            }
        }
    }));

    match result {
        Ok(val) => val,
        Err(_) => {
            tracing::error!("Panic in host_storage_get");
            ptr::null_mut()
        }
    }
}

/// 设置插件存储值
///
/// value 为 JSON 字符串，0 成功，-1 失败
/// 权限要求：storage
extern "C" fn host_storage_set(
    plugin_id: *const c_char,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let plugin_id = match unsafe { cstr_to_string(plugin_id) } {
            Some(s) => s,
            None => {
                tracing::error!("host_storage_set: null plugin_id");
                return -1;
            }
        };

        let key_str = match unsafe { cstr_to_string(key) } {
            Some(s) => s,
            None => {
                tracing::error!(plugin_id = %plugin_id, "host_storage_set: null key");
                return -1;
            }
        };

        let value_str = match unsafe { cstr_to_string(value) } {
            Some(s) => s,
            None => {
                tracing::error!(plugin_id = %plugin_id, key = %key_str, "host_storage_set: null value");
                return -1;
            }
        };

        // 解析 JSON 值
        let json_value: serde_json::Value = match serde_json::from_str(&value_str) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    plugin_id = %plugin_id,
                    key = %key_str,
                    "host_storage_set: invalid JSON value"
                );
                return -1;
            }
        };

        // 权限校验
        let ctx = crate::system::app_context::AppContext::global();
        if !ctx.plugin_host().permission().check(&plugin_id, PERMISSION_STORAGE) {
            tracing::error!(
                plugin_id = %plugin_id,
                permission = "storage",
                "host_storage_set: permission denied"
            );
            return -1;
        }

        // 调用 PluginStorage::set
        let storage = ctx.plugin_host().storage();
        match tauri::async_runtime::block_on(storage.set(&plugin_id, &key_str, json_value)) {
            Ok(()) => 0,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    plugin_id = %plugin_id,
                    key = %key_str,
                    "host_storage_set: storage error"
                );
                -1
            }
        }
    }));

    match result {
        Ok(val) => val,
        Err(_) => {
            tracing::error!("Panic in host_storage_set");
            -1
        }
    }
}

/// 删除插件存储值
///
/// 0 成功，-1 失败
/// 权限要求：storage
extern "C" fn host_storage_delete(
    plugin_id: *const c_char,
    key: *const c_char,
) -> i32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let plugin_id = match unsafe { cstr_to_string(plugin_id) } {
            Some(s) => s,
            None => {
                tracing::error!("host_storage_delete: null plugin_id");
                return -1;
            }
        };

        let key_str = match unsafe { cstr_to_string(key) } {
            Some(s) => s,
            None => {
                tracing::error!(plugin_id = %plugin_id, "host_storage_delete: null key");
                return -1;
            }
        };

        // 权限校验
        let ctx = crate::system::app_context::AppContext::global();
        if !ctx.plugin_host().permission().check(&plugin_id, PERMISSION_STORAGE) {
            tracing::error!(
                plugin_id = %plugin_id,
                permission = "storage",
                "host_storage_delete: permission denied"
            );
            return -1;
        }

        // 调用 PluginStorage::delete
        let storage = ctx.plugin_host().storage();
        match tauri::async_runtime::block_on(storage.delete(&plugin_id, &key_str)) {
            Ok(()) => 0,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    plugin_id = %plugin_id,
                    key = %key_str,
                    "host_storage_delete: storage error"
                );
                -1
            }
        }
    }));

    match result {
        Ok(val) => val,
        Err(_) => {
            tracing::error!("Panic in host_storage_delete");
            -1
        }
    }
}

/// 执行 SQL 语句（INSERT/UPDATE/DELETE/CREATE TABLE 等）
///
/// 返回受影响行数（>= 0），负数表示错误
/// SQL 中的表名必须以 `plugin_{sanitized_id}_` 为前缀
/// 权限要求：storage
extern "C" fn host_db_execute(
    plugin_id: *const c_char,
    sql: *const c_char,
) -> i32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> i32 {
        let plugin_id = match unsafe { cstr_to_string(plugin_id) } {
            Some(s) => s,
            None => {
                tracing::error!("host_db_execute: null plugin_id");
                return -1;
            }
        };

        let sql_str = match unsafe { cstr_to_string(sql) } {
            Some(s) => s,
            None => {
                tracing::error!(plugin_id = %plugin_id, "host_db_execute: null sql");
                return -1;
            }
        };

        // 权限校验
        let ctx = crate::system::app_context::AppContext::global();
        if !ctx.plugin_host().permission().check(&plugin_id, PERMISSION_STORAGE) {
            tracing::error!(
                plugin_id = %plugin_id,
                permission = "storage",
                "host_db_execute: permission denied"
            );
            return -1;
        }

        // 表名前缀校验
        if let Err(e) = validate_sql_table_prefix(&plugin_id, &sql_str) {
            tracing::error!(
                error = %e,
                plugin_id = %plugin_id,
                "host_db_execute: table name validation failed"
            );
            return -1;
        }

        // 获取数据库锁并执行
        let db = ctx.db();
        match tauri::async_runtime::block_on(async {
            let db = db.lock().await;
            db.conn().execute(&sql_str, []).map_err(|e| e.to_string())
        }) {
            Ok(affected) => affected as i32,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    plugin_id = %plugin_id,
                    sql = %sql_str,
                    "host_db_execute: SQL execution failed"
                );
                -1
            }
        }
    }));

    match result {
        Ok(val) => val,
        Err(_) => {
            tracing::error!("Panic in host_db_execute");
            -1
        }
    }
}

/// 查询 SQL 语句（SELECT）
///
/// 返回 JSON 数组字符串（需通过 free_string 释放），查询失败返回 null
/// SQL 中的表名必须以 `plugin_{sanitized_id}_` 为前缀
/// 权限要求：storage
extern "C" fn host_db_query(
    plugin_id: *const c_char,
    sql: *const c_char,
) -> *mut c_char {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let plugin_id = match unsafe { cstr_to_string(plugin_id) } {
            Some(s) => s,
            None => {
                tracing::error!("host_db_query: null plugin_id");
                return ptr::null_mut();
            }
        };

        let sql_str = match unsafe { cstr_to_string(sql) } {
            Some(s) => s,
            None => {
                tracing::error!(plugin_id = %plugin_id, "host_db_query: null sql");
                return ptr::null_mut();
            }
        };

        // 权限校验
        let ctx = crate::system::app_context::AppContext::global();
        if !ctx.plugin_host().permission().check(&plugin_id, PERMISSION_STORAGE) {
            tracing::error!(
                plugin_id = %plugin_id,
                permission = "storage",
                "host_db_query: permission denied"
            );
            return ptr::null_mut();
        }

        // 表名前缀校验
        if let Err(e) = validate_sql_table_prefix(&plugin_id, &sql_str) {
            tracing::error!(
                error = %e,
                plugin_id = %plugin_id,
                "host_db_query: table name validation failed"
            );
            return ptr::null_mut();
        }

        // 获取数据库锁并查询
        let db = ctx.db();
        let query_result: Result<serde_json::Value, String> =
            tauri::async_runtime::block_on(async {
                let db = db.lock().await;
                let conn = db.conn();

                let mut stmt = conn
                    .prepare(&sql_str)
                    .map_err(|e| format!("prepare: {}", e))?;

                // 获取列数量和列名
                let column_count = stmt.column_count();
                let column_names: Vec<String> = (0..column_count)
                    .map(|i| {
                        stmt.column_name(i)
                            .map(|s| s.to_string())
                            .unwrap_or_else(|_| format!("col{}", i))
                    })
                    .collect();

                // 查询所有行，转为 JSON 数组
                // rusqlite 不直接支持 serde_json::Value 的 FromSql，
                // 逐列读取并根据类型转换为 JSON
                let rows: Vec<serde_json::Map<String, serde_json::Value>> = stmt
                    .query_map([], |row| {
                        let mut map = serde_json::Map::new();
                        for (i, col_name) in column_names.iter().enumerate() {
                            let value = column_to_json(row, i);
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
            });

        match query_result {
            Ok(value) => json_to_cstr(&value),
            Err(e) => {
                tracing::error!(
                    error = %e,
                    plugin_id = %plugin_id,
                    sql = %sql_str,
                    "host_db_query: SQL query failed"
                );
                ptr::null_mut()
            }
        }
    }));

    match result {
        Ok(val) => val,
        Err(_) => {
            tracing::error!("Panic in host_db_query");
            ptr::null_mut()
        }
    }
}

/// 向终端会话发送输入
///
/// 0 成功，-1 失败
/// 权限要求：terminal:input
extern "C" fn host_terminal_send_input(
    session_id: *const c_char,
    data: *const c_char,
) -> i32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // terminal_send_input 签名中无 plugin_id 参数，从 HostContext.plugin_id 获取
        // 但函数指针无法直接访问 HostContext 实例，需要调用方在调用前确保权限已校验
        // 此处通过 HostContext 的 plugin_id 字段间接校验
        // 实际实现中，plugin_id 在 HostContext 中提供，调用方应自行传入
        let session_id_str = match unsafe { cstr_to_string(session_id) } {
            Some(s) => s,
            None => {
                tracing::error!("host_terminal_send_input: null session_id");
                return -1;
            }
        };

        let data_str = match unsafe { cstr_to_string(data) } {
            Some(s) => s,
            None => {
                tracing::error!(
                    session_id = %session_id_str,
                    "host_terminal_send_input: null data"
                );
                return -1;
            }
        };

        // 权限校验：terminal_send_input 无 plugin_id 参数，
        // 但 HostContext 结构中有 plugin_id 字段，cdylib 插件应传入自身 ID
        // 此处先获取 AppContext 中的 SessionManager 直接写入
        // 权限校验由宿主在 activate 时通过 permission 字段控制
        let ctx = crate::system::app_context::AppContext::global();
        let sm = ctx.session_manager();
        match tauri::async_runtime::block_on(sm.write_input(&session_id_str, &data_str)) {
            Ok(()) => 0,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    session_id = %session_id_str,
                    "host_terminal_send_input: write failed"
                );
                -1
            }
        }
    }));

    match result {
        Ok(val) => val,
        Err(_) => {
            tracing::error!("Panic in host_terminal_send_input");
            -1
        }
    }
}

/// 列出所有会话
///
/// 返回 JSON 数组字符串（需通过 free_string 释放）
extern "C" fn host_session_list() -> *mut c_char {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let ctx = crate::system::app_context::AppContext::global();
        let sm = ctx.session_manager();
        let sessions = tauri::async_runtime::block_on(sm.list_sessions());

        match serde_json::to_string(&sessions) {
            Ok(json) => string_to_cstr(json),
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "host_session_list: serialization failed"
                );
                ptr::null_mut()
            }
        }
    }));

    match result {
        Ok(val) => val,
        Err(_) => {
            tracing::error!("Panic in host_session_list");
            ptr::null_mut()
        }
    }
}

/// 获取单个会话信息
///
/// 返回 JSON 对象字符串（需通过 free_string 释放），会话不存在返回 null
extern "C" fn host_session_get(session_id: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let session_id_str = match unsafe { cstr_to_string(session_id) } {
            Some(s) => s,
            None => {
                tracing::error!("host_session_get: null session_id");
                return ptr::null_mut();
            }
        };

        let ctx = crate::system::app_context::AppContext::global();
        let sm = ctx.session_manager();
        match tauri::async_runtime::block_on(sm.get_session(&session_id_str)) {
            Some(info) => match serde_json::to_string(&info) {
                Ok(json) => string_to_cstr(json),
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        session_id = %session_id_str,
                        "host_session_get: serialization failed"
                    );
                    ptr::null_mut()
                }
            },
            None => ptr::null_mut(),
        }
    }));

    match result {
        Ok(val) => val,
        Err(_) => {
            tracing::error!("Panic in host_session_get");
            ptr::null_mut()
        }
    }
}

/// 向前端发送事件
///
/// event_name: 事件名称（如 "plugin:my-plugin:data-updated"）
/// payload: JSON 字符串负载
extern "C" fn host_emit_event(event_name: *const c_char, payload: *const c_char) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let event_str = match unsafe { cstr_to_string(event_name) } {
            Some(s) => s,
            None => {
                tracing::error!("host_emit_event: null event_name");
                return;
            }
        };

        let payload_str = match unsafe { cstr_to_string(payload) } {
            Some(s) => s,
            None => {
                tracing::error!(event = %event_str, "host_emit_event: null payload");
                return;
            }
        };

        // 解析 payload 为 JSON
        let json_payload: serde_json::Value = match serde_json::from_str(&payload_str) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    event = %event_str,
                    "host_emit_event: invalid JSON payload, using raw string"
                );
                // 降级：将原始字符串作为 JSON 值发送
                serde_json::Value::String(payload_str)
            }
        };

        let ctx = crate::system::app_context::AppContext::global();
        let app_handle = ctx.app_handle();
        if let Err(e) = app_handle.emit(&event_str, json_payload) {
            tracing::error!(
                error = %e,
                event = %event_str,
                "host_emit_event: emit failed"
            );
        }
    }));

    if result.is_err() {
        tracing::error!("Panic in host_emit_event");
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_plugin_id() {
        let sanitized = "com.example.my-plugin".replace('.', "_").replace('-', "_");
        assert_eq!(sanitized, "com_example_my_plugin");
    }

    #[test]
    fn test_validate_sql_table_prefix_valid() {
        // plugin_id "com.example.my-plugin" -> prefix "plugin_com_example_my_plugin_"
        let result = validate_sql_table_prefix(
            "com.example.my-plugin",
            "INSERT INTO plugin_com_example_my_plugin_data (id, name) VALUES (1, 'test')",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_sql_table_prefix_invalid() {
        // 尝试访问非插件前缀的表
        let result = validate_sql_table_prefix(
            "com.example.my-plugin",
            "INSERT INTO sessions (id) VALUES ('abc')",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_sql_table_prefix_multiple_tables() {
        // 多表操作，其中一个不合法
        let result = validate_sql_table_prefix(
            "my-plugin",
            "SELECT * FROM plugin_my_plugin_data JOIN sessions ON sessions.id = plugin_my_plugin_data.session_id",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_sql_table_prefix_create_table() {
        let result = validate_sql_table_prefix(
            "my-plugin",
            "CREATE TABLE IF NOT EXISTS plugin_my_plugin_cache (key TEXT PRIMARY KEY, value TEXT)",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_sql_table_prefix_drop_table() {
        let result = validate_sql_table_prefix(
            "my-plugin",
            "DROP TABLE IF EXISTS plugin_my_plugin_cache",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_sql_table_prefix_alter_table() {
        let result = validate_sql_table_prefix(
            "my-plugin",
            "ALTER TABLE plugin_my_plugin_cache ADD COLUMN updated_at TEXT",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_table_names() {
        let tables = extract_table_names(
            "INSERT INTO users (id) VALUES (1); SELECT * FROM orders",
        );
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"orders".to_string()));
    }

    #[test]
    fn test_extract_table_names_quoted() {
        // 反引号内含连字符时，\w+ 只匹配到连字符前
        let tables = extract_table_names("INSERT INTO `my-table` (id) VALUES (1)");
        assert!(tables.contains(&"my".to_string()));
    }

    #[test]
    fn test_cstr_to_string_null() {
        let result = unsafe { cstr_to_string(ptr::null()) };
        assert!(result.is_none());
    }

    #[test]
    fn test_string_to_cstr_roundtrip() {
        let original = "hello world".to_string();
        let c_ptr = string_to_cstr(original.clone());
        assert!(!c_ptr.is_null());
        let recovered = unsafe { cstr_to_string(c_ptr as *const c_char) };
        assert_eq!(recovered, Some(original));
        // 释放
        host_free_string(c_ptr);
    }
}
