//! 密钥托管域宿主实现（v15 host-auth / secret-store）
//!
//! 属主隔离：属主 = 调用方插件实例的 plugin_id（由 wasm_runtime::component 的
//! Host trait 绑定传入，guest 无法伪造/旁路）；不同插件天然隔离，越权访问
//! 其它命名空间在 SQL 层即无命中（权限门之外的第二道闸）。
//!
//! 真源 = 主库 `plugin_secrets` 表（schema.sql，重启持久化）；内存缓存为
//! read-through（get 命中直接返回，set/delete 失效对应键）。
//!
//! 明文不落日志红线（AGENTS.md §8）：本模块日志只记 `value.len()`，禁止打印
//! 值本身；错误消息不含值内容。

use crate::plugin::manager::wasm_runtime::{block_on_async, WasmHostContext};
use crate::plugin::permission::PERMISSION_AUTH;
use chrono::Utc;
use rusqlite::OptionalExtension;

/// 读取属主密钥（权限门 + 内存缓存 read-through + 主库真源）
pub(crate) fn auth_secret_get(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    key: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_secret_get") {
        return Err("permission denied".to_string());
    }
    // 缓存读
    let cache = host_ctx.secrets_cache.clone();
    {
        let guard = cache.read().map_err(|e| format!("secret cache poisoned: {}", e))?;
        if let Some(v) = guard.get(&(plugin_id.to_string(), key.to_string())) {
            return Ok(Some(v.clone()));
        }
    }
    // 主库读
    let db = host_ctx.db.clone();
    let pid = plugin_id.to_string();
    let k = key.to_string();
    let value: Option<String> = block_on_async(async move {
        let db = db.lock().await;
        db.conn()
            .query_row(
                "SELECT value FROM plugin_secrets WHERE plugin_id = ?1 AND key = ?2",
                rusqlite::params![pid, k],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| format!("database error: {}", e))
    })?;
    // 回填缓存
    if let Some(v) = &value {
        let mut guard = cache.write().map_err(|e| format!("secret cache poisoned: {}", e))?;
        guard.insert((plugin_id.to_string(), key.to_string()), v.clone());
    }
    Ok(value)
}

/// 写入/覆盖属主密钥（覆盖写；缓存失效后由下次 get 回填）
pub(crate) fn auth_secret_set(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_secret_set") {
        return Err("permission denied".to_string());
    }
    // 明文不落日志：只记长度
    tracing::info!(
        plugin_id = %plugin_id,
        key = %key,
        value_len = value.len(),
        "host_auth_secret_set: secret stored (length only)"
    );
    let db = host_ctx.db.clone();
    let pid = plugin_id.to_string();
    let k = key.to_string();
    let v = value.to_string();
    let now = Utc::now().to_rfc3339();
    block_on_async(async move {
        let db = db.lock().await;
        db.conn()
            .execute(
                "INSERT INTO plugin_secrets (plugin_id, key, value, updated_at) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(plugin_id, key) DO UPDATE SET value = ?3, updated_at = ?4",
                rusqlite::params![pid, k, v, now],
            )
            .map_err(|e| format!("database error: {}", e))?;
        Ok::<(), String>(())
    })?;
    // 缓存失效
    host_ctx
        .secrets_cache
        .write()
        .map_err(|e| format!("secret cache poisoned: {}", e))?
        .remove(&(plugin_id.to_string(), key.to_string()));
    Ok(())
}

/// 删除属主密钥（键不存在也视为成功；缓存同步移除）
pub(crate) fn auth_secret_delete(host_ctx: &WasmHostContext, plugin_id: &str, key: &str) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_secret_delete") {
        return Err("permission denied".to_string());
    }
    let db = host_ctx.db.clone();
    let pid = plugin_id.to_string();
    let k = key.to_string();
    block_on_async(async move {
        let db = db.lock().await;
        db.conn()
            .execute(
                "DELETE FROM plugin_secrets WHERE plugin_id = ?1 AND key = ?2",
                rusqlite::params![pid, k],
            )
            .map_err(|e| format!("database error: {}", e))?;
        Ok::<(), String>(())
    })?;
    host_ctx
        .secrets_cache
        .write()
        .map_err(|e| format!("secret cache poisoned: {}", e))?
        .remove(&(plugin_id.to_string(), key.to_string()));
    Ok(())
}

