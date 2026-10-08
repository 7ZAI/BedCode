//! host_db_* — 插件 SQLite 访问（host-database 主库 5 函数 + host-plugin-database
//! 插件私有库 5 函数；票 05 对齐桌面 13 原语）
//!
//! 与桌面端同构（`bedcode-wasm-core::host_api::database.rs` 移植，去掉端口层）：
//! - 主库：表名前缀纵深（`plugin_{sanitized_id}_`，跨插件隔离不变量）+ 结果集
//!   护栏（行数/字节双上限）+ execute-batch 事务封装（裸事务控制拒绝）；
//! - 插件私有库：每插件属主分区（独立 SQLite 库，惰性打开 + 停用回收）；
//! - 统一权限门 `PERMISSION_STORAGE`（移动端现状：host-database 与 host-storage
//!   同权限位，与桌面 check_permission 同语义）。
//!
//! **实现差异点名**：① 表名前缀校验用正则提取（桌面为 SQLite authorizer 回调——
//! 语义护栏等价，强度弱于 authorizer，见 wasm_host::validate_sql_table_prefix）；
//! ② 无语句超时护栏（rusqlite 未开 hooks feature，桌面 progress handler 同款
//! 未移植——护栏缺口留 D1 审计项，SQL 仍由宿主连接 Mutex 串行，无并发风险）。

use super::super::WasmPluginState;
use crate::plugin::wasm_host;
use std::sync::{Arc, Mutex};

/// 结果集护栏：行数 / 字节双上限（与桌面 bedcode-server-base::constants 同值）
const QUERY_MAX_ROWS: usize = 10_000;
const QUERY_MAX_BYTES: usize = 32 * 1024 * 1024;
/// execute-batch 语句数上限
const EXECUTE_BATCH_MAX_STATEMENTS: usize = 64;

/// 统一权限门：storage（host-database 与 host-plugin-database 同门）
fn require_storage_permission(state: &WasmPluginState) -> Result<(), String> {
    if state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE)
    {
        Ok(())
    } else {
        Err("permission denied: storage".to_string())
    }
}

/// 主库表名前缀校验（跨插件数据隔离不变量；表名前缀纵深，票 05 与桌面同语义）
fn validate_main_prefix(state: &WasmPluginState, sql: &str) -> Result<(), String> {
    wasm_host::validate_sql_table_prefix(&state.plugin_id, sql)
        .map_err(|e| format!("table name validation failed: {e}"))
}

// ==================== 主库（host-database 5 函数） ====================

/// 执行 SQL，返回受影响行数
pub(crate) fn db_execute(state: &WasmPluginState, sql: &str) -> Result<u32, String> {
    require_storage_permission(state)?;
    reject_bare_transaction_control(sql)?;
    validate_main_prefix(state, sql)?;
    let db = state.host_ctx.db.clone();
    let affected = {
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(sql, [])
            .map_err(|e| format!("SQL execution failed: {e}"))?
    };
    Ok(affected as u32)
}

/// 查询 SQL，返回行集 JSON 字符串（含结果集护栏：行数/字节双上限）
pub(crate) fn db_query(state: &WasmPluginState, sql: &str) -> Result<Option<String>, String> {
    require_storage_permission(state)?;
    validate_main_prefix(state, sql)?;
    let db = state.host_ctx.db.clone();
    let value = {
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());
        query_to_json(&state.plugin_id, &conn, sql)?
    };
    serde_json::to_string(&value).map(Some).map_err(|e| format!("JSON serialization failed: {e}"))
}

/// 参数化执行（params-json 为 JSON 值数组，按序绑定）
pub(crate) fn db_execute_params(state: &WasmPluginState, sql: &str, params_json: &str) -> Result<u32, String> {
    require_storage_permission(state)?;
    reject_bare_transaction_control(sql)?;
    validate_main_prefix(state, sql)?;
    let params = parse_params_json(params_json)?;
    let db = state.host_ctx.db.clone();
    let affected = {
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());
        execute_with_params(&conn, sql, &params)?
    };
    Ok(affected as u32)
}

/// 参数化查询（同上绑定语义 + 结果集护栏）
pub(crate) fn db_query_params(state: &WasmPluginState, sql: &str, params_json: &str) -> Result<Option<String>, String> {
    require_storage_permission(state)?;
    validate_main_prefix(state, sql)?;
    let params = parse_params_json(params_json)?;
    let db = state.host_ctx.db.clone();
    let value = {
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());
        query_with_params_to_json(&state.plugin_id, &conn, sql, &params)?
    };
    serde_json::to_string(&value).map(Some).map_err(|e| format!("JSON serialization failed: {e}"))
}

