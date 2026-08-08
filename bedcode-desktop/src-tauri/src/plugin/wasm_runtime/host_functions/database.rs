//! 数据库域 Host Functions（主库前缀隔离 + 插件独立库）
//!
//! 含 SQL 表名前缀校验与 rusqlite 列 → JSON 转换辅助

use super::memory::{read_wasm_string_consume, write_result_to_out_ptr, write_wasm_string};
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext, WasmPluginState};
use crate::plugin::permission::PERMISSION_STORAGE;
use regex::Regex;

// ==================== 逻辑层（core 胶水与 Component Model 绑定共用） ====================

/// 解析参数绑定 JSON 数组字符串（空串视为空数组）
fn parse_params_json(params_json: &str) -> Result<Vec<serde_json::Value>, String> {
    if params_json.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(params_json)
        .map_err(|e| format!("invalid params JSON array: {}", e))
}

/// 逻辑层：主库执行 SQL（权限 + 表名前缀校验），返回受影响行数
pub(crate) fn db_execute(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    sql: &str,
) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_db_execute") {
        return Err("permission denied".to_string());
    }
    validate_sql_table_prefix(plugin_id, sql).map_err(|e| e.to_string())?;
    let db = host_ctx.db.clone();
    block_on_async(async {
        let db = db.lock().await;
        db.conn().execute(sql, []).map_err(|e| e.to_string())
    })
    .map(|affected| affected as u32)
    .map_err(|e| format!("database error: {}", e))
}

/// 逻辑层：主库查询（权限 + 表名前缀校验），返回行数组 JSON 字符串
pub(crate) fn db_query(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    sql: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_db_query") {
        return Err("permission denied".to_string());
    }
    validate_sql_table_prefix(plugin_id, sql).map_err(|e| e.to_string())?;
    let db = host_ctx.db.clone();
    let value = block_on_async(async {
        let db = db.lock().await;
        query_to_json(db.conn(), sql)
    })
    .map_err(|e| format!("database error: {}", e))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {}", e))
}

/// 逻辑层：插件独立库执行 SQL（权限校验，无表名前缀校验）
pub(crate) fn plugin_db_execute(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    sql: &str,
) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_plugin_db_execute") {
        return Err("permission denied".to_string());
    }
    block_on_async(async {
        let db_arc = host_ctx.get_or_create_plugin_db(plugin_id).await.map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        db.conn().execute(sql, []).map(|n| n as u32).map_err(|e| e.to_string())
    })
    .map_err(|e| format!("database error: {}", e))
}

/// 逻辑层：插件独立库查询（权限校验，无表名前缀校验）
pub(crate) fn plugin_db_query(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    sql: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_plugin_db_query") {
        return Err("permission denied".to_string());
    }
    let value = block_on_async(async {
        let db_arc = host_ctx.get_or_create_plugin_db(plugin_id).await.map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        query_to_json(db.conn(), sql)
    })
    .map_err(|e| format!("database error: {}", e))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {}", e))
}

/// 逻辑层：主库执行参数绑定 SQL（权限 + 表名前缀校验）
pub(crate) fn db_execute_params(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    sql: &str,
    params_json: &str,
) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_db_execute_params") {
        return Err("permission denied".to_string());
    }
    validate_sql_table_prefix(plugin_id, sql).map_err(|e| e.to_string())?;
    let params = parse_params_json(params_json)?;
    let db = host_ctx.db.clone();
    block_on_async(async {
        let db = db.lock().await;
        execute_with_params(db.conn(), sql, &params)
    })
    .map(|affected| affected as u32)
    .map_err(|e| format!("database error: {}", e))
}

/// 逻辑层：主库参数绑定查询（权限 + 表名前缀校验）
pub(crate) fn db_query_params(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    sql: &str,
    params_json: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_db_query_params") {
        return Err("permission denied".to_string());
    }
    validate_sql_table_prefix(plugin_id, sql).map_err(|e| e.to_string())?;
    let params = parse_params_json(params_json)?;
    let db = host_ctx.db.clone();
    let value = block_on_async(async {
        let db = db.lock().await;
        query_with_params_to_json(db.conn(), sql, &params)
    })
    .map_err(|e| format!("database error: {}", e))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {}", e))
}

