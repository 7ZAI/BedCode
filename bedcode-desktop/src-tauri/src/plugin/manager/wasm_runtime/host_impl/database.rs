//! 数据库域宿主实现（主库前缀隔离 + 插件独立库）
//!
//! 含 SQL 表名前缀校验与 rusqlite 列 → JSON 转换辅助

use crate::plugin::manager::wasm_runtime::{block_on_async, WasmHostContext};
use crate::plugin::permission::PERMISSION_STORAGE;
use crate::system::constants::plugin::{
    PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS, PLUGIN_DB_QUERY_MAX_BYTES, PLUGIN_DB_QUERY_MAX_ROWS,
    PLUGIN_DB_STATEMENT_TIMEOUT_SECS,
};
use regex::Regex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ==================== 逻辑层（Component Model 绑定调用） ====================

/// 解析参数绑定 JSON 数组字符串（空串视为空数组）
fn parse_params_json(params_json: &str) -> Result<Vec<serde_json::Value>, String> {
    if params_json.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(params_json).map_err(|e| format!("invalid params JSON array: {}", e))
}

/// SQL 执行超时护栏：连接上安装 SQLite progress handler，运行 `f` 后（含 panic 路径）
/// 经 Drop 守卫自动移除 handler。
///
/// SQLite progress handler 每 `num_ops`（1000）次虚拟机步回调一次；超时即返回 true
/// （中断），后续 `sqlite3_step` 返回 SQLITE_INTERRUPT → 映射为「查询超时」错误。
/// 主库是内核与全部插件共用的单一连接（全局 Mutex），慢查询会阻塞配对/会话配置/
/// 设置等全部内核 DB 读写——本护栏是内核可用性保护（票据 05）。
/// 超时上限不可由插件调整（安全边界）；触发记结构化 warn!（plugin_id 字段）。
fn with_statement_timeout<T>(
    plugin_id: &str,
    conn: &rusqlite::Connection,
    timeout: Duration,
    f: impl FnOnce(&rusqlite::Connection) -> Result<T, String>,
) -> Result<T, String> {
    let deadline = Instant::now();
    let timed_out = Arc::new(AtomicBool::new(false));
    let flag = timed_out.clone();
    conn.progress_handler(
        1000,
        Some(move || {
            if deadline.elapsed() > timeout {
                flag.store(true, Ordering::Relaxed);
                true
            } else {
                false
            }
        }),
    );
    let _guard = ProgressHandlerGuard { conn };
    let result = f(conn);
    if timed_out.load(Ordering::Relaxed) {
        tracing::warn!(
            plugin_id = %plugin_id,
            timeout_secs = timeout.as_secs(),
            "plugin SQL statement timed out and was interrupted"
        );
        Err(format!(
            "database error: statement timed out after {}s (limit: {}s)",
            timeout.as_secs(),
            PLUGIN_DB_STATEMENT_TIMEOUT_SECS
        ))
    } else {
        result
    }
}

/// progress handler 生命周期守卫：作用域结束（含 panic 展开）时移除 handler，
/// 避免超时 handler 残留在共享主库连接上误中断内核自身的 DB 读写
struct ProgressHandlerGuard<'a> {
    conn: &'a rusqlite::Connection,
}

impl Drop for ProgressHandlerGuard<'_> {
    fn drop(&mut self) {
        self.conn.progress_handler(0, None::<fn() -> bool>);
    }
}

/// 主库执行 SQL（权限 + 表名前缀校验 + 超时护栏），返回受影响行数
pub(crate) fn db_execute(host_ctx: &WasmHostContext, plugin_id: &str, sql: &str) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_db_execute") {
        return Err("permission denied".to_string());
    }
    reject_bare_transaction_control(sql)?;
    validate_sql_table_prefix(plugin_id, sql).map_err(|e| e.to_string())?;
    let db = host_ctx.db.clone();
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    block_on_async(async {
        let db = db.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| {
            conn.execute(sql, []).map_err(|e| e.to_string())
        })
    })
    .map(|affected| affected as u32)
    .map_err(|e| format!("database error: {}", e))
}

