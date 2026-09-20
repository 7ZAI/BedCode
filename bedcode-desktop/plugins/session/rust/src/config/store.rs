//! 配置存储端口（票 08）：真源 = 插件私有库（host-plugin-database，`storage` 权限）
//!
//! 端口按**行语义**（而非 SQL）暴露：校验 / 排序 / 迁移编排等策略留在
//! [`super::ops`]，可被 native 单测完整覆盖；SQL 与列映射只存在于 wasm 实现里。
//! 与 `trust/source.rs` 同模式——native 链接不引用 wasm 专属 import 符号。
//!
//! 表结构（插件私有库，无主库 `plugin_<id>_` 前缀约束）：
//! - `session_configs`：列与主库旧表同形（便于迁移逐字段搬运与宿主投影）
//! - `plugin_meta`：键值元数据（迁移 marker；范式同宿主 `peer_migration.rs`
//!   的「键存在性 = 版本戳」，避免引入 schema_version 框架）

use super::model::SessionConfig;

/// 建表语句（逐条执行——`plugin_db_execute` 是单语句版）
pub const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS session_configs (\
     id TEXT PRIMARY KEY, \
     name TEXT NOT NULL, \
     environment TEXT NOT NULL, \
     wsl_distro TEXT, \
     working_dir TEXT NOT NULL, \
     command TEXT NOT NULL, \
     auto_start INTEGER NOT NULL DEFAULT 0, \
     created_at TEXT NOT NULL, \
     updated_at TEXT NOT NULL)",
    "CREATE INDEX IF NOT EXISTS idx_session_configs_name ON session_configs(name)",
    "CREATE TABLE IF NOT EXISTS plugin_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
];

/// 迁移 marker 键：存在即「已迁移」（一次性语义；幂等靠逐条存在性检查）
pub const MIGRATION_MARKER: &str = "config.migrated_at";

/// 配置存储端口（真源读写；排序与解读归调用方）
pub trait ConfigStore {
    /// 建表（幂等）
    fn ensure_schema(&self) -> Result<(), String>;
    /// 全表（不排序——业务排序归 [`super::ops`]）
    fn all(&self) -> Result<Vec<SessionConfig>, String>;
    fn get(&self, id: &str) -> Result<Option<SessionConfig>, String>;
    /// 按 id 覆盖写（`INSERT OR REPLACE`）
    fn put(&self, config: &SessionConfig) -> Result<(), String>;
    /// 删除，返回是否命中（未知 id 幂等 false）
    fn remove(&self, id: &str) -> Result<bool, String>;
    fn marker(&self, key: &str) -> Result<Option<String>, String>;
    fn set_marker(&self, key: &str, value: &str) -> Result<(), String>;
}