/// 逻辑层：插件独立库执行参数绑定 SQL（权限校验，无表名前缀校验）
pub(crate) fn plugin_db_execute_params(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    sql: &str,
    params_json: &str,
) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_plugin_db_execute_params") {
        return Err("permission denied".to_string());
    }
    let params = parse_params_json(params_json)?;
    block_on_async(async {
        let db_arc = host_ctx.get_or_create_plugin_db(plugin_id).await.map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        execute_with_params(db.conn(), sql, &params).map(|n| n as u32)
    })
    .map_err(|e| format!("database error: {}", e))
}

/// 逻辑层：插件独立库参数绑定查询（权限校验，无表名前缀校验）
pub(crate) fn plugin_db_query_params(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    sql: &str,
    params_json: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_plugin_db_query_params") {
        return Err("permission denied".to_string());
    }
    let params = parse_params_json(params_json)?;
    let value = block_on_async(async {
        let db_arc = host_ctx.get_or_create_plugin_db(plugin_id).await.map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        query_with_params_to_json(db.conn(), sql, &params)
    })
    .map_err(|e| format!("database error: {}", e))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {}", e))
}

// ==================== Host Functions（core module 胶水） ====================

/// 数据库：执行 SQL
///
/// 参数：(sql_ptr, sql_len)
/// 返回：受影响行数（>= 0），负数表示错误
pub(super) fn host_db_execute(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let sql = match read_wasm_string_consume(&mut caller, sql_ptr, sql_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_db_execute: failed to read SQL");
            return -1;
        }
    };

    match db_execute(&host_ctx, &plugin_id, &sql) {
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
pub(super) fn host_db_query(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let sql = match read_wasm_string_consume(&mut caller, sql_ptr, sql_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_db_query: failed to read SQL");
            return -1;
        }
    };

    let query_result = db_query(&host_ctx, &plugin_id, &sql);

    write_query_result(&mut caller, query_result, &plugin_id, &sql, out_ptr)
}

/// 插件独立数据库：执行 SQL
///
/// 参数：(sql_ptr, sql_len)
/// 返回：受影响行数（>= 0），负数表示错误
///
/// 与 host_db_execute 的区别：
/// - 使用插件独立数据库连接（无全局 Mutex 竞争）
/// - 无表名前缀校验（整个数据库都是插件的）
pub(super) fn host_plugin_db_execute(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let sql = match read_wasm_string_consume(&mut caller, sql_ptr, sql_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_plugin_db_execute: failed to read SQL");
            return -1;
        }
    };

    match plugin_db_execute(&host_ctx, &plugin_id, &sql) {
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
pub(super) fn host_plugin_db_query(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let sql = match read_wasm_string_consume(&mut caller, sql_ptr, sql_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_plugin_db_query: failed to read SQL");
            return -1;
        }
    };

    let query_result = plugin_db_query(&host_ctx, &plugin_id, &sql);

    write_query_result(&mut caller, query_result, &plugin_id, &sql, out_ptr)
}

/// 数据库：执行 SQL 参数绑定版
///
/// 参数：(sql_ptr, sql_len, params_ptr, params_len)
/// 返回：受影响行数（>= 0），负数表示错误
///
/// params 为 JSON 数组字符串（如 `["abc", 42, true, null]`），
/// 按序绑定到 SQL 中的 `?1`、`?2` …（或 `?`）占位符，rusqlite 真绑定防注入
pub(super) fn host_db_execute_params(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
    params_ptr: u32,
    params_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let (sql, params) = match read_sql_and_params(&mut caller, &plugin_id, "host_db_execute_params", sql_ptr, sql_len, params_ptr, params_len) {
        Ok(v) => v,
        Err(code) => return code,
    };

    match db_execute_params(&host_ctx, &plugin_id, &sql, &params) {
        Ok(affected) => affected as i32,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, sql = %sql, "host_db_execute_params: SQL execution failed");
            -1
        }
    }
}

/// 数据库：查询 SQL 参数绑定版
///
/// 参数：(sql_ptr, sql_len, params_ptr, params_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
pub(super) fn host_db_query_params(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
    params_ptr: u32,
    params_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let (sql, params) = match read_sql_and_params(&mut caller, &plugin_id, "host_db_query_params", sql_ptr, sql_len, params_ptr, params_len) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let query_result = db_query_params(&host_ctx, &plugin_id, &sql, &params);

    write_query_result(&mut caller, query_result, &plugin_id, &sql, out_ptr)
}

