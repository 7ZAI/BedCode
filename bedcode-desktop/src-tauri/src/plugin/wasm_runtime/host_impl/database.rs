//! 数据库域宿主实现（主库前缀隔离 + 插件独立库）
//!
//! 含 SQL 表名前缀校验与 rusqlite 列 → JSON 转换辅助

use crate::plugin::permission::PERMISSION_STORAGE;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};
use regex::Regex;

// ==================== 逻辑层（Component Model 绑定调用） ====================

/// 解析参数绑定 JSON 数组字符串（空串视为空数组）
fn parse_params_json(params_json: &str) -> Result<Vec<serde_json::Value>, String> {
    if params_json.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(params_json)
        .map_err(|e| format!("invalid params JSON array: {}", e))
}

/// 主库执行 SQL（权限 + 表名前缀校验），返回受影响行数
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

/// 主库查询（权限 + 表名前缀校验），返回行数组 JSON 字符串
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

/// 插件独立库执行 SQL（权限校验，无表名前缀校验）
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

/// 插件独立库查询（权限校验，无表名前缀校验）
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

/// 主库执行参数绑定 SQL（权限 + 表名前缀校验）
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

/// 主库参数绑定查询（权限 + 表名前缀校验）
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

/// 插件独立库执行参数绑定 SQL（权限校验，无表名前缀校验）
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

/// 插件独立库参数绑定查询（权限校验，无表名前缀校验）
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

// ==================== 参数绑定辅助 ====================

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
