//! 主库隔离纵深（票 02 / P0-1）：插件在自己前缀之外的表**引擎层不可见**
//!
//! 正则层（`validate_sql_table_prefix`）只提供更早、更可读的失败文案；真正的边界是
//! SQLite authorizer 在 prepare 阶段逐动作仲裁。攻击者视角：插件已合法持有主库面，
//! 试图借一条语句读到 / 搬走别人的表（`plugin_secrets` 明文密钥面）。
//!
//! 迁移自宿主同名用例；断言面 = 域函数对**真实主库**的执行结果。

use crate::host_api::database::{db_execute, db_query, plugin_db_query};
use crate::host_api::sqlite_ports::SqlitePorts;
use crate::host_api::sqlite_scaffold::*;
use crate::permission::{PERMISSION_DATABASE_MAIN, PERMISSION_STORAGE};
use std::sync::Arc;

// ==================== 主库隔离纵深（票 02，P0-1） ====================
//
// 断言面 = 域函数对**真实主库**的执行结果（`FakePorts` 的库跑过生产 init_schema，
// 里面就有 `plugin_secrets`），不断言 `extract_table_names` 的内部返回。
// 攻击者视角：插件已合法持有主库面，试图借一条语句读到/搬走别人的表。

const ATTACKER: &str = "com.bedcode.attacker";
const ATTACKER_PREFIX: &str = "plugin_com_bedcode_attacker_";
const SECRET: &str = "PLAINTEXT-HOST-MANAGED-SECRET";

/// 授予主库面（票 02：主库独立权限位）
fn grant_main_db(fake: &FakePorts, plugin_id: &str) {
    fake.grant(plugin_id, &[PERMISSION_DATABASE_MAIN]);
}

/// 直接在宿主主库上播种（模拟「别的插件/宿主自己的数据本来就在这张库里」）
fn seed_main(fake: &FakePorts, sql: &str) {
    let db = Arc::clone(&fake.main_db());
    drive(async move {
        let db = db.lock().await;
        db.conn().execute_batch(sql).expect("宿主侧播种语句应成功");
    });
}

/// 主库里某张表当前的行数（用于断言「拒绝不留副作用」）
fn main_row_count(fake: &FakePorts, table: &str) -> i64 {
    let db = Arc::clone(&fake.main_db());
    drive(async move {
        let db = db.lock().await;
        db.conn()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0))
            .unwrap_or(-1)
    })
}

/// 攻击者自己的表与一张暂存表就位（红测的前提：跨表读写的两端都真实存在）
fn attacker_tables_ready(fake: &FakePorts) {
    grant_main_db(fake, ATTACKER);
    seed_main(
        fake,
        &format!(
            "CREATE TABLE {ATTACKER_PREFIX}x (k TEXT); \
             CREATE TABLE {ATTACKER_PREFIX}stolen (value TEXT); \
             INSERT INTO {ATTACKER_PREFIX}x (k) VALUES ('own-row');"
        ),
    );
}

/// 反例①（逗号多表读）：`FROM 自己的表 a, plugin_secrets b` 只提取到第一个表名，
/// 校验放行后明文密钥被整行读出——本轮 P0-1 的原始攻击链
#[test]
fn main_db_comma_multitable_read_of_secrets_is_denied() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    seed_main(
        &fake,
        &format!(
            "INSERT OR REPLACE INTO plugin_secrets (plugin_id, key, value, updated_at) \
             VALUES ('com.bedcode.victim', 'jwt-key', '{SECRET}', '2026-09-21');"
        ),
    );
    attacker_tables_ready(&fake);

    let sql = &format!("SELECT b.value FROM {ATTACKER_PREFIX}x a, plugin_secrets b WHERE a.k = 'own-row'");
    let result = db_query(ports, ATTACKER, "SELECT 1 AS one");
    assert!(result.is_ok(), "本插件单表查询应放行: {result:?}");

    let outcome = db_query(ports, ATTACKER, sql);
    assert!(outcome.is_err(), "逗号多表跨读主库他表必须被拒，实际放行: {outcome:?}");
    if let Ok(Some(rows)) = &outcome {
        panic!("跨表读放行且取回数据（密钥泄露）: {rows}");
    }
}

