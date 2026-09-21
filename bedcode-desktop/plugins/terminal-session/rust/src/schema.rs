//! 私有库表名域前缀统一（票 16 / spec D5「表名随域重组」）
//!
//! 合并插件的私有库里同时住着**会话域**与**任务域**的表（票 08 落配置、票 15 落
//! 任务队列），任务域的表名是 auto-task 时代的原名（`preset_tasks`、`scheduled_jobs`
//! …），既不体现归属也不好审计。本模块把它们统一到 `session_*` / `task_*` 前缀。
//!
//! **改名不是重写数据**：`ALTER TABLE … RENAME TO …` 只改名字，行原样保留，因此
//! 「一次幂等重命名迁移」的代价与风险都集中在**顺序**和**可回退**两件事上：
//!
//! - 顺序：重命名必须在各域 `ensure_schema`（`CREATE TABLE IF NOT EXISTS`）**之前**
//!   跑。先建后改会让新名以空表形态先占位，重命名于是被跳过，旧表里的真实数据
//!   被永久留在无人读的名字下（现象是「升级后列表变空」而不是报错）。
//! - 可回退：实际发生的 `(旧名 → 新名)` 记在 `plugin_meta` 的账本键里，
//!   [`rollback_to_legacy_names`] 据此逆向改名——这就是清单要求的「旧名回滚窗口」。
//!   窗口不靠旧名视图维持：SQLite 上普通视图不可写（实测 INSERT 报
//!   `cannot modify ... because it is a view`），要可写就得给四张表各配三个
//!   `INSTEAD OF` 触发器，那份机械复杂度不值得（回滚走的是同一套重命名，零复制）。
//!
//! **旧名索引一并清理**：`ALTER TABLE RENAME` 不会改索引名（实测重命名后
//! `idx_session_mapping_session` 仍挂在 `task_session_mapping` 上），而各域 schema
//! 用新名再建一遍索引，于是同一张表出现两套同覆盖列的索引。本迁移在重命名后按表
//! 枚举并删除非 `sqlite_autoindex_*` 的索引，新索引由随后的 `ensure_schema` 建回
//! （`CREATE INDEX IF NOT EXISTS`，幂等）。

use serde::Serialize;

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 一条表名重命名（旧名 → 新名，`'static` 便于账本与日志零拷贝引用）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableRename {
    /// 迁移前的表名（票 15 及更早的 auto-task 命名）
    pub legacy: &'static str,
    /// 迁移后的表名（带域前缀）
    pub prefixed: &'static str,
}

/// 会话域与任务域的表名统一清单（spec D5：`session_*` / `task_*` 前缀）
///
/// 不在清单里的表已经合规：`task_history` / `task_queue`（任务域）、
/// `session_configs`（会话域）、`plugin_meta`（跨域迁移元数据，无业务语义）。
pub const TABLE_RENAMES: &[TableRename] = &[
    // Claude Code 会话 ↔ 床码会话映射：任务域记账（不是会话引擎的表）
    TableRename {
        legacy: "session_mapping",
        prefixed: "task_session_mapping",
    },
    // auto_execute / auto_answer 两个任务开关：同上，任务域语义
    TableRename {
        legacy: "session_settings",
        prefixed: "task_session_settings",
    },
    TableRename {
        legacy: "preset_tasks",
        prefixed: "task_preset",
    },
    TableRename {
        legacy: "scheduled_jobs",
        prefixed: "task_scheduled",
    },
];

/// 账本键：值 = JSON 对象 `{旧名: 新名}`（只记录**实际发生**的重命名）
pub const RENAME_LEDGER: &str = "schema.rename_ledger";

/// 迁移元数据表（与 `config::store::SCHEMA` 里的同一张表，语句幂等重复无副作用）
///
/// 本模块自己的账本不能依赖「配置面恰好先建过表」，因此显式带一条建表语句。
pub const META_SCHEMA: &str =
    "CREATE TABLE IF NOT EXISTS plugin_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)";

