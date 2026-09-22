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

    /// 测试专用：用已有 Connection 构造（迁移测试需先建旧表再跑生产迁移）
    #[cfg(test)]
    fn with_conn(conn: Connection) -> Self {
        Self { conn }
    }

    /// Initialize database schema
    pub fn init_schema(&self) -> crate::Result<()> {
        self.conn.execute_batch(include_str!("schema.sql"))?;
        self.run_migrations()?;
        Ok(())
    }

    /// Apply schema migrations for columns added after initial schema
    fn run_migrations(&self) -> crate::Result<()> {
        // v24（2026-09-22 认证记录下沉 + session_configs 删除）：主库
        // `pairings` / `connection_history` / `session_configs` 三表退役，
        // 其列级迁移（pairings 列追加 / session_configs CHECK 约束）随之删除——
        // 旧库存量数据由宿主侧 handoff（auth_records_migration）迁入认证中心
        // 插件私有库，不在此建表。
        Ok(())
    }

    /// Get a reference to the connection
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// 获取可变连接引用（事务/批量场景需要 `&mut Connection`）
    ///
    /// 调用方必须保证独占访问（宿主 DB 域持有全局 Mutex 锁时使用）
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 打开内存库并跑生产初始化（票据 21 修复：不再复制迁移逻辑，
    /// 直接调用 `init_schema`，使迁移被破坏时测试真实变红）
    fn open_in_memory_and_init() -> Database {
        let db = Database::new(Path::new(":memory:")).expect("open in-memory db");
        db.init_schema().expect("init schema");
        db
    }

    /// v24（2026-09-22）：session_configs / pairings / connection_history 三表退役，
    /// 新增表只读主库 schema；旧表不建（init_schema 零残留表断言）
    #[test]
    fn retired_tables_are_not_created() {
        let db = open_in_memory_and_init();
        for table in ["session_configs", "pairings", "connection_history"] {
            let count: i64 = db
                .conn()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    rusqlite::params![table],
                    |row| row.get(0),
                )
                .expect("count query");
            assert_eq!(count, 0, "退役表 {table} 不得被建出");
        }
    }
}