/// 列举属主密钥名（不返回值本身，供诊断/清理）
pub(crate) fn auth_secret_keys(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<Vec<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_AUTH, "host_auth_secret_keys") {
        return Err("permission denied".to_string());
    }
    let db = host_ctx.db.clone();
    let pid = plugin_id.to_string();
    block_on_async(async move {
        let db = db.lock().await;
        let mut stmt = db
            .conn()
            .prepare("SELECT key FROM plugin_secrets WHERE plugin_id = ?1 ORDER BY key")
            .map_err(|e| format!("database error: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params![pid], |row| row.get::<_, String>(0))
            .map_err(|e| format!("database error: {}", e))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| format!("database error: {}", e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::manager::wasm_runtime::host_impl::tests::{build_host_ctx, grant_permissions};
    use crate::session::{SessionConfigManager, SessionManager};
    use std::sync::Arc;

    /// 文件后备库宿主上下文（重启持久化测试用；构造路径与 host_impl::tests::build_host_ctx 同构）
    fn file_host_ctx(db_path: &std::path::Path) -> Arc<WasmHostContext> {
        let db = crate::db::Database::new(db_path).expect("open db");
        db.init_schema().expect("init schema（含 plugin_secrets）");
        let db = Arc::new(tokio::sync::Mutex::new(db));
        let storage = Arc::new(crate::plugin::manager::storage::PluginStorage::new(db.clone()));
        let fs_auth = Arc::new(crate::plugin::security::fs_auth::FsAuthChecker::new(storage.clone(), None));
        let session_manager = Arc::new(SessionManager::from_database(
            crate::db::Database::new(&db_path.with_extension("session.db")).expect("session db"),
            Arc::new(std::path::PathBuf::from(".")),
        ));
        let config_manager = Arc::new(SessionConfigManager::new(Arc::new(tokio::sync::Mutex::new({
            let db = crate::db::Database::new(&db_path.with_extension("config.db")).expect("config db");
            db.init_schema().expect("init schema");
            db
        }))));
        Arc::new(WasmHostContext::new(
            db,
            Arc::new(tokio::sync::Mutex::new(Default::default())),
            storage,
            session_manager,
            config_manager,
            None,
            Arc::new(crate::plugin::permission::PermissionManager::new()),
            fs_auth,
            Arc::new(crate::plugin::bus::MessageBus::new()),
        ))
    }

    #[test]
    fn test_set_get_roundtrip_and_overwrite() {
        let host_ctx = build_host_ctx();
        grant_permissions(&host_ctx, "com.bedcode.test-a", &[crate::plugin::permission::PERMISSION_AUTH]);
        assert_eq!(auth_secret_get(&host_ctx, "com.bedcode.test-a", "jwt.key").unwrap(), None);
        auth_secret_set(&host_ctx, "com.bedcode.test-a", "jwt.key", "secret-v1").unwrap();
        assert_eq!(
            auth_secret_get(&host_ctx, "com.bedcode.test-a", "jwt.key").unwrap().as_deref(),
            Some("secret-v1")
        );
        // 覆盖写
        auth_secret_set(&host_ctx, "com.bedcode.test-a", "jwt.key", "secret-v2").unwrap();
        assert_eq!(
            auth_secret_get(&host_ctx, "com.bedcode.test-a", "jwt.key").unwrap().as_deref(),
            Some("secret-v2")
        );
    }

    #[test]
    fn test_owner_isolation_across_plugins() {
        let host_ctx = build_host_ctx();
        grant_permissions(&host_ctx, "com.bedcode.test-a", &[crate::plugin::permission::PERMISSION_AUTH]);
        grant_permissions(&host_ctx, "com.bedcode.test-b", &[crate::plugin::permission::PERMISSION_AUTH]);
        auth_secret_set(&host_ctx, "com.bedcode.test-a", "seed", "a-secret").unwrap();
        // B 同名 key 读不到 A 的值（命名空间隔离）
        assert_eq!(auth_secret_get(&host_ctx, "com.bedcode.test-b", "seed").unwrap(), None);
        // B 删不掉 A 的密钥
        auth_secret_delete(&host_ctx, "com.bedcode.test-b", "seed").unwrap();
        assert_eq!(
            auth_secret_get(&host_ctx, "com.bedcode.test-a", "seed").unwrap().as_deref(),
            Some("a-secret")
        );
        // B 写同名 key 不覆盖 A
        auth_secret_set(&host_ctx, "com.bedcode.test-b", "seed", "b-secret").unwrap();
        assert_eq!(
            auth_secret_get(&host_ctx, "com.bedcode.test-a", "seed").unwrap().as_deref(),
            Some("a-secret")
        );
        assert_eq!(
            auth_secret_get(&host_ctx, "com.bedcode.test-b", "seed").unwrap().as_deref(),
            Some("b-secret")
        );
    }

    #[test]
    fn test_delete_and_keys() {
        let host_ctx = build_host_ctx();
        grant_permissions(&host_ctx, "com.bedcode.test-a", &[crate::plugin::permission::PERMISSION_AUTH]);
        auth_secret_set(&host_ctx, "com.bedcode.test-a", "k1", "v1").unwrap();
        auth_secret_set(&host_ctx, "com.bedcode.test-a", "k2", "v2").unwrap();
        let keys = auth_secret_keys(&host_ctx, "com.bedcode.test-a").unwrap();
        assert_eq!(keys, vec!["k1".to_string(), "k2".to_string()]);
        auth_secret_delete(&host_ctx, "com.bedcode.test-a", "k1").unwrap();
        // 幂等删除
        auth_secret_delete(&host_ctx, "com.bedcode.test-a", "k1").unwrap();
        assert_eq!(auth_secret_get(&host_ctx, "com.bedcode.test-a", "k1").unwrap(), None);
        assert_eq!(auth_secret_keys(&host_ctx, "com.bedcode.test-a").unwrap(), vec!["k2".to_string()]);
    }

    #[test]
    fn test_permission_denied_without_auth_grant() {
        let host_ctx = build_host_ctx();
        // 未授权 auth 权限的插件 → 全部四函数拒绝（Rust 端最终仲裁）
        assert_eq!(auth_secret_get(&host_ctx, "com.bedcode.no-auth", "k").unwrap_err(), "permission denied");
        assert_eq!(
            auth_secret_set(&host_ctx, "com.bedcode.no-auth", "k", "v").unwrap_err(),
            "permission denied"
        );
        assert_eq!(
            auth_secret_delete(&host_ctx, "com.bedcode.no-auth", "k").unwrap_err(),
            "permission denied"
        );
        assert_eq!(auth_secret_keys(&host_ctx, "com.bedcode.no-auth").unwrap_err(), "permission denied");
    }

    #[test]
    fn test_persistence_across_restart() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("persist.db");
        // 第一代上下文：写入
        {
            let host_ctx = file_host_ctx(&db_path);
            grant_permissions(&host_ctx, "com.bedcode.test-a", &[crate::plugin::permission::PERMISSION_AUTH]);
            auth_secret_set(&host_ctx, "com.bedcode.test-a", "jwt.key", "persisted-secret").unwrap();
        }
        // 第二代上下文（全新内存缓存 + 全新连接）：重启后密钥稳定
        {
            let host_ctx = file_host_ctx(&db_path);
            grant_permissions(&host_ctx, "com.bedcode.test-a", &[crate::plugin::permission::PERMISSION_AUTH]);
            assert_eq!(
                auth_secret_get(&host_ctx, "com.bedcode.test-a", "jwt.key")
                    .unwrap()
                    .as_deref(),
                Some("persisted-secret")
            );
        }
    }
}