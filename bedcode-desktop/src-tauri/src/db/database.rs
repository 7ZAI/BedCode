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
        // 其列级迁移（pairings 列追加 / session_configs CHECK 约束）随之删除。
        // 2026-09-23 用户裁定不再兼容旧版本存量用户：宿主侧 legacy 迁移链
        // （auth_records / quick_actions / session_db / task_data）整体退役，
        // 旧库滞留表不读不迁不清理（无残留迁移代码）。
        //
        // 2026-09-27 授权策略增强（票 01）：`plugin_auth_policies` /
        // `plugin_auth_records` 两表 + `idx_auth_records_lookup` 索引全由
        // `schema.sql` 的 `IF NOT EXISTS` 建出（单一事实源，见 AGENTS §9），
        // 无需列级迁移；幂等性由 `auth_tables_migration_is_idempotent` 与
        // `auth_tables_are_created_on_legacy_db` 两条锁覆盖。
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

    /// sqlite_master 里某类对象的同名条目数（建表 / 建索引幂等的唯一可观测面）
    fn master_count(db: &Database, kind: &str, name: &str) -> i64 {
        db.conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                rusqlite::params![kind, name],
                |row| row.get(0),
            )
            .expect("sqlite_master count query")
    }

    /// 授权两表 + 索引的建表语句幂等（spec §5.1 / C13 正例）：
    /// 同一库连跑两次生产初始化，结果一致，且已播种的授权数据不被清掉
    #[test]
    fn auth_tables_migration_is_idempotent() {
        let db = open_in_memory_and_init();
        db.conn()
            .execute(
                "INSERT INTO plugin_auth_policies (plugin_id, resource, strategy, updated_at) \
                 VALUES ('com.bedcode.test', 'fs', 'always_ask', 1)",
                [],
            )
            .expect("seed policy");
        db.conn()
            .execute(
                "INSERT INTO plugin_auth_records \
                 (plugin_id, resource, target, effect, ops, prefix_match, source, created_at) \
                 VALUES ('com.bedcode.test', 'fs', '/tmp/authed', 'allow', '[\"read\"]', 0, 'user', 1)",
                [],
            )
            .expect("seed record");

        db.init_schema().expect("second init_schema must succeed");

        assert_eq!(
            master_count(&db, "table", "plugin_auth_policies"),
            1,
            "建表语句必须幂等：不得重复建 / 不得消失"
        );
        assert_eq!(master_count(&db, "table", "plugin_auth_records"), 1);
        assert_eq!(
            master_count(&db, "index", "idx_auth_records_lookup"),
            1,
            "建索引语句必须幂等"
        );
        let policies: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM plugin_auth_policies", [], |r| r.get(0))
            .expect("count policies");
        let records: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM plugin_auth_records", [], |r| r.get(0))
            .expect("count records");
        assert_eq!((policies, records), (1, 1), "重复初始化不得清掉已有授权数据");
    }

    /// C13 反例：旧库（有 `fs_granted_paths`、无授权两表）跑一次生产初始化
    /// 即建表成功；旧记录原样保留（存量用户零感知，spec §5.2）
    ///
    /// 旧库形态用「生产初始化后删掉新表」构造：既得到真实的历史库形状
    /// （settings / plugin_storage / plugin_secrets + 旧 fs 授权数据），
    /// 又不必在测试里复制一份历史 DDL（复制即漂移源）。
    #[test]
    fn auth_tables_are_created_on_legacy_db() {
        let db = open_in_memory_and_init();
        db.conn()
            .execute(
                "INSERT INTO plugin_storage (plugin_id, key, value, updated_at) \
                 VALUES ('com.bedcode.agent-hub', 'fs_granted_paths', '[\"/legacy/dir\"]', '2026-09-01T00:00:00Z')",
                [],
            )
            .expect("seed legacy fs grant");
        db.conn()
            .execute_batch(
                "DROP TABLE plugin_auth_records; DROP TABLE plugin_auth_policies;",
            )
            .expect("simulate legacy db without auth tables");
        assert_eq!(master_count(&db, "table", "plugin_auth_policies"), 0);
        assert_eq!(
            master_count(&db, "index", "idx_auth_records_lookup"),
            0,
            "索引随表一并删除，重新初始化必须重建它"
        );

        db.init_schema().expect("init on legacy db must succeed");

        assert_eq!(master_count(&db, "table", "plugin_auth_policies"), 1);
        assert_eq!(master_count(&db, "table", "plugin_auth_records"), 1);
        assert_eq!(master_count(&db, "index", "idx_auth_records_lookup"), 1);
        let legacy: String = db
            .conn()
            .query_row(
                "SELECT value FROM plugin_storage \
                 WHERE plugin_id = 'com.bedcode.agent-hub' AND key = 'fs_granted_paths'",
                [],
                |r| r.get(0),
            )
            .expect("legacy grant must survive");
        assert_eq!(
            legacy, "[\"/legacy/dir\"]",
            "旧授权记录不得被迁移或清理（读路径回退才认它）"
        );
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

