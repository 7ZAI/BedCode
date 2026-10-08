//! 主库 schema（`bedcode_plugins.db`，对齐桌面 wasm-core `src/db/schema.sql`；票 05）
//!
//! 单一事实源：本文件的 `SCHEMA_SQL`。迁移**必须幂等**（`CREATE TABLE IF NOT EXISTS`
//! + 无破坏性列变更；改 schema 必须补幂等测试——AGENTS §9）。
//!
//! 表清单（与桌面同构；桌面语义注释摘录）：
//! - `settings`：宿主/插件键值配置；
//! - `plugin_storage`：host-storage 真源（票 05b 起插件 KV 迁主库表，不再文件落盘）；
//! - `plugin_secrets`：密钥托管（host-auth 凭据指定存储位；明文不落日志）；
//! - `plugin_auth_policies`：授权策略（egress 三档，票 19 用；缺行 = default）；
//! - `plugin_auth_records`：授权记录（egress 授权记忆持久化，票 19 用）。
//!
//! 移动端差异：不建桌面已退役表（pairings / connection_history / session_configs
//! 等下沉插件私有库）；本 schema 只含内核机制表。

/// 主库 schema（幂等建表）
pub const SCHEMA_SQL: &str = r#"
-- App settings table
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Plugin key-value storage (per-plugin isolation)
CREATE TABLE IF NOT EXISTS plugin_storage (
    plugin_id TEXT NOT NULL,
    key       TEXT NOT NULL,
    value     TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (plugin_id, key)
);

-- 密钥托管（host-auth secret-store，按插件属主隔离）
-- 明文不落日志（宿主只记长度）；本表是凭据的唯一指定存储位（AGENTS §8）
CREATE TABLE IF NOT EXISTS plugin_secrets (
    plugin_id  TEXT NOT NULL,
    key        TEXT NOT NULL,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (plugin_id, key)
);

-- 授权策略（每个 (插件, 受管资源) 至多一行，回答「要不要问用户」：
-- always_ask / default / always_allow；缺行 = default。策略只决定是否询问，
-- 不放宽任何安全硬闸门）
CREATE TABLE IF NOT EXISTS plugin_auth_policies (
    plugin_id  TEXT NOT NULL,
    resource   TEXT NOT NULL,      -- 'fs' | 'network'
    strategy   TEXT NOT NULL,      -- 'always_ask' | 'default' | 'always_allow'
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, resource)
);

-- 授权记录（授权记忆：用户已批准/拒绝的 (插件, 目标) 对，票 19 egress 持久化用）
CREATE TABLE IF NOT EXISTS plugin_auth_records (
    plugin_id   TEXT NOT NULL,
    target      TEXT NOT NULL,
    resource    TEXT NOT NULL,
    decision    TEXT NOT NULL,     -- 'allow' | 'deny'
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, target, resource)
);
"#;

/// 幂等建表（可在任意连接上安全重复执行）
pub fn init_schema(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA_SQL)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 幂等：同库跑两次无错（AGENTS §9：改 schema 必须补幂等测试）
    #[test]
    fn schema_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
        init_schema(&conn).expect("first init");
        init_schema(&conn).expect("second init (idempotent)");
        // 表在册
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN \
                 ('settings','plugin_storage','plugin_secrets','plugin_auth_policies','plugin_auth_records')",
                [],
                |row| row.get(0),
            )
            .expect("count tables");
        assert_eq!(count, 5, "all five mechanism tables present");
    }

    /// 新表可读写（plugin_storage 主键约束生效）
    #[test]
    fn plugin_storage_table_reads_writes() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
        init_schema(&conn).expect("init");
        conn.execute(
            "INSERT INTO plugin_storage (plugin_id, key, value, updated_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["com.bedcode.test", "k", r#"{"n":1}"#, "2026-10-07T00:00:00Z"],
        )
        .expect("insert");
        // 同主键冲突
        assert!(conn
            .execute(
                "INSERT INTO plugin_storage (plugin_id, key, value, updated_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params!["com.bedcode.test", "k", "dup", "2026-10-07T00:00:00Z"],
            )
            .is_err());
    }
}