/// 主库查询（权限 + 表名前缀校验 + 超时护栏），返回行数组 JSON 字符串
pub(crate) fn db_query(host_ctx: &WasmHostContext, plugin_id: &str, sql: &str) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_db_query") {
        return Err("permission denied".to_string());
    }
    validate_sql_table_prefix(plugin_id, sql).map_err(|e| e.to_string())?;
    let db = host_ctx.db.clone();
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    let value = block_on_async(async {
        let db = db.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| {
            query_to_json(plugin_id, conn, sql)
        })
    })
    .map_err(|e| format!("database error: {}", e))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {}", e))
}

/// 插件独立库执行 SQL（权限校验 + 超时护栏，无表名前缀校验）
pub(crate) fn plugin_db_execute(host_ctx: &WasmHostContext, plugin_id: &str, sql: &str) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_plugin_db_execute") {
        return Err("permission denied".to_string());
    }
    reject_bare_transaction_control(sql)?;
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    block_on_async(async {
        let db_arc = host_ctx
            .get_or_create_plugin_db(plugin_id)
            .await
            .map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| {
            conn.execute(sql, []).map(|n| n as u32).map_err(|e| e.to_string())
        })
    })
    .map_err(|e| format!("database error: {}", e))
}

/// 插件独立库查询（权限校验 + 超时护栏，无表名前缀校验）
pub(crate) fn plugin_db_query(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    sql: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_plugin_db_query") {
        return Err("permission denied".to_string());
    }
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    let value = block_on_async(async {
        let db_arc = host_ctx
            .get_or_create_plugin_db(plugin_id)
            .await
            .map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| {
            query_to_json(plugin_id, conn, sql)
        })
    })
    .map_err(|e| format!("database error: {}", e))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {}", e))
}

/// 主库执行参数绑定 SQL（权限 + 表名前缀校验 + 超时护栏）
pub(crate) fn db_execute_params(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    sql: &str,
    params_json: &str,
) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_db_execute_params") {
        return Err("permission denied".to_string());
    }
    reject_bare_transaction_control(sql)?;
    validate_sql_table_prefix(plugin_id, sql).map_err(|e| e.to_string())?;
    let params = parse_params_json(params_json)?;
    let db = host_ctx.db.clone();
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    block_on_async(async {
        let db = db.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| {
            execute_with_params(conn, sql, &params)
        })
    })
    .map(|affected| affected as u32)
    .map_err(|e| format!("database error: {}", e))
}

/// 主库参数绑定查询（权限 + 表名前缀校验 + 超时护栏）
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
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    let value = block_on_async(async {
        let db = db.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| {
            query_with_params_to_json(plugin_id, conn, sql, &params)
        })
    })
    .map_err(|e| format!("database error: {}", e))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {}", e))
}

/// 插件独立库执行参数绑定 SQL（权限校验 + 超时护栏，无表名前缀校验）
pub(crate) fn plugin_db_execute_params(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    sql: &str,
    params_json: &str,
) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_plugin_db_execute_params") {
        return Err("permission denied".to_string());
    }
    reject_bare_transaction_control(sql)?;
    let params = parse_params_json(params_json)?;
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    block_on_async(async {
        let db_arc = host_ctx
            .get_or_create_plugin_db(plugin_id)
            .await
            .map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| {
            execute_with_params(conn, sql, &params).map(|n| n as u32)
        })
    })
    .map_err(|e| format!("database error: {}", e))
}

/// 插件独立库参数绑定查询（权限校验 + 超时护栏，无表名前缀校验）
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
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    let value = block_on_async(async {
        let db_arc = host_ctx
            .get_or_create_plugin_db(plugin_id)
            .await
            .map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| {
            query_with_params_to_json(plugin_id, conn, sql, &params)
        })
    })
    .map_err(|e| format!("database error: {}", e))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {}", e))
}

// ==================== 事务批次执行（票据 06）：execute-batch ====================

/// 解析 execute-batch 的 sqls-json（SQL 字符串数组）
fn parse_sqls_json(sqls_json: &str) -> Result<Vec<String>, String> {
    serde_json::from_str::<Vec<String>>(sqls_json).map_err(|e| format!("invalid sqls JSON array: {}", e))
}

