//! 认证记录存储端口（2026-09-22 认证记录下沉）：真源 = 插件私有库
//! （host-plugin-database，`storage` 权限）
//!
//! **为什么需要它**：用户裁定（2026-09-22）`pairings` / `connection_history`
//! 不再留宿主主库（AGENTS.md §8「留宿主」口径撤销），认证记录归认证中心
//! （本插件）自持。宿主 host-auth 记录面原语（v18/v19 七函数）随之退役；
//! 本端口是配对设备 / 连接历史的**唯一写者与读源**。
//!
//! 与 [`crate::config::store`] / [`crate::quick_actions::store`] 同模式：端口按
//! **行语义**（而非 SQL）暴露，校验 / 排序 / 迁移编排等策略留在 [`super::ops`]，
//! 可被 native 单测完整覆盖；SQL 与列映射只存在于 wasm 实现里（native 链接不
//! 引用 wasm 专属 import 符号）。
//!
//! 表结构（插件私有库，无主库 `plugin_<id>_` 前缀约束）：
//! - `auth_pairings`：配对设备公开记录（**不含凭据列** `public_key` /
//!   `session_token`——公钥随 §8 凭据红线留宿主 `plugin_secrets`，session_token
//!   是死列直接丢弃，凭据零复制）
//! - `auth_connection_history`：连接历史（无凭据列）
//! - `plugin_meta`：迁移 marker（与配置域共用同一张表）

use super::model::{PairingRecord, ConnectionEventRecord};

/// 建表语句（逐条执行——`plugin_db_execute` 是单语句版）
pub const SCHEMA: &[&str] = &[
    // 配对设备：公开记录面（id / 展示名 / 指纹 / 地址 / 稳定 UID 锚点 / 时间戳 /
    // 计数 / 活跃标记）。软删（is_active=0）保留行——撤销检测依赖「已撤销记录
    // 仍可见」，删除行会让撤销决策 fail-open
    "CREATE TABLE IF NOT EXISTS auth_pairings (\
     id TEXT PRIMARY KEY, \
     device_name TEXT NOT NULL, \
     device_fingerprint TEXT NOT NULL UNIQUE, \
     address TEXT, \
     uid_hash TEXT, \
     paired_at TEXT NOT NULL, \
     last_seen TEXT, \
     connect_count INTEGER NOT NULL DEFAULT 1, \
     is_active INTEGER NOT NULL DEFAULT 1)",
    "CREATE INDEX IF NOT EXISTS idx_auth_pairings_fingerprint ON auth_pairings(device_fingerprint)",
    // 连接历史：按 device_id（= auth_pairings.id）键控
    "CREATE TABLE IF NOT EXISTS auth_connection_history (\
     id INTEGER PRIMARY KEY AUTOINCREMENT, \
     device_id TEXT NOT NULL, \
     auth_method TEXT NOT NULL, \
     result TEXT NOT NULL, \
     address TEXT, \
     connected_at TEXT NOT NULL, \
     disconnected_at TEXT)",
    "CREATE INDEX IF NOT EXISTS idx_auth_history_device ON auth_connection_history(device_id, connected_at DESC)",
];

/// 迁移 marker 键：存在即「已迁移」（一次性语义；幂等靠 marker 存在性检查）。
/// 与配置域 `config.migrated_at` 同形（域前缀隔离）
pub const MIGRATION_MARKER: &str = "auth_records.migrated_at";

/// 认证记录存储端口（真源读写；排序与解读归调用方）
pub trait AuthRecordsStore {
    /// 建表（幂等）
    fn ensure_schema(&self) -> Result<(), String>;

    // ==================== 配对设备 ====================

    /// 全部配对记录（**含软删行**，不排序——撤销检测依赖「已撤销记录仍可见」，
    /// 过滤与排序归 [`super::ops`]）
    fn pairings_all(&self) -> Result<Vec<PairingRecord>, String>;
    /// 按指纹查配对记录（含软删；供撤销检测 / 生物凭证判定）
    fn pairing_by_fingerprint(&self, fingerprint: &str) -> Result<Option<PairingRecord>, String>;
    /// 写入/归并配对记录（`INSERT OR IGNORE` + 更新；uid_hash 归并由调用方编排）
    fn pairing_put(&self, record: &PairingRecord) -> Result<(), String>;
    /// 软删配对（`is_active = 0`）；返回是否命中了活跃记录
    fn pairing_revoke(&self, id: &str) -> Result<bool, String>;

