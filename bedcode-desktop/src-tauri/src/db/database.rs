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
        let existing_columns: Vec<String> = {
            let mut stmt = self.conn.prepare("PRAGMA table_info(pairings)")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        for col in &["address", "session_token", "last_seen", "uid_hash"] {
            if !existing_columns.iter().any(|c| c == col) {
                self.conn
                    .execute(&format!("ALTER TABLE pairings ADD COLUMN {col} TEXT"), [])?;
            }
        }

        // connect_count 列迁移（默认 1，表示至少配对过一次）
        if !existing_columns.iter().any(|c| c == "connect_count") {
            self.conn
                .execute("ALTER TABLE pairings ADD COLUMN connect_count INTEGER DEFAULT 1", [])?;
        }

        // session_configs.environment CHECK 约束迁移：允许新增 'linux'
        // SQLite 不支持 ALTER CHECK，重建表以替换约束；保留所有现有数据。
        self.migrate_session_configs_check_constraint()?;

        Ok(())
    }

    /// 重写 session_configs 表以更新 CHECK(environment) 约束，支持 linux 环境
    ///
    /// SQLite 修改 CHECK 约束的官方做法：建新表 → 拷数据 → 删旧表 → 改名。
    /// 复用现有 schema.sql 中的目标表结构。
    fn migrate_session_configs_check_constraint(&self) -> crate::Result<()> {
        // 仅在旧 CHECK 约束存在时才需要迁移（用元数据推断）。
        let sql = self.conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'session_configs'",
            [],
            |row| row.get::<_, String>(0),
        );
        let Ok(table_sql) = sql else { return Ok(()) };

        // 旧约束只允许 windows/wsl2；如果新约束已生效则跳过
        let needs_migration =
            table_sql.contains("environment IN ('windows', 'wsl2')") && !table_sql.contains("'linux'");
        if !needs_migration {
            return Ok(());
        }

        tracing::info!("Migrating session_configs CHECK constraint to include 'linux'");

        // 用事务保证原子性
        self.conn.execute_batch(
            "BEGIN;
             CREATE TABLE session_configs_new (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 environment TEXT NOT NULL CHECK(environment IN ('windows', 'wsl2', 'linux')),
                 wsl_distro TEXT,
                 working_dir TEXT NOT NULL,
                 command TEXT NOT NULL,
                 auto_start INTEGER DEFAULT 0,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );
             INSERT INTO session_configs_new
                 (id, name, environment, wsl_distro, working_dir, command, auto_start, created_at, updated_at)
             SELECT id, name, environment, wsl_distro, working_dir, command, auto_start, created_at, updated_at
                 FROM session_configs;
             DROP TABLE session_configs;
             ALTER TABLE session_configs_new RENAME TO session_configs;
             CREATE INDEX IF NOT EXISTS idx_session_configs_name ON session_configs(name);
             COMMIT;",
        )?;

        Ok(())
    }

    /// 统计 legacy `session_configs` 表行数（票 02 阶段 A 观测信号）
    ///
    /// 该表是会话配置的**旧真源**，v21 之后只剩一个用途：`com.bedcode.terminal-session`
    /// 激活时经 `LegacyConfigSource` 一次性迁入插件私有库。退表（阶段 B）前需
    /// 由发布侧确认「还有多少安装点的遗留行尚未迁入插件私有库」——本计数即该
    /// 信号的数据源（宿主启动时以结构化字段 `legacy_rows` 落 info 日志）。
    /// 表不存在时返回 0（全新安装，或阶段 B 删表之后）。
    pub fn count_legacy_session_configs(&self) -> crate::Result<usize> {
        let table_exists: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'session_configs'",
            [],
            |row| row.get(0),
        )?;
        if table_exists == 0 {
            return Ok(0);
        }
        let rows: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM session_configs", [], |row| row.get(0))?;
        Ok(rows.max(0) as usize)
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

    /// 获取表 CHECK 约束的当前 SQL 定义
    fn session_configs_table_sql(conn: &Connection) -> Option<String> {
        conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'session_configs'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
    }

    /// 打开内存库并跑生产初始化（票据 21 修复：不再复制迁移逻辑，
    /// 直接调用 `init_schema`，使迁移被破坏时测试真实变红）
    fn open_in_memory_and_init() -> Database {
        let db = Database::new(Path::new(":memory:")).expect("open in-memory db");
        db.init_schema().expect("init schema");
        db
    }

    /// 票 02 阶段 A：legacy 行数计数是「未迁移安装点」的发布侧信号，必须
    /// 逐行准确，且在阶段 B 删表之后仍可调用（表缺失 → 0，不 panic）
    #[test]
    fn count_legacy_session_configs_tracks_rows_and_survives_table_drop() {
        let db = open_in_memory_and_init();
        assert_eq!(
            db.count_legacy_session_configs().expect("count on fresh db"),
            0,
            "全新库（或已迁移安装）不得报出遗留行"
        );

        db.conn()
            .execute(
                "INSERT INTO session_configs
                 (id, name, environment, wsl_distro, working_dir, command, auto_start, created_at, updated_at)
                 VALUES ('legacy-1', '旧配置', 'linux', NULL, '/tmp', 'bash', 0,
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("insert legacy row");
        assert_eq!(db.count_legacy_session_configs().expect("count"), 1);

        // 阶段 B（删表）之后本函数仍被启动路径调用：表缺失必须归零而非报错
        db.conn().execute("DROP TABLE session_configs", []).expect("drop table");
        assert_eq!(db.count_legacy_session_configs().expect("count after drop"), 0);
    }

    #[test]
    fn fresh_db_uses_new_check_constraint_with_linux() {
        let db = open_in_memory_and_init();

        let sql = session_configs_table_sql(db.conn()).expect("table should exist");
        assert!(sql.contains("'linux'"), "新 schema 应允许 linux: {}", sql);
    }

    #[test]
    fn old_db_is_migrated_and_preserves_data() {
        // 模拟老库：以旧 CHECK 约束建表，插入 windows 与 wsl2 各一条
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE session_configs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                environment TEXT NOT NULL CHECK(environment IN ('windows', 'wsl2')),
                wsl_distro TEXT,
                working_dir TEXT NOT NULL,
                command TEXT NOT NULL,
                auto_start INTEGER DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX idx_session_configs_name ON session_configs(name);
            INSERT INTO session_configs VALUES
              ('id-1', 'win-cfg', 'windows', NULL, 'C:/work', 'claude', 0, '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z'),
              ('id-2', 'wsl-cfg', 'wsl2', 'Ubuntu', '/home/me', 'claude', 1, '2024-01-02T00:00:00Z', '2024-01-02T00:00:00Z');",
        )
        .unwrap();

        // 用生产迁移路径初始化（init_schema 内部跑 schema.sql + run_migrations）
        let db = Database::with_conn(conn);
        db.init_schema().unwrap();
        let conn = &db.conn;

        let sql = session_configs_table_sql(conn).unwrap();
        assert!(sql.contains("'linux'"), "迁移后应允许 linux: {}", sql);
        assert!(
            !sql.contains("environment IN ('windows', 'wsl2')"),
            "不应再包含旧约束: {}",
            sql
        );

        // 数据保留
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_configs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);

        let env: String = conn
            .query_row("SELECT environment FROM session_configs WHERE id = 'id-1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(env, "windows");

        let wsl_distro: Option<String> = conn
            .query_row("SELECT wsl_distro FROM session_configs WHERE id = 'id-2'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(wsl_distro.as_deref(), Some("Ubuntu"));
    }

    #[test]
    fn migration_is_idempotent() {
        let db = open_in_memory_and_init();
        let first_sql = session_configs_table_sql(db.conn()).unwrap();

        // 再跑一次迁移：应被识别为「已迁移」直接跳过
        db.init_schema().unwrap();
        let second_sql = session_configs_table_sql(db.conn()).unwrap();
        assert_eq!(first_sql, second_sql);

        // 数据仍可插入 linux
        db.conn
            .execute(
                "INSERT INTO session_configs (id, name, environment, working_dir, command, created_at, updated_at)
                 VALUES ('id-3', 'linux-cfg', 'linux', '/tmp', 'claude', '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
    }

    #[test]
    fn migrated_db_accepts_linux_value() {
        // 老库迁移后，INSERT 'linux' 不应再触发 CHECK 约束
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE session_configs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                environment TEXT NOT NULL CHECK(environment IN ('windows', 'wsl2')),
                wsl_distro TEXT,
                working_dir TEXT NOT NULL,
                command TEXT NOT NULL,
                auto_start INTEGER DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
        )
        .unwrap();

        let db = Database::with_conn(conn);
        db.init_schema().unwrap();

        db.conn
            .execute(
                "INSERT INTO session_configs (id, name, environment, working_dir, command, created_at, updated_at)
                 VALUES ('id-linux', 'n', 'linux', '/tmp', 'claude', '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
                [],
            )
            .expect("INSERT 'linux' should succeed after migration");
    }
}
