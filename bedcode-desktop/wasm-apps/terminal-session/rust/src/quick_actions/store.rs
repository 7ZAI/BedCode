//! 快捷指令存储端口（票 02）：真源 = 插件私有库（host-plugin-database，`storage` 权限）
//!
//! 与 [`crate::config::store`] 同模式：端口按**行语义**（而非 SQL）暴露，校验 /
//! 排序 / 迁移编排等策略留在 [`super::ops`]，可被 native 单测完整覆盖；
//! SQL 与列映射只存在于 wasm 实现里（native 链接不引用 wasm 专属 import 符号）。
//!
//! 表结构（插件私有库，无主库 `plugin_<id>_` 前缀约束）：
//! - `quick_actions`：列与主库旧表同形（便于迁移逐字段搬运与宿主投影）
//! - `plugin_meta`：键值元数据（迁移 marker；与配置域共用同一张表）

use super::model::QuickAction;

/// 建表语句（逐条执行——`plugin_db_execute` 是单语句版）
pub const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS quick_actions (\
     id TEXT PRIMARY KEY, \
     name TEXT NOT NULL, \
     content TEXT NOT NULL, \
     icon TEXT, \
     color TEXT, \
     category TEXT, \
     sort_order INTEGER NOT NULL DEFAULT 0, \
     created_at TEXT NOT NULL)",
    "CREATE INDEX IF NOT EXISTS idx_quick_actions_sort_order ON quick_actions(sort_order)",
];

/// 迁移 marker 键：存在即「已迁移」（一次性语义；幂等靠 marker 存在性检查）。
/// 与配置域 `config.migrated_at` 同形（域前缀隔离）
pub const MIGRATION_MARKER: &str = "quick_actions.migrated_at";

/// 快捷指令存储端口（真源读写；排序与解读归调用方）
pub trait QuickActionStore {
    /// 建表（幂等）
    fn ensure_schema(&self) -> Result<(), String>;
    /// 全表（不排序——业务排序归 [`super::ops`]）
    fn all(&self) -> Result<Vec<QuickAction>, String>;
    /// 按 id 覆盖写（`INSERT OR REPLACE`）
    fn put(&self, action: &QuickAction) -> Result<(), String>;
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
    fn row_to_action(row: &serde_json::Value) -> Result<QuickAction, String> {
        let required = |key: &str| -> Result<String, String> {
            row.get(key)
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| format!("quick action row missing column '{}': {}", key, row))
        };
        Ok(QuickAction {
            id: required("id")?,
            name: required("name")?,
            content: required("content")?,
            icon: row
                .get("icon")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            color: row
                .get("color")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            category: row
                .get("category")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            sort_order: row.get("sort_order").and_then(|v| v.as_i64()).unwrap_or(0),
            created_at: required("created_at")?,
        })
    }

    /// JSON 数组（查询结果）→ 模型列表
    fn rows_to_actions(rows: Option<serde_json::Value>) -> Result<Vec<QuickAction>, String> {
        let rows = rows.unwrap_or_else(|| serde_json::json!([]));
        let array = rows
            .as_array()
            .ok_or_else(|| format!("plugin db query did not return an array: {}", rows))?
            .clone();
        array.iter().map(row_to_action).collect()
    }

    impl QuickActionStore for WasmHost {
        fn ensure_schema(&self) -> Result<(), String> {
            for stmt in SCHEMA {
                self.plugin_db_execute(stmt)
                    .map_err(|e| format!("plugin db execute failed: {}", e.message))?;
            }
            Ok(())
        }

        fn all(&self) -> Result<Vec<QuickAction>, String> {
            self.plugin_db_query("SELECT * FROM quick_actions")
                .map_err(|e| format!("plugin db query failed: {}", e.message))
                .and_then(rows_to_actions)
        }

        fn put(&self, action: &QuickAction) -> Result<(), String> {
            self.plugin_db_execute_params(
                "INSERT OR REPLACE INTO quick_actions \
                 (id, name, content, icon, color, category, sort_order, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                &sql_params![
                    action.id,
                    action.name,
                    action.content,
                    action.icon,
                    action.color,
                    action.category,
                    action.sort_order,
                    action.created_at
                ],
            )
            .map_err(|e| format!("plugin db write failed: {}", e.message))?;
            Ok(())
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
    pub struct MockQuickActionStore {
        rows: Mutex<Vec<QuickAction>>,
        markers: Mutex<Vec<(String, String)>>,
    }

    impl MockQuickActionStore {
        pub fn new(rows: Vec<QuickAction>) -> Self {
            Self {
                rows: Mutex::new(rows),
                markers: Mutex::new(Vec::new()),
            }
        }

        pub fn all_rows(&self) -> Vec<QuickAction> {
            self.rows.lock().unwrap().clone()
        }

        /// 清空全部行（模拟「插件侧删除」场景）
        pub fn clear_rows(&self) {
            self.rows.lock().unwrap().clear();
        }

        /// 清空 marker（幂等重试窗口测试用）
        pub fn clear_markers(&self) {
            self.markers.lock().unwrap().clear();
        }
    }

    impl QuickActionStore for MockQuickActionStore {
        fn ensure_schema(&self) -> Result<(), String> {
            Ok(())
        }

        fn all(&self) -> Result<Vec<QuickAction>, String> {
            Ok(self.rows.lock().unwrap().clone())
        }

        fn put(&self, action: &QuickAction) -> Result<(), String> {
            // 与 wasm 实现的 INSERT OR REPLACE 同语义：同 id 覆盖不追加
            let mut rows = self.rows.lock().unwrap();
            if let Some(existing) = rows.iter_mut().find(|a| a.id == action.id) {
                *existing = action.clone();
            } else {
                rows.push(action.clone());
            }
            Ok(())
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
            self.markers
                .lock()
                .unwrap()
                .push((key.to_string(), value.to_string()));
            Ok(())
        }
    }
}