/// 事务内顺序执行多语句（sqls-json 为 SQL 字符串数组），返回受影响行数合计
pub(crate) fn db_execute_batch(state: &WasmPluginState, sqls_json: &str) -> Result<u32, String> {
    require_storage_permission(state)?;
    let sqls = parse_sqls_json(sqls_json)?;
    if sqls.len() > EXECUTE_BATCH_MAX_STATEMENTS {
        return Err(format!(
            "database error: execute-batch exceeds {} statements limit",
            EXECUTE_BATCH_MAX_STATEMENTS
        ));
    }
    for sql in &sqls {
        validate_main_prefix(state, sql)?;
    }
    let db = state.host_ctx.db.clone();
    let affected = {
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());
        execute_batch_on_conn(&conn, &sqls)?
    };
    Ok(affected)
}

// ==================== 插件私有库（host-plugin-database 5 函数） ====================

/// 取插件私有库连接（惰性打开 + 缓存；属主分区——仅本插件可访问）
///
/// 生产：`app_data_dir/plugins/<sanitized_id>.db`；无头/测试：内存库。
/// **生命周期**：连接表在 WasmHostContext.plugin_dbs（每次激活新建 host_ctx），
/// 插件停用时 state drop → host_ctx Arc 计数归零 → 连接 Drop 关闭——无需显式
/// purge，停用回收由生命周期自动完成
fn plugin_db_conn(state: &WasmPluginState) -> Result<Arc<Mutex<rusqlite::Connection>>, String> {
    let table = state.host_ctx.plugin_dbs.clone();
    {
        let guard = table.lock().expect("plugin dbs table lock poisoned");
        if let Some(conn) = guard.get(&state.plugin_id) {
            return Ok(conn.clone());
        }
    }
    let conn = match state.host_ctx.app_handle.as_ref() {
        Some(app) => {
            let dir = crate::peer_net::app_data_dir(app)
                .map_err(|e| format!("plugin db: resolve app data dir failed: {e}"))?;
            let dir = dir.join(crate::system::constants::plugin::PLUGIN_DATA_DIR);
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("plugin db: create plugin data dir failed: {e}"))?;
            let path = dir.join(format!("{}.db", state.plugin_id.replace(['.', '-'], "_")));
            rusqlite::Connection::open(path)
                .map_err(|e| format!("plugin db: open {} failed: {e}", state.plugin_id))?
        }
        None => rusqlite::Connection::open_in_memory()
            .map_err(|e| format!("plugin db: open in-memory failed: {e}"))?,
    };
    let arc = Arc::new(Mutex::new(conn));
    {
        let mut guard = table.lock().expect("plugin dbs table lock poisoned");
        guard.insert(state.plugin_id.clone(), arc.clone());
    }
    Ok(arc)
}

/// 执行 SQL（插件私有库；属主分区无前缀校验——整库归本插件）
pub(crate) fn plugin_db_execute(state: &WasmPluginState, sql: &str) -> Result<u32, String> {
    require_storage_permission(state)?;
    reject_bare_transaction_control(sql)?;
    let conn = plugin_db_conn(state)?;
    let affected = {
        let conn = conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(sql, [])
            .map_err(|e| format!("SQL execution failed: {e}"))?
    };
    Ok(affected as u32)
}

/// 查询 SQL（插件私有库；结果集护栏同主库）
pub(crate) fn plugin_db_query(state: &WasmPluginState, sql: &str) -> Result<Option<String>, String> {
    require_storage_permission(state)?;
    let conn = plugin_db_conn(state)?;
    let value = {
        let conn = conn.lock().unwrap_or_else(|e| e.into_inner());
        query_to_json(&state.plugin_id, &conn, sql)?
    };
    serde_json::to_string(&value).map(Some).map_err(|e| format!("JSON serialization failed: {e}"))
}

/// 参数化执行（插件私有库）
pub(crate) fn plugin_db_execute_params(
    state: &WasmPluginState,
    sql: &str,
    params_json: &str,
) -> Result<u32, String> {
    require_storage_permission(state)?;
    let params = parse_params_json(params_json)?;
    let conn = plugin_db_conn(state)?;
    let affected = {
        let conn = conn.lock().unwrap_or_else(|e| e.into_inner());
        execute_with_params(&conn, sql, &params)?
    };
    Ok(affected as u32)
}