/// 事务控制语句白名单检测：拒绝以裸事务控制语句开头/结尾的 execute（票据 06）
///
/// 跨调用裸事务（BEGIN → 多次 execute → COMMIT）期间，连接 Mutex 按语句释放，
/// 内核自己的写入会插进插件事务窗口，插件 ROLLBACK 会一并丢弃内核写入——
/// execute-batch 是唯一受支持的事务入口（单次调用内持有事务）。
///
/// 尽力而为的检测：去除首尾空白与前导注释后取首/末 token 匹配白名单，
/// 覆盖 BEGIN/COMMIT/END/ROLLBACK/SAVEPOINT/RELEASE 常见形态，不做完整 SQL 解析。
fn reject_bare_transaction_control(sql: &str) -> Result<(), String> {
    let mut s = sql.trim();
    // 剥离前导注释（-- 行注释 / /* 块注释），最多剥 8 层防病态输入
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

/// 事务内顺序执行多语句：任一句失败整体回滚，返回受影响行数合计（票据 06）
///
/// 经 `unchecked_transaction`（rusqlite 0.32 安全 API，运行时检查嵌套，调用方持有
/// 数据库全局锁独占连接，无并发风险）在 `&Connection` 上开启事务；事务对象 Drop 时
/// 未提交自动回滚。超时护栏由外层 `with_statement_timeout` 覆盖整个批次——
/// progress handler 挂在连接上，事务内所有语句的 VM 步受同一超时约束（长事务不会
/// 无限期阻塞内核 DB）。
fn execute_batch_on_conn(conn: &rusqlite::Connection, sqls: &[String]) -> Result<u32, String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("begin transaction: {}", e))?;
    let mut total_affected: u32 = 0;
    for sql in sqls {
        let affected = tx
            .execute(sql.as_str(), [])
            .map_err(|e| format!("execute '{}': {}", sql, e))?;
        total_affected = total_affected
            .checked_add(affected as u32)
            .ok_or_else(|| "database error: execute-batch affected row count overflow".to_string())?;
    }
    tx.commit().map_err(|e| format!("commit transaction: {}", e))?;
    Ok(total_affected)
}

/// 主库事务批次执行（权限 + 表名前缀 + 语句数上限 + 超时护栏）
pub(crate) fn db_execute_batch(host_ctx: &WasmHostContext, plugin_id: &str, sqls_json: &str) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_db_execute_batch") {
        return Err("permission denied".to_string());
    }
    let sqls = parse_sqls_json(sqls_json)?;
    if sqls.len() > PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS {
        return Err(format!(
            "database error: execute-batch exceeds {} statements limit",
            PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS
        ));
    }
    for sql in &sqls {
        validate_sql_table_prefix(plugin_id, sql).map_err(|e| e.to_string())?;
    }
    let db = host_ctx.db.clone();
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    block_on_async(async {
        let db = db.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| execute_batch_on_conn(conn, &sqls))
    })
    .map_err(|e| format!("database error: {}", e))
}

/// 插件独立库事务批次执行（权限 + 语句数上限 + 超时护栏，无表名前缀校验）
pub(crate) fn plugin_db_execute_batch(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    sqls_json: &str,
) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_STORAGE, "host_plugin_db_execute_batch") {
        return Err("permission denied".to_string());
    }
    let sqls = parse_sqls_json(sqls_json)?;
    if sqls.len() > PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS {
        return Err(format!(
            "database error: execute-batch exceeds {} statements limit",
            PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS
        ));
    }
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    block_on_async(async {
        let db_arc = host_ctx
            .get_or_create_plugin_db(plugin_id)
            .await
            .map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| execute_batch_on_conn(conn, &sqls))
    })
    .map_err(|e| format!("database error: {}", e))
}

// ==================== 参数绑定辅助 ====================

/// 将 JSON 参数绑定到预编译语句（1-based 索引，rusqlite 真绑定，防注入）
fn bind_json_params(stmt: &mut rusqlite::Statement<'_>, params: &[serde_json::Value]) -> rusqlite::Result<()> {
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
fn execute_with_params(conn: &rusqlite::Connection, sql: &str, params: &[serde_json::Value]) -> Result<usize, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
    bind_json_params(&mut stmt, params).map_err(|e| format!("bind: {}", e))?;
    stmt.raw_execute().map_err(|e| format!("execute: {}", e))
}