/// 重命名迁移结果（宿主日志与闭环测试断言的外部可见面）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SchemaMigrationReport {
    /// 本次实际改名的表（`"旧名→新名"`，按 [`TABLE_RENAMES`] 顺序）
    pub renamed: Vec<String>,
    /// 旧名已不在（新名或全新库）→ 无需改名的条目数：幂等重跑的主要计数
    pub already_prefixed: usize,
    /// 旧名与新名**同时存在**→ 不动任何一侧的条目（`"旧名|新名"`）
    ///
    /// 半迁移态（如回滚后旧表又长出数据）不能靠猜决定合并谁：静默改名会让一侧
    /// 的数据凭空消失，宁可留着让上层日志与票面可见。
    pub ambiguous: Vec<String>,
    /// 随重命名清理掉的旧索引数量
    pub indexes_dropped: usize,
    /// 账本是否被写过（本次或此前已有账则为 false——不重复落戳）
    pub ledger_written: bool,
}

/// 存储端口（行语义，SQL 只存在于 wasm 实现里；同 `config::store::ConfigStore` 范式）
pub trait SchemaStore {
    /// 建 `plugin_meta`（幂等）
    fn ensure_meta(&self) -> Result<(), String>;
    /// 现有表名（不含 `sqlite_*` 内部表）
    fn table_names(&self) -> Result<Vec<String>, String>;
    /// 指定表上的索引名（不含 `sqlite_autoindex_*`）
    fn indexes_of(&self, table: &str) -> Result<Vec<String>, String>;
    fn rename_table(&self, from: &str, to: &str) -> Result<(), String>;
    fn drop_index(&self, name: &str) -> Result<(), String>;
    fn meta_get(&self, key: &str) -> Result<Option<String>, String>;
    fn meta_set(&self, key: &str, value: &str) -> Result<(), String>;
}

/// 读账本（缺省 = 无账本 = 空映射）
fn read_ledger(store: &impl SchemaStore) -> Result<Vec<(String, String)>, String> {
    let raw = store.meta_get(RENAME_LEDGER)?;
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let parsed: std::collections::BTreeMap<String, String> = serde_json::from_str(&raw)
        .map_err(|e| format!("rename ledger unreadable: {} (raw: {})", e, raw))?;
    Ok(parsed.into_iter().collect())
}

fn write_ledger(store: &impl SchemaStore, entries: &[(String, String)]) -> Result<(), String> {
    let map: std::collections::BTreeMap<&str, &str> = entries
        .iter()
        .map(|(f, t)| (f.as_str(), t.as_str()))
        .collect();
    let raw = serde_json::to_string(&map)
        .map_err(|e| format!("rename ledger serialize failed: {}", e))?;
    store.meta_set(RENAME_LEDGER, &raw)
}

