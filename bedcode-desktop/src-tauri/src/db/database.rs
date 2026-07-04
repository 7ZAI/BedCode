//! Database wrapper
//!
//! 数据库连接管理

use rusqlite::Connection;
use std::path::Path;

/// Database wrapper
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Create a new database connection
    pub fn new(path: &Path) -> crate::Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self { conn })
    }

    /// Initialize database schema
    pub fn init_schema(&self) -> crate::Result<()> {
        self.conn.execute_batch(include_str!("schema.sql"))?;
        self.run_migrations()?;
        Ok(())
    }

    /// Apply schema migrations for columns added after initial schema
    fn run_migrations(&self) -> crate::Result<()> {
        let existing_columns: Vec<String> = {
            let mut stmt = self.conn.prepare("PRAGMA table_info(pairings)")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        for col in &["address", "session_token", "last_seen"] {
            if !existing_columns.iter().any(|c| c == col) {
                self.conn.execute(
                    &format!("ALTER TABLE pairings ADD COLUMN {col} TEXT"),
                    [],
                )?;
            }
        }

        // connect_count 列迁移（默认 1，表示至少配对过一次）
        if !existing_columns.iter().any(|c| c == "connect_count") {
            self.conn.execute(
                "ALTER TABLE pairings ADD COLUMN connect_count INTEGER DEFAULT 1",
                [],
            )?;
        }

        Ok(())
    }

    /// Get a reference to the connection
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}