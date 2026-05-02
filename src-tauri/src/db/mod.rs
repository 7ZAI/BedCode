//! Database module

mod models;
mod operations;

pub use models::*;

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
        Ok(())
    }

    /// Get a reference to the connection
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}
