//! host_db_* — 插件 SQLite 访问

use crate::plugin::wasm_host;
use super::super::WasmPluginState;
use super::support::{guarded_host_call, has_permission, read_wasm_string, write_result_to_out_ptr, write_wasm_string};

pub(crate) fn host_db_execute(
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

pub(crate) fn host_db_query(
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
