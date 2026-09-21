//! 终端会话中心私有库 id 路径迁移（票 07 B2）
//!
//! **为什么需要它**：票 06 把插件 id 从 `com.bedcode.session` 改名为
//! `com.bedcode.terminal-session`。插件私有库按插件 id 分文件
//! （`app_data/plugins/<plugin_id>/plugin.db`，见
//! [`task_data_migration::plugin_db_path`]），所以既有用户的会话配置 / 任务历史 /
//! 迁移账本物理地在旧路径 `…/plugins/com.bedcode.session/plugin.db`，改名后的插件
//! 读的是 `…/plugins/com.bedcode.terminal-session/plugin.db` ——「升级后配置与任务
//! 一条不丢」由本迁移保证。
//!
//! 形状沿用 [`task_data_migration`]（存在性 / 账本即版本戳、best-effort 不阻断启动、
//! 可对旧库重跑、`INSERT OR IGNORE` 幂等）：
//! - 目标库 `plugin_meta` 已有 `session_db.migrated_from=com.bedcode.session` 账本
//!   → 跳过（已搬完，账本键是唯一事实源）；
//! - 旧路径库不存在 → 跳过（首装 / 用户已清理）；
//! - 目标库不存在（改名后插件从未激活过 → 新路径库未建）→ 纯文件重命名整体位移；
//! - 两库都在 → 逐表按**列名交集** `INSERT OR IGNORE` 拷入目标库；task 域改名表
//!   （`session_mapping` → `task_session_mapping` 等）经
//!   [`task_data_migration::TASK_TABLE_COPIES`] 字典对齐；目标库没有的表不建
//!   （不替插件决定 schema），旧库独有表留在旧文件。
//!
//! **不搬**：`sqlite_%` 内部表（sqlite_sequence 由拷贝数据自建）；WAL/SHM 附属文件
//! （拷贝走 SQL 级，库内事务保证一致性）。
//!
//! **时序前提**：跑在 `PluginHost::new()` 之后（改名插件已激活、目标库已建表），
//! 与 [`task_data_migration::run`] 同点挂钩；两迁移写同一目标库的不同账本键，
//! 无顺序依赖。

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use tauri::Manager;

use super::task_data_migration::{plugin_db_path, TASK_TABLE_COPIES};

/// 被改名的旧 id（票 06 前）
pub const LEGACY_PLUGIN_ID: &str = "com.bedcode.session";
/// 接管方（改名后的终端会话中心）
pub const TARGET_PLUGIN_ID: &str = "com.bedcode.terminal-session";

/// 幂等版本戳键（落在目标库 `plugin_meta`，与 task_data 迁移账本同表同形）
pub const MIGRATION_LEDGER: &str = "session_db.migrated_from=com.bedcode.session";

/// 搬运结果（宿主日志与测试断言的外部可见面）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SessionDbMigrationReport {
    /// 整体未执行搬运的原因（首装、已迁移过）
    pub skipped: Option<String>,
    /// `表名` → 实际插入行数（`INSERT OR IGNORE` 去重后）
    pub copied: BTreeMap<String, u64>,
    /// 旧库独有表（目标库没有，不建空表）——数据留在旧文件里
    pub missing_target: Vec<String>,
    /// 单表失败描述（列集无交集、SQL 错误），不短路其余表
    pub failed: Vec<String>,
    /// 纯文件重命名分支的目标路径（插件从未以新 id 激活过的升级路径）
    pub renamed_to: Option<String>,
}

/// 迁移入口（setup 阶段调用一次；失败只记日志，绝不阻断启动）
pub fn run(app: &tauri::AppHandle) {
    let Ok(app_data_dir) = app.path().app_data_dir() else {
        tracing::warn!("session db migration skipped (app data dir unavailable)");
        return;
    };
    match migrate(&app_data_dir) {
        Ok(report) => {
            if let Some(reason) = &report.skipped {
                tracing::info!(plugin_id = %TARGET_PLUGIN_ID, "会话库 id 路径迁移跳过: {reason}");
                return;
            }
            if let Some(target) = &report.renamed_to {
                tracing::info!(
                    plugin_id = %TARGET_PLUGIN_ID,
                    target = %target,
                    "旧 id 私有库整体位移（插件未激活过新 id）"
                );
                return;
            }
            tracing::info!(
                plugin_id = %TARGET_PLUGIN_ID,
                inserted = report.inserted_total(),
                copied = ?report.copied,
                missing_target = ?report.missing_target,
                "旧 id 私有库已搬入新 id 私有库"
            );
            for failure in &report.failed {
                // 单表失败是可恢复缺口（其余表照常搬），但影响数据完整性，按 error 留痕
                tracing::error!(plugin_id = %TARGET_PLUGIN_ID, "会话库表搬运失败: {failure}");
            }
        }
        Err(e) => tracing::warn!(
            error = %e,
            "会话库 id 路径迁移整体失败（不阻断启动；下次启动重试）"
        ),
    }
}

