//! Plugin Storage
//!
//! 插件持久化存储 — SQLite plugin_storage 表
//! 按 plugin_id 隔离，插件只能读写自己的空间

use crate::db::Database;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 插件存储管理器
pub struct PluginStorage {
    db: Arc<Mutex<Database>>,
}

impl PluginStorage {
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }

    /// 获取插件存储值
    pub async fn get(&self, plugin_id: &str, key: &str) -> crate::Result<Option<serde_json::Value>> {
        let db = self.db.lock().await;
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT value FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2"
        )?;

        let result = stmt.query_row(
            rusqlite::params![plugin_id, key],
            |row| row.get::<_, String>(0),
        );

        match result {
            Ok(json_str) => {
                let value: serde_json::Value = serde_json::from_str(&json_str)?;
                Ok(Some(value))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(crate::AppError::Database(e)),
        }
    }

    /// 设置插件存储值
    pub async fn set(&self, plugin_id: &str, key: &str, value: serde_json::Value) -> crate::Result<()> {
        let db = self.db.lock().await;
        let json_str = serde_json::to_string(&value)?;
        let now = Utc::now().to_rfc3339();

        db.conn().execute(
            "INSERT INTO plugin_storage (plugin_id, key, value, updated_at) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(plugin_id, key) DO UPDATE SET value = ?3, updated_at = ?4",
            rusqlite::params![plugin_id, key, json_str, now],
        )?;

        Ok(())
    }

    /// 删除插件存储值
    pub async fn delete(&self, plugin_id: &str, key: &str) -> crate::Result<()> {
        let db = self.db.lock().await;
        db.conn().execute(
            "DELETE FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2",
            rusqlite::params![plugin_id, key],
        )?;
        Ok(())
    }

    /// 清空插件所有存储（插件卸载时使用）
    pub async fn clear_all(&self, plugin_id: &str) -> crate::Result<()> {
        let db = self.db.lock().await;
        db.conn().execute(
            "DELETE FROM plugin_storage WHERE plugin_id = ?1",
            rusqlite::params![plugin_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Arc<Mutex<Database>> {
        let db = Database::new(&std::path::Path::new(":memory:")).unwrap();
        db.init_schema().unwrap();
        Arc::new(Mutex::new(db))
    }

    #[tokio::test]
    async fn test_storage_get_set_delete() {
        let db = test_db().await;
        let storage = PluginStorage::new(db);

        assert!(storage.get("plugin-1", "key1").await.unwrap().is_none());

        storage.set("plugin-1", "key1", serde_json::json!("hello")).await.unwrap();
        let val = storage.get("plugin-1", "key1").await.unwrap();
        assert_eq!(val, Some(serde_json::json!("hello")));

        storage.set("plugin-1", "key1", serde_json::json!("world")).await.unwrap();
        let val = storage.get("plugin-1", "key1").await.unwrap();
        assert_eq!(val, Some(serde_json::json!("world")));

        storage.delete("plugin-1", "key1").await.unwrap();
        assert!(storage.get("plugin-1", "key1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_storage_isolation() {
        let db = test_db().await;
        let storage = PluginStorage::new(db);

        storage.set("plugin-a", "key1", serde_json::json!("a")).await.unwrap();
        storage.set("plugin-b", "key1", serde_json::json!("b")).await.unwrap();

        assert_eq!(storage.get("plugin-a", "key1").await.unwrap(), Some(serde_json::json!("a")));
        assert_eq!(storage.get("plugin-b", "key1").await.unwrap(), Some(serde_json::json!("b")));

        storage.clear_all("plugin-a").await.unwrap();
        assert!(storage.get("plugin-a", "key1").await.unwrap().is_none());
        assert_eq!(storage.get("plugin-b", "key1").await.unwrap(), Some(serde_json::json!("b")));
    }
}
