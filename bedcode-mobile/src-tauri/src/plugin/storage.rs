//! Mobile Plugin Storage（票 05b：文件落盘 → 主库 `plugin_storage` 表）
//!
//! 真源 = `bedcode_plugins.db` 的 `plugin_storage` 表（schema 见 [`crate::plugin::db_schema`]），
//! 与桌面 wasm-core `storage.rs`（`PluginStorage`）同构——本模块**不再有任何文件落盘路径**：
//! 旧 `app_data_dir/plugins/{plugin_id}.json` 由启动迁移（[`PluginStorage::migrate_file_store_to_db`]）
//! 一次性导入主库后删除（旧读路径删除，fail-visible 三形态①，AGENTS §5.1.4）。
//!
//! 每插件按 `plugin_id` 分区（表主键 `(plugin_id, key)`），插件只能读写自己的空间；
//! 系统级数据（fs_auth 白名单 / approval 审批记录）经 `__system__` 属主写入同一表。
//!
//! 共享主库连接（`Arc<Mutex<rusqlite::Connection>>`）：与 host-database 同连接，
//! std Mutex 串行所有 SQL（host fn 为同步上下文，无需经 tokio 锁绕行——05a 同款注释）。

use crate::system::constants::plugin::PLUGIN_STORAGE_DIR;
use crate::Result;
use chrono::Utc;
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// 插件键值存储管理器（DB-backed；`plugin_storage` 表）
pub struct PluginStorage {
    /// 主库连接（`bedcode_plugins.db`；std Mutex，SQL 为同步操作）
    db: Arc<Mutex<rusqlite::Connection>>,
}

impl PluginStorage {
    /// 创建存储管理器（共享主库连接）
    pub fn new(db: Arc<Mutex<rusqlite::Connection>>) -> Self {
        Self { db }
    }

    /// 获取值
    pub async fn get(&self, plugin_id: &str, key: &str) -> Result<Option<Value>> {
        let conn = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare("SELECT value FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2")?;
        match stmt.query_row(rusqlite::params![plugin_id, key], |row| row.get::<_, String>(0)) {
            Ok(json_str) => Ok(Some(serde_json::from_str(&json_str)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(crate::AppError::Database(e)),
        }
    }

    /// 设置值（upsert：`ON CONFLICT(plugin_id, key) DO UPDATE`）
    pub async fn set(&self, plugin_id: &str, key: &str, value: Value) -> Result<()> {
        let conn = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let json_str = serde_json::to_string(&value)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO plugin_storage (plugin_id, key, value, updated_at) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(plugin_id, key) DO UPDATE SET value = ?3, updated_at = ?4",
            rusqlite::params![plugin_id, key, json_str, now],
        )?;
        Ok(())
    }

    /// 删除值
    pub async fn delete(&self, plugin_id: &str, key: &str) -> Result<()> {
        let conn = self.db.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "DELETE FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2",
            rusqlite::params![plugin_id, key],
        )?;
        Ok(())
    }