impl SessionDbMigrationReport {
    pub fn inserted_total(&self) -> u64 {
        self.copied.values().sum()
    }
}

/// 搬运主体（app_data_dir 注入，便于无头幂等测试）
pub fn migrate(app_data_dir: &Path) -> crate::Result<SessionDbMigrationReport> {
    let legacy_path = plugin_db_path(app_data_dir, LEGACY_PLUGIN_ID);
    let target_path = plugin_db_path(app_data_dir, TARGET_PLUGIN_ID);

    if !legacy_path.exists() {
        return Ok(SessionDbMigrationReport {
            skipped: Some(format!("旧 id 路径私有库不存在: {}", legacy_path.display())),
            ..Default::default()
        });
    }
    if !target_path.exists() {
        // 改名后插件从未激活过（新路径库未建）：整体位移最安全——不丢任何表/账本，
        // 且目标库不存在时无并发写者，无锁竞争
        std::fs::rename(&legacy_path, &target_path).map_err(|e| {
            crate::AppError::Plugin(format!(
                "session db migration: rename '{}' → '{}' failed: {e}",
                legacy_path.display(),
                target_path.display()
            ))
        })?;
        return Ok(SessionDbMigrationReport {
            renamed_to: Some(target_path.display().to_string()),
            ..Default::default()
        });
    }

    let mut report = SessionDbMigrationReport::default();

    // 目标库读写：插件连接仍在（`plugin_dbs` 缓存），SQLite 按文件加锁，
    // busy_timeout 兜住启动期插件定时 tick 的并发写（同 task_data 迁移）
    let target = Connection::open(&target_path).map_err(|e| {
        crate::AppError::Plugin(format!(
            "session db migration: open target db '{}' failed: {e}",
            target_path.display()
        ))
    })?;
    sql_ctx(
        "set busy_timeout",
        target.busy_timeout(std::time::Duration::from_secs(5)),
    )?;
    if already_migrated(&target)? {
        report.skipped = Some(format!("{MIGRATION_LEDGER} 已在账，搬运只跑一次"));
        return Ok(report);
    }

    let legacy = Connection::open_with_flags(&legacy_path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| {
        crate::AppError::Plugin(format!(
            "session db migration: open legacy db '{}' failed: {e}",
            legacy_path.display()
        ))
    })?;
    // ATTACH 到目标连接上，跨库拷贝才是一条 SQL 能做的事；文件名走参数而非拼接
    sql_ctx(
        "ATTACH legacy db",
        target.execute("ATTACH DATABASE ?1 AS session_legacy", [legacy_path.to_string_lossy()]),
    )?;

    for legacy_table in legacy_tables(&legacy)? {
        let target_table = task_table_target_name(&legacy_table);
        let key = format!("{legacy_table}→{target_table}");
        if !table_exists(&target, "main", &target_table)? {
            report.missing_target.push(key);
            continue;
        }
        match copy_table(&target, &legacy_table, &target_table) {
            Ok(inserted) => {
                report.copied.insert(key, inserted);
            }
            Err(e) => report.failed.push(format!("{key}: {e}")),
        }
    }

    sql_ctx(
        "DETACH legacy db",
        target.execute("DETACH DATABASE session_legacy", []),
    )?;

    // 版本戳最后写：中途崩溃时账没落，下次启动重跑（`INSERT OR IGNORE` 幂等）
    mark_migrated(&target, &report)?;
    Ok(report)
}

