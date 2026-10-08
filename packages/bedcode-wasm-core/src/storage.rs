//! Plugin Storage（中立层）
//!
//! 插件持久化存储 — SQLite plugin_storage 表
//! 按 plugin_id 隔离，插件只能读写自己的空间
//! 同时存储系统级数据（如插件激活状态）
//!
//! **位置纪律（票 03：wasm_core 依赖单向化）**：原定义在
//! `crate::manager::storage`，使 `security` / `host_api` 只为用它就
//! 反向依赖 manager（见 `.scratch/2026-09-24-wasm-core-decouple/spec.md` C6）。
//! 归位到本中立层后只依赖 `crate::db`（SQLite 引擎，同在宿主内——ADR 0036 撤销
//! 票 07/08 的 crate 化），可被 `manager` / `host_api` / `security` 任意引用，
//! 自身不依赖任何 wasm_core 兄弟模块。

use crate::db::Database;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 系统级 plugin_id，用于存储非插件私有的全局数据
///
/// **本模块是真源**（系统级数据的写入方在这里：激活状态等）。同一判据的 fail-closed
/// 消费方是插件面存储原语（`host_api::storage::ensure_not_system_space`），它经再导出
/// 取**同一个值**——两处指同一份常量，不存在同值副本（票 08 期间真源一度在
/// `bedcode-sqlite-engine`，ADR 0036 撤销 crate 时一并归位）。
pub(crate) const SYSTEM_PLUGIN_ID: &str = "__system__";
/// 插件激活状态持久化 key
const ACTIVATION_STATE_KEY: &str = "activation_state";

/// 插件存储管理器
pub struct PluginStorage {
    db: Arc<Mutex<Database>>,
}

impl PluginStorage {
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }

    /// 共享底层数据库句柄（只给需要**直接查宿主主库表**的宿主组件用）
    ///
    /// 当前唯一消费者是授权策略与授权记录真源（`security::auth_policy`，表
    /// `plugin_auth_policies` / `plugin_auth_records` 不在 `plugin_storage` 里）：
    /// 它由 `FsAuthChecker` 从本模块取得句柄构造，避免 `WasmRuntime` 再多传一路 db。
    /// 插件可见面仍只有 `get` / `set` / `delete`——本访问器不进任何原语。
    ///
    /// `pub(crate)`（R-10）：全量 `Arc<Mutex<Database>>` 句柄暴露给任意持有者
    /// 会绕过窄存储隔离直读宿主每张表；本句柄只应在 crate 内（安全/能力模块）
    /// 出现，外部（SDK / 其它 crate）不得触达。
    pub(crate) fn db(&self) -> Arc<Mutex<Database>> {
        self.db.clone()
    }

    /// 获取插件存储值
    pub async fn get(&self, plugin_id: &str, key: &str) -> crate::Result<Option<serde_json::Value>> {
        let db = self.db.lock().await;
        let conn = db.conn();
        let mut stmt = conn.prepare("SELECT value FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2")?;

        let result = stmt.query_row(rusqlite::params![plugin_id, key], |row| row.get::<_, String>(0));

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

    /// 原子读-改-写（同一 DB 锁内完成 get → f → set，跨调用窗口串行化同 key 并发修改）
    ///
    /// 现有 `get` / `set` 各自独立加锁，读-改-写跨锁：并发写同一 key 时后提交覆盖
    /// 先提交（lost update）。本原语把整段读写放进一次锁持有，读到的必是最近提交值、
    /// 写出的必落在下一个读者之前——比如审批记录的 approve/revoke
    /// （`security::approval`）两个实例并发时不再互相覆盖。
    ///
    /// `f` 返回写入的 value（始终 upsert）；实现内直接读写 conn，**不得**再调
    /// `get` / `set` / `delete`（它们会再抢同一把锁 → 自死锁）。
    pub async fn update<F>(&self, plugin_id: &str, key: &str, f: F) -> crate::Result<()>
    where
        F: FnOnce(Option<serde_json::Value>) -> crate::Result<serde_json::Value>,
    {
        let db = self.db.lock().await;
        let conn = db.conn();
        let previous = {
            let mut stmt = conn.prepare("SELECT value FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2")?;
            match stmt.query_row(rusqlite::params![plugin_id, key], |row| row.get::<_, String>(0)) {
                Ok(json_str) => Some(serde_json::from_str::<serde_json::Value>(&json_str)?),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(crate::AppError::Database(e)),
            }
        };
        let next = f(previous)?;
        let json_str = serde_json::to_string(&next)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO plugin_storage (plugin_id, key, value, updated_at) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(plugin_id, key) DO UPDATE SET value = ?3, updated_at = ?4",
            rusqlite::params![plugin_id, key, json_str, now],
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

    // ==================== System-level: Activation State ====================

    /// 保存插件激活状态映射（plugin_id → is_activated）
    ///
    /// 使用系统级 plugin_id `__system__` 隔离，不与任何插件数据冲突
    pub async fn save_activated_plugins(&self, activated: &HashMap<String, bool>) -> crate::Result<()> {
        let value = serde_json::to_value(activated)?;
        self.set(SYSTEM_PLUGIN_ID, ACTIVATION_STATE_KEY, value).await
    }

    /// 加载插件激活状态映射
    ///
    /// 首次启动或数据损坏时返回空 HashMap，**并真正清掉坏行**（R-06）——
    /// 旧实现损坏时返回 Err 且不重整：损坏行每次启动都失败，激活状态从此
    /// 永不恢复（与 approval S-10 同一 fail-visible 反面教材）。复位后下次
    /// 启动从干净状态继续。
    pub async fn load_activated_plugins(&self) -> crate::Result<HashMap<String, bool>> {
        match self.get(SYSTEM_PLUGIN_ID, ACTIVATION_STATE_KEY).await? {
            Some(value) => match serde_json::from_value::<HashMap<String, bool>>(value) {
                Ok(map) => Ok(map),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to parse activation state, resetting");
                    if let Err(clear_err) = self.delete(SYSTEM_PLUGIN_ID, ACTIVATION_STATE_KEY).await {
                        tracing::warn!(error = %clear_err, "Failed to clear corrupted activation state");
                    }
                    Ok(HashMap::new())
                }
            },
            None => Ok(HashMap::new()),
        }
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

        storage
            .set("plugin-1", "key1", serde_json::json!("hello"))
            .await
            .unwrap();
        let val = storage.get("plugin-1", "key1").await.unwrap();
        assert_eq!(val, Some(serde_json::json!("hello")));

        storage
            .set("plugin-1", "key1", serde_json::json!("world"))
            .await
            .unwrap();
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

        assert_eq!(
            storage.get("plugin-a", "key1").await.unwrap(),
            Some(serde_json::json!("a"))
        );
        assert_eq!(
            storage.get("plugin-b", "key1").await.unwrap(),
            Some(serde_json::json!("b"))
        );

        storage.clear_all("plugin-a").await.unwrap();
        assert!(storage.get("plugin-a", "key1").await.unwrap().is_none());
        assert_eq!(
            storage.get("plugin-b", "key1").await.unwrap(),
            Some(serde_json::json!("b"))
        );
    }
}
