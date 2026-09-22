//! Database operations

use super::{
    Database, LegacyAuthRows, LegacyConnectionRow, LegacyPairingRow, LegacyQuickActionRow, Setting,
};
use crate::Result;
use chrono::{DateTime, Utc};

/// Helper function to parse datetime from string, with proper error handling for rusqlite
/// Helper function to parse datetime from string, with proper error handling for rusqlite
fn parse_datetime_sql(s: &str, field_name: &str) -> std::result::Result<DateTime<Utc>, rusqlite::Error> {
    s.parse()
        .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Invalid datetime for {}: {}", field_name, e)))
}

impl Database {
    /// legacy 主库 `quick_actions` 行只读视图（票 02 迁移 handoff 用）
    ///
    /// 表不存在（契约退役后的全新安装 / 已清理）→ `Ok(None)`；存在 → 全行
    /// （`sort_order` 升序，与旧业务读取同序）。`created_at` 保持 DB 原始字符串
    /// 不重解析——逐字节搬运给插件。
    pub fn list_legacy_quick_action_rows(&self) -> Result<Option<Vec<LegacyQuickActionRow>>> {
        let table_exists: bool = self.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='quick_actions')",
            [],
            |row| row.get(0),
        )?;
        if !table_exists {
            return Ok(None);
        }
        let mut stmt = self.conn().prepare(
            "SELECT id, name, content, icon, color, category, sort_order, created_at
             FROM quick_actions ORDER BY sort_order",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(LegacyQuickActionRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    content: row.get(2)?,
                    icon: row.get(3)?,
                    color: row.get(4)?,
                    category: row.get(5)?,
                    sort_order: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Some(rows))
    }

    /// legacy 主库 `quick_actions` 行播种（票 02 闭环测试用；契约退役后
    /// 仅测试/存量迁移场景需要写这条路径）——表不存在时先按旧 schema 建表
    /// （模拟存量旧库；全新安装的表在 schema.sql 已不再创建）
    #[cfg(test)]
    pub fn seed_legacy_quick_action_row(&self, row: &LegacyQuickActionRow) -> Result<()> {
        self.conn().execute(
            "CREATE TABLE IF NOT EXISTS quick_actions (\
             id TEXT PRIMARY KEY, \
             name TEXT NOT NULL, \
             content TEXT NOT NULL, \
             icon TEXT, \
             color TEXT, \
             category TEXT, \
             sort_order INTEGER DEFAULT 0, \
             created_at TEXT NOT NULL)",
            [],
        )?;
        self.conn().execute(
            "INSERT INTO quick_actions (id, name, content, icon, color, category, sort_order, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                row.id,
                row.name,
                row.content,
                row.icon,
                row.color,
                row.category,
                row.sort_order,
                row.created_at
            ],
        )?;
        Ok(())
    }

    // ==================== Legacy Auth Records（2026-09-22 认证记录下沉，迁移只读视图） ====================

    /// legacy 主库 `pairings` / `connection_history` 行只读视图（迁移 handoff 用）
    ///
    /// **表不存在**（全新安装 / 已清理）→ `Ok(None)`（两表以 `pairings` 为准探测：
    /// 同批退役，同一存量旧库要么都有要么都没有）。存在 → 公开行 + 生物凭证
    /// 公钥元组。时间戳保持 DB 原始字符串不重解析——逐字节搬运给插件；
    /// `session_token` 死列（零生产消费者）不读、不迁移（凭据零复制）。
    pub fn list_legacy_auth_rows(&self) -> Result<Option<LegacyAuthRows>> {
        let pairings_exist: bool = self.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='pairings')",
            [],
            |row| row.get(0),
        )?;
        if !pairings_exist {
            return Ok(None);
        }

        // 配对公开行（含软删行：撤销语义依赖「已撤销记录仍可见」）+ 公钥元组
        let mut pairing_stmt = self.conn().prepare(
            "SELECT id, device_name, device_fingerprint, public_key, address, uid_hash, \
             paired_at, last_seen, connect_count, is_active \
             FROM pairings ORDER BY paired_at",
        )?;
        let mut pairings = Vec::new();
        let mut biometric_public_keys = Vec::new();
        let pairing_rows = pairing_stmt.query_map([], |row| {
            let public_key: String = row.get(3)?;
            Ok((
                LegacyPairingRow {
                    id: row.get(0)?,
                    device_name: row.get(1)?,
                    device_fingerprint: row.get(2)?,
                    address: row.get(4)?,
                    uid_hash: row.get(5)?,
                    paired_at: row.get(6)?,
                    last_seen: row.get(7)?,
                    connect_count: row.get(8)?,
                    is_active: row.get::<_, i64>(9)? != 0,
                },
                // §8：公钥只随指纹元组进 plugin_secrets，不进入公开行 JSON
                (row.get::<_, String>(2)?, public_key),
            ))
        })?;
        for row in pairing_rows {
            let (public_row, key_pair) = row?;
            pairings.push(public_row);
            biometric_public_keys.push(key_pair);
        }

        // 连接历史行
        let mut history_stmt = self.conn().prepare(
            "SELECT id, device_id, auth_method, result, address, connected_at, disconnected_at \
             FROM connection_history ORDER BY connected_at",
        )?;
        let history = history_stmt
            .query_map([], |row| {
                Ok(LegacyConnectionRow {
                    id: row.get(0)?,
                    device_id: row.get(1)?,
                    auth_method: row.get(2)?,
                    result: row.get(3)?,
                    address: row.get(4)?,
                    connected_at: row.get(5)?,
                    disconnected_at: row.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(Some(LegacyAuthRows {
            pairings,
            history,
            biometric_public_keys,
        }))
    }

    /// legacy 主库 `pairings` / `connection_history` 行播种（迁移闭环测试用；
    /// 表不存在时先按旧 schema 建表——模拟存量旧库，全新安装的表在 schema.sql
    /// 已不再创建）。`session_token` 列也建出，验证迁移方明确不读（凭据零复制）。
    #[cfg(test)]
    pub fn seed_legacy_auth_rows(
        &self,
        pairing: &LegacyPairingRow,
        public_key: &str,
        session_token: Option<&str>,
        history: &[LegacyConnectionRow],
    ) -> Result<()> {
        self.conn()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS pairings (\
                 id TEXT PRIMARY KEY, \
                 device_name TEXT NOT NULL, \
                 device_fingerprint TEXT NOT NULL UNIQUE, \
                 public_key TEXT NOT NULL, \
                 address TEXT, \
                 session_token TEXT, \
                 uid_hash TEXT, \
                 paired_at TEXT NOT NULL, \
                 last_seen TEXT, \
                 connect_count INTEGER DEFAULT 1, \
                 is_active INTEGER DEFAULT 1);\
                 CREATE TABLE IF NOT EXISTS connection_history (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 device_id TEXT NOT NULL, \
                 auth_method TEXT NOT NULL, \
                 result TEXT NOT NULL, \
                 address TEXT, \
                 connected_at TEXT NOT NULL, \
                 disconnected_at TEXT);",
            )?;
        self.conn().execute(
            "INSERT OR REPLACE INTO pairings \
             (id, device_name, device_fingerprint, public_key, address, session_token, \
              uid_hash, paired_at, last_seen, connect_count, is_active) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                pairing.id,
                pairing.device_name,
                pairing.device_fingerprint,
                public_key,
                pairing.address,
                session_token,
                pairing.uid_hash,
                pairing.paired_at,
                pairing.last_seen,
                pairing.connect_count,
                pairing.is_active
            ],
        )?;
        for h in history {
            // 显式 id（旧表自增列也允许显式插入）；幂等：同 id 覆盖
            self.conn().execute(
                "INSERT OR REPLACE INTO connection_history \
                 (id, device_id, auth_method, result, address, connected_at, disconnected_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    h.id, h.device_id, h.auth_method, h.result, h.address, h.connected_at, h.disconnected_at
                ],
            )?;
        }
        Ok(())
    }

    /// 迁移成功后清理 legacy 表（本模块是两表存续的唯一意图方：schema.sql 已
    /// 不再创建；仅在存量旧库持有，迁移成功即 DROP，避免残留死表）
    pub fn drop_legacy_auth_tables(&self) -> Result<()> {
        self.conn().execute_batch(
            "DROP TABLE IF EXISTS connection_history; DROP TABLE IF EXISTS pairings;",
        )?;
        Ok(())
    }

    // ==================== Settings ====================

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let value = self
            .conn()
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                rusqlite::params![key],
                |row| row.get(0),
            )
            .ok();
        Ok(value)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![key, value, now],
        )?;
        Ok(())
    }

    pub fn get_all_settings(&self) -> Result<Vec<Setting>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT key, value, updated_at FROM settings ORDER BY key")?;

        let settings = stmt
            .query_map([], |row| {
                Ok(Setting {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    updated_at: parse_datetime_sql(&row.get::<_, String>(2)?, "updated_at")?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(settings)
    }
}