/// 参数化查询（插件私有库；结果集护栏同主库）
pub(crate) fn plugin_db_query_params(
    state: &WasmPluginState,
    sql: &str,
    params_json: &str,
) -> Result<Option<String>, String> {
    require_storage_permission(state)?;
    let params = parse_params_json(params_json)?;
    let conn = plugin_db_conn(state)?;
    let value = {
        let conn = conn.lock().unwrap_or_else(|e| e.into_inner());
        query_with_params_to_json(&state.plugin_id, &conn, sql, &params)?
    };
    serde_json::to_string(&value).map(Some).map_err(|e| format!("JSON serialization failed: {e}"))
}

/// 事务批次执行（插件私有库；语句数上限同主库）
pub(crate) fn plugin_db_execute_batch(state: &WasmPluginState, sqls_json: &str) -> Result<u32, String> {
    require_storage_permission(state)?;
    let sqls = parse_sqls_json(sqls_json)?;
    if sqls.len() > EXECUTE_BATCH_MAX_STATEMENTS {
        return Err(format!(
            "database error: execute-batch exceeds {} statements limit",
            EXECUTE_BATCH_MAX_STATEMENTS
        ));
    }
    let conn = plugin_db_conn(state)?;
    let affected = {
        let conn = conn.lock().unwrap_or_else(|e| e.into_inner());
        execute_batch_on_conn(&conn, &sqls)?
    };
    Ok(affected)
}

// ==================== 辅助（移植桌面 database.rs） ====================

fn parse_params_json(params_json: &str) -> Result<Vec<serde_json::Value>, String> {
    serde_json::from_str::<Vec<serde_json::Value>>(params_json)
        .map_err(|e| format!("invalid params JSON array: {e}"))
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
    let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {e}"))?;
    bind_json_params(&mut stmt, params).map_err(|e| format!("bind: {e}"))?;
    stmt.raw_execute().map_err(|e| format!("execute: {e}"))
}

/// 结果集护栏：行数 / 序列化字节双上限逐行检查（安全边界，不可由插件调整）
fn push_row_capped(
    plugin_id: &str,
    rows_out: &mut Vec<serde_json::Value>,
    total_bytes: &mut usize,
    row: serde_json::Value,
) -> Result<(), String> {
    if rows_out.len() >= QUERY_MAX_ROWS {
        tracing::warn!(plugin_id = %plugin_id, limit = QUERY_MAX_ROWS, "plugin query result exceeded row limit");
        return Err(format!(
            "database error: query result exceeds {} rows limit (add LIMIT or batch)",
            QUERY_MAX_ROWS
        ));
    }
    *total_bytes += serde_json::to_vec(&row)
        .map_err(|e| format!("serialize row to measure size: {e}"))?
        .len();
    if *total_bytes > QUERY_MAX_BYTES {
        tracing::warn!(plugin_id = %plugin_id, limit = QUERY_MAX_BYTES, "plugin query result exceeded byte limit");
        return Err(format!(
            "database error: query result exceeds {} bytes limit (add LIMIT or batch)",
            QUERY_MAX_BYTES
        ));
    }
    rows_out.push(row);
    Ok(())
}

/// 参数绑定查询 → JSON 行数组（含结果集护栏）
fn query_with_params_to_json(
    plugin_id: &str,
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[serde_json::Value],
) -> Result<serde_json::Value, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {e}"))?;
    bind_json_params(&mut stmt, params).map_err(|e| format!("bind: {e}"))?;

    let column_count = stmt.column_count();
    let column_names: Vec<String> = (0..column_count)
        .map(|i| {
            stmt.column_name(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| format!("col{}", i))
        })
        .collect();

    let mut rows_out: Vec<serde_json::Value> = Vec::new();
    let mut total_bytes: usize = 0;
    let mut rows = stmt.raw_query();
    while let Some(row) = rows.next().map_err(|e| format!("next: {e}"))? {
        let mut map = serde_json::Map::new();
        for (i, col_name) in column_names.iter().enumerate() {
            map.insert(col_name.clone(), wasm_host::column_to_json(row, i));
        }
        push_row_capped(
            plugin_id,
            &mut rows_out,
            &mut total_bytes,
            serde_json::Value::Object(map),
        )?;
    }

    Ok(serde_json::Value::Array(rows_out))
}