/// 旧库全部用户表（排除 sqlite_% 内部表）
fn legacy_tables(conn: &Connection) -> crate::Result<Vec<String>> {
    let mut stmt = sql_ctx(
        "list legacy tables",
        conn.prepare(
            "SELECT name FROM sqlite_master \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        ),
    )?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(Result::ok)
        .collect();
    Ok(names)
}

/// task 域表名对齐：旧库里仍是改名前的表名（票 16 前升级路径）时经
/// [`TASK_TABLE_COPIES`] 映射到接管方的现名；其余表原名直拷。
fn task_table_target_name(legacy_table: &str) -> String {
    TASK_TABLE_COPIES
        .iter()
        .find(|(legacy, _)| *legacy == legacy_table)
        .map(|(_, target)| (*target).to_string())
        .unwrap_or_else(|| legacy_table.to_string())
}

/// 按**列名交集**拷贝：只搬两库都有的列，顺序按目标表定义（同 task_data 迁移——
/// 任何一侧加过一列就整表失败会坑掉升级路径；无交集时显性失败）
fn copy_table(target: &Connection, legacy_table: &str, target_table: &str) -> crate::Result<u64> {
    let target_columns = columns(target, "main", target_table)?;
    let legacy_columns: std::collections::HashSet<String> =
        columns(target, "session_legacy", legacy_table)?.into_iter().collect();
    let shared: Vec<&str> = target_columns
        .iter()
        .filter(|c| legacy_columns.contains(*c))
        .map(String::as_str)
        .collect();
    if shared.is_empty() {
        return Err(crate::AppError::Plugin(format!(
            "{legacy_table} 与 {target_table} 无公共列，拒绝搬运"
        )));
    }
    let quoted = shared.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    // 表名来自 sqlite_master 枚举 + 常量字典（无注入面）；列名逐一反引号包裹
    let sql = format!(
        "INSERT OR IGNORE INTO main.\"{target_table}\" ({quoted}) \
         SELECT {quoted} FROM session_legacy.\"{legacy_table}\""
    );
    let inserted = sql_ctx(
        &format!("copy {legacy_table} → {target_table}"),
        target.execute(&sql, []),
    )?;
    u64::try_from(inserted).map_err(|e| crate::AppError::Plugin(format!("session db migration: row count overflow: {e}")))
}

/// 账本判定（表或键缺失都算「未迁移过」，不视为错误）
/// rusqlite 错误自描述（"no such table" 等）但说不出**是哪一步**：统一包一层
/// 操作描述，避免裸 `?` 把上下文透传掉（AGENTS §6）
fn sql_ctx<T>(op: &str, result: rusqlite::Result<T>) -> crate::Result<T> {
    result.map_err(|e| crate::AppError::Plugin(format!("session db migration: {op} failed: {e}")))
}

fn already_migrated(target: &Connection) -> crate::Result<bool> {
    // 目标库是改名插件建的，plugin_meta 必然存在（插件 activate 的 schema 面）；
    // 即便缺失也按「未迁移」处理，mark_migrated 会建表兜底
    if !table_exists(target, "main", "plugin_meta")? {
        return Ok(false);
    }
    let value: Option<String> = sql_ctx(
        "read migration ledger",
        target.query_row(
            "SELECT value FROM plugin_meta WHERE key = ?1",
            [MIGRATION_LEDGER],
            |row| row.get(0),
        ),
    )
    .ok();
    Ok(value.is_some())
}

fn mark_migrated(target: &Connection, report: &SessionDbMigrationReport) -> crate::Result<()> {
    sql_ctx(
        "ensure plugin_meta",
        target.execute(
            "CREATE TABLE IF NOT EXISTS plugin_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        ),
    )?;
    let value = serde_json::to_string(&serde_json::json!({
        "from": LEGACY_PLUGIN_ID,
        "inserted": report.copied,
        "missing_target": report.missing_target,
        "failed": report.failed,
    }))?;
    sql_ctx(
        "write migration ledger",
        target.execute(
            "INSERT OR REPLACE INTO plugin_meta (key, value) VALUES (?1, ?2)",
            rusqlite::params![MIGRATION_LEDGER, value],
        ),
    )?;
    Ok(())
}

fn table_exists(conn: &Connection, schema: &str, table: &str) -> crate::Result<bool> {
    let count: i64 = sql_ctx(
        &format!("probe table {schema}.{table}"),
        conn.query_row(
            &format!("SELECT COUNT(*) FROM {schema}.sqlite_master WHERE type = 'table' AND name = ?1"),
            [table],
            |row| row.get(0),
        ),
    )?;
    Ok(count > 0)
}

fn columns(conn: &Connection, schema: &str, table: &str) -> crate::Result<Vec<String>> {
    let mut stmt = sql_ctx(
        &format!("read columns of {schema}.{table}"),
        conn.prepare(&format!("PRAGMA {schema}.table_info(\"{table}\")")),
    )?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    Ok(names)
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "bedcode_session_db_migration_{tag}_{}_{}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(plugin_db_path(&dir, LEGACY_PLUGIN_ID).parent().unwrap()).unwrap();
        std::fs::create_dir_all(plugin_db_path(&dir, TARGET_PLUGIN_ID).parent().unwrap()).unwrap();
        dir
    }

    fn create_db(path: &Path, statements: &[&str]) {
        let conn = Connection::open(path).unwrap();
        for sql in statements {
            conn.execute(sql, []).unwrap();
        }
    }

    fn row_count(path: &Path, table: &str) -> i64 {
        let conn = Connection::open(path).unwrap();
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    fn ledger_present(path: &Path) -> bool {
        let conn = Connection::open(path).unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM plugin_meta WHERE key = ?1",
            [MIGRATION_LEDGER],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            > 0
    }

    /// 升级现场：旧 id 库（老表名时代，task 域未被票 16 改名）+ 新 id 库
    /// （改名插件激活时建的现表名）
    fn seed_upgrade_pair(dir: &Path) -> (PathBuf, PathBuf) {
        let legacy_path = plugin_db_path(dir, LEGACY_PLUGIN_ID);
        let target_path = plugin_db_path(dir, TARGET_PLUGIN_ID);
        create_db(
            &legacy_path,
            &[
                "CREATE TABLE plugin_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
                "CREATE TABLE session_configs (id TEXT PRIMARY KEY, name TEXT, command TEXT)",
                "CREATE TABLE session_mapping (claude_session_id TEXT PRIMARY KEY, session_id TEXT)",
                "CREATE TABLE task_history (id TEXT PRIMARY KEY, session_id TEXT, status TEXT)",
                "INSERT INTO session_configs VALUES ('c1','开发','zsh')",
                "INSERT INTO session_configs VALUES ('c2','部署','bash')",
                "INSERT INTO session_mapping VALUES ('cl1','s1')",
                "INSERT INTO task_history VALUES ('h1','s1','completed')",
            ],
        );
        create_db(
            &target_path,
            &[
                "CREATE TABLE plugin_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
                "CREATE TABLE session_configs (id TEXT PRIMARY KEY, name TEXT, command TEXT)",
                "CREATE TABLE task_session_mapping (claude_session_id TEXT PRIMARY KEY, session_id TEXT)",
                "CREATE TABLE task_history (id TEXT PRIMARY KEY, session_id TEXT, status TEXT)",
            ],
        );
        (legacy_path, target_path)
    }

    /// 全部用户表（旧 + 新）都搬：配置、task 域改名表对齐、账本落章
    #[test]
    fn t1_upgrade_moves_configs_and_renamed_task_tables() {
        let dir = temp_dir("t1");
        let (legacy_path, target_path) = seed_upgrade_pair(&dir);

        let report = migrate(&dir).expect("迁移成功");
        assert_eq!(report.skipped, None);
        assert_eq!(report.copied.get("session_configs→session_configs"), Some(&2));
        assert_eq!(report.copied.get("session_mapping→task_session_mapping"), Some(&1));
        assert_eq!(report.copied.get("task_history→task_history"), Some(&1));
        assert_eq!(
            report.missing_target.len(),
            0,
            "新库应含全部目标表: {:?}",
            report.missing_target
        );
        assert!(report.failed.is_empty());

        // 数据落新库
        assert_eq!(row_count(&target_path, "session_configs"), 2);
        assert_eq!(row_count(&target_path, "task_session_mapping"), 1);
        assert_eq!(row_count(&target_path, "task_history"), 1);
        // 旧文件保留（账本为准，不做破坏性清理）
        assert!(legacy_path.exists());
        // 账本落章
        assert!(ledger_present(&target_path));
    }

    /// 幂等：账本在案后第二次运行跳过，不再拷贝
    #[test]
    fn t2_second_run_skips_via_ledger() {
        let dir = temp_dir("t2");
        let (_, target_path) = seed_upgrade_pair(&dir);
        migrate(&dir).expect("首次迁移成功");

        let report = migrate(&dir).expect("第二次迁移");
        assert!(report.skipped.is_some(), "账本在案必须跳过: {report:?}");
        assert!(report.copied.is_empty());
        assert_eq!(row_count(&target_path, "session_configs"), 2, "已搬数据不被重拷");
    }

    /// 首装 / 清理后：旧库不存在 → 跳过
    #[test]
    fn t3_legacy_missing_skips() {
        let dir = temp_dir("t3");
        let target_path = plugin_db_path(&dir, TARGET_PLUGIN_ID);
        create_db(
            &target_path,
            &[
                "CREATE TABLE plugin_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
                "CREATE TABLE session_configs (id TEXT PRIMARY KEY, name TEXT, command TEXT)",
            ],
        );

        let report = migrate(&dir).expect("迁移成功");
        assert!(report.skipped.is_some(), "旧库缺失必须跳过: {report:?}");
        assert!(report.copied.is_empty());
    }

    /// 插件从未以新 id 激活过（新库不存在）→ 纯文件重命名整体位移，数据完好
    #[test]
    fn t4_target_missing_pure_rename() {
        let dir = temp_dir("t4");
        let legacy_path = plugin_db_path(&dir, LEGACY_PLUGIN_ID);
        let target_path = plugin_db_path(&dir, TARGET_PLUGIN_ID);
        create_db(
            &legacy_path,
            &[
                "CREATE TABLE session_configs (id TEXT PRIMARY KEY, name TEXT)",
                "CREATE TABLE plugin_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
                "INSERT INTO session_configs VALUES ('c1','开发')",
            ],
        );

        let report = migrate(&dir).expect("迁移成功");
        assert_eq!(report.renamed_to.as_deref(), Some(target_path.to_str().unwrap()));
        assert!(!legacy_path.exists(), "文件已位移");
        assert_eq!(row_count(&target_path, "session_configs"), 1, "整库随文件走");

        // 位移后旧库缺失 → 再次运行跳过（幂等闭环）
        let report = migrate(&dir).expect("第二次迁移");
        assert!(report.skipped.is_some());
    }

    /// 同主键冲突：目标已行优先（INSERT OR IGNORE），只补缺的新行
    #[test]
    fn t5_existing_pk_conflict_keeps_target_row() {
        let dir = temp_dir("t5");
        let legacy_path = plugin_db_path(&dir, LEGACY_PLUGIN_ID);
        let target_path = plugin_db_path(&dir, TARGET_PLUGIN_ID);
        create_db(
            &legacy_path,
            &[
                "CREATE TABLE session_configs (id TEXT PRIMARY KEY, name TEXT, command TEXT)",
                "INSERT INTO session_configs VALUES ('c1','旧名','zsh')",
            ],
        );
        create_db(
            &target_path,
            &[
                "CREATE TABLE plugin_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
                "CREATE TABLE session_configs (id TEXT PRIMARY KEY, name TEXT, command TEXT)",
                // 改名后插件已写入一行：目标行必须保留（用户新改的配置不被旧库覆盖）
                "INSERT INTO session_configs VALUES ('c1','新名','fish')",
            ],
        );

        let report = migrate(&dir).expect("迁移成功");
        assert_eq!(report.copied.get("session_configs→session_configs"), Some(&0));
        let conn = Connection::open(&target_path).unwrap();
        let name: String = conn
            .query_row("SELECT name FROM session_configs WHERE id = 'c1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "新名", "冲突行必须保留目标库（新）值");
    }

    /// 旧库独有表（目标没有）→ 不建表、记 missing_target，其余表照常搬
    #[test]
    fn t6_legacy_only_table_reported_missing_target() {
        let dir = temp_dir("t6");
        let legacy_path = plugin_db_path(&dir, LEGACY_PLUGIN_ID);
        let target_path = plugin_db_path(&dir, TARGET_PLUGIN_ID);
        create_db(
            &legacy_path,
            &[
                "CREATE TABLE session_configs (id TEXT PRIMARY KEY, name TEXT, command TEXT)",
                "CREATE TABLE old_domain_only (id TEXT PRIMARY KEY)",
                "INSERT INTO session_configs VALUES ('c1','开发','zsh')",
                "INSERT INTO old_domain_only VALUES ('x')",
            ],
        );
        create_db(
            &target_path,
            &[
                "CREATE TABLE plugin_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
                "CREATE TABLE session_configs (id TEXT PRIMARY KEY, name TEXT, command TEXT)",
            ],
        );

        let report = migrate(&dir).expect("迁移成功");
        assert!(report.missing_target.iter().any(|k| k.starts_with("old_domain_only")), "{report:?}");
        assert_eq!(report.copied.get("session_configs→session_configs"), Some(&1));
        // 不替插件建表
        let count: i64 = Connection::open(&target_path)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1",
                ["old_domain_only"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "目标库不得出现旧库独有表");
    }
}