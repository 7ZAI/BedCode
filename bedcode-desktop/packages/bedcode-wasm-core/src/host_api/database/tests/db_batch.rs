//! 事务批次（票据 06）：`execute-batch` 的全提交 / 全回滚语义与语句数上限
//!
//! 另含裸事务控制（`BEGIN` / `COMMIT` / `SAVEPOINT` …）在 execute 级的拒绝——跨调用
//! 持有事务会占住全局连接锁，必须引导到批次原语。

use crate::system::constants::PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS;
use crate::host_api::database::{db_execute, db_execute_batch, db_query, reject_bare_transaction_control};
use crate::host_api::sqlite_scaffold::*;
use crate::permission::{PERMISSION_DATABASE_MAIN, PERMISSION_STORAGE};

// ==================== 事务批次（票据 06）：execute-batch ====================

/// 事务批次语义：全部成功才提交；任一句失败整体回滚（前序语句一并回滚）
#[test]
fn execute_batch_commits_all_or_rolls_back_all() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    fake.grant("p1", &[PERMISSION_DATABASE_MAIN, PERMISSION_STORAGE]);
    db_execute(
        ports,
        "p1",
        "CREATE TABLE plugin_p1_batch (id INTEGER PRIMARY KEY, v TEXT)",
    )
    .expect("create table");

    // 成功批次：全部提交
    let affected = db_execute_batch(
        ports,
        "p1",
        r#"["INSERT INTO plugin_p1_batch (id,v) VALUES (1,'a')",
            "INSERT INTO plugin_p1_batch (id,v) VALUES (2,'b')",
            "INSERT INTO plugin_p1_batch (id,v) VALUES (3,'c')"]"#,
    )
    .expect("batch must commit");
    assert_eq!(affected, 3);
    let out = db_query(ports, "p1", "SELECT count(*) AS n FROM plugin_p1_batch")
        .unwrap()
        .unwrap();
    assert_eq!(out, "[{\"n\":3}]");

    // 失败批次（第二句主键冲突）：整体回滚，已执行的第一句也不算数
    let err = db_execute_batch(
        ports,
        "p1",
        r#"["INSERT INTO plugin_p1_batch (id,v) VALUES (4,'d')",
            "INSERT INTO plugin_p1_batch (id,v) VALUES (1,'dup')"]"#,
    )
    .expect_err("conflicting statements must roll back the whole batch");
    assert!(err.contains("UNIQUE") || err.contains("duplicate"), "got: {}", err);
    let out = db_query(ports, "p1", "SELECT count(*) AS n FROM plugin_p1_batch")
        .unwrap()
        .unwrap();
    assert_eq!(out, "[{\"n\":3}]", "failed batch must roll back prior statements");
}

/// 语句数上限：超限批次在执行前被拒（不放行）
#[test]
fn execute_batch_statement_count_capped() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    fake.grant("p1", &[PERMISSION_DATABASE_MAIN, PERMISSION_STORAGE]);
    let many: Vec<String> = (0..PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS + 1)
        .map(|i| format!("INSERT INTO plugin_p1_x VALUES ({})", i))
        .collect();
    let sqls = serde_json::to_string(&many).unwrap();
    let err = db_execute_batch(ports, "p1", &sqls).expect_err("over-limit batch must be rejected");
    assert!(err.contains("statements limit"), "got: {}", err);
}

/// 跨调用裸事务被拒：execute 级 BEGIN/COMMIT/SAVEPOINT 等引导 execute-batch
#[test]
fn bare_transaction_control_rejected_at_execute() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    fake.grant("p1", &[PERMISSION_DATABASE_MAIN, PERMISSION_STORAGE]);
    for sql in [
        "BEGIN",
        "BEGIN TRANSACTION",
        "COMMIT;",
        "ROLLBACK",
        "SAVEPOINT sp1",
        "RELEASE sp1",
        "END TRANSACTION",
    ] {
        let err = db_execute(ports, "p1", sql).expect_err(&format!("'{}' must be rejected", sql));
        assert!(err.contains("execute-batch"), "'{}' err: {}", sql, err);
    }
}

/// 白名单检测正反例：正常 DML/DDL 放行；前导注释/尾部 COMMIT 形态也能识别
#[test]
fn transaction_control_whitelist_allows_normal_sql() {
    assert!(reject_bare_transaction_control("INSERT INTO t (x) VALUES ('begin')").is_ok());
    assert!(reject_bare_transaction_control("UPDATE t SET x = 'commit' WHERE id = 1").is_ok());
    assert!(reject_bare_transaction_control("SELECT 1").is_ok());
    assert!(reject_bare_transaction_control("-- 注释\nBEGIN").is_err());
    assert!(reject_bare_transaction_control("/* 注释 */ SELECT 1").is_ok());
    assert!(reject_bare_transaction_control("INSERT INTO t VALUES (1); COMMIT").is_err());
}
