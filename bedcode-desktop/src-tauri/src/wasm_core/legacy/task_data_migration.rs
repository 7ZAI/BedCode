//! 旧 auto-task 私有库 → 合并插件私有库的一次性搬运（票 17 / spec D5、用户故事 4）
//!
//! **为什么需要它**：票 16 把任务域表名统一到 `task_*` 前缀，但那只是**在合并插件自己
//! 的私有库内**改名（`ALTER TABLE … RENAME`，零复制）。六张任务表历史上物理位于
//! `app_data/plugins/com.bedcode.auto-task/plugin.db`，而插件私有库按插件 id 分文件，
//! 合并插件读的是 `…/com.bedcode.terminal-session/plugin.db`——所以「升级后任务历史一条不丢」
//! 到票 17 才真正成立。形状沿用已退役的宿主侧一次性迁移 `peer_migration`（业务
//! 数据清零票 06 删除，见 git 历史）：
//! 存在性即版本戳、best-effort 不阻断启动、可对旧库重跑。
//!
//! **顺序前提**：本迁移跑在 `PluginHost::new()` **之后**——合并插件 `activate` 里的
//! `schema::migrate_via_host()`（库内改名）与各域 `ensure_schema`（建表）已完成，
//! 目标表此刻存在。反过来说，目标库还不存在时（插件从未激活过）本轮整体跳过，
//! 下次启动再搬：搬运的前提恰恰是「接管方已经把自己的表建好」。
//!
//! **不搬的东西**：`plugin_meta`（各自的迁移账本，语义按插件 id 隔离）、
//! `session_configs`（票 08 已从主库迁入私有库，与旧插件库无关）。
//!
//! **副作用已知**：合并插件 `activate` 里的 `recover_creating_jobs`（定时任务 creating
//! 随进程销毁 → failed）先于本迁移跑完，故此刻拷进来的 `creating` 行不会被那一轮兜底
//! 处理——由定时域的静默看门狗在后续 tick 收口，不做二次恢复（避免与 tick 竞争）。

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use tauri::Manager;

/// 被退役的旧插件（票 17）
pub const LEGACY_PLUGIN_ID: &str = "com.bedcode.auto-task";
/// 接管方（终端会话中心）
pub const TARGET_PLUGIN_ID: &str = "com.bedcode.terminal-session";

/// 幂等版本戳键（落在目标库 `plugin_meta`，与票 16 的库内改名账本同表同形）
pub const MIGRATION_LEDGER: &str = "task_data.migrated_from=com.bedcode.auto-task";

/// `(旧库表名 → 新库表名)` 六张任务表
///
/// 后四条与插件侧 `plugins/terminal-session/rust/src/schema.rs::TABLE_RENAMES` 逐字一致，
/// 前两条本已合规只需原名拷贝。真源在插件（它决定自己库里的表名），宿主这份是
/// 消费方副本——漂移由 [`task_table_copies_match_plugin_rename_list`] 撞红。
pub const TASK_TABLE_COPIES: &[(&str, &str)] = &[
    ("task_history", "task_history"),
    ("task_queue", "task_queue"),
    ("session_mapping", "task_session_mapping"),
    ("session_settings", "task_session_settings"),
    ("preset_tasks", "task_preset"),
    ("scheduled_jobs", "task_scheduled"),
];

/// 搬运结果（宿主日志与测试断言的外部可见面）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TaskDataMigrationReport {
    /// 整体未执行搬运的原因（首装、已迁移过、目标库尚未创建）
    pub skipped: Option<String>,
    /// `"旧名→新名"` → 实际插入行数（`INSERT OR IGNORE` 去重后）
    pub copied: BTreeMap<String, u64>,
    /// 旧库里没有该表（旧版本未启用该域，或已搬完并清理）
    pub missing_legacy: Vec<String>,
    /// 目标库里没有接管表（插件该域未建表）——不建空表，避免替插件决定 schema
    pub missing_target: Vec<String>,
    /// 单表失败描述（列集无交集、SQL 错误），不短路其余表
    pub failed: Vec<String>,
}

impl TaskDataMigrationReport {
    pub fn inserted_total(&self) -> u64 {
        self.copied.values().sum()
    }
}

/// 插件私有库文件路径（与宿主 `WasmHostContext` 的解析同源：
/// `app_data_dir/plugins/<plugin_id>/plugin.db`）
pub fn plugin_db_path(app_data_dir: &Path, plugin_id: &str) -> std::path::PathBuf {
    app_data_dir.join("plugins").join(plugin_id).join("plugin.db")
}