/// 反例②（逗号多表搬数据）：把别人的表内容写进**自己**的表，绕过之后可任意读走
#[test]
fn main_db_comma_multitable_exfil_write_is_denied() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    seed_main(
        &fake,
        &format!(
            "INSERT OR REPLACE INTO plugin_secrets (plugin_id, key, value, updated_at) \
             VALUES ('com.bedcode.victim', 'jwt-key', '{SECRET}', '2026-09-21');"
        ),
    );
    attacker_tables_ready(&fake);

    let sql = &format!(
        "INSERT INTO {ATTACKER_PREFIX}stolen (value) \
         SELECT b.value FROM {ATTACKER_PREFIX}x a, plugin_secrets b"
    );
    let outcome = db_execute(ports, ATTACKER, sql);
    assert!(outcome.is_err(), "逗号多表跨写搬运必须被拒，实际放行: {outcome:?}");
    assert_eq!(
        main_row_count(&fake, &format!("{ATTACKER_PREFIX}stolen")),
        0,
        "被拒的语句不得留下任何副作用（暂存表必须仍为空）"
    );
}

/// 反例③（引号/方括号标识符变体）：正则只吃裸标识符，带引号的他表名照样是跨表读
#[test]
fn main_db_quoted_identifier_cross_read_is_denied() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    attacker_tables_ready(&fake);

    for sql in [
        &format!("SELECT b.value FROM {ATTACKER_PREFIX}x a, \"plugin_secrets\" b")[..],
        &format!("SELECT b.value FROM {ATTACKER_PREFIX}x a, [plugin_secrets] b")[..],
        &format!("SELECT b.value FROM {ATTACKER_PREFIX}x a, `plugin_secrets` b")[..],
        // 库名限定：main 是宿主主库自身的 schema 名
        "SELECT value FROM main.plugin_secrets",
    ] {
        let outcome = db_query(ports, ATTACKER, sql);
        assert!(outcome.is_err(), "跨表读变体未被拒: {sql} → {outcome:?}");
    }
}

/// 反例④（ATTACH）：挂载任意数据库文件 = 既能把别处的库读进来，也能往里写
#[test]
fn main_db_attach_is_denied() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    attacker_tables_ready(&fake);
    for sql in ["ATTACH ':memory:' AS evil", "ATTACH DATABASE ':memory:' AS evil"] {
        let outcome = db_execute(ports, ATTACKER, sql);
        assert!(outcome.is_err(), "ATTACH 任意库必须被拒: {sql} → {outcome:?}");
    }
}

/// 反例⑤（PRAGMA）：`database_list` 直接把宿主主库文件路径交给插件
#[test]
fn main_db_pragma_is_denied() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    attacker_tables_ready(&fake);
    for sql in [
        "PRAGMA database_list",
        "PRAGMA writable_schema = ON",
        "SELECT name FROM sqlite_master",
    ] {
        let outcome = db_query(ports, ATTACKER, sql);
        assert!(outcome.is_err(), "主库自省面必须被拒: {sql} → {outcome:?}");
    }
}

