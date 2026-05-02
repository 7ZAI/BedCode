//! Database operations

use super::{Database, Pairing, SessionConfig, History, QuickAction, Message, MessageType, Setting};
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

/// Helper function to parse a message row from the database
fn parse_message_row(row: &rusqlite::Row) -> std::result::Result<Message, rusqlite::Error> {
    Ok(Message {
        id: row.get(0)?,
        session_id: row.get(1)?,
        history_id: row.get(2)?,
        message_type: MessageType::from_str(&row.get::<_, String>(3)?).unwrap_or(MessageType::Output),
        content: row.get(4)?,
        timestamp: parse_datetime_sql(&row.get::<_, String>(5)?, "timestamp")?,
        metadata: row.get::<_, Option<String>>(6)?.and_then(|s| serde_json::from_str(&s).ok()),
    })
}

impl Database {
    // ==================== Pairing Operations ====================

    pub fn add_pairing(&self, device_name: &str, fingerprint: &str, public_key: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        self.conn().execute(
            "INSERT INTO pairings (id, device_name, device_fingerprint, public_key, paired_at, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, 1)",
            rusqlite::params![id, device_name, fingerprint, public_key, now],
        )?;

        Ok(id)
    }

    pub fn get_pairings(&self) -> Result<Vec<Pairing>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, device_name, device_fingerprint, public_key, paired_at, last_seen, is_active
             FROM pairings WHERE is_active = 1 ORDER BY paired_at DESC"
        )?;

        let pairings = stmt.query_map([], |row| {
            Ok(Pairing {
                id: row.get(0)?,
                device_name: row.get(1)?,
                device_fingerprint: row.get(2)?,
                public_key: row.get(3)?,
                paired_at: parse_datetime_sql(&row.get::<_, String>(4)?, "paired_at")?,
                last_seen: parse_optional_datetime_sql(row.get::<_, Option<String>>(5)?, "last_seen")?,
                is_active: row.get::<_, i32>(6)? == 1,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(pairings)
    }

    pub fn remove_pairing(&self, id: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE pairings SET is_active = 0 WHERE id = ?1",
            rusqlite::params![id],
        )?;
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

    // ==================== Session Config Operations ====================

    pub fn create_session_config(&self, config: &SessionConfig) -> Result<()> {
        self.conn().execute(
            "INSERT INTO session_configs (id, name, environment, wsl_distro, working_dir, command, tmux_session, auto_start, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                config.id,
                config.name,
                config.environment,
                config.wsl_distro,
                config.working_dir,
                config.command,
                config.tmux_session,
                config.auto_start as i32,
                config.created_at.to_rfc3339(),
                config.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_session_configs(&self) -> Result<Vec<SessionConfig>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, name, environment, wsl_distro, working_dir, command, tmux_session, auto_start, created_at, updated_at
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
                tmux_session: row.get(6)?,
                auto_start: row.get::<_, i32>(7)? == 1,
                created_at: parse_datetime_sql(&row.get::<_, String>(8)?, "created_at")?,
                updated_at: parse_datetime_sql(&row.get::<_, String>(9)?, "updated_at")?,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(configs)
    }

    pub fn get_session_config(&self, id: &str) -> Result<Option<SessionConfig>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, name, environment, wsl_distro, working_dir, command, tmux_session, auto_start, created_at, updated_at
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
                tmux_session: row.get(6)?,
                auto_start: row.get::<_, i32>(7)? == 1,
                created_at: parse_datetime_sql(&row.get::<_, String>(8)?, "created_at")?,
                updated_at: parse_datetime_sql(&row.get::<_, String>(9)?, "updated_at")?,
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
                tmux_session = ?6,
                auto_start = ?7,
                updated_at = ?8
             WHERE id = ?9",
            rusqlite::params![
                config.name,
                config.environment,
                config.wsl_distro,
                config.working_dir,
                config.command,
                config.tmux_session,
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

    // ==================== History ====================

    pub fn add_history(&self, session_id: &str, session_name: &str, device_id: Option<&str>) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        self.conn().execute(
            "INSERT INTO history (id, session_id, session_name, device_id, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, session_id, session_name, device_id, now],
        )?;

        Ok(id)
    }

    pub fn end_history(&self, id: &str, output_summary: Option<&str>) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn().execute(
            "UPDATE history SET ended_at = ?1, output_summary = ?2 WHERE id = ?3",
            rusqlite::params![now, output_summary, id],
        )?;
        Ok(())
    }

    pub fn get_history(&self, limit: Option<usize>) -> Result<Vec<History>> {
        // Use parameterized query for limit
        let sql = "SELECT id, session_id, session_name, device_id, started_at, ended_at, output_summary
                   FROM history ORDER BY started_at DESC LIMIT ?1";

        let mut stmt = self.conn().prepare(sql)?;

        // Use -1 for no limit (SQLite doesn't support OFFSET without LIMIT, so we use a very large number)
        let limit_value = limit.map(|n| n as i32).unwrap_or(-1);

        let history = stmt.query_map(rusqlite::params![limit_value], |row| {
            Ok(History {
                id: row.get(0)?,
                session_id: row.get(1)?,
                session_name: row.get(2)?,
                device_id: row.get(3)?,
                started_at: parse_datetime_sql(&row.get::<_, String>(4)?, "started_at")?,
                ended_at: parse_optional_datetime_sql(row.get::<_, Option<String>>(5)?, "ended_at")?,
                output_summary: row.get(6)?,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(history)
    }

    // ==================== Messages ====================

    pub fn add_message(&self, message: &Message) -> Result<()> {
        self.conn().execute(
            "INSERT INTO messages (id, session_id, history_id, message_type, content, timestamp, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                message.id,
                message.session_id,
                message.history_id,
                message.message_type.as_str(),
                message.content,
                message.timestamp.to_rfc3339(),
                message.metadata.as_ref().map(|m| m.to_string()),
            ],
        )?;
        Ok(())
    }

    pub fn get_messages(&self, session_id: &str, limit: Option<usize>, before: Option<DateTime<Utc>>) -> Result<Vec<Message>> {
        // Build query with proper parameterization
        // Note: SQLite doesn't support parameterized LIMIT in the same position,
        // so we need different queries for different cases
        let sql = match (before.is_some(), limit) {
            (true, Some(_n)) => "SELECT id, session_id, history_id, message_type, content, timestamp, metadata
                               FROM messages WHERE session_id = ?1 AND timestamp < ?2 ORDER BY timestamp DESC LIMIT ?3",
            (true, None) => "SELECT id, session_id, history_id, message_type, content, timestamp, metadata
                            FROM messages WHERE session_id = ?1 AND timestamp < ?2 ORDER BY timestamp DESC",
            (false, Some(_n)) => "SELECT id, session_id, history_id, message_type, content, timestamp, metadata
                                FROM messages WHERE session_id = ?1 ORDER BY timestamp DESC LIMIT ?2",
            (false, None) => "SELECT id, session_id, history_id, message_type, content, timestamp, metadata
                             FROM messages WHERE session_id = ?1 ORDER BY timestamp DESC",
        };

        let mut stmt = self.conn().prepare(sql)?;

        let messages = match (before, limit) {
            (Some(dt), Some(n)) => {
                stmt.query_map(rusqlite::params![session_id, dt.to_rfc3339(), n as i32], parse_message_row)?.collect::<std::result::Result<Vec<_>, _>>()?
            }
            (Some(dt), None) => {
                stmt.query_map(rusqlite::params![session_id, dt.to_rfc3339()], parse_message_row)?.collect::<std::result::Result<Vec<_>, _>>()?
            }
            (None, Some(n)) => {
                stmt.query_map(rusqlite::params![session_id, n as i32], parse_message_row)?.collect::<std::result::Result<Vec<_>, _>>()?
            }
            (None, None) => {
                stmt.query_map(rusqlite::params![session_id], parse_message_row)?.collect::<std::result::Result<Vec<_>, _>>()?
            }
        };

        // Reverse to get chronological order
        Ok(messages.into_iter().rev().collect())
    }

    pub fn search_messages(&self, query: &str, limit: Option<usize>) -> Result<Vec<Message>> {
        let sql = match limit {
            Some(_) => "SELECT id, session_id, history_id, message_type, content, timestamp, metadata
                        FROM messages WHERE content LIKE ?1 ORDER BY timestamp DESC LIMIT ?2",
            None => "SELECT id, session_id, history_id, message_type, content, timestamp, metadata
                     FROM messages WHERE content LIKE ?1 ORDER BY timestamp DESC",
        };

        let mut stmt = self.conn().prepare(sql)?;
        let search_pattern = format!("%{}%", query);

        let messages = match limit {
            Some(n) => {
                stmt.query_map(rusqlite::params![search_pattern, n as i32], parse_message_row)?.collect::<std::result::Result<Vec<_>, _>>()?
            }
            None => {
                stmt.query_map(rusqlite::params![search_pattern], parse_message_row)?.collect::<std::result::Result<Vec<_>, _>>()?
            }
        };

        Ok(messages)
    }

    pub fn clear_messages(&self, session_id: Option<&str>, before: Option<DateTime<Utc>>) -> Result<usize> {
        let rows_deleted = match (session_id, before) {
            (Some(sid), Some(dt)) => {
                self.conn().execute(
                    "DELETE FROM messages WHERE session_id = ?1 AND timestamp < ?2",
                    rusqlite::params![sid, dt.to_rfc3339()],
                )?
            }
            (Some(sid), None) => {
                self.conn().execute(
                    "DELETE FROM messages WHERE session_id = ?1",
                    rusqlite::params![sid],
                )?
            }
            (None, Some(dt)) => {
                self.conn().execute(
                    "DELETE FROM messages WHERE timestamp < ?1",
                    rusqlite::params![dt.to_rfc3339()],
                )?
            }
            (None, None) => {
                self.conn().execute("DELETE FROM messages", [])?
            }
        };
        Ok(rows_deleted)
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
