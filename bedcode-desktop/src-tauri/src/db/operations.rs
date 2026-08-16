//! Database operations

use super::{ConnectionHistory, Database, Pairing, QuickAction, SessionConfig, Setting};
use super::CONNECTION_HISTORY_MAX_PER_DEVICE;
use crate::Result;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Helper function to parse datetime from string, with proper error handling for rusqlite
fn parse_datetime_sql(s: &str, field_name: &str) -> std::result::Result<DateTime<Utc>, rusqlite::Error> {
    s.parse().map_err(|e| {
        rusqlite::Error::InvalidParameterName(format!(
            "Invalid datetime for {}: {}",
            field_name, e
        ))
    })
}

/// Helper function to parse optional datetime from string for rusqlite
fn parse_optional_datetime_sql(s: Option<String>, field_name: &str) -> std::result::Result<Option<DateTime<Utc>>, rusqlite::Error> {
    s.map(|dt| parse_datetime_sql(&dt, field_name))
        .transpose()
}

impl Database {
    // ==================== Pairing Operations ====================

    pub fn add_pairing(&self, device_name: &str, fingerprint: &str, public_key: &str, address: Option<&str>) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        // UPSERT：新设备插入 connect_count=1，已有设备更新 last_seen + connect_count+1
        self.conn().execute(
            "INSERT INTO pairings (id, device_name, device_fingerprint, public_key, address, paired_at, last_seen, connect_count, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 1)
             ON CONFLICT(device_fingerprint) DO UPDATE SET
                device_name = excluded.device_name,
                public_key = excluded.public_key,
                address = excluded.address,
                last_seen = excluded.last_seen,
                connect_count = connect_count + 1,
                is_active = 1",
            rusqlite::params![id, device_name, fingerprint, public_key, address, now, now],
        )?;

        // 返回实际记录 id（冲突时取已有 id）
        let existing_id: String = self.conn().query_row(
            "SELECT id FROM pairings WHERE device_fingerprint = ?1",
            rusqlite::params![fingerprint],
            |row| row.get(0),
        ).unwrap_or(id);

