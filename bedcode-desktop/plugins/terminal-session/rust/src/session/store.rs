//! 会话登记存储端口（会话引擎整体下沉 P1）：真源 = 插件私有库
//! （`host-plugin-database`，`storage` 权限）
//!
//! 端口按**行语义**（而非 SQL）暴露：状态机与时间戳策略留在 [`super::ops`]、
//! 缓存与降级留在 [`super::registry`]，可被 native 单测完整覆盖；SQL 与列映射
//! 只存在于 wasm 实现里（与 `config/store.rs` / `auth_records/store.rs` 同模式
//! ——native 链接不引用 wasm 专属 import 符号）。
//!
//! 表结构（插件私有库，无主库 `plugin_<id>_` 前缀约束）：
//! - `sessions`：会话记录一行（`status` / `canonical_renderer` 写 JSON 文本，
//!   `Error` 载荷与 wire 形状无损）
//! - `session_annotations`：注解槽 `(session_id, key) → value`（宿主
//!   `annotate` 原语的镜像；会话移除时连带清理）
//!
//! **进程域事实对账**（见 [`SessionStore::clear_all`]）：会话与 PTY 同生命周期，
//! 宿主真源是进程内存——进程重启后旧行不可回收，故激活时清表（调用点
//! [super::ensure_schema_via_host]），避免后续读取面看到幽灵会话。

use super::model::{SessionRecord, SessionStatus};

/// 建表语句（逐条执行——`plugin_db_execute` 是单语句版）
pub const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS sessions (\
     id TEXT PRIMARY KEY, \
     pty_id TEXT, \
     config_id TEXT NOT NULL, \
     name TEXT NOT NULL, \
     status TEXT NOT NULL, \
     created_at TEXT NOT NULL, \
     started_at TEXT, \
     stopped_at TEXT, \
     canonical_renderer TEXT, \
     owner TEXT, \
     updated_at TEXT NOT NULL)",
    "CREATE INDEX IF NOT EXISTS idx_sessions_config_id ON sessions(config_id)",
    "CREATE TABLE IF NOT EXISTS session_annotations (\
     session_id TEXT NOT NULL, \
     key TEXT NOT NULL, \
     value TEXT NOT NULL, \
     PRIMARY KEY (session_id, key))",
];

/// 会话登记存储端口（真源读写；排序与解读归调用方）
pub trait SessionStore {
    /// 建表（幂等）
    fn ensure_schema(&self) -> Result<(), String>;
    /// 全表（按 `created_at, id` 稳定序——宿主 `HashMap` 迭代序不确定，本域给稳定序）
    fn all(&self) -> Result<Vec<SessionRecord>, String>;
    fn get(&self, id: &str) -> Result<Option<SessionRecord>, String>;
    /// 按 id 覆盖写（`INSERT OR REPLACE`）
    fn put(&self, record: &SessionRecord) -> Result<(), String>;
    /// 删除会话行，返回是否命中（未知 id 幂等 false）
    fn remove(&self, id: &str) -> Result<bool, String>;
    /// 某会话的注解槽（按 key 稳定序）
    fn all_annotations(&self, session_id: &str) -> Result<Vec<(String, String)>, String>;
    /// 写单个注解键（覆盖同键）
    fn put_annotation(&self, session_id: &str, key: &str, value: &str) -> Result<(), String>;
    /// 清某会话的全部注解（会话移除时连带清理）
    fn clear_annotations(&self, session_id: &str) -> Result<(), String>;
    /// 清空会话域两张表（进程启动对账），返回删除的会话行数
    fn clear_all(&self) -> Result<usize, String>;
}

