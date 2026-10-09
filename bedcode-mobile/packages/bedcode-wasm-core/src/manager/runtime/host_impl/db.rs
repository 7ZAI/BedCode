//! host_db_* — 插件私有库访问（host-plugin-database 5 函数）
//!
//! **2026-10-09 双端机制决策（用户指令）**：主库由 wasm-core 管理，**不给任何
//! wasm-app / 插件直接调用的方法**——`host-database`（主库）接口自移动 WIT 面
//! 移除（ABI 18→19），本文件随之收缩为**纯插件私有库 adapter**（`host-plugin-database`
//! 5 原语）：机制语义（权限门 `storage` / 语句超时护栏 / 结果集护栏 / 批次事务）
//! 在共享核 `bedcode-host-api-core::database`（双端单点，票 18 批次 3 + 主库收归）。
//! 属主分区（独立 SQLite 库，惰性打开 + 停用回收）与移动 state 形状留在本端。

use super::super::WasmPluginState;
use bedcode_host_api_core::database as core_db;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 统一权限门：storage（host-plugin-database 私有库同 host-storage）
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

/// 取插件私有库连接（惰性打开 + 缓存；属主分区——仅本插件可访问）
///
/// 生产：`app_data_dir/plugins/<sanitized_id>.db`；无头/测试：内存库。
/// **生命周期**：连接表在 WasmHostContext.plugin_dbs（每次激活新建 host_ctx），
/// 插件停用时 state drop → host_ctx Arc 计数归零 → 连接 Drop 关闭——无需显式
/// purge，停用回收由生命周期自动完成
fn plugin_db_conn(state: &WasmPluginState) -> Result<Arc<Mutex<crate::db::Database>>, String> {
    let table = state.host_ctx.plugin_dbs.clone();
    {
        let guard = table.lock().expect("plugin dbs table lock poisoned");
        if let Some(conn) = guard.get(&state.plugin_id) {
            return Ok(conn.clone());
        }
    }
    let db = match state.host_ctx.app_handle.as_ref() {
        Some(app) => {
            let ports = state.host_ctx.ports.clone();
            let app = (**app).clone();
            let dir = state
                .runtime_handle
                .block_on(ports.app_data_dir(&app))
                .map_err(|e| format!("plugin db: resolve app data dir failed: {e}"))?;
            let dir = dir.join(crate::system::constants::PLUGIN_DATA_DIR);
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("plugin db: create plugin data dir failed: {e}"))?;
            let path = dir.join(format!("{}.db", state.plugin_id.replace(['.', '-'], "_")));
            crate::db::Database::new(&path)
                .map_err(|e| format!("plugin db: open {} failed: {e}", state.plugin_id))?
        }
        None => crate::db::Database::new(std::path::Path::new(":memory:"))
            .map_err(|e| format!("plugin db: open in-memory failed: {e}"))?,
    };
    let arc = Arc::new(Mutex::new(db));
    {
        let mut guard = table.lock().expect("plugin dbs table lock poisoned");
        guard.insert(state.plugin_id.clone(), arc.clone());
    }
    Ok(arc)
}