    // ==================== 连接历史 ====================

    /// 某设备的连接历史（倒序由调用方排序；此处原样返回）
    fn history_by_device(&self, device_id: &str) -> Result<Vec<ConnectionEventRecord>, String>;
    /// 追加连接事件（指纹解析 device_id 由调用方完成；本端口只落库）
    fn history_append(&self, record: &ConnectionEventRecord) -> Result<(), String>;
    /// 回填断开时间（最近一条未关闭的连接）
    fn history_close_open(&self, device_id: &str, disconnected_at: &str) -> Result<bool, String>;
    /// 清空某设备的连接历史；返回是否命中至少一条
    fn history_clear(&self, device_id: &str) -> Result<bool, String>;

    // ==================== 迁移账本 ====================

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

    /// 行（snake_case 列名，宿主 `query_to_json` 输出）→ 配对模型
    fn row_to_pairing(row: &serde_json::Value) -> Result<PairingRecord, String> {
        let required = |key: &str| -> Result<String, String> {
            row.get(key)
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| format!("auth pairing row missing column '{}': {}", key, row))
        };
        Ok(PairingRecord {
            id: required("id")?,
            device_name: required("device_name")?,
            device_fingerprint: required("device_fingerprint")?,
            address: row.get("address").and_then(|v| v.as_str()).map(str::to_string),
            uid_hash: row.get("uid_hash").and_then(|v| v.as_str()).map(str::to_string),
            paired_at: required("paired_at")?,
            last_seen: row.get("last_seen").and_then(|v| v.as_str()).map(str::to_string),
            connect_count: row.get("connect_count").and_then(|v| v.as_i64()).unwrap_or(0) as u32,
            is_active: row.get("is_active").and_then(|v| v.as_i64()).unwrap_or(1) != 0,
        })
    }

    /// 行 → 连接历史模型
    fn row_to_history(row: &serde_json::Value) -> Result<ConnectionEventRecord, String> {
        let required = |key: &str| -> Result<String, String> {
            row.get(key)
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| format!("auth history row missing column '{}': {}", key, row))
        };
        Ok(ConnectionEventRecord {
            id: row.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
            device_id: required("device_id")?,
            auth_method: required("auth_method")?,
            result: required("result")?,
            address: row.get("address").and_then(|v| v.as_str()).map(str::to_string),
            connected_at: required("connected_at")?,
            disconnected_at: row.get("disconnected_at").and_then(|v| v.as_str()).map(str::to_string),
        })
    }

    /// JSON 数组（查询结果）→ 模型列表；`None` / 非数组 → 空
    fn rows_to<T>(rows: Option<serde_json::Value>, mapper: impl Fn(&serde_json::Value) -> Result<T, String>) -> Result<Vec<T>, String> {
        let rows = rows.unwrap_or_else(|| serde_json::json!([]));
        let array = rows
            .as_array()
            .ok_or_else(|| format!("plugin db query did not return an array: {}", rows))?
            .clone();
        array.iter().map(mapper).collect()
    }

    impl AuthRecordsStore for WasmHost {
        fn ensure_schema(&self) -> Result<(), String> {
            for stmt in SCHEMA {
                self.plugin_db_execute(stmt)
                    .map_err(|e| format!("plugin db execute failed: {}", e.message))?;
            }
            Ok(())
        }

        fn pairings_all(&self) -> Result<Vec<PairingRecord>, String> {
            self.plugin_db_query("SELECT * FROM auth_pairings")
                .map_err(|e| format!("plugin db query failed: {}", e.message))
                .and_then(|rows| rows_to(rows, row_to_pairing))
        }

        fn pairing_by_fingerprint(&self, fingerprint: &str) -> Result<Option<PairingRecord>, String> {
            let rows = self
                .plugin_db_query_params(
                    "SELECT * FROM auth_pairings WHERE device_fingerprint = ?1",
                    &sql_params![fingerprint],
                )
                .map_err(|e| format!("plugin db query failed: {}", e.message))?;
            let mut list = rows_to(rows, row_to_pairing)?;
            Ok(list.pop())
        }

        fn pairing_put(&self, record: &PairingRecord) -> Result<(), String> {
            self.plugin_db_execute_params(
                "INSERT INTO auth_pairings \
                 (id, device_name, device_fingerprint, address, uid_hash, paired_at, last_seen, connect_count, is_active) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
                 ON CONFLICT(device_fingerprint) DO UPDATE SET \
                   device_name = excluded.device_name, \
                   address = excluded.address, \
                   uid_hash = COALESCE(excluded.uid_hash, auth_pairings.uid_hash), \
                   last_seen = excluded.last_seen, \
                   connect_count = excluded.connect_count, \
                   is_active = excluded.is_active",
                &sql_params![
                    record.id,
                    record.device_name,
                    record.device_fingerprint,
                    record.address,
                    record.uid_hash,
                    record.paired_at,
                    record.last_seen,
                    record.connect_count,
                    if record.is_active { 1 } else { 0 }
                ],
            )
            .map_err(|e| format!("plugin db write failed: {}", e.message))?;
            Ok(())
        }

        fn pairing_revoke(&self, id: &str) -> Result<bool, String> {
            let changed = self
                .plugin_db_execute_params(
                    "UPDATE auth_pairings SET is_active = 0 WHERE id = ?1 AND is_active = 1",
                    &sql_params![id],
                )
                .map_err(|e| format!("plugin db write failed: {}", e.message))?;
            Ok(changed > 0)
        }

        fn history_by_device(&self, device_id: &str) -> Result<Vec<ConnectionEventRecord>, String> {
            let rows = self
                .plugin_db_query_params(
                    "SELECT * FROM auth_connection_history WHERE device_id = ?1 ORDER BY connected_at DESC",
                    &sql_params![device_id],
                )
                .map_err(|e| format!("plugin db query failed: {}", e.message))?;
            rows_to(rows, row_to_history)
        }

        fn history_append(&self, record: &ConnectionEventRecord) -> Result<(), String> {
            self.plugin_db_execute_params(
                "INSERT INTO auth_connection_history \
                 (device_id, auth_method, result, address, connected_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                &sql_params![
                    record.device_id,
                    record.auth_method,
                    record.result,
                    record.address,
                    record.connected_at
                ],
            )
            .map_err(|e| format!("plugin db write failed: {}", e.message))?;
            Ok(())
        }

        fn history_close_open(&self, device_id: &str, disconnected_at: &str) -> Result<bool, String> {
            let changed = self
                .plugin_db_execute_params(
                    "UPDATE auth_connection_history SET disconnected_at = ?1 \
                     WHERE id = (SELECT id FROM auth_connection_history \
                                 WHERE device_id = ?2 AND disconnected_at IS NULL \
                                 ORDER BY connected_at DESC LIMIT 1)",
                    &sql_params![disconnected_at, device_id],
                )
                .map_err(|e| format!("plugin db write failed: {}", e.message))?;
            Ok(changed > 0)
        }

        fn history_clear(&self, device_id: &str) -> Result<bool, String> {
            let changed = self
                .plugin_db_execute_params(
                    "DELETE FROM auth_connection_history WHERE device_id = ?1",
                    &sql_params![device_id],
                )
                .map_err(|e| format!("plugin db write failed: {}", e.message))?;
            Ok(changed > 0)
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
    pub struct MockAuthRecords {
        pub pairings: Mutex<Vec<PairingRecord>>,
        pub history: Mutex<Vec<ConnectionEventRecord>>,
        pub meta: Mutex<std::collections::HashMap<String, String>>,
    }

    impl MockAuthRecords {
        pub fn new(pairings: Vec<PairingRecord>) -> Self {
            Self {
                pairings: Mutex::new(pairings),
                history: Mutex::new(Vec::new()),
                meta: Mutex::new(std::collections::HashMap::new()),
            }
        }
    }

    impl AuthRecordsStore for MockAuthRecords {
        fn ensure_schema(&self) -> Result<(), String> {
            Ok(())
        }

        fn pairings_all(&self) -> Result<Vec<PairingRecord>, String> {
            Ok(self.pairings.lock().unwrap().clone())
        }

        fn pairing_by_fingerprint(&self, fingerprint: &str) -> Result<Option<PairingRecord>, String> {
            Ok(self
                .pairings
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.device_fingerprint == fingerprint)
                .cloned())
        }

        fn pairing_put(&self, record: &PairingRecord) -> Result<(), String> {
            let mut pairings = self.pairings.lock().unwrap();
            // 归并语义：指纹相同 → 更新既有行；uid_hash 命中（指纹不同，稳定设备
            // UID 再派生）→ 复用该行 id（连接历史不分裂）；全新 → 追加
            if let Some(existing) = pairings.iter_mut().find(|r| r.device_fingerprint == record.device_fingerprint) {
                *existing = record.clone();
            } else if let Some(hash) = record.uid_hash.as_deref() {
                if let Some(anchor) = pairings.iter_mut().find(|r| r.uid_hash.as_deref() == Some(hash)) {
                    let mut merged = record.clone();
                    merged.id = anchor.id.clone();
                    *anchor = merged;
                } else {
                    pairings.push(record.clone());
                }
            } else {
                pairings.push(record.clone());
            }
            Ok(())
        }

        fn pairing_revoke(&self, id: &str) -> Result<bool, String> {
            let mut pairings = self.pairings.lock().unwrap();
            let Some(record) = pairings.iter_mut().find(|r| r.id == id) else {
                return Ok(false);
            };
            if !record.is_active {
                return Ok(false);
            }
            record.is_active = false;
            Ok(true)
        }

        fn history_by_device(&self, device_id: &str) -> Result<Vec<ConnectionEventRecord>, String> {
            let mut rows: Vec<ConnectionEventRecord> = self
                .history
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.device_id == device_id)
                .cloned()
                .collect();
            rows.sort_by(|a, b| b.connected_at.cmp(&a.connected_at));
            Ok(rows)
        }

        fn history_append(&self, record: &ConnectionEventRecord) -> Result<(), String> {
            self.history.lock().unwrap().push(record.clone());
            Ok(())
        }

        fn history_close_open(&self, device_id: &str, disconnected_at: &str) -> Result<bool, String> {
            let mut history = self.history.lock().unwrap();
            let Some(record) = history
                .iter_mut()
                .filter(|r| r.device_id == device_id && r.disconnected_at.is_none())
                .max_by(|a, b| a.connected_at.cmp(&b.connected_at))
            else {
                return Ok(false);
            };
            record.disconnected_at = Some(disconnected_at.to_string());
            Ok(true)
        }

        fn history_clear(&self, device_id: &str) -> Result<bool, String> {
            let mut history = self.history.lock().unwrap();
            let before = history.len();
            history.retain(|r| r.device_id != device_id);
            Ok(history.len() < before)
        }

        fn marker(&self, key: &str) -> Result<Option<String>, String> {
            Ok(self.meta.lock().unwrap().get(key).cloned())
        }

        fn set_marker(&self, key: &str, value: &str) -> Result<(), String> {
            self.meta.lock().unwrap().insert(key.to_string(), value.to_string());
            Ok(())
        }
    }

    /// mock 撤销：软删保留行（撤销检测依赖软删可见）
    #[test]
    fn mock_revoke_soft_deletes_keeps_row() {
        let store = MockAuthRecords::new(vec![PairingRecord {
            id: "p-1".into(),
            device_name: "Phone".into(),
            device_fingerprint: "fp-1".into(),
            address: None,
            uid_hash: None,
            paired_at: "2026-09-19T00:00:00Z".into(),
            last_seen: None,
            connect_count: 1,
            is_active: true,
        }]);
        assert!(store.pairing_revoke("p-1").expect("revoke"));
        assert!(!store.pairing_revoke("p-1").expect("revoke again idempotent"));
        let all = store.pairings_all().expect("all");
        assert_eq!(all.len(), 1, "软删保留行");
        assert!(!all[0].is_active);
    }

    /// mock 历史：倒序由端口保证（ORDER BY connected_at DESC 语义）
    #[test]
    fn mock_history_sorted_desc() {
        let store = MockAuthRecords::default();
        store
            .history_append(&ConnectionEventRecord {
                id: 0,
                device_id: "d-1".into(),
                auth_method: "qr".into(),
                result: "success".into(),
                address: None,
                connected_at: "2026-09-19T00:00:00Z".into(),
                disconnected_at: None,
            })
            .expect("append");
        store
            .history_append(&ConnectionEventRecord {
                id: 0,
                device_id: "d-1".into(),
                auth_method: "jwt".into(),
                result: "success".into(),
                address: None,
                connected_at: "2026-09-20T00:00:00Z".into(),
                disconnected_at: None,
            })
            .expect("append");
        let rows = store.history_by_device("d-1").expect("list");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].auth_method, "jwt", "最新在前");
    }
}