// ==================== wasm：插件私有库实现 ====================

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use super::*;
    use bedcode_plugin_api::host::HostPluginDatabase;
    use bedcode_plugin_api::sql_params;
    use bedcode_plugin_api::wasm_host::WasmHost;

    /// 行（snake_case 列名，宿主 `query_to_json` 输出）→ 模型
    fn row_to_config(row: &serde_json::Value) -> Result<SessionConfig, String> {
        let required = |key: &str| -> Result<String, String> {
            row.get(key)
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| format!("config row missing column '{}': {}", key, row))
        };
        Ok(SessionConfig {
            id: required("id")?,
            name: required("name")?,
            environment: required("environment")?,
            wsl_distro: row
                .get("wsl_distro")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            working_dir: row
                .get("working_dir")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            command: row
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            auto_start: row.get("auto_start").and_then(|v| v.as_i64()).unwrap_or(0) != 0,
            created_at: required("created_at")?,
            updated_at: required("updated_at")?,
        })
    }

    /// JSON 数组（查询结果）→ 模型列表
    fn rows_to_configs(rows: Option<serde_json::Value>) -> Result<Vec<SessionConfig>, String> {
        let rows = rows.unwrap_or_else(|| serde_json::json!([]));
        let array = rows
            .as_array()
            .ok_or_else(|| format!("plugin db query did not return an array: {}", rows))?
            .clone();
        array.iter().map(row_to_config).collect()
    }

    impl ConfigStore for WasmHost {
        fn ensure_schema(&self) -> Result<(), String> {
            for stmt in SCHEMA {
                self.plugin_db_execute(stmt)
                    .map_err(|e| format!("plugin db execute failed: {}", e.message))?;
            }
            Ok(())
        }

        fn all(&self) -> Result<Vec<SessionConfig>, String> {
            self.plugin_db_query("SELECT * FROM session_configs")
                .map_err(|e| format!("plugin db query failed: {}", e.message))
                .and_then(rows_to_configs)
        }

        fn get(&self, id: &str) -> Result<Option<SessionConfig>, String> {
            let rows = self
                .plugin_db_query_params(
                    "SELECT * FROM session_configs WHERE id = ?1",
                    &sql_params![id],
                )
                .map_err(|e| format!("plugin db query failed: {}", e.message))?;
            Ok(rows_to_configs(rows)?.into_iter().next())
        }

        fn put(&self, config: &SessionConfig) -> Result<(), String> {
            self.plugin_db_execute_params(
                "INSERT OR REPLACE INTO session_configs \
                 (id, name, environment, wsl_distro, working_dir, command, auto_start, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                &sql_params![
                    config.id,
                    config.name,
                    config.environment,
                    config.wsl_distro,
                    config.working_dir,
                    config.command,
                    config.auto_start as i64,
                    config.created_at,
                    config.updated_at
                ],
            )
            .map_err(|e| format!("plugin db write failed: {}", e.message))?;
            Ok(())
        }

        fn remove(&self, id: &str) -> Result<bool, String> {
            let affected = self
                .plugin_db_execute_params(
                    "DELETE FROM session_configs WHERE id = ?1",
                    &sql_params![id],
                )
                .map_err(|e| format!("plugin db delete failed: {}", e.message))?;
            Ok(affected > 0)
        }

        fn marker(&self, key: &str) -> Result<Option<String>, String> {
            let rows = self
                .plugin_db_query_params(
                    "SELECT value FROM plugin_meta WHERE key = ?1",
                    &sql_params![key],
                )
                .map_err(|e| format!("plugin db query failed: {}", e.message))?;
            let rows = rows.unwrap_or_else(|| serde_json::json!([]));
            Ok(rows
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r.get("value"))
                .and_then(|v| v.as_str())
                .map(str::to_string))
        }

        fn set_marker(&self, key: &str, value: &str) -> Result<(), String> {
            self.plugin_db_execute_params(
                "INSERT OR REPLACE INTO plugin_meta (key, value) VALUES (?1, ?2)",
                &sql_params![key, value],
            )
            .map_err(|e| format!("plugin db write failed: {}", e.message))?;
            Ok(())
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 内存实现（native 单测注入）：行语义与私有库对齐
    #[derive(Default)]
    pub struct MockConfigStore {
        rows: Mutex<Vec<SessionConfig>>,
        markers: Mutex<Vec<(String, String)>>,
        /// 记录 `put` 调用次数（迁移幂等测试断言「不重复写入」用）
        pub puts: Mutex<u32>,
    }

    impl MockConfigStore {
        pub fn new(rows: Vec<SessionConfig>) -> Self {
            Self {
                rows: Mutex::new(rows),
                markers: Mutex::new(Vec::new()),
                puts: Mutex::new(0),
            }
        }

        pub fn put_count(&self) -> u32 {
            *self.puts.lock().unwrap()
        }

        /// 造一条配置（测试构造用）
        pub fn config(id: &str, name: &str) -> SessionConfig {
            SessionConfig {
                id: id.to_string(),
                name: name.to_string(),
                environment: "linux".to_string(),
                wsl_distro: None,
                working_dir: "/srv/proj".to_string(),
                command: "bash".to_string(),
                auto_start: false,
                created_at: "2026-09-01T00:00:00Z".to_string(),
                updated_at: "2026-09-01T00:00:00Z".to_string(),
            }
        }
    }

    impl ConfigStore for MockConfigStore {
        fn ensure_schema(&self) -> Result<(), String> {
            Ok(())
        }

        fn all(&self) -> Result<Vec<SessionConfig>, String> {
            Ok(self.rows.lock().unwrap().clone())
        }

        fn get(&self, id: &str) -> Result<Option<SessionConfig>, String> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }

        fn put(&self, config: &SessionConfig) -> Result<(), String> {
            *self.puts.lock().unwrap() += 1;
            let mut rows = self.rows.lock().unwrap();
            match rows.iter_mut().find(|c| c.id == config.id) {
                Some(slot) => *slot = config.clone(),
                None => rows.push(config.clone()),
            }
            Ok(())
        }

        fn remove(&self, id: &str) -> Result<bool, String> {
            let mut rows = self.rows.lock().unwrap();
            let before = rows.len();
            rows.retain(|c| c.id != id);
            Ok(rows.len() != before)
        }

        fn marker(&self, key: &str) -> Result<Option<String>, String> {
            Ok(self
                .markers
                .lock()
                .unwrap()
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone()))
        }

        fn set_marker(&self, key: &str, value: &str) -> Result<(), String> {
            let mut markers = self.markers.lock().unwrap();
            match markers.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = value.to_string(),
                None => markers.push((key.to_string(), value.to_string())),
            }
            Ok(())
        }
    }

    /// 内存实现的行语义（put 覆盖 / remove 命中 / marker 幂等）——它是策略单测的
    /// 依赖底座，先自证
    #[test]
    fn mock_store_roundtrip() {
        let store = MockConfigStore::new(vec![]);
        store.ensure_schema().expect("schema");
        store.put(&MockConfigStore::config("c1", "A")).expect("put");
        assert_eq!(store.all().unwrap().len(), 1);
        let mut updated = MockConfigStore::config("c1", "A2");
        updated.command = "zsh".to_string();
        store.put(&updated).expect("put update");
        assert_eq!(store.all().unwrap().len(), 1, "同 id 覆盖不新增行");
        assert_eq!(store.get("c1").unwrap().unwrap().command, "zsh");
        assert!(store.remove("c1").unwrap());
        assert!(!store.remove("c1").unwrap(), "未知 id 幂等 false");
        assert!(store.get("c1").unwrap().is_none());
        assert!(store.marker(MIGRATION_MARKER).unwrap().is_none());
        store
            .set_marker(MIGRATION_MARKER, "2026-09-20T00:00:00Z")
            .unwrap();
        assert_eq!(
            store.marker(MIGRATION_MARKER).unwrap().as_deref(),
            Some("2026-09-20T00:00:00Z")
        );
    }
}
