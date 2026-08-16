//! host_db_* — 插件 SQLite 访问（逻辑层）

use crate::plugin::wasm_host;
use super::super::WasmPluginState;

/// 逻辑层：执行 SQL（表名前缀校验不变量保留）
pub(crate) fn db_execute(state: &WasmPluginState, sql: &str) -> Result<u32, String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE)
    {
        return Err("permission denied: storage".to_string());
    }

    // 表名前缀校验（跨插件数据隔离不变量）
    wasm_host::validate_sql_table_prefix(&state.plugin_id, sql)
        .map_err(|e| format!("table name validation failed: {}", e))?;

    let db = state.host_ctx.db.clone();
    // 作用域收窄 guard 生命周期；poison 容忍（panic 截获后锁中毒不连锁 panic）
    let affected = {
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(sql, [])
            .map_err(|e| format!("SQL execution failed: {}", e))?
    };
    Ok(affected as u32)
}

/// 逻辑层：查询 SQL（返回行集 JSON 字符串；表名前缀校验不变量保留）
pub(crate) fn db_query(state: &WasmPluginState, sql: &str) -> Result<Option<String>, String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE)
    {
        return Err("permission denied: storage".to_string());
    }

    wasm_host::validate_sql_table_prefix(&state.plugin_id, sql)
        .map_err(|e| format!("table name validation failed: {}", e))?;

    let db = state.host_ctx.db.clone();
    let query_result: Result<serde_json::Value, String> = (|| {
        // poison 容忍：host fn 内 panic 被截获后锁会中毒，不能连锁 panic
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());

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

    query_result.map(|value| {
        serde_json::to_string(&value).map_err(|e| format!("JSON serialization failed: {}", e))
    })?
    .map(Some)
}