/// 插件独立数据库：执行 SQL 参数绑定版
///
/// 参数：(sql_ptr, sql_len, params_ptr, params_len)
/// 返回：受影响行数（>= 0），负数表示错误（无表名前缀校验）
pub(super) fn host_plugin_db_execute_params(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
    params_ptr: u32,
    params_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let (sql, params) = match read_sql_and_params(&mut caller, &plugin_id, "host_plugin_db_execute_params", sql_ptr, sql_len, params_ptr, params_len) {
        Ok(v) => v,
        Err(code) => return code,
    };

    match plugin_db_execute_params(&host_ctx, &plugin_id, &sql, &params) {
        Ok(affected) => affected as i32,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, sql = %sql, "host_plugin_db_execute_params: SQL execution failed");
            -1
        }
    }
}

/// 插件独立数据库：查询 SQL 参数绑定版
///
/// 参数：(sql_ptr, sql_len, params_ptr, params_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
pub(super) fn host_plugin_db_query_params(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sql_ptr: u32,
    sql_len: u32,
    params_ptr: u32,
    params_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let (sql, params) = match read_sql_and_params(&mut caller, &plugin_id, "host_plugin_db_query_params", sql_ptr, sql_len, params_ptr, params_len) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let query_result = plugin_db_query_params(&host_ctx, &plugin_id, &sql, &params);

    write_query_result(&mut caller, query_result, &plugin_id, &sql, out_ptr)
}

// ==================== Shared Query Helpers ====================

/// 从 WASM 内存读取 SQL 与参数 JSON 字符串（4 个 params 版 host function 共用）
///
/// 仅负责内存读取，JSON 解析与绑定在逻辑层（`db_execute_params` 等）完成
fn read_sql_and_params(
    caller: &mut wasmtime::Caller<'_, WasmPluginState>,
    plugin_id: &str,
    api: &str,
    sql_ptr: u32,
    sql_len: u32,
    params_ptr: u32,
    params_len: u32,
) -> Result<(String, String), i32> {
    let sql = match read_wasm_string_consume(caller, sql_ptr, sql_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "{}: failed to read SQL", api);
            return Err(-1);
        }
    };
    let params_str = match read_wasm_string_consume(caller, params_ptr, params_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "{}: failed to read params", api);
            return Err(-1);
        }
    };
    Ok((sql, params_str))
}

/// 将 JSON 参数绑定到预编译语句（1-based 索引，rusqlite 真绑定，防注入）
fn bind_json_params(
    stmt: &mut rusqlite::Statement<'_>,
    params: &[serde_json::Value],
) -> rusqlite::Result<()> {
    for (i, p) in params.iter().enumerate() {
        let idx = i + 1;
        match p {
            serde_json::Value::Null => stmt.raw_bind_parameter(idx, rusqlite::types::Null)?,
            serde_json::Value::Bool(b) => stmt.raw_bind_parameter(idx, *b)?,
            serde_json::Value::Number(n) => {
                // 整数优先；非整数按浮点绑定
                if let Some(iv) = n.as_i64() {
                    stmt.raw_bind_parameter(idx, iv)?;
                } else {
                    stmt.raw_bind_parameter(idx, n.as_f64().unwrap_or(0.0))?;
                }
            }
            serde_json::Value::String(s) => stmt.raw_bind_parameter(idx, s.as_str())?,
            // 数组/对象 fallback：序列化为 JSON 字符串存储
            other => stmt.raw_bind_parameter(idx, serde_json::to_string(other).unwrap_or_default())?,
        }
    }
    Ok(())
}

/// 执行参数绑定 SQL，返回受影响行数
fn execute_with_params(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[serde_json::Value],
) -> Result<usize, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
    bind_json_params(&mut stmt, params).map_err(|e| format!("bind: {}", e))?;
    stmt.raw_execute().map_err(|e| format!("execute: {}", e))
}