/// 一次性幂等重命名迁移：旧表名 → 域前缀表名
///
/// 可对同一库重复执行：第二次 `renamed` 为空、`already_prefixed` 覆盖全部条目。
/// 必须在各域 `ensure_schema` 之前调用（见模块文档「顺序」）。
pub fn migrate_to_prefixed_names(
    store: &impl SchemaStore,
) -> Result<SchemaMigrationReport, String> {
    store.ensure_meta()?;
    // 账本先读后改：坏账本必须在**任何改名发生之前**显性失败，否则半途而废的库
    // （名已改、账未落）会让回滚无从依据
    let existing_ledger = read_ledger(store)?;
    let tables = store.table_names()?;
    let has = |name: &str| tables.iter().any(|t| t == name);

    let mut report = SchemaMigrationReport::default();
    for entry in TABLE_RENAMES {
        if !has(entry.legacy) {
            report.already_prefixed += 1;
            continue;
        }
        if has(entry.prefixed) {
            report
                .ambiguous
                .push(format!("{}|{}", entry.legacy, entry.prefixed));
            continue;
        }
        store.rename_table(entry.legacy, entry.prefixed)?;
        // 旧索引随重命名一起清掉（名字不会自动跟改），新索引由 ensure_schema 建回
        for index in store.indexes_of(entry.prefixed)? {
            store.drop_index(&index)?;
            report.indexes_dropped += 1;
        }
        report
            .renamed
            .push(format!("{}→{}", entry.legacy, entry.prefixed));
    }

    if !report.renamed.is_empty() {
        // 账本累积：回滚需要知道历史上改过哪些名字，而不只是最近一批
        let mut ledger = existing_ledger;
        for entry in TABLE_RENAMES {
            if report
                .renamed
                .iter()
                .any(|r| r == &format!("{}→{}", entry.legacy, entry.prefixed))
            {
                if !ledger.iter().any(|(legacy, _)| legacy == entry.legacy) {
                    ledger.push((entry.legacy.to_string(), entry.prefixed.to_string()));
                }
            }
        }
        write_ledger(store, &ledger)?;
        report.ledger_written = true;
    }
    Ok(report)
}

/// 回滚：按账本把新名改回旧名（票 16「旧名回滚窗口」的执行面）
///
/// 只处理账本里**确实存在新名且旧名未被占用**的条目；冲突条目原样保留在账本里
/// （下次回滚仍可重试），绝不覆盖同名表。
pub fn rollback_to_legacy_names(store: &impl SchemaStore) -> Result<SchemaMigrationReport, String> {
    store.ensure_meta()?;
    let ledger = read_ledger(store)?;
    let tables = store.table_names()?;
    let has = |name: &str| tables.iter().any(|t| t == name);

    let mut report = SchemaMigrationReport::default();
    let mut remaining: Vec<(String, String)> = Vec::new();
    for (legacy, prefixed) in ledger {
        if !has(&prefixed) {
            // 新名不在了（表被删或已回滚过）→ 该条账目作废
            continue;
        }
        if has(&legacy) {
            report.ambiguous.push(format!("{}|{}", legacy, prefixed));
            remaining.push((legacy, prefixed));
            continue;
        }
        store.rename_table(&prefixed, &legacy)?;
        for index in store.indexes_of(&legacy)? {
            store.drop_index(&index)?;
            report.indexes_dropped += 1;
        }
        report.renamed.push(format!("{}→{}", prefixed, legacy));
    }
    write_ledger(store, &remaining)?;
    report.ledger_written = !report.renamed.is_empty();
    Ok(report)
}

/// 本插件私有库的全部表名（新名）——供闭环测试与源码扫描护栏对账
pub fn prefixed_table_names() -> Vec<&'static str> {
    let mut names = vec![
        "session_configs",
        "plugin_meta",
        "task_history",
        "task_queue",
    ];
    names.extend(TABLE_RENAMES.iter().map(|e| e.prefixed));
    names
}