/// 执行 SQL（插件私有库；属主分区无前缀校验——整库归本插件；超时护栏 120s）
pub(crate) fn plugin_db_execute(state: &WasmPluginState, sql: &str) -> Result<u32, String> {
    require_storage_permission(state)?;
    core_db::reject_bare_transaction_control(sql)?;
    let conn = plugin_db_conn(state)?;
    let timeout = Duration::from_secs(core_db::PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    let affected = {
        let guard = conn.lock().unwrap_or_else(|e| e.into_inner());
        let conn = guard.conn();
        core_db::with_statement_timeout(&state.plugin_id, conn, timeout, |c| {
            c.execute(sql, []).map(|n| n as u32).map_err(|e| e.to_string())
        })
    }
    .map_err(|e| format!("database error: {e}"))?;
    Ok(affected)
}

/// 查询 SQL（插件私有库；结果集护栏同主库）
pub(crate) fn plugin_db_query(state: &WasmPluginState, sql: &str) -> Result<Option<String>, String> {
    require_storage_permission(state)?;
    let conn = plugin_db_conn(state)?;
    let timeout = Duration::from_secs(core_db::PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    let value = {
        let guard = conn.lock().unwrap_or_else(|e| e.into_inner());
        let conn = guard.conn();
        core_db::with_statement_timeout(&state.plugin_id, conn, timeout, |c| {
            core_db::query_to_json(&state.plugin_id, c, sql)
        })
    }
    .map_err(|e| format!("database error: {e}"))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {e}"))
}

/// 参数化执行（插件私有库；params-json 为 JSON 值数组，按序真绑定防注入）
pub(crate) fn plugin_db_execute_params(
    state: &WasmPluginState,
    sql: &str,
    params_json: &str,
) -> Result<u32, String> {
    require_storage_permission(state)?;
    core_db::reject_bare_transaction_control(sql)?;
    let params = core_db::parse_params_json(params_json)?;
    let conn = plugin_db_conn(state)?;
    let timeout = Duration::from_secs(core_db::PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    let affected = {
        let guard = conn.lock().unwrap_or_else(|e| e.into_inner());
        let conn = guard.conn();
        core_db::with_statement_timeout(&state.plugin_id, conn, timeout, |c| {
            core_db::execute_with_params(c, sql, &params).map(|n| n as u32)
        })
    }
    .map_err(|e| format!("database error: {e}"))?;
    Ok(affected)
}

/// 参数化查询（插件私有库；结果集护栏同主库）
pub(crate) fn plugin_db_query_params(
    state: &WasmPluginState,
    sql: &str,
    params_json: &str,
) -> Result<Option<String>, String> {
    require_storage_permission(state)?;
    let params = core_db::parse_params_json(params_json)?;
    let conn = plugin_db_conn(state)?;
    let timeout = Duration::from_secs(core_db::PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    let value = {
        let guard = conn.lock().unwrap_or_else(|e| e.into_inner());
        let conn = guard.conn();
        core_db::with_statement_timeout(&state.plugin_id, conn, timeout, |c| {
            core_db::query_with_params_to_json(&state.plugin_id, c, sql, &params)
        })
    }
    .map_err(|e| format!("database error: {e}"))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {e}"))
}

/// 事务批次执行（插件私有库；语句数上限 64，任一句失败整体回滚）
pub(crate) fn plugin_db_execute_batch(state: &WasmPluginState, sqls_json: &str) -> Result<u32, String> {
    require_storage_permission(state)?;
    let sqls = core_db::parse_sqls_json(sqls_json)?;
    if sqls.len() > core_db::PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS {
        return Err(format!(
            "database error: execute-batch exceeds {} statements limit",
            core_db::PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS
        ));
    }
    let conn = plugin_db_conn(state)?;
    let timeout = Duration::from_secs(core_db::PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    let affected = {
        let guard = conn.lock().unwrap_or_else(|e| e.into_inner());
        let conn = guard.conn();
        core_db::with_statement_timeout(&state.plugin_id, conn, timeout, |c| {
            core_db::execute_batch_on_conn(c, &sqls)
        })
    }
    .map_err(|e| format!("database error: {e}"))?;
    Ok(affected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::MessageBus;
    use crate::manager::runtime::WasmHostContext;
    use crate::security::fs_auth::FsAuthChecker;
    use crate::storage::PluginStorage;
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
        let db = Arc::new(Mutex::new(crate::db::Database::from_connection(
            rusqlite::Connection::open_in_memory().expect("open in-memory db"),
        )));
        let storage = PluginStorage::test_storage();
        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), None, Vec::new()));
        let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(|_, _| {});
        let host_ctx = Arc::new(WasmHostContext::new_headless(
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

    /// 权限门 fail-closed：未声明 storage 即拒（插件私有库）
    #[test]
    fn plugin_db_requires_storage_permission() {
        let mut state = ctx();
        state.granted_permissions = HashSet::new();
        assert!(plugin_db_execute(&state, "CREATE TABLE kv (k TEXT PRIMARY KEY, v TEXT)").is_err());
        assert!(plugin_db_query(&state, "SELECT * FROM kv").is_err());
    }

    /// 参数化往返：绑定 → 查询回读（防注入真绑定）
    #[test]
    fn params_roundtrip_plugin_db() {
        let state = ctx();
        let _ = plugin_db_execute(&state, "CREATE TABLE kv (k TEXT PRIMARY KEY, v TEXT)").unwrap();
        let _ = plugin_db_execute_params(&state, "INSERT INTO kv (k, v) VALUES (?1, ?2)", r#"["a", "hello"]"#).unwrap();
        let out = plugin_db_query_params(&state, "SELECT v FROM kv WHERE k = ?1", r#"["a"]"#).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out.unwrap()).unwrap();
        assert_eq!(v[0]["v"], "hello");
    }

    /// execute-batch 事务原子性：中途失败整体回滚（插件私有库）
    #[test]
    fn execute_batch_rolls_back_on_error() {
        let state = ctx();
        let _ = plugin_db_execute(&state, "CREATE TABLE tx (k TEXT PRIMARY KEY, v TEXT)").unwrap();
        let sqls = serde_json::json!([
            "INSERT INTO tx (k, v) VALUES ('a', '1')",
            "INSERT INTO tx (k, v) VALUES ('b', '2')",
            "INSERT INTO tx (k, v) VALUES ('a', '3')", // 冲突 → 回滚
        ]);
        assert!(plugin_db_execute_batch(&state, &sqls.to_string()).is_err());
        let out = plugin_db_query(&state, "SELECT COUNT(*) AS n FROM tx").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out.unwrap()).unwrap();
        assert_eq!(v[0]["n"], 0, "batch failure must roll back all statements");
    }

    /// 裸事务控制拒绝：BEGIN/COMMIT 在 execute 层被拒（跨调用裸事务窗口）
    #[test]
    fn bare_transaction_control_rejected() {
        let state = ctx();
        assert!(plugin_db_execute(&state, "BEGIN").is_err());
        assert!(plugin_db_execute(&state, "COMMIT").is_err());
        assert!(plugin_db_execute(&state, "-- 注释\nROLLBACK").is_err());
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