/// 查询 → JSON 行数组（含结果集护栏；主库与插件库共用）
fn query_to_json(
    plugin_id: &str,
    conn: &rusqlite::Connection,
    sql: &str,
) -> Result<serde_json::Value, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {e}"))?;

    let column_count = stmt.column_count();
    let column_names: Vec<String> = (0..column_count)
        .map(|i| {
            stmt.column_name(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| format!("col{}", i))
        })
        .collect();

    let mut rows_out: Vec<serde_json::Value> = Vec::new();
    let mut total_bytes: usize = 0;
    let rows = stmt
        .query_map([], |row| {
            let mut map = serde_json::Map::new();
            for (i, col_name) in column_names.iter().enumerate() {
                let value = wasm_host::column_to_json(row, i);
                map.insert(col_name.clone(), value);
            }
            Ok(map)
        })
        .map_err(|e| format!("query_map: {e}"))?;
    for row in rows {
        let map = row.map_err(|e| format!("row: {e}"))?;
        push_row_capped(
            plugin_id,
            &mut rows_out,
            &mut total_bytes,
            serde_json::Value::Object(map),
        )?;
    }

    Ok(serde_json::Value::Array(rows_out))
}

fn parse_sqls_json(sqls_json: &str) -> Result<Vec<String>, String> {
    serde_json::from_str::<Vec<String>>(sqls_json).map_err(|e| format!("invalid sqls JSON array: {e}"))
}

/// 裸事务控制拒绝：跨调用裸事务（BEGIN → 多次 execute → COMMIT）期间内核写入
/// 会插进插件事务窗口——execute-batch 是唯一受支持的事务入口（桌面票据 06 同款）
fn reject_bare_transaction_control(sql: &str) -> Result<(), String> {
    let mut s = sql.trim();
    for _ in 0..8 {
        if let Some(rest) = s.strip_prefix("--") {
            s = rest.split_once('\n').map(|(_, after)| after).unwrap_or("").trim_start();
        } else if let Some(rest) = s.strip_prefix("/*") {
            s = rest.split_once("*/").map(|(_, after)| after).unwrap_or("").trim_start();
        } else {
            break;
        }
    }
    let first = s.split_whitespace().next().map(normalize_sql_token);
    let last = s.split_whitespace().next_back().map(normalize_sql_token);
    let is_bare_transaction = matches!(
        first.as_deref(),
        Some("begin" | "commit" | "end" | "rollback" | "savepoint" | "release")
    ) || matches!(last.as_deref(), Some("commit" | "rollback" | "end" | "release"));
    if is_bare_transaction {
        return Err(
            "database error: bare transaction control statements are rejected at execute level \
             (use execute-batch for multi-statement transactions)"
                .to_string(),
        );
    }
    Ok(())
}

/// 归一化 SQL token：去尾分号 + 小写（用于事务控制白名单匹配）
fn normalize_sql_token(t: &str) -> String {
    t.trim_end_matches(';').to_lowercase()
}