/// 正例（纵深不得过拦）：本插件前缀对象的完整生命周期照常可用
#[test]
fn main_db_own_prefix_tables_still_work_end_to_end() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    attacker_tables_ready(&fake);
    let table = format!("{ATTACKER_PREFIX}notes");
    db_execute(
        ports,
        ATTACKER,
        &format!("CREATE TABLE {table} (id INTEGER PRIMARY KEY, v TEXT)"),
    )
    .expect("建自己的表应放行");
    db_execute(ports, ATTACKER, &format!("INSERT INTO {table} (v) VALUES ('a')")).expect("写自己的表应放行");
    assert_eq!(
        db_query(ports, ATTACKER, &format!("SELECT v FROM {table}"))
            .expect("读自己的表应放行")
            .as_deref(),
        Some(r#"[{"v":"a"}]"#),
        "查询结果应原样返回"
    );
    db_execute(ports, ATTACKER, &format!("UPDATE {table} SET v = 'b' WHERE v = 'a'")).expect("更新自己的表应放行");
    db_execute(ports, ATTACKER, &format!("DELETE FROM {table} WHERE v = 'b'")).expect("删除自己的表应放行");
    // 自连接（自己的表出现两次）与带引号形态都必须放行
    db_query(ports, ATTACKER, &format!("SELECT a.id FROM {table} a, {table} b")).expect("本插件表之间的逗号多表应放行");
    db_query(ports, ATTACKER, &format!("SELECT * FROM \"{table}\"")).expect("带引号标识符的本插件表应放行");
    db_execute(ports, ATTACKER, &format!("ALTER TABLE {table} ADD COLUMN extra TEXT")).expect("改自己的表应放行");
    db_execute(ports, ATTACKER, &format!("DROP TABLE {table}")).expect("删自己的表应放行");
}

/// 正例（DDL 记账必然触达 sqlite_master / sqlite_sequence，不得因此过拦）
#[test]
fn main_db_own_prefix_ddl_family_still_works() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    attacker_tables_ready(&fake);
    let table = format!("{ATTACKER_PREFIX}auto");
    // AUTOINCREMENT 会写 sqlite_sequence
    db_execute(
        ports,
        ATTACKER,
        &format!("CREATE TABLE {table} (id INTEGER PRIMARY KEY AUTOINCREMENT, v TEXT)"),
    )
    .expect("AUTOINCREMENT 建表应放行");
    db_execute(ports, ATTACKER, &format!("INSERT INTO {table} (v) VALUES ('a'), ('b')")).expect("批量插入应放行");
    db_execute(
        ports,
        ATTACKER,
        &format!("CREATE INDEX {ATTACKER_PREFIX}ix ON {table} (v)"),
    )
    .expect("在自己表上建索引应放行（隐式 Reindex 不得被拒）");
    db_execute(ports, ATTACKER, &format!("DROP INDEX {ATTACKER_PREFIX}ix")).expect("删自己的索引应放行");
    db_execute(
        ports,
        ATTACKER,
        &format!("CREATE VIEW {ATTACKER_PREFIX}v AS SELECT v FROM {table}"),
    )
    .expect("在自己表上建视图应放行");
    assert_eq!(
        db_query(ports, ATTACKER, &format!("SELECT v FROM {ATTACKER_PREFIX}v"))
            .expect("读自己的视图应放行")
            .as_deref(),
        Some(r#"[{"v":"a"},{"v":"b"}]"#)
    );
    db_execute(ports, ATTACKER, &format!("DROP VIEW {ATTACKER_PREFIX}v")).expect("删自己的视图应放行");
    // 触发器用例本票不覆盖：`CREATE TRIGGER ... BEGIN ... END` 以 END 结尾，
    // 会被既有的 `reject_bare_transaction_control`（末 token 白名单）当成裸事务控制
    // 拒掉——与本票的表名纵深无关，是启发式检测的既有误拒（已登记票 02 Comments）
    db_execute(ports, ATTACKER, &format!("DROP TABLE {table}")).expect("删表应放行");
}

/// 反例（引擎层才是边界）：正则层不认的写法必须由守卫拦下，且给出引擎的拒因
#[test]
fn main_db_authorizer_error_names_the_boundary() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    attacker_tables_ready(&fake);
    let err = db_query(ports, ATTACKER, "SELECT * FROM plugin_settings").unwrap_err();
    assert!(
        err.contains("does not match required prefix"),
        "正则层能识别的写法应保持既有文案: {err}"
    );
    let err = db_query(
        ports,
        ATTACKER,
        &format!("SELECT b.value FROM {ATTACKER_PREFIX}x a, plugin_secrets b"),
    )
    .unwrap_err();
    assert!(
        err.contains("prohibited") && err.contains("plugin_secrets"),
        "正则层漏掉的写法应由引擎守卫拒绝并报出被禁对象（不是静默返回空）: {err}"
    );
    assert!(!err.contains(SECRET), "拒绝文案不得回带被查内容: {err}");
}