/// 结果集护栏：行数 / 序列化字节双上限逐行检查（票据 05）
///
/// 上限常量不可由插件调整（安全边界）；触发记结构化 warn!（plugin_id 字段），
/// 错误消息指明上限并引导插件加 LIMIT 或分批。行数上限先于字节测量判断，
/// 避免对超出行数的额外行做无谓序列化。
fn push_row_capped(
    plugin_id: &str,
    rows_out: &mut Vec<serde_json::Value>,
    total_bytes: &mut usize,
    row: serde_json::Value,
) -> Result<(), String> {
    if rows_out.len() >= PLUGIN_DB_QUERY_MAX_ROWS {
        tracing::warn!(
            plugin_id = %plugin_id,
            limit = PLUGIN_DB_QUERY_MAX_ROWS,
            "plugin query result exceeded row limit"
        );
        return Err(format!(
            "database error: query result exceeds {} rows limit (add LIMIT or batch)",
            PLUGIN_DB_QUERY_MAX_ROWS
        ));
    }
    *total_bytes += serde_json::to_vec(&row)
        .map_err(|e| format!("serialize row to measure size: {}", e))?
        .len();
    if *total_bytes > PLUGIN_DB_QUERY_MAX_BYTES {
        tracing::warn!(
            plugin_id = %plugin_id,
            limit = PLUGIN_DB_QUERY_MAX_BYTES,
            "plugin query result exceeded byte limit"
        );
        return Err(format!(
            "database error: query result exceeds {} bytes limit (add LIMIT or batch)",
            PLUGIN_DB_QUERY_MAX_BYTES
        ));
    }
    rows_out.push(row);
    Ok(())
}