    /// 删除插件全部存储（按属主整区清除）
    pub async fn clear_plugin(&self, plugin_id: &str) -> Result<()> {
        let conn = self.db.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "DELETE FROM plugin_storage WHERE plugin_id = ?1",
            rusqlite::params![plugin_id],
        )?;
        Ok(())
    }

    /// 迁移旧文件落盘 KV（`app_data_dir/plugins/{plugin_id}.json`）→ 主库
    /// `plugin_storage` 表（票 05b 真源搬迁；setup 阶段调用一次）
    ///
    /// - 幂等：逐 key `INSERT OR IGNORE`——DB 已有值优先（文件数据一律先于 DB
    ///   写入，绝不可能更新更新的值），重复执行无副作用；
    /// - 单文件全部成功导入后才删除 `.json`（中途失败不丢数据、不删文件）；
    /// - 只挑 `.json` 后缀（05a 的插件私有库 `plugins/<sanitized_id>.db` 不碰）；
    /// - 任一文件解析失败仅记日志跳过（best-effort，与 peer_migration 同款口径）。
    pub(crate) fn migrate_file_store_to_db(&self, app_data_dir: &Path) -> Result<()> {
        let plugins_dir = app_data_dir.join(PLUGIN_STORAGE_DIR);
        if !plugins_dir.exists() {
            return Ok(());
        }
        let conn = self.db.lock().unwrap_or_else(|e| e.into_inner());
        let entries = match std::fs::read_dir(&plugins_dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(dir = %plugins_dir.display(), error = %e, "legacy plugin storage scan skipped");
                return Ok(());
            }
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let is_json = path
                .extension()
                .map(|ext| ext == std::ffi::OsStr::new("json"))
                .unwrap_or(false);
            if !is_json || !path.is_file() {
                continue;
            }
            let plugin_id = match path.file_stem().and_then(|s| s.to_str()) {
                Some(id) if !id.is_empty() => id.to_string(),
                _ => continue,
            };
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(plugin_id = %plugin_id, error = %e, "legacy storage file read skipped");
                    continue;
                }
            };
            let map: std::collections::HashMap<String, Value> = match serde_json::from_str(&content) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(plugin_id = %plugin_id, error = %e, "legacy storage file parse skipped");
                    continue;
                }
            };
            if map.is_empty() {
                continue;
            }
            let now = Utc::now().to_rfc3339();
            for (key, value) in &map {
                let json_str = serde_json::to_string(value)?;
                conn.execute(
                    "INSERT OR IGNORE INTO plugin_storage (plugin_id, key, value, updated_at) \
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![plugin_id, key, json_str, now],
                )?;
            }
            // 全部成功导入后才删除旧文件（旧读路径彻底移除，fail-visible ①）
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!(plugin_id = %plugin_id, error = %e, "legacy storage file removal failed");
            }
            tracing::info!(plugin_id = %plugin_id, count = map.len(), "legacy plugin storage file migrated to db");
        }
        Ok(())
    }
}