/// 事务内顺序执行多语句：任一句失败整体回滚，返回受影响行数合计
fn execute_batch_on_conn(conn: &rusqlite::Connection, sqls: &[String]) -> Result<u32, String> {
    // 每语句先做裸事务控制拒绝（与 execute 层同口径——batch 内部也不允许）
    for sql in sqls {
        reject_bare_transaction_control(sql)?;
    }
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("begin transaction: {e}"))?;
    let mut total_affected: u32 = 0;
    for sql in sqls {
        let affected = tx
            .execute(sql.as_str(), [])
            .map_err(|e| format!("execute '{sql}': {e}"))?;
        total_affected = total_affected
            .checked_add(affected as u32)
            .ok_or_else(|| "database error: execute-batch affected row count overflow".to_string())?;
    }
    tx.commit().map_err(|e| format!("commit transaction: {e}"))?;
    Ok(total_affected)
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::fs_auth::FsAuthChecker;
    use crate::plugin::message_bus::MessageBus;
    use crate::plugin::storage::PluginStorage;
    use crate::plugin::wasm_runtime::WasmHostContext;
    use std::collections::HashSet;

    /// 测试保活的 tokio runtime（WasmPluginState.runtime_handle 字段要求）
    fn test_runtime() -> tokio::runtime::Handle {
        static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
        RT.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build test runtime")
        })
        .handle()
        .clone()
    }

    fn storage_state(plugin_id: &str) -> WasmPluginState {
        let db = Arc::new(Mutex::new(
            rusqlite::Connection::open_in_memory().expect("open in-memory db"),
        ));
        let storage = PluginStorage::test_storage();
        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), None));
        let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(|_, _| {});
        let host_ctx = Arc::new(WasmHostContext::new(
            db,
            storage,
            None,
            fs_auth,
            Arc::new(MessageBus::new()),
            status_reporter,
        ));
        WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx,
            runtime_handle: test_runtime(),
            granted_permissions: HashSet::from([
                bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE.to_string(),
            ]),
            on_message_binary: None,
        }
    }

    fn ctx() -> WasmPluginState {
        storage_state("com.bedcode.db-test")
    }

    /// 参数化往返：绑定 → 查询回读（防注入真绑定）
    #[test]
    fn params_roundtrip_main_db() {
        let state = ctx();
        // 建表 + 插入（参数绑定）
        let _ = db_execute(&state, "CREATE TABLE plugin_com_bedcode_db_test_kv (k TEXT PRIMARY KEY, v TEXT)").unwrap();
        let _ = db_execute_params(&state, "INSERT INTO plugin_com_bedcode_db_test_kv (k, v) VALUES (?1, ?2)", r#"["a", "hello"]"#).unwrap();
        // 参数化查询回读
        let out = db_query_params(&state, "SELECT v FROM plugin_com_bedcode_db_test_kv WHERE k = ?1", r#"["a"]"#).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out.unwrap()).unwrap();
        assert_eq!(v[0]["v"], "hello");
    }

    /// 表名前缀纵深：非本插件前缀表被拒（主库）
    #[test]
    fn main_db_prefix_is_enforced() {
        let state = ctx();
        assert!(db_execute(&state, "CREATE TABLE other_table (x TEXT)").is_err());
        assert!(db_execute(&state, "CREATE TABLE plugin_other_plugin_kv (x TEXT)").is_err());
    }

    /// execute-batch 事务原子性：中途失败整体回滚
    #[test]
    fn execute_batch_rolls_back_on_error() {
        let state = ctx();
        let _ = db_execute(&state, "CREATE TABLE plugin_com_bedcode_db_test_tx (k TEXT PRIMARY KEY, v TEXT)").unwrap();
        let sqls = serde_json::json!([
            "INSERT INTO plugin_com_bedcode_db_test_tx (k, v) VALUES ('a', '1')",
            "INSERT INTO plugin_com_bedcode_db_test_tx (k, v) VALUES ('b', '2')",
            "INSERT INTO plugin_com_bedcode_db_test_tx (k, v) VALUES ('a', '3')", // 冲突 → 回滚
        ]);
        assert!(db_execute_batch(&state, &sqls.to_string()).is_err());
        let out = db_query(&state, "SELECT COUNT(*) AS n FROM plugin_com_bedcode_db_test_tx").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out.unwrap()).unwrap();
        assert_eq!(v[0]["n"], 0, "batch failure must roll back all statements");
    }

    /// 裸事务控制拒绝：BEGIN/COMMIT 在 execute 层被拒（跨调用裸事务窗口）
    #[test]
    fn bare_transaction_control_rejected() {
        let state = ctx();
        assert!(db_execute(&state, "BEGIN").is_err());
        assert!(db_execute(&state, "COMMIT").is_err());
        assert!(db_execute(&state, "-- 注释\nROLLBACK").is_err());
    }

    /// 插件私有库：属主分区（独立库 + 连接缓存）
    #[test]
    fn plugin_db_owned_and_cached() {
        let state = ctx();
        let _ = plugin_db_execute(&state, "CREATE TABLE kv (k TEXT PRIMARY KEY, v TEXT)").unwrap();
        let _ = plugin_db_execute(&state, "INSERT INTO kv VALUES ('a', '1')").unwrap();
        let out = plugin_db_query(&state, "SELECT v FROM kv WHERE k = 'a'").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out.unwrap()).unwrap();
        assert_eq!(v[0]["v"], "1");
        // 连接表缓存：重复访问不重复打开
        {
            let table = state.host_ctx.plugin_dbs.lock().unwrap();
            assert!(table.contains_key(&state.plugin_id), "connection must be cached");
        }
    }
}