/// 参数绑定查询 → JSON 行数组
fn query_with_params_to_json(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[serde_json::Value],
) -> Result<serde_json::Value, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
    bind_json_params(&mut stmt, params).map_err(|e| format!("bind: {}", e))?;

    let column_count = stmt.column_count();
    let column_names: Vec<String> = (0..column_count)
        .map(|i| {
            stmt.column_name(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| format!("col{}", i))
        })
        .collect();

    let mut rows_out: Vec<serde_json::Value> = Vec::new();
    let mut rows = stmt.raw_query();
    while let Some(row) = rows.next().map_err(|e| format!("next: {}", e))? {
        let mut map = serde_json::Map::new();
        for (i, col_name) in column_names.iter().enumerate() {
            map.insert(col_name.clone(), column_to_json(row, i));
        }
        rows_out.push(serde_json::Value::Object(map));
    }

    Ok(serde_json::Value::Array(rows_out))
}

/// 执行查询并将结果集转换为 JSON 行数组
///
/// 主库与插件库查询共用，消除原先两份重复的列名提取 + query_map 逻辑
fn query_to_json(
    conn: &rusqlite::Connection,
    sql: &str,
) -> Result<serde_json::Value, String> {
    let mut stmt = conn
        .prepare(sql)
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
}

/// 将查询结果 JSON 写入 WASM 线性内存（主库与插件库查询共用出口）
fn write_query_result(
    caller: &mut wasmtime::Caller<'_, WasmPluginState>,
    query_result: Result<Option<String>, String>,
    plugin_id: &str,
    sql: &str,
    out_ptr: u32,
) -> i32 {
    match query_result {
        Ok(Some(json_str)) => match write_wasm_string(caller, &json_str) {
            Some((ptr, len)) => write_result_to_out_ptr(caller, out_ptr, ptr, len),
            None => {
                tracing::error!(plugin_id = %plugin_id, "db_query: failed to write result to WASM memory");
                -1
            }
        },
        Ok(None) => write_result_to_out_ptr(caller, out_ptr, 0, 0),
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, sql = %sql, "db_query: SQL query failed");
            -1
        }
    }
}

// ==================== SQL Table Name Validation ====================

/// 验证 SQL 语句中的表名是否以插件专属前缀开头
///
/// WASM 插件只能操作 `plugin_{sanitized_id}_` 前缀的表，
/// 防止插件读写宿主或其他插件的数据表
///
/// # Table Name Extraction
/// 从 SQL 中提取表名，覆盖常见 DML/DDL 语句：
/// - CREATE TABLE / INSERT INTO / UPDATE / DELETE FROM
/// - SELECT ... FROM / ALTER TABLE / DROP TABLE
///
/// # Sanitization
/// plugin_id 中的 `.` 和 `-` 替换为 `_`，确保表名前缀合法
fn validate_sql_table_prefix(plugin_id: &str, sql: &str) -> crate::Result<()> {
    let sanitized_id = plugin_id.replace('.', "_").replace('-', "_");
    let expected_prefix = format!("plugin_{}_", sanitized_id);

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
                    if !tables.contains(&name) {
                        tables.push(name);
                    }
                }
            }
        }
    }

    tables
}

// ==================== Database Column Conversion ====================

/// 将 rusqlite 行的指定列转换为 serde_json::Value
///
/// 按类型优先级尝试读取：i64 -> f64 -> String -> bool -> blob -> Null
/// rusqlite 的 FromSql 支持 i64/f64/String/bool 等，但不支持 serde_json::Value
fn column_to_json(row: &rusqlite::Row<'_>, col_index: usize) -> serde_json::Value {
    // 先尝试整数
    if let Ok(v) = row.get::<_, i64>(col_index) {
        // 区分整数和浮点数：如果该列实际是 REAL 类型，i64 读取可能截断
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
    // 尝试 blob（Vec<u8>）— 转为 hex 字符串
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
        let result = validate_sql_table_prefix(
            "com.example.my-plugin",
            "INSERT INTO plugin_com_example_my_plugin_data (id, name) VALUES (1, 'test')",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_sql_table_prefix_invalid() {
        let result = validate_sql_table_prefix(
            "com.example.my-plugin",
            "INSERT INTO sessions (id) VALUES ('abc')",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_sql_table_prefix_multiple_tables() {
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
        let tables = extract_table_names("INSERT INTO `my-table` (id) VALUES (1)");
        assert!(tables.contains(&"my".to_string()));
    }
}