// ==================== wasm：插件私有库实现 ====================

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use super::*;
    use bedcode_plugin_api::host::HostPluginDatabase;
    use bedcode_plugin_api::sql_params;
    use bedcode_plugin_api::wasm_host::WasmHost;

    /// 行（snake_case 列名，宿主 `query_to_json` 输出）→ 模型
    fn row_to_record(row: &serde_json::Value) -> Result<SessionRecord, String> {
        let required = |key: &str| -> Result<String, String> {
            row.get(key)
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| format!("session row missing column '{}': {}", key, row))
        };
        let status_text = required("status")?;
        let status: SessionStatus = serde_json::from_str(&status_text)
            .map_err(|e| format!("session row has invalid status JSON ({}): {}", status_text, e))?;
        // JSON 文本列：空串 / NULL 视为缺失（不静默降级为「无归属」以外的语义）
        let canonical_renderer = match row.get("canonical_renderer").and_then(|v| v.as_str()) {
            Some(raw) if !raw.trim().is_empty() => Some(
                serde_json::from_str(raw)
                    .map_err(|e| format!("session row has invalid canonicalRenderer JSON ({}): {}", raw, e))?,
            ),
            _ => None,
        };
        Ok(SessionRecord {
            id: required("id")?,
            pty_id: row.get("pty_id").and_then(|v| v.as_str()).map(str::to_string),
            config_id: required("config_id")?,
            name: required("name")?,
            status,
            created_at: required("created_at")?,
            started_at: row.get("started_at").and_then(|v| v.as_str()).map(str::to_string),
            stopped_at: row.get("stopped_at").and_then(|v| v.as_str()).map(str::to_string),
            canonical_renderer,
            owner: row.get("owner").and_then(|v| v.as_str()).map(str::to_string),
            updated_at: required("updated_at")?,
        })
    }

    /// JSON 数组（查询结果）→ 模型列表
    fn rows_to_records(rows: Option<serde_json::Value>) -> Result<Vec<SessionRecord>, String> {
        let rows = rows.unwrap_or_else(|| serde_json::json!([]));
        let array = rows
            .as_array()
            .ok_or_else(|| format!("plugin db query did not return an array: {}", rows))?
            .clone();
        array.iter().map(row_to_record).collect()
    }

    /// `status` / `canonical_renderer` → 写库文本
    fn status_text(status: &SessionStatus) -> Result<String, String> {
        serde_json::to_string(status).map_err(|e| format!("status serialize failed: {}", e))
    }

    fn canonical_text(
        canonical: &Option<crate::actions::RendererSource>,
    ) -> Result<Option<String>, String> {
        match canonical {
            None => Ok(None),
            Some(source) => serde_json::to_string(source)
                .map(Some)
                .map_err(|e| format!("canonicalRenderer serialize failed: {}", e)),
        }
    }

    impl SessionStore for WasmHost {
        fn ensure_schema(&self) -> Result<(), String> {
            for stmt in SCHEMA {
                self.plugin_db_execute(stmt)
                    .map_err(|e| format!("plugin db execute failed: {}", e.message))?;
            }
            Ok(())
        }

        fn all(&self) -> Result<Vec<SessionRecord>, String> {
            self.plugin_db_query("SELECT * FROM sessions ORDER BY created_at, id")
                .map_err(|e| format!("plugin db query failed: {}", e.message))
                .and_then(rows_to_records)
        }

        fn get(&self, id: &str) -> Result<Option<SessionRecord>, String> {
            let rows = self
                .plugin_db_query_params("SELECT * FROM sessions WHERE id = ?1", &sql_params![id])
                .map_err(|e| format!("plugin db query failed: {}", e.message))?;
            Ok(rows_to_records(rows)?.into_iter().next())
        }

        fn put(&self, record: &SessionRecord) -> Result<(), String> {
            let status = status_text(&record.status)?;
            let canonical = canonical_text(&record.canonical_renderer)?;
            self.plugin_db_execute_params(
                "INSERT OR REPLACE INTO sessions \
                 (id, pty_id, config_id, name, status, created_at, started_at, stopped_at, \
                  canonical_renderer, owner, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                &sql_params![
                    record.id,
                    record.pty_id,
                    record.config_id,
                    record.name,
                    status,
                    record.created_at,
                    record.started_at,
                    record.stopped_at,
                    canonical,
                    record.owner,
                    record.updated_at
                ],
            )
            .map_err(|e| format!("plugin db write failed: {}", e.message))?;
            Ok(())
        }

        fn remove(&self, id: &str) -> Result<bool, String> {
            let affected = self
                .plugin_db_execute_params("DELETE FROM sessions WHERE id = ?1", &sql_params![id])
                .map_err(|e| format!("plugin db delete failed: {}", e.message))?;
            Ok(affected > 0)
        }

        fn all_annotations(&self, session_id: &str) -> Result<Vec<(String, String)>, String> {
            let rows = self
                .plugin_db_query_params(
                    "SELECT key, value FROM session_annotations WHERE session_id = ?1 ORDER BY key",
                    &sql_params![session_id],
                )
                .map_err(|e| format!("plugin db query failed: {}", e.message))?;
            let rows = rows.unwrap_or_else(|| serde_json::json!([]));
            let array = rows
                .as_array()
                .ok_or_else(|| format!("plugin db query did not return an array: {}", rows))?;
            let mut out = Vec::with_capacity(array.len());
            for row in array {
                let key = row
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| format!("annotation row missing column 'key': {}", row))?;
                let value = row
                    .get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| format!("annotation row missing column 'value': {}", row))?;
                out.push((key.to_string(), value.to_string()));
            }
            Ok(out)
        }

        fn put_annotation(&self, session_id: &str, key: &str, value: &str) -> Result<(), String> {
            self.plugin_db_execute_params(
                "INSERT OR REPLACE INTO session_annotations (session_id, key, value) VALUES (?1, ?2, ?3)",
                &sql_params![session_id, key, value],
            )
            .map_err(|e| format!("plugin db write failed: {}", e.message))?;
            Ok(())
        }

        fn clear_annotations(&self, session_id: &str) -> Result<(), String> {
            self.plugin_db_execute_params(
                "DELETE FROM session_annotations WHERE session_id = ?1",
                &sql_params![session_id],
            )
            .map_err(|e| format!("plugin db delete failed: {}", e.message))?;
            Ok(())
        }

        fn clear_all(&self) -> Result<usize, String> {
            let affected = self
                .plugin_db_execute("DELETE FROM sessions")
                .map_err(|e| format!("plugin db delete failed: {}", e.message))?;
            self.plugin_db_execute("DELETE FROM session_annotations")
                .map_err(|e| format!("plugin db delete failed: {}", e.message))?;
            // `plugin_db_execute` 回传受影响行数（i32）；负数无意义，钳到 0
            Ok(affected.max(0) as usize)
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::actions::RendererSource;
    use std::sync::Mutex;

    /// 内存实现（native 单测注入）：行语义与私有库对齐
    #[derive(Default)]
    pub struct MockSessionStore {
        rows: Mutex<Vec<SessionRecord>>,
        annotations: Mutex<Vec<(String, String, String)>>,
    }

    impl MockSessionStore {
        pub fn new(rows: Vec<SessionRecord>) -> Self {
            Self {
                rows: Mutex::new(rows),
                annotations: Mutex::new(Vec::new()),
            }
        }

        pub fn record(id: &str, status: SessionStatus) -> SessionRecord {
            SessionRecord {
                id: id.to_string(),
                pty_id: None,
                config_id: "c1".to_string(),
                name: format!("会话-{id}"),
                status,
                created_at: "2026-09-23T09:00:00Z".to_string(),
                started_at: None,
                stopped_at: None,
                canonical_renderer: Some(RendererSource::Desktop),
                owner: Some("com.bedcode.terminal-session".to_string()),
                updated_at: "2026-09-23T09:00:00Z".to_string(),
            }
        }
    }

    impl SessionStore for MockSessionStore {
        fn ensure_schema(&self) -> Result<(), String> {
            Ok(())
        }

        fn all(&self) -> Result<Vec<SessionRecord>, String> {
            Ok(self.rows.lock().unwrap().clone())
        }

        fn get(&self, id: &str) -> Result<Option<SessionRecord>, String> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.id == id)
                .cloned())
        }

        fn put(&self, record: &SessionRecord) -> Result<(), String> {
            let mut rows = self.rows.lock().unwrap();
            match rows.iter_mut().find(|r| r.id == record.id) {
                Some(slot) => *slot = record.clone(),
                None => rows.push(record.clone()),
            }
            Ok(())
        }

        fn remove(&self, id: &str) -> Result<bool, String> {
            let mut rows = self.rows.lock().unwrap();
            let before = rows.len();
            rows.retain(|r| r.id != id);
            Ok(rows.len() != before)
        }

        fn all_annotations(&self, session_id: &str) -> Result<Vec<(String, String)>, String> {
            let mut rows: Vec<(String, String)> = self
                .annotations
                .lock()
                .unwrap()
                .iter()
                .filter(|(sid, _, _)| sid == session_id)
                .map(|(_, k, v)| (k.clone(), v.clone()))
                .collect();
            rows.sort();
            Ok(rows)
        }

        fn put_annotation(&self, session_id: &str, key: &str, value: &str) -> Result<(), String> {
            let mut rows = self.annotations.lock().unwrap();
            match rows
                .iter_mut()
                .find(|(sid, k, _)| sid == session_id && k == key)
            {
                Some(slot) => slot.2 = value.to_string(),
                None => rows.push((session_id.to_string(), key.to_string(), value.to_string())),
            }
            Ok(())
        }

        fn clear_annotations(&self, session_id: &str) -> Result<(), String> {
            self.annotations
                .lock()
                .unwrap()
                .retain(|(sid, _, _)| sid != session_id);
            Ok(())
        }

        fn clear_all(&self) -> Result<usize, String> {
            let removed = self.rows.lock().unwrap().len();
            self.rows.lock().unwrap().clear();
            self.annotations.lock().unwrap().clear();
            Ok(removed)
        }
    }

    /// 内存实现的行语义（put 覆盖 / remove 命中 / 注解覆盖 / clear_all 计数）——
    /// 它是策略单测的依赖底座，先自证
    #[test]
    fn mock_store_roundtrip() {
        let store = MockSessionStore::new(vec![]);
        store.ensure_schema().expect("schema");
        store
            .put(&MockSessionStore::record("s1", SessionStatus::Running))
            .expect("put");
        assert_eq!(store.all().unwrap().len(), 1);

        let mut updated = MockSessionStore::record("s1", SessionStatus::Stopped);
        updated.name = "改名后".to_string();
        store.put(&updated).expect("put update");
        assert_eq!(store.all().unwrap().len(), 1, "同 id 覆盖不新增行");
        assert_eq!(store.get("s1").unwrap().unwrap().name, "改名后");
        assert_eq!(store.get("s1").unwrap().unwrap().status, SessionStatus::Stopped);

        assert!(store.remove("s1").unwrap());
        assert!(!store.remove("s1").unwrap(), "未知 id 幂等 false");

        store.put_annotation("s1", "taskStatus", "asking").unwrap();
        store.put_annotation("s1", "taskStatus", "idle").unwrap();
        store.put_annotation("s1", "taskReason", "等待").unwrap();
        assert_eq!(
            store.all_annotations("s1").unwrap(),
            vec![
                ("taskReason".to_string(), "等待".to_string()),
                ("taskStatus".to_string(), "idle".to_string()),
            ],
            "同键覆盖 + 按 key 稳定序"
        );
        store.clear_annotations("s1").unwrap();
        assert!(store.all_annotations("s1").unwrap().is_empty());

        store.put_annotation("s2", "k", "v").unwrap();
        store
            .put(&MockSessionStore::record("s2", SessionStatus::Running))
            .unwrap();
        assert_eq!(store.clear_all().unwrap(), 1, "clear_all 返回删除的会话行数");
        assert!(store.all().unwrap().is_empty());
        assert!(store.all_annotations("s2").unwrap().is_empty(), "注解一并清空");
    }
}