/// 迁移入口（setup 阶段调用一次；失败只记日志，绝不阻断启动）
pub fn run(app: &tauri::AppHandle) {
    let Ok(app_data_dir) = app.path().app_data_dir() else {
        tracing::warn!("task data migration skipped (app data dir unavailable)");
        return;
    };
    match migrate(&app_data_dir) {
        Ok(report) => {
            if let Some(reason) = &report.skipped {
                tracing::info!(plugin_id = %TARGET_PLUGIN_ID, "任务数据一次性搬运跳过: {reason}");
            } else {
                tracing::info!(
                    plugin_id = %TARGET_PLUGIN_ID,
                    inserted = report.inserted_total(),
                    copied = ?report.copied,
                    missing_legacy = ?report.missing_legacy,
                    missing_target = ?report.missing_target,
                    "旧 auto-task 私有库任务数据已搬入合并插件私有库"
                );
            }
            for failure in &report.failed {
                // 单表失败是可恢复缺口（其余表照常搬），但影响用户故事 4，按 error 留痕
                tracing::error!(plugin_id = %TARGET_PLUGIN_ID, "任务表搬运失败: {failure}");
            }
        }
        Err(e) => tracing::warn!(
            error = %e,
            "任务数据一次性搬运整体失败（不阻断启动；下次启动重试）"
        ),
    }
}

/// 搬运主体（app_data_dir 注入，便于无头幂等测试）
pub fn migrate(app_data_dir: &Path) -> crate::Result<TaskDataMigrationReport> {
    let legacy_path = plugin_db_path(app_data_dir, LEGACY_PLUGIN_ID);
    let target_path = plugin_db_path(app_data_dir, TARGET_PLUGIN_ID);

    if !legacy_path.exists() {
        return Ok(TaskDataMigrationReport {
            skipped: Some(format!("旧插件私有库不存在: {}", legacy_path.display())),
            ..Default::default()
        });
    }
    if !target_path.exists() {
        return Ok(TaskDataMigrationReport {
            skipped: Some(format!(
                "合并插件私有库尚未创建（插件未激活过），本轮不搬: {}",
                target_path.display()
            )),
            ..Default::default()
        });
    }

    let mut report = TaskDataMigrationReport::default();

    // 目标库读写：插件连接仍在（`plugin_dbs` 缓存），SQLite 按文件加锁，
    // busy_timeout 兜住启动期插件定时 tick 的并发写
    let target = Connection::open(&target_path).map_err(|e| {
        crate::AppError::Plugin(format!(
            "task migration: open target db '{}' failed: {e}",
            target_path.display()
        ))
    })?;
    target
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| crate::AppError::Plugin(format!("task migration: busy_timeout failed: {e}")))?;
    if already_migrated(&target)? {
        report.skipped = Some(format!("{MIGRATION_LEDGER} 已在账，搬运只跑一次"));
        return Ok(report);
    }

    let legacy = Connection::open_with_flags(&legacy_path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| {
        crate::AppError::Plugin(format!(
            "task migration: open legacy db '{}' failed: {e}",
            legacy_path.display()
        ))
    })?;
    // ATTACH 到目标连接上，跨库拷贝才是一条 SQL 能做的事；文件名走参数而非拼接
    sql_ctx(
        "ATTACH legacy db",
        target.execute("ATTACH DATABASE ?1 AS legacy_db", [legacy_path.to_string_lossy()]),
    )?;

    for (legacy_table, target_table) in TASK_TABLE_COPIES {
        let key = format!("{legacy_table}→{target_table}");
        if !table_exists(&legacy, "main", legacy_table)? {
            report.missing_legacy.push(key);
            continue;
        }
        if !table_exists(&target, "main", target_table)? {
            report.missing_target.push(key);
            continue;
        }
        match copy_table(&target, legacy_table, target_table) {
            Ok(inserted) => {
                report.copied.insert(key, inserted);
            }
            Err(e) => report.failed.push(format!("{key}: {e}")),
        }
    }

    sql_ctx("DETACH legacy db", target.execute("DETACH DATABASE legacy_db", []))?;

    // 版本戳最后写：中途崩溃时账没落，下次启动重跑（`INSERT OR IGNORE` 幂等）
    mark_migrated(&target, &report)?;
    Ok(report)
}

/// 账本判定（表或键缺失都算「未迁移过」，不视为错误）
/// rusqlite 错误自描述（"no such table" 等）但说不出**是哪一步**：统一包一层
/// 操作描述，避免裸 `?` 把上下文透传掉（AGENTS §6）
fn sql_ctx<T>(op: &str, result: rusqlite::Result<T>) -> crate::Result<T> {
    result.map_err(|e| crate::AppError::Plugin(format!("task migration: {op} failed: {e}")))
}