/// 参数绑定查询 → JSON 行数组（含结果集护栏：行数/字节上限）
fn query_with_params_to_json(
    plugin_id: &str,
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
    let mut total_bytes: usize = 0;
    let mut rows = stmt.raw_query();
    while let Some(row) = rows.next().map_err(|e| format!("next: {}", e))? {
        let mut map = serde_json::Map::new();
        for (i, col_name) in column_names.iter().enumerate() {
            map.insert(col_name.clone(), column_to_json(row, i));
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

/// 执行查询并将结果集转换为 JSON 行数组（含结果集护栏：行数/字节上限）
///
/// 主库与插件库查询共用，消除原先两份重复的列名提取 + query_map 逻辑；
/// 逐行计数而非全量物化，超限立即截断报错（避免无上限结果集拷入 guest 内存
/// 耗尽单次调用 fuel 预算触发 trap 污染 Store）。
fn query_to_json(plugin_id: &str, conn: &rusqlite::Connection, sql: &str) -> Result<serde_json::Value, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;

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
    let mut rows = stmt
        .query_map([], |row| {
            let mut map = serde_json::Map::new();
            for (i, col_name) in column_names.iter().enumerate() {
                let value = column_to_json(row, i);
                map.insert(col_name.clone(), value);
            }
            Ok(map)
        })
        .map_err(|e| format!("query_map: {}", e))?;
    while let Some(row) = rows.next() {
        let map = row.map_err(|e| format!("row: {}", e))?;
        push_row_capped(
            plugin_id,
            &mut rows_out,
            &mut total_bytes,
            serde_json::Value::Object(map),
        )?;
    }

    Ok(serde_json::Value::Array(rows_out))
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
        return serde_json::Value::Number(serde_json::Number::from_f64(v).unwrap_or(serde_json::Number::from(0)));
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
        let result = validate_sql_table_prefix("com.example.my-plugin", "INSERT INTO sessions (id) VALUES ('abc')");
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
        let result = validate_sql_table_prefix("my-plugin", "DROP TABLE IF EXISTS plugin_my_plugin_cache");
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
        let tables = extract_table_names("INSERT INTO users (id) VALUES (1); SELECT * FROM orders");
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"orders".to_string()));
    }

    #[test]
    fn test_extract_table_names_quoted() {
        let tables = extract_table_names("INSERT INTO `my-table` (id) VALUES (1)");
        assert!(tables.contains(&"my".to_string()));
    }

    // ==================== 执行护栏（票据 05）：超时 / 行数上限 / 字节上限 ====================

    /// 内存连接 + n 行数据（事务内批量插入，测试用）
    fn mem_conn_with_rows(n: usize) -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (x INTEGER)").unwrap();
        {
            let tx = conn.transaction().unwrap();
            for i in 0..n {
                tx.execute("INSERT INTO t (x) VALUES (?1)", rusqlite::params![i as i64])
                    .unwrap();
            }
            tx.commit().unwrap();
        }
        conn
    }

    /// 慢查询（大跨连）被超时护栏硬中断：超时错误与 SQL 错误可区分
    #[test]
    fn statement_timeout_interrupts_slow_query() {
        let conn = mem_conn_with_rows(5000);
        let err = with_statement_timeout("p1", &conn, Duration::from_millis(20), |c| {
            // 5000×5000 = 2500 万行结果，远超 20ms
            query_to_json("p1", c, "SELECT count(*) FROM t a CROSS JOIN t b")
        })
        .expect_err("slow query must be interrupted by timeout guard");
        assert!(
            err.contains("timed out"),
            "timeout error should be distinguishable, got: {}",
            err
        );
    }

    /// 约束/表缺失错误 ≠ 超时（错误分类保持：超时/超限 ≠ SQL 错误 ≠ 权限拒绝）
    #[test]
    fn sql_error_not_aliased_as_timeout() {
        let conn = mem_conn_with_rows(10);
        let err = with_statement_timeout("p1", &conn, Duration::from_secs(5), |c| {
            query_to_json("p1", c, "SELECT * FROM nope")
        })
        .expect_err("missing table must surface as SQL error");
        assert!(
            !err.contains("timed out") && err.contains("nope"),
            "SQL error should not be disguised as timeout, got: {}",
            err
        );
    }

    /// 快查询在超时窗口内放行；结果行数与字节均在上限内
    #[test]
    fn statement_within_timeout_and_limits_passes() {
        let conn = mem_conn_with_rows(100);
        let value = with_statement_timeout("p1", &conn, Duration::from_secs(5), |c| {
            query_to_json("p1", c, "SELECT x FROM t ORDER BY x")
        })
        .expect("fast small query must pass");
        assert_eq!(value.as_array().unwrap().len(), 100);
    }

    /// 结果集行数超上限：截断并报错（引导插件加 LIMIT 或分批）
    #[test]
    fn query_exceeding_row_limit_rejected() {
        let conn = mem_conn_with_rows(PLUGIN_DB_QUERY_MAX_ROWS + 1);
        let err = query_to_json("p1", &conn, "SELECT x FROM t").expect_err("result over row limit must be rejected");
        assert!(
            err.contains("rows limit") && err.contains("LIMIT"),
            "error should state row limit and guidance, got: {}",
            err
        );
    }

    /// 结果集序列化字节超上限（单行大字段）：截断并报错
    #[test]
    fn query_exceeding_byte_limit_rejected() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (x TEXT)").unwrap();
        let big = "a".repeat(PLUGIN_DB_QUERY_MAX_BYTES + 1);
        conn.execute("INSERT INTO t (x) VALUES (?1)", rusqlite::params![big.as_str()])
            .unwrap();
        let err =
            query_to_json("p1", &conn, "SELECT x FROM t").expect_err("oversized row must be rejected by byte guard");
        assert!(
            err.contains("bytes limit"),
            "error should state byte limit, got: {}",
            err
        );
    }

    /// 入口级回归：主库 execute/query/参数绑定经护栏全链路（权限 + 前缀 + 超时/上限）
    #[test]
    fn db_entry_execute_and_query_with_guards() {
        use crate::plugin::manager::wasm_runtime::host_impl::tests as host_tests;
        let ctx = host_tests::build_host_ctx();
        host_tests::grant_permissions(&ctx, "p1", &[PERMISSION_STORAGE]);
        db_execute(&ctx, "p1", "CREATE TABLE plugin_p1_t (x INTEGER)").expect("create table");
        for i in 0..3i64 {
            let params = format!("[{}]", i);
            db_execute_params(&ctx, "p1", "INSERT INTO plugin_p1_t (x) VALUES (?1)", &params)
                .expect("insert with params");
        }
        let out = db_query(&ctx, "p1", "SELECT x FROM plugin_p1_t ORDER BY x")
            .expect("query")
            .expect("query result json");
        assert_eq!(out, "[{\"x\":0},{\"x\":1},{\"x\":2}]");
        let out_params = db_query_params(&ctx, "p1", "SELECT x FROM plugin_p1_t WHERE x >= ?1 ORDER BY x", "[1]")
            .expect("query params")
            .expect("query params json");
        assert_eq!(out_params, "[{\"x\":1},{\"x\":2}]");
    }

    // ==================== 事务批次（票据 06）：execute-batch ====================

    /// 事务批次语义：全部成功才提交；任一句失败整体回滚（前序语句一并回滚）
    #[test]
    fn execute_batch_commits_all_or_rolls_back_all() {
        use crate::plugin::manager::wasm_runtime::host_impl::tests as host_tests;
        let ctx = host_tests::build_host_ctx();
        host_tests::grant_permissions(&ctx, "p1", &[PERMISSION_STORAGE]);
        db_execute(
            &ctx,
            "p1",
            "CREATE TABLE plugin_p1_batch (id INTEGER PRIMARY KEY, v TEXT)",
        )
        .expect("create table");

        // 成功批次：全部提交
        let affected = db_execute_batch(
            &ctx,
            "p1",
            r#"["INSERT INTO plugin_p1_batch (id,v) VALUES (1,'a')",
                "INSERT INTO plugin_p1_batch (id,v) VALUES (2,'b')",
                "INSERT INTO plugin_p1_batch (id,v) VALUES (3,'c')"]"#,
        )
        .expect("batch must commit");
        assert_eq!(affected, 3);
        let out = db_query(&ctx, "p1", "SELECT count(*) AS n FROM plugin_p1_batch")
            .unwrap()
            .unwrap();
        assert_eq!(out, "[{\"n\":3}]");

        // 失败批次（第二句主键冲突）：整体回滚，已执行的第一句也不算数
        let err = db_execute_batch(
            &ctx,
            "p1",
            r#"["INSERT INTO plugin_p1_batch (id,v) VALUES (4,'d')",
                "INSERT INTO plugin_p1_batch (id,v) VALUES (1,'dup')"]"#,
        )
        .expect_err("conflicting statements must roll back the whole batch");
        assert!(err.contains("UNIQUE") || err.contains("duplicate"), "got: {}", err);
        let out = db_query(&ctx, "p1", "SELECT count(*) AS n FROM plugin_p1_batch")
            .unwrap()
            .unwrap();
        assert_eq!(out, "[{\"n\":3}]", "failed batch must roll back prior statements");
    }

    /// 语句数上限：超限批次在执行前被拒（不放行）
    #[test]
    fn execute_batch_statement_count_capped() {
        use crate::plugin::manager::wasm_runtime::host_impl::tests as host_tests;
        let ctx = host_tests::build_host_ctx();
        host_tests::grant_permissions(&ctx, "p1", &[PERMISSION_STORAGE]);
        let many: Vec<String> = (0..PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS + 1)
            .map(|i| format!("INSERT INTO plugin_p1_x VALUES ({})", i))
            .collect();
        let sqls = serde_json::to_string(&many).unwrap();
        let err = db_execute_batch(&ctx, "p1", &sqls).expect_err("over-limit batch must be rejected");
        assert!(err.contains("statements limit"), "got: {}", err);
    }

    /// 跨调用裸事务被拒：execute 级 BEGIN/COMMIT/SAVEPOINT 等引导 execute-batch
    #[test]
    fn bare_transaction_control_rejected_at_execute() {
        use crate::plugin::manager::wasm_runtime::host_impl::tests as host_tests;
        let ctx = host_tests::build_host_ctx();
        host_tests::grant_permissions(&ctx, "p1", &[PERMISSION_STORAGE]);
        for sql in [
            "BEGIN",
            "BEGIN TRANSACTION",
            "COMMIT;",
            "ROLLBACK",
            "SAVEPOINT sp1",
            "RELEASE sp1",
            "END TRANSACTION",
        ] {
            let err = db_execute(&ctx, "p1", sql).expect_err(&format!("'{}' must be rejected", sql));
            assert!(err.contains("execute-batch"), "'{}' err: {}", sql, err);
        }
    }

    /// 白名单检测正反例：正常 DML/DDL 放行；前导注释/尾部 COMMIT 形态也能识别
    #[test]
    fn transaction_control_whitelist_allows_normal_sql() {
        assert!(reject_bare_transaction_control("INSERT INTO t (x) VALUES ('begin')").is_ok());
        assert!(reject_bare_transaction_control("UPDATE t SET x = 'commit' WHERE id = 1").is_ok());
        assert!(reject_bare_transaction_control("SELECT 1").is_ok());
        assert!(reject_bare_transaction_control("-- 注释\nBEGIN").is_err());
        assert!(reject_bare_transaction_control("/* 注释 */ SELECT 1").is_ok());
        assert!(reject_bare_transaction_control("INSERT INTO t VALUES (1); COMMIT").is_err());
    }
}