#[cfg(test)]
impl PluginStorage {
    /// 测试用存储（内存库 + schema 幂等建表；生产以共享主库连接构造）
    pub(crate) fn test_storage() -> Arc<PluginStorage> {
        let db = Arc::new(Mutex::new(
            rusqlite::Connection::open_in_memory().expect("open in-memory db"),
        ));
        crate::plugin::db_schema::init_schema(&db.lock().expect("plugin db lock poisoned")).expect("init schema");
        Arc::new(PluginStorage::new(db))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn roundtrip_and_upsert() {
        let storage = PluginStorage::test_storage();
        storage
            .set("com.bedcode.test", "k1", serde_json::json!({"n": 1}))
            .await
            .expect("set");
        let got = storage.get("com.bedcode.test", "k1").await.expect("get");
        assert_eq!(got, Some(serde_json::json!({"n": 1})));
        // upsert 覆盖旧值
        storage
            .set("com.bedcode.test", "k1", serde_json::json!({"n": 2}))
            .await
            .expect("set again");
        let got = storage.get("com.bedcode.test", "k1").await.expect("get");
        assert_eq!(got, Some(serde_json::json!({"n": 2})));
        // 不存在返回 None（非空）
        let miss = storage.get("com.bedcode.test", "nope").await.expect("get");
        assert_eq!(miss, None);
    }

    #[tokio::test]
    async fn per_plugin_isolation() {
        let storage = PluginStorage::test_storage();
        storage
            .set("com.bedcode.a", "k", serde_json::json!("va"))
            .await
            .expect("set a");
        storage
            .set("com.bedcode.b", "k", serde_json::json!("vb"))
            .await
            .expect("set b");
        assert_eq!(
            storage.get("com.bedcode.a", "k").await.expect("get a"),
            Some(serde_json::json!("va"))
        );
        assert_eq!(
            storage.get("com.bedcode.b", "k").await.expect("get b"),
            Some(serde_json::json!("vb"))
        );
        // 删除 b 不影响 a
        storage.delete("com.bedcode.b", "k").await.expect("delete b");
        assert_eq!(
            storage.get("com.bedcode.a", "k").await.expect("get a"),
            Some(serde_json::json!("va"))
        );
    }

    #[tokio::test]
    async fn clear_plugin_removes_only_owner() {
        let storage = PluginStorage::test_storage();
        storage
            .set("com.bedcode.a", "k", serde_json::json!("va"))
            .await
            .expect("set a");
        storage
            .set("com.bedcode.a", "k2", serde_json::json!("va2"))
            .await
            .expect("set a2");
        storage
            .set("com.bedcode.b", "k", serde_json::json!("vb"))
            .await
            .expect("set b");
        storage.clear_plugin("com.bedcode.a").await.expect("clear a");
        assert_eq!(storage.get("com.bedcode.a", "k").await.expect("get"), None);
        assert_eq!(storage.get("com.bedcode.a", "k2").await.expect("get"), None);
        assert_eq!(
            storage.get("com.bedcode.b", "k").await.expect("get b"),
            Some(serde_json::json!("vb"))
        );
    }

    /// 文件落盘 → 主库迁移：数据入表 + 旧文件删除；幂等（重复迁移无副作用）
    #[test]
    fn file_store_migrates_into_db() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let plugins_dir = tmp.path().join(PLUGIN_STORAGE_DIR);
        std::fs::create_dir_all(&plugins_dir).expect("create plugins dir");
        // 旧文件：两个插件各若干 key
        let map_a: HashMap<String, Value> = HashMap::from([
            ("k1".to_string(), serde_json::json!(1)),
            ("k2".to_string(), serde_json::json!({"s": "v"})),
        ]);
        let map_b: HashMap<String, Value> = HashMap::from([("only".to_string(), serde_json::json!(true))]);
        std::fs::write(
            plugins_dir.join("com.bedcode.a.json"),
            serde_json::to_string(&map_a).expect("serialize"),
        )
        .expect("write a");
        std::fs::write(
            plugins_dir.join("com.bedcode.b.json"),
            serde_json::to_string(&map_b).expect("serialize"),
        )
        .expect("write b");
        // 非 .json 文件不碰（05a 插件私有库 .db 同目录）
        std::fs::write(plugins_dir.join("com.bedcode.x.db"), b"not-json").expect("write db");

        let storage = PluginStorage::test_storage();
        storage.migrate_file_store_to_db(tmp.path()).expect("migrate");

        let rt = tokio::runtime::Runtime::new().expect("runtime");
        assert_eq!(
            rt.block_on(storage.get("com.bedcode.a", "k1")).expect("get"),
            Some(serde_json::json!(1))
        );
        assert_eq!(
            rt.block_on(storage.get("com.bedcode.a", "k2")).expect("get"),
            Some(serde_json::json!({"s": "v"}))
        );
        assert_eq!(
            rt.block_on(storage.get("com.bedcode.b", "only")).expect("get"),
            Some(serde_json::json!(true))
        );
        // 旧文件已删除（旧读路径彻底移除）；.db 仍在
        assert!(!plugins_dir.join("com.bedcode.a.json").exists());
        assert!(!plugins_dir.join("com.bedcode.b.json").exists());
        assert!(plugins_dir.join("com.bedcode.x.db").exists());
        // 幂等：再次迁移无副作用（无文件可搬，且不报错）
        storage.migrate_file_store_to_db(tmp.path()).expect("migrate again");
        assert_eq!(
            rt.block_on(storage.get("com.bedcode.a", "k1")).expect("get"),
            Some(serde_json::json!(1))
        );
    }

    /// DB 已有值优先：迁移不得用旧文件覆盖更新值（INSERT OR IGNORE 语义）
    #[test]
    fn file_store_migration_never_clobbers_db() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let plugins_dir = tmp.path().join(PLUGIN_STORAGE_DIR);
        std::fs::create_dir_all(&plugins_dir).expect("create plugins dir");
        let map_a: HashMap<String, Value> = HashMap::from([("k".to_string(), serde_json::json!("stale"))]);
        std::fs::write(
            plugins_dir.join("com.bedcode.a.json"),
            serde_json::to_string(&map_a).expect("serialize"),
        )
        .expect("write");

        let storage = PluginStorage::test_storage();
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        rt.block_on(storage.set("com.bedcode.a", "k", serde_json::json!("fresh")))
            .expect("set fresh before migrate");
        storage.migrate_file_store_to_db(tmp.path()).expect("migrate");
        assert_eq!(
            rt.block_on(storage.get("com.bedcode.a", "k")).expect("get"),
            Some(serde_json::json!("fresh")),
            "旧文件数据不得覆盖 DB 已有值"
        );
    }
}