// ==================== wasm：宿主私有库实现 ====================

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use super::*;
    use bedcode_plugin_api::host::HostPluginDatabase;
    use bedcode_plugin_api::sql_params;
    use bedcode_plugin_api::wasm_host::WasmHost;

    /// 查询结果（JSON 数组）取某一列的字符串值
    fn column(rows: Option<serde_json::Value>, key: &str) -> Vec<String> {
        rows.unwrap_or_else(|| serde_json::json!([]))
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|r| r.get(key).and_then(|v| v.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    impl SchemaStore for WasmHost {
        fn ensure_meta(&self) -> Result<(), String> {
            self.plugin_db_execute(META_SCHEMA)
                .map_err(|e| format!("plugin_meta init failed: {}", e.message))?;
            Ok(())
        }

        fn table_names(&self) -> Result<Vec<String>, String> {
            let rows = self
                .plugin_db_query("SELECT name FROM sqlite_master WHERE type = 'table'")
                .map_err(|e| format!("sqlite_master table query failed: {}", e.message))?;
            Ok(column(rows, "name")
                .into_iter()
                .filter(|n| !n.starts_with("sqlite_"))
                .collect())
        }

        fn indexes_of(&self, table: &str) -> Result<Vec<String>, String> {
            let rows = self
                .plugin_db_query_params(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = ?1",
                    &sql_params![table],
                )
                .map_err(|e| format!("sqlite_master index query failed: {}", e.message))?;
            Ok(column(rows, "name")
                .into_iter()
                .filter(|n| !n.starts_with("sqlite_autoindex"))
                .collect())
        }

        fn rename_table(&self, from: &str, to: &str) -> Result<(), String> {
            // 表名来自本模块常量与账本（不由外部输入拼接），标识符无需转义
            self.plugin_db_execute(&format!("ALTER TABLE {} RENAME TO {}", from, to))
                .map_err(|e| format!("rename table {} → {} failed: {}", from, to, e.message))?;
            Ok(())
        }

        fn drop_index(&self, name: &str) -> Result<(), String> {
            self.plugin_db_execute(&format!("DROP INDEX IF EXISTS {}", name))
                .map_err(|e| format!("drop index {} failed: {}", name, e.message))?;
            Ok(())
        }

        fn meta_get(&self, key: &str) -> Result<Option<String>, String> {
            let rows = self
                .plugin_db_query_params(
                    "SELECT value FROM plugin_meta WHERE key = ?1",
                    &sql_params![key],
                )
                .map_err(|e| format!("plugin_meta read failed: {}", e.message))?;
            Ok(column(rows, "value").into_iter().next())
        }

        fn meta_set(&self, key: &str, value: &str) -> Result<(), String> {
            self.plugin_db_execute_params(
                "INSERT OR REPLACE INTO plugin_meta (key, value) VALUES (?1, ?2)",
                &sql_params![key, value],
            )
            .map_err(|e| format!("plugin_meta write failed: {}", e.message))?;
            Ok(())
        }
    }
}

/// 表名前缀统一迁移（wasm 专属入口：activate 在建表之前调用）
#[cfg(target_arch = "wasm32")]
pub fn migrate_via_host() -> Result<SchemaMigrationReport, String> {
    migrate_to_prefixed_names(&WasmHost)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn migrate_via_host() -> Result<SchemaMigrationReport, String> {
    Err("schema migration unavailable outside wasm runtime".to_string())
}

/// 回滚窗口：按账本把新名改回旧名（运维/降级路径，不在常规生命周期里）
#[cfg(target_arch = "wasm32")]
pub fn rollback_via_host() -> Result<SchemaMigrationReport, String> {
    rollback_to_legacy_names(&WasmHost)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn rollback_via_host() -> Result<SchemaMigrationReport, String> {
    Err("schema rollback unavailable outside wasm runtime".to_string())
}

// ==================== Tests ====================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    /// 内存私有库（表名 + 索引名 + plugin_meta 行语义）
    #[derive(Default)]
    pub struct MockSchemaStore {
        tables: Mutex<Vec<String>>,
        /// `(表名, 索引名)`——重命名后索引跟着表走但**名字不变**（与 SQLite 一致）
        indexes: Mutex<Vec<(String, String)>>,
        meta: Mutex<BTreeMap<String, String>>,
        /// 副作用记录：断言「不重复改名」「不覆盖同名表」
        pub rename_log: Mutex<Vec<(String, String)>>,
        pub drop_log: Mutex<Vec<String>>,
    }

    impl MockSchemaStore {
        pub fn new(tables: &[&str]) -> Self {
            Self {
                tables: Mutex::new(tables.iter().map(|t| t.to_string()).collect()),
                ..Default::default()
            }
        }

        /// 造一张带索引的旧表
        pub fn with_index(table: &str, index: &str) -> Self {
            let store = Self::new(&[table]);
            store
                .indexes
                .lock()
                .unwrap()
                .push((table.to_string(), index.to_string()));
            store
        }

        pub fn has_table(&self, name: &str) -> bool {
            self.tables.lock().unwrap().iter().any(|t| t == name)
        }

        pub fn ledger(&self) -> Vec<(String, String)> {
            let raw = self
                .meta
                .lock()
                .unwrap()
                .get(RENAME_LEDGER)
                .cloned()
                .unwrap_or_else(|| "{}".to_string());
            serde_json::from_str::<BTreeMap<String, String>>(&raw)
                .unwrap_or_default()
                .into_iter()
                .collect()
        }
    }

    impl SchemaStore for MockSchemaStore {
        fn ensure_meta(&self) -> Result<(), String> {
            Ok(())
        }

        fn table_names(&self) -> Result<Vec<String>, String> {
            Ok(self.tables.lock().unwrap().clone())
        }

        fn indexes_of(&self, table: &str) -> Result<Vec<String>, String> {
            Ok(self
                .indexes
                .lock()
                .unwrap()
                .iter()
                .filter(|(t, _)| t == table)
                .map(|(_, i)| i.clone())
                .collect())
        }

        fn rename_table(&self, from: &str, to: &str) -> Result<(), String> {
            let mut tables = self.tables.lock().unwrap();
            if !tables.iter().any(|t| t == from) {
                return Err(format!("no such table: {}", from));
            }
            if tables.iter().any(|t| t == to) {
                return Err(format!("table already exists: {}", to));
            }
            tables.retain(|t| t != from);
            tables.push(to.to_string());
            drop(tables);
            let mut indexes = self.indexes.lock().unwrap();
            for (t, _) in indexes.iter_mut() {
                if t == from {
                    *t = to.to_string();
                }
            }
            self.rename_log
                .lock()
                .unwrap()
                .push((from.to_string(), to.to_string()));
            Ok(())
        }

        fn drop_index(&self, name: &str) -> Result<(), String> {
            let mut indexes = self.indexes.lock().unwrap();
            let before = indexes.len();
            indexes.retain(|(_, i)| i != name);
            if indexes.len() != before {
                self.drop_log.lock().unwrap().push(name.to_string());
            }
            Ok(())
        }

        fn meta_get(&self, key: &str) -> Result<Option<String>, String> {
            Ok(self.meta.lock().unwrap().get(key).cloned())
        }

        fn meta_set(&self, key: &str, value: &str) -> Result<(), String> {
            self.meta
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_string());
            Ok(())
        }
    }

    /// 票 15 时期的私有库：四张旧名表 + 一张带旧索引的表
    fn legacy_store() -> MockSchemaStore {
        let store = MockSchemaStore::new(&[
            "session_configs",
            "plugin_meta",
            "task_history",
            "task_queue",
            "session_mapping",
            "session_settings",
            "preset_tasks",
            "scheduled_jobs",
        ]);
        *store.indexes.lock().unwrap() = vec![
            (
                "session_mapping".to_string(),
                "idx_session_mapping_session".to_string(),
            ),
            (
                "task_history".to_string(),
                "idx_task_history_status".to_string(),
            ),
        ];
        store
    }

    /// 旧库一次改齐：四张表全部落新名、旧索引清理、账本可回滚
    #[test]
    fn migrate_renames_every_legacy_table_once() {
        let store = legacy_store();
        let report = migrate_to_prefixed_names(&store).expect("migrate");

        assert_eq!(
            report.renamed.len(),
            4,
            "改齐四条, got: {:?}",
            report.renamed
        );
        assert!(store.has_table("task_session_mapping"));
        assert!(store.has_table("task_session_settings"));
        assert!(store.has_table("task_preset"));
        assert!(store.has_table("task_scheduled"));
        assert!(!store.has_table("session_mapping"), "旧名不应残留");
        assert!(!store.has_table("preset_tasks"));
        // 合规表零改动
        assert!(store.has_table("session_configs"));
        assert!(store.has_table("task_history"));
        // 已合规表的索引不得被牵连清理
        assert_eq!(report.indexes_dropped, 1, "只清旧名表带过来的索引");
        assert_eq!(
            *store.drop_log.lock().unwrap(),
            vec!["idx_session_mapping_session".to_string()]
        );
        assert_eq!(report.ledger_written, true);
        assert_eq!(store.ledger().len(), 4);
    }

    /// 幂等：对同一库重复执行不产生第二次改名、不新增账本条目
    #[test]
    fn migrate_is_idempotent_and_repeatable() {
        let store = legacy_store();
        migrate_to_prefixed_names(&store).expect("first");
        let second = migrate_to_prefixed_names(&store).expect("second");

        assert!(
            second.renamed.is_empty(),
            "重跑不得再改名, got: {:?}",
            second.renamed
        );
        assert_eq!(second.already_prefixed, 4);
        assert!(second.ambiguous.is_empty());
        assert_eq!(second.ledger_written, false, "无实际改名就不写账");
        assert_eq!(
            store.rename_log.lock().unwrap().len(),
            4,
            "每条改名只发生一次"
        );
        assert_eq!(store.ledger().len(), 4, "账本不重复累积");
    }

    /// 全新库（只有合规名）：迁移是 no-op，且不写账本
    #[test]
    fn migrate_on_fresh_database_does_nothing() {
        let store = MockSchemaStore::new(&["session_configs", "plugin_meta"]);
        let report = migrate_to_prefixed_names(&store).expect("migrate");
        assert!(report.renamed.is_empty());
        assert_eq!(report.already_prefixed, 4);
        assert_eq!(report.ledger_written, false);
        assert!(store.ledger().is_empty());
    }

    /// 半迁移态（旧名与新名同时存在）→ 两侧都不动，歧义显性上报
    ///
    /// 这是「先建表后改名」或「回滚后旧表又长数据」的现场。自动合并必然丢一侧
    /// 的行，因此这里只登记，不改名——把决定权留给读日志的人。
    #[test]
    fn migrate_keeps_both_sides_when_names_collide() {
        let store = MockSchemaStore::new(&["preset_tasks", "task_preset"]);
        let report = migrate_to_prefixed_names(&store).expect("migrate");

        assert!(report.renamed.is_empty());
        assert_eq!(
            report.ambiguous,
            vec!["preset_tasks|task_preset".to_string()]
        );
        assert!(store.has_table("preset_tasks"), "冲突时不得删除任何一侧");
        assert!(store.has_table("task_preset"));
        assert!(store.rename_log.lock().unwrap().is_empty());
    }

    /// 回滚窗口：按账本逆向改回旧名，账本随之收缩
    #[test]
    fn rollback_restores_legacy_names_from_ledger() {
        let store = legacy_store();
        migrate_to_prefixed_names(&store).expect("migrate");

        let back = rollback_to_legacy_names(&store).expect("rollback");
        assert_eq!(
            back.renamed.len(),
            4,
            "四条全部回滚, got: {:?}",
            back.renamed
        );
        assert!(store.has_table("session_mapping"));
        assert!(store.has_table("scheduled_jobs"));
        assert!(!store.has_table("task_session_mapping"));
        assert!(store.ledger().is_empty(), "回滚后账本清空");

        // 回滚后再升级：同样的四条改回来（迁移与回滚互为逆操作，可反复）
        let again = migrate_to_prefixed_names(&store).expect("re-migrate");
        assert_eq!(again.renamed.len(), 4);
        assert_eq!(store.ledger().len(), 4);
    }

    /// 回滚遇到旧名被占用 → 保留账目与两张表（不覆盖）
    #[test]
    fn rollback_does_not_overwrite_an_occupied_legacy_name() {
        let store = legacy_store();
        migrate_to_prefixed_names(&store).expect("migrate");
        // 模拟旧构建在回滚前又建了同名表
        store
            .tables
            .lock()
            .unwrap()
            .push("session_mapping".to_string());

        let back = rollback_to_legacy_names(&store).expect("rollback");
        assert_eq!(
            back.renamed.len(),
            3,
            "冲突那条跳过, got: {:?}",
            back.renamed
        );
        assert_eq!(back.ambiguous.len(), 1);
        assert!(store.has_table("task_session_mapping"), "冲突时新名表保留");
        assert_eq!(store.ledger().len(), 1, "未回滚成的条目留在账本");
        assert_eq!(store.ledger()[0].0, "session_mapping");
    }

    /// 账本不可读（被外部写坏）→ 在任何改名之前显性报错
    #[test]
    fn migrate_rejects_corrupt_ledger_before_touching_tables() {
        let store = legacy_store();
        store
            .meta
            .lock()
            .unwrap()
            .insert(RENAME_LEDGER.to_string(), "not json".to_string());
        let err = migrate_to_prefixed_names(&store).expect_err("坏账本必须报错");
        assert!(err.contains("ledger"), "错误信息需含账本上下文, got: {err}");
        assert!(
            store.rename_log.lock().unwrap().is_empty(),
            "报错不得留下半途迁移的库"
        );
        assert!(store.has_table("scheduled_jobs"), "旧名全部保留");
    }

    /// 改名清单本身不重复、不把表改成自己（防手滑：`(a, a)` 会让 SQLite 直接报错）
    #[test]
    fn rename_list_is_wellformed() {
        let mut seen: Vec<&str> = Vec::new();
        for entry in TABLE_RENAMES {
            assert_ne!(entry.legacy, entry.prefixed, "旧名与新名不得相同");
            assert!(
                entry.prefixed.starts_with("session_") || entry.prefixed.starts_with("task_"),
                "新名必须带域前缀: {}",
                entry.prefixed
            );
            assert!(!seen.contains(&entry.legacy), "旧名重复: {}", entry.legacy);
            assert!(
                !seen.contains(&entry.prefixed),
                "新名重复: {}",
                entry.prefixed
            );
            seen.push(entry.legacy);
            seen.push(entry.prefixed);
        }
        assert_eq!(TABLE_RENAMES.len(), 4);
    }

    /// 内存实现的自证（rename / drop / ledger 的行语义与 SQLite 对齐）
    #[test]
    fn mock_store_roundtrip() {
        let store = MockSchemaStore::with_index("t_old", "idx_t_old");
        assert_eq!(
            store.indexes_of("t_old").unwrap(),
            vec!["idx_t_old".to_string()]
        );
        store.rename_table("t_old", "t_new").unwrap();
        assert!(store.has_table("t_new") && !store.has_table("t_old"));
        assert_eq!(
            store.indexes_of("t_new").unwrap(),
            vec!["idx_t_old".to_string()],
            "索引跟表走、名字不变（SQLite 实测行为）"
        );
        store.drop_index("idx_t_old").unwrap();
        assert!(store.indexes_of("t_new").unwrap().is_empty());
        assert!(store.rename_table("missing", "other").is_err());
    }

    /// 统一后的表名清单与本模块对账（票面要求的「会话域与任务域前缀统一」可见）
    #[test]
    fn prefixed_names_cover_both_domains() {
        let names = prefixed_table_names();
        for expected in [
            "session_configs",
            "task_history",
            "task_queue",
            "task_session_mapping",
            "task_session_settings",
            "task_preset",
            "task_scheduled",
        ] {
            assert!(names.contains(&expected), "缺 {}: {:?}", expected, names);
        }
        for legacy in TABLE_RENAMES {
            assert!(!names.contains(&legacy.legacy), "旧名不得出现在统一清单");
        }
    }
}