        Ok(existing_id)
    }

    /// 更新已配对设备的 last_seen 和 connect_count（JWT 重连时调用）
    ///
    /// `device_name` 为 Some 时同步更新展示名（重连时设备上报了新的真实设备名），
    /// None 时保留原值
    pub fn update_pairing_last_seen(&self, fingerprint: &str, device_name: Option<&str>) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "UPDATE pairings SET last_seen = ?1, connect_count = connect_count + 1,
             device_name = COALESCE(?2, device_name)
             WHERE device_fingerprint = ?3 AND is_active = 1",
            rusqlite::params![now, device_name, fingerprint],
        )?;
        Ok(())
    }

    pub fn update_pairing_token(&self, pairing_id: &str, token: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE pairings SET session_token = ?1 WHERE id = ?2",
            rusqlite::params![token, pairing_id],
        )?;
        Ok(())
    }

    pub fn verify_session_token(&self, device_fingerprint: &str, token: &str) -> Result<bool> {
        let count: i32 = self.conn().query_row(
            "SELECT COUNT(*) FROM pairings WHERE device_fingerprint = ?1 AND session_token = ?2 AND is_active = 1",
            rusqlite::params![device_fingerprint, token],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn get_pairings(&self) -> Result<Vec<Pairing>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, device_name, device_fingerprint, public_key, address, session_token, paired_at, last_seen, connect_count, is_active
             FROM pairings WHERE is_active = 1 ORDER BY paired_at DESC"
        )?;

        let pairings = stmt.query_map([], |row| {
            Ok(Pairing {
                id: row.get(0)?,
                device_name: row.get(1)?,
                device_fingerprint: row.get(2)?,
                public_key: row.get(3)?,
                address: row.get(4)?,
                session_token: row.get(5)?,
                paired_at: parse_datetime_sql(&row.get::<_, String>(6)?, "paired_at")?,
                last_seen: parse_optional_datetime_sql(row.get::<_, Option<String>>(7)?, "last_seen")?,
                connect_count: row.get(8)?,
                is_active: row.get::<_, i32>(9)? == 1,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(pairings)
    }

    pub fn remove_pairing(&self, id: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE pairings SET is_active = 0 WHERE id = ?1",
            rusqlite::params![id],
        )?;
        // 移除设备连带删除连接历史（无审计需求）
        self.delete_connection_history(id)?;
        Ok(())
    }

    pub fn verify_pairing(&self, fingerprint: &str) -> Result<bool> {
        let count: i32 = self.conn().query_row(
            "SELECT COUNT(*) FROM pairings WHERE device_fingerprint = ?1 AND is_active = 1",
            rusqlite::params![fingerprint],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    // ==================== Connection History Operations ====================

    /// 根据指纹查找活跃配对记录 id（设备 ID）
    pub fn find_pairing_id_by_fingerprint(&self, fingerprint: &str) -> Result<Option<String>> {
        let result = self.conn().query_row(
            "SELECT id FROM pairings WHERE device_fingerprint = ?1 AND is_active = 1",
            rusqlite::params![fingerprint],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 根据指纹获取活跃配对记录（含公钥，用于生物认证验签）
    pub fn get_pairing_by_fingerprint(&self, fingerprint: &str) -> Result<Option<Pairing>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, device_name, device_fingerprint, public_key, address, session_token, paired_at, last_seen, connect_count, is_active
             FROM pairings WHERE device_fingerprint = ?1 AND is_active = 1"
        )?;

        let pairing = stmt.query_row(rusqlite::params![fingerprint], |row| {
            Ok(Pairing {
                id: row.get(0)?,
                device_name: row.get(1)?,
                device_fingerprint: row.get(2)?,
                public_key: row.get(3)?,
                address: row.get(4)?,
                session_token: row.get(5)?,
                paired_at: parse_datetime_sql(&row.get::<_, String>(6)?, "paired_at")?,
                last_seen: parse_optional_datetime_sql(row.get::<_, Option<String>>(7)?, "last_seen")?,
                connect_count: row.get(8)?,
                is_active: row.get::<_, i32>(9)? == 1,
            })
        }).ok();

        Ok(pairing)
    }

    /// 更新配对记录的公钥（生物凭证绑定/解绑）
    pub fn update_pairing_public_key(&self, pairing_id: &str, public_key: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE pairings SET public_key = ?1 WHERE id = ?2 AND is_active = 1",
            rusqlite::params![public_key, pairing_id],
        )?;
        Ok(())
    }

    /// 记录连接事件（按设备 ID），插入后清理超限的旧记录
    pub fn record_connection_event(&self, device_id: &str, auth_method: &str, result: &str, address: Option<&str>) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "INSERT INTO connection_history (device_id, auth_method, result, address, connected_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![device_id, auth_method, result, address, now],
        )?;
        self.prune_connection_history(device_id)?;
        Ok(())
    }

    /// 记录连接事件（按设备指纹解析设备 ID，未配对/未激活则忽略）
    pub fn record_connection_event_by_fingerprint(&self, fingerprint: &str, auth_method: &str, result: &str, address: Option<&str>) -> Result<()> {
        if let Some(device_id) = self.find_pairing_id_by_fingerprint(fingerprint)? {
            self.record_connection_event(&device_id, auth_method, result, address)?;
        }
        Ok(())
    }

    /// 回填最近一条未关闭连接的断开时间
    pub fn close_open_connection_event(&self, device_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let id: Option<i64> = self.conn().query_row(
            "SELECT id FROM connection_history WHERE device_id = ?1 AND disconnected_at IS NULL
             ORDER BY connected_at DESC LIMIT 1",
            rusqlite::params![device_id],
            |row| row.get(0),
        ).ok();
        if let Some(id) = id {
            self.conn().execute(
                "UPDATE connection_history SET disconnected_at = ?1 WHERE id = ?2",
                rusqlite::params![now, id],
            )?;
        }
        Ok(())
    }

    /// 获取设备连接历史（按时间倒序）
    pub fn get_connection_history(&self, device_id: &str) -> Result<Vec<ConnectionHistory>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, device_id, auth_method, result, address, connected_at, disconnected_at
             FROM connection_history WHERE device_id = ?1 ORDER BY connected_at DESC"
        )?;

        let rows = stmt.query_map(rusqlite::params![device_id], |row| {
            Ok(ConnectionHistory {
                id: row.get(0)?,
                device_id: row.get(1)?,
                auth_method: row.get(2)?,
                result: row.get(3)?,
                address: row.get(4)?,
                connected_at: parse_datetime_sql(&row.get::<_, String>(5)?, "connected_at")?,
                disconnected_at: parse_optional_datetime_sql(row.get::<_, Option<String>>(6)?, "disconnected_at")?,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 删除设备连接历史（移除配对时级联清理）
    pub fn delete_connection_history(&self, device_id: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM connection_history WHERE device_id = ?1",
            rusqlite::params![device_id],
        )?;
        Ok(())
    }

    /// 每设备最多保留 MAX 条，超限删除最旧的
    fn prune_connection_history(&self, device_id: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM connection_history WHERE device_id = ?1 AND id NOT IN (
                SELECT id FROM connection_history WHERE device_id = ?1
                ORDER BY connected_at DESC LIMIT ?2
            )",
            rusqlite::params![device_id, CONNECTION_HISTORY_MAX_PER_DEVICE],
        )?;
        Ok(())
    }

    // ==================== Session Config Operations ====================

    pub fn create_session_config(&self, config: &SessionConfig) -> Result<()> {
        self.conn().execute(
            "INSERT INTO session_configs (id, name, environment, wsl_distro, working_dir, command, auto_start, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                config.id,
                config.name,
                config.environment,
                config.wsl_distro,
                config.working_dir,
                config.command,
                config.auto_start as i32,
                config.created_at.to_rfc3339(),
                config.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_session_configs(&self) -> Result<Vec<SessionConfig>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, name, environment, wsl_distro, working_dir, command, auto_start, created_at, updated_at
             FROM session_configs ORDER BY name"
        )?;

        let configs = stmt.query_map([], |row| {
            Ok(SessionConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                environment: row.get(2)?,
                wsl_distro: row.get(3)?,
                working_dir: row.get(4)?,
                command: row.get(5)?,
                auto_start: row.get::<_, i32>(6)? == 1,
                created_at: parse_datetime_sql(&row.get::<_, String>(7)?, "created_at")?,
                updated_at: parse_datetime_sql(&row.get::<_, String>(8)?, "updated_at")?,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(configs)
    }

    pub fn get_session_config(&self, id: &str) -> Result<Option<SessionConfig>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, name, environment, wsl_distro, working_dir, command, auto_start, created_at, updated_at
             FROM session_configs WHERE id = ?1"
        )?;

        let config = stmt.query_row(rusqlite::params![id], |row| {
            Ok(SessionConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                environment: row.get(2)?,
                wsl_distro: row.get(3)?,
                working_dir: row.get(4)?,
                command: row.get(5)?,
                auto_start: row.get::<_, i32>(6)? == 1,
                created_at: parse_datetime_sql(&row.get::<_, String>(7)?, "created_at")?,
                updated_at: parse_datetime_sql(&row.get::<_, String>(8)?, "updated_at")?,
            })
        }).ok();

        Ok(config)
    }

    pub fn delete_session_config(&self, id: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM session_configs WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    pub fn update_session_config(&self, config: &SessionConfig) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "UPDATE session_configs SET
                name = ?1,
                environment = ?2,
                wsl_distro = ?3,
                working_dir = ?4,
                command = ?5,
                auto_start = ?6,
                updated_at = ?7
             WHERE id = ?8",
            rusqlite::params![
                config.name,
                config.environment,
                config.wsl_distro,
                config.working_dir,
                config.command,
                config.auto_start as i32,
                now,
                config.id,
            ],
        )?;
        Ok(())
    }

    // ==================== Quick Actions ====================

    pub fn get_quick_actions(&self) -> Result<Vec<QuickAction>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, name, content, icon, color, category, sort_order, created_at
             FROM quick_actions ORDER BY sort_order"
        )?;

        let actions = stmt.query_map([], |row| {
            Ok(QuickAction {
                id: row.get(0)?,
                name: row.get(1)?,
                content: row.get(2)?,
                icon: row.get(3)?,
                color: row.get(4)?,
                category: row.get(5)?,
                sort_order: row.get(6)?,
                created_at: parse_datetime_sql(&row.get::<_, String>(7)?, "created_at")?,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(actions)
    }

    pub fn create_quick_action(&self, action: &QuickAction) -> Result<()> {
        self.conn().execute(
            "INSERT INTO quick_actions (id, name, content, icon, color, category, sort_order, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                action.id,
                action.name,
                action.content,
                action.icon,
                action.color,
                action.category,
                action.sort_order,
                action.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn update_quick_action(&self, action: &QuickAction) -> Result<()> {
        self.conn().execute(
            "UPDATE quick_actions SET name = ?1, content = ?2, icon = ?3, color = ?4, category = ?5, sort_order = ?6
             WHERE id = ?7",
            rusqlite::params![
                action.name,
                action.content,
                action.icon,
                action.color,
                action.category,
                action.sort_order,
                action.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_quick_action(&self, id: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM quick_actions WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    // ==================== Settings ====================

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let value = self.conn().query_row(
            "SELECT value FROM settings WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        ).ok();
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
        let mut stmt = self.conn().prepare(
            "SELECT key, value, updated_at FROM settings ORDER BY key"
        )?;

        let settings = stmt.query_map([], |row| {
            Ok(Setting {
                key: row.get(0)?,
                value: row.get(1)?,
                updated_at: parse_datetime_sql(&row.get::<_, String>(2)?, "updated_at")?,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(settings)
    }
}