fn already_migrated(target: &Connection) -> crate::Result<bool> {
    Ok(ledger_value(target)?.is_some())
}

fn ledger_value(target: &Connection) -> crate::Result<Option<String>> {
    if !table_exists(target, "main", "plugin_meta")? {
        return Ok(None);
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
    Ok(value)
}

fn mark_migrated(target: &Connection, report: &TaskDataMigrationReport) -> crate::Result<()> {
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
        "missing_legacy": report.missing_legacy,
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

/// 按**列名交集**拷贝：只搬两库都有的列，顺序按目标表定义
///
/// 用 `SELECT *` 的话，任何一侧加过一列就整表失败（升级路径上真会出现：旧库是若干
/// 个版本前的形状）。交集让老数据照样落地，多余列留在旧库里由用户手动清理旧插件时
/// 一起消失。无交集时显性失败——那说明两库根本不是同一域，静默跳过更糟。
fn copy_table(target: &Connection, legacy_table: &str, target_table: &str) -> crate::Result<u64> {
    let target_columns = columns(target, "main", target_table)?;
    let legacy_columns: std::collections::HashSet<String> =
        columns(target, "legacy_db", legacy_table)?.into_iter().collect();
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
    // 表名来自 TASK_TABLE_COPIES 常量（无注入面）；列名逐一反引号包裹
    let sql = format!(
        "INSERT OR IGNORE INTO main.\"{target_table}\" ({quoted}) \
         SELECT {quoted} FROM legacy_db.\"{legacy_table}\""
    );
    let inserted = sql_ctx(
        &format!("copy {legacy_table} → {target_table}"),
        target.execute(&sql, []),
    )?;
    u64::try_from(inserted).map_err(|e| crate::AppError::Plugin(format!("task migration: row count overflow: {e}")))
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
        let dir = std::env::temp_dir().join(format!("bedcode_task_migration_{tag}_{}_{}", std::process::id(), nanos));
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

    fn row_count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    /// 旧库（auto-task 表名）+ 新库（合并插件表名）的最小可搬运现场
    fn seed_pair(dir: &Path) -> (Connection, Connection) {
        let legacy_path = plugin_db_path(dir, LEGACY_PLUGIN_ID);
        let target_path = plugin_db_path(dir, TARGET_PLUGIN_ID);
        create_db(
            &legacy_path,
            &[
                "CREATE TABLE task_history (id TEXT PRIMARY KEY, session_id TEXT, status TEXT, description TEXT)",
                "CREATE TABLE task_queue (id TEXT PRIMARY KEY, session_id TEXT, prompt TEXT, position INTEGER)",
                "CREATE TABLE session_mapping (claude_session_id TEXT PRIMARY KEY, session_id TEXT)",
                "CREATE TABLE session_settings (session_id TEXT PRIMARY KEY, auto_execute INTEGER, auto_answer INTEGER)",
                "CREATE TABLE preset_tasks (id TEXT PRIMARY KEY, prompt TEXT)",
                "CREATE TABLE scheduled_jobs (id TEXT PRIMARY KEY, config_id TEXT, status TEXT)",
                "INSERT INTO task_history VALUES ('h1','s1','completed','跑测试')",
                "INSERT INTO task_history VALUES ('h2','s1','interrupted','跑回归')",
                "INSERT INTO task_queue VALUES ('q1','s1','夜间任务',0)",
                "INSERT INTO session_mapping VALUES ('c1','s1')",
                "INSERT INTO session_settings VALUES ('s1',1,0)",
                "INSERT INTO preset_tasks VALUES ('p1','预设')",
                "INSERT INTO scheduled_jobs VALUES ('j1','cfg1','pending')",
            ],
        );
        create_db(
            &target_path,
            &[
                "CREATE TABLE plugin_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
                "CREATE TABLE task_history (id TEXT PRIMARY KEY, session_id TEXT, status TEXT, description TEXT)",
                "CREATE TABLE task_queue (id TEXT PRIMARY KEY, session_id TEXT, prompt TEXT, position INTEGER)",
                "CREATE TABLE task_session_mapping (claude_session_id TEXT PRIMARY KEY, session_id TEXT)",
                "CREATE TABLE task_session_settings (session_id TEXT PRIMARY KEY, auto_execute INTEGER, auto_answer INTEGER)",
                "CREATE TABLE task_preset (id TEXT PRIMARY KEY, prompt TEXT)",
                "CREATE TABLE task_scheduled (id TEXT PRIMARY KEY, config_id TEXT, status TEXT)",
            ],
        );
        (
            Connection::open(&legacy_path).unwrap(),
            Connection::open(&target_path).unwrap(),
        )
    }

    #[test]
    fn m1_moves_every_task_table_into_merged_plugin_db() {
        let dir = temp_dir("m1");
        let (legacy, target) = seed_pair(&dir);
        assert_eq!(row_count(&legacy, "task_history"), 2);
        assert_eq!(row_count(&target, "task_history"), 0);

        let report = migrate(&dir).expect("迁移不应失败");
        assert_eq!(report.skipped, None, "首次启动不应跳过: {:?}", report.skipped);
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(report.copied.len(), 6, "{:?}", report.copied);
        assert_eq!(report.inserted_total(), 7, "{:?}", report.copied);
        // 六张表逐条落到合并插件的表名下
        for (legacy_table, target_table) in TASK_TABLE_COPIES {
            assert_eq!(
                row_count(&target, target_table),
                row_count(&legacy, legacy_table),
                "{legacy_table} → {target_table} 行数不等"
            );
        }
        // 值本身也得在（列名交集不是空拷贝）
        let description: String = target
            .query_row("SELECT description FROM task_history WHERE id='h1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(description, "跑测试");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn m2_is_idempotent_across_restarts_and_never_duplicates() {
        let dir = temp_dir("m2");
        let (_legacy, target) = seed_pair(&dir);
        migrate(&dir).unwrap();

        // 用户在合并插件里新增一行后再重跑：老行不重复、新行不受扰
        target
            .execute("INSERT INTO task_history VALUES ('h9','s2','completed','新记录')", [])
            .unwrap();
        let second = migrate(&dir).expect("重跑不应报错");
        assert!(second.skipped.is_some(), "账本已落，第二次必须整体跳过");
        assert_eq!(row_count(&target, "task_history"), 3);
        assert_eq!(
            target
                .query_row("SELECT COUNT(*) FROM task_history WHERE id IN ('h1','h2')", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            2,
            "同主键不得二次插入"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn m3_skips_when_legacy_or_target_db_absent() {
        // 全新安装：旧库不存在 → 跳过且不报错（temp_dir 只建目录，两份库文件都还没生成）
        let dir = temp_dir("m3_legacy_absent");
        assert!(!plugin_db_path(&dir, LEGACY_PLUGIN_ID).exists());
        let report = migrate(&dir).unwrap();
        assert!(report.skipped.is_some());
        assert_eq!(report.inserted_total(), 0);

        // 旧库在、插件从未激活（目标库还没建）→ 跳过，留下次启动重试
        let dir = temp_dir("m3_target_absent");
        Connection::open(plugin_db_path(&dir, LEGACY_PLUGIN_ID))
            .unwrap()
            .execute("CREATE TABLE task_history (id TEXT PRIMARY KEY)", [])
            .unwrap();
        // 目标库文件压根不存在（temp_dir 只建目录），迁移不得替插件建库
        assert!(!plugin_db_path(&dir, TARGET_PLUGIN_ID).exists());
        let report = migrate(&dir).unwrap();
        assert!(report.skipped.is_some(), "{:?}", report.skipped);
        assert!(!plugin_db_path(&dir, TARGET_PLUGIN_ID).exists(), "不该替插件建库");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn m4_partial_schema_degrades_per_table_without_blocking_others() {
        let dir = temp_dir("m4");
        seed_pair(&dir);
        // 制造三种局部缺口：旧库缺 scheduled_jobs、目标库缺 task_preset、列集不同
        let legacy_path = plugin_db_path(&dir, LEGACY_PLUGIN_ID);
        Connection::open(&legacy_path)
            .unwrap()
            .execute("DROP TABLE scheduled_jobs", [])
            .unwrap();
        let target_path = plugin_db_path(&dir, TARGET_PLUGIN_ID);
        Connection::open(&target_path)
            .unwrap()
            .execute("DROP TABLE task_preset", [])
            .unwrap();

        let report = migrate(&dir).unwrap();
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(report.missing_legacy, vec!["scheduled_jobs→task_scheduled"]);
        assert_eq!(report.missing_target, vec!["preset_tasks→task_preset"]);
        // 其余四张照搬（单表缺口不短路）
        assert_eq!(report.copied.len(), 4, "{:?}", report.copied);
        let target = Connection::open(&target_path).unwrap();
        assert_eq!(row_count(&target, "task_history"), 2);
        assert_eq!(row_count(&target, "task_session_mapping"), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn m5_copies_the_column_intersection_not_positional_star() {
        let dir = temp_dir("m5");
        let legacy_path = plugin_db_path(&dir, LEGACY_PLUGIN_ID);
        let target_path = plugin_db_path(&dir, TARGET_PLUGIN_ID);
        // 旧库多一列 result（后来被删掉）、列序也不同
        create_db(
            &legacy_path,
            &[
                "CREATE TABLE task_history (status TEXT, id TEXT PRIMARY KEY, description TEXT, result TEXT)",
                "INSERT INTO task_history VALUES ('completed','h1','跑测试','ok')",
            ],
        );
        create_db(
            &target_path,
            &["CREATE TABLE task_history (id TEXT PRIMARY KEY, session_id TEXT, status TEXT, description TEXT)"],
        );

        let report = migrate(&dir).unwrap();
        assert_eq!(report.copied.get("task_history→task_history"), Some(&1));
        let target = Connection::open(&target_path).unwrap();
        let (id, status): (String, String) = target
            .query_row("SELECT id, status FROM task_history", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        // 按列名对位（不是 SELECT * 的位置对位）：status 落 status、id 落 id
        assert_eq!((id.as_str(), status.as_str()), ("h1", "completed"));

        // 无公共列：显性失败而不是静默搬空（那说明两库不是同一域）
        std::fs::remove_dir_all(&dir).ok();
        let dir = temp_dir("m5_disjoint");
        let legacy_path = plugin_db_path(&dir, LEGACY_PLUGIN_ID);
        let target_path = plugin_db_path(&dir, TARGET_PLUGIN_ID);
        create_db(&legacy_path, &["CREATE TABLE task_history (only_legacy TEXT)"]);
        create_db(
            &target_path,
            &["CREATE TABLE task_history (id TEXT PRIMARY KEY, session_id TEXT)"],
        );
        let report = migrate(&dir).unwrap();
        assert_eq!(
            report.failed,
            vec![
                "task_history→task_history: Plugin error: task_history 与 task_history 无公共列，拒绝搬运".to_string()
            ],
            "无公共列必须登记失败: {:?}",
            report.failed
        );
        assert_eq!(report.inserted_total(), 0);
        let target = Connection::open(&target_path).unwrap();
        assert_eq!(row_count(&target, "task_history"), 0);
        // 失败也落账：下次启动不重复搬同一批坏数据（现象会一直留在日志里）
        assert!(ledger_value(&target).unwrap().is_some());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn m6_ledger_is_written_with_a_summary_and_survives_crash_before_it() {
        let dir = temp_dir("m6");
        seed_pair(&dir);
        let target_path = plugin_db_path(&dir, TARGET_PLUGIN_ID);
        // 搬运前账本里没有戳
        let before = Connection::open(&target_path).unwrap();
        assert!(ledger_value(&before).unwrap().is_none());
        drop(before);

        let report = migrate(&dir).unwrap();
        let target = Connection::open(&target_path).unwrap();
        let stamp = ledger_value(&target).unwrap().expect("搬运后必须落戳");
        let parsed: serde_json::Value = serde_json::from_str(&stamp).unwrap();
        assert_eq!(parsed["from"], LEGACY_PLUGIN_ID);
        assert_eq!(parsed["inserted"]["task_history→task_history"], 2);
        assert_eq!(report.copied.len(), 6);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 宿主副本与插件侧改名清单逐条一致（真源在插件，漂移即红）
    #[test]
    fn task_table_copies_match_plugin_rename_list() {
        let source = std::fs::read_to_string(format!(
            "{}/../plugins/terminal-session/rust/src/schema.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("插件 schema.rs 必须可读（改名清单真源）");
        let plugin_pairs: Vec<(String, String)> = source
            .split("TableRename {")
            .filter_map(|chunk| {
                let legacy = extract_str_field(chunk, "legacy")?;
                let prefixed = extract_str_field(chunk, "prefixed")?;
                Some((legacy, prefixed))
            })
            .collect();
        assert_eq!(plugin_pairs.len(), 4, "插件侧改名条目应为 4: {plugin_pairs:?}");

        let host_renames: Vec<(&str, &str)> = TASK_TABLE_COPIES
            .iter()
            .filter(|(from, to)| from != to)
            .copied()
            .collect();
        assert_eq!(host_renames.len(), 4);
        for (legacy, prefixed) in plugin_pairs {
            assert!(
                host_renames
                    .iter()
                    .any(|(from, to)| *from == legacy.as_str() && *to == prefixed.as_str()),
                "宿主副本缺少改名条目: {legacy} → {prefixed}"
            );
        }
    }

    fn extract_str_field(chunk: &str, field: &str) -> Option<String> {
        let marker = format!("{field}: \"");
        let start = chunk.find(&marker)? + marker.len();
        let rest = &chunk[start..];
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    }
}