/// 反例（目录表）：DDL 会隐式记账到 sqlite_master，但插件自己点名一律拒
#[test]
fn main_db_schema_catalog_direct_reference_is_denied() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    attacker_tables_ready(&fake);
    for sql in [
        "SELECT name FROM sqlite_master WHERE type = 'table'",
        "SELECT * FROM \"sqlite_master\"",
        "SELECT * FROM [sqlite_sequence]",
    ] {
        let outcome = db_query(ports, ATTACKER, sql);
        assert!(outcome.is_err(), "目录表直接读必须被拒: {sql} → {outcome:?}");
    }
    for sql in [
        "UPDATE sqlite_master SET tbl_name = 'x' WHERE 1 = 1",
        "INSERT INTO sqlite_sequence (name, seq) VALUES ('anything', 1)",
        "DELETE FROM sqlite_master WHERE 1 = 1",
    ] {
        let outcome = db_execute(ports, ATTACKER, sql);
        assert!(outcome.is_err(), "目录表直接写必须被拒: {sql} → {outcome:?}");
    }
    // 拒了之后目录仍在：证明拒绝发生在 prepare，而不是把库改坏了
    assert!(
        main_row_count(&fake, "plugin_secrets") >= 0,
        "主库应仍可正常自省（宿主侧）"
    );
}

/// 反例（RENAME TO）：把自有表改名到别的前缀 = 在前缀之外凭空造表
#[test]
fn main_db_rename_to_foreign_prefix_is_denied() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    attacker_tables_ready(&fake);
    let outcome = db_execute(
        ports,
        ATTACKER,
        &format!("ALTER TABLE {ATTACKER_PREFIX}x RENAME TO plugin_secrets"),
    );
    assert!(outcome.is_err(), "RENAME TO 他表名必须被拒: {outcome:?}");
    // 自己的前缀内改名仍可用
    db_execute(
        ports,
        ATTACKER,
        &format!("ALTER TABLE {ATTACKER_PREFIX}x RENAME TO {ATTACKER_PREFIX}y"),
    )
    .expect("前缀内改名应放行");
}

/// 正例（扫描器不过拦）：字符串字面量里出现目录表名不算插件点名
#[test]
fn main_db_catalog_name_inside_literal_is_allowed() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    attacker_tables_ready(&fake);
    db_execute(
        ports,
        ATTACKER,
        "INSERT INTO plugin_com_bedcode_attacker_x (k) VALUES ('sqlite_master')",
    )
    .expect("字符串字面量里的目录表名不得触发拒绝");
    assert_eq!(
        db_query(
            ports,
            ATTACKER,
            "SELECT k FROM plugin_com_bedcode_attacker_x WHERE k = 'sqlite_master'",
        )
        .expect("读回自己的数据应放行")
        .as_deref(),
        Some(r#"[{"k":"sqlite_master"}]"#)
    );
}

/// 纵深只作用在主库面：同一条跨表 SQL，主库报前缀不符，私有库不受该约束
/// （无头上下文拿不到私有库句柄，因此比对的是错误归属而非执行结果）
#[test]
fn private_db_face_unaffected_by_main_db_isolation() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    fake.grant(ATTACKER, &[PERMISSION_DATABASE_MAIN, PERMISSION_STORAGE]);
    let sql = "SELECT * FROM plugin_secrets";
    let main_err = db_query(ports, ATTACKER, sql).expect_err("主库跨表读必须被拒");
    assert!(
        main_err.contains("does not match required prefix") || main_err.contains("prohibited"),
        "主库拒绝应给出隔离理由，got: {main_err}"
    );
    let private_err =
        plugin_db_query(ports, ATTACKER, sql).expect_err("无头上下文取不到私有库句柄（但不应因主库纵深而拒）");
    assert!(
        !private_err.contains("does not match required prefix") && !private_err.contains("prohibited"),
        "私有库不该被主库表名前缀/授权仲裁拦下，got: {private_err}"
    );
}
