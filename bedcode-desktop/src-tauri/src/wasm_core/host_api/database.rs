//! `host-database`（主库 5 条）+ `host-plugin-database`（插件私有库 5 条）能力域实现
//!
//! 含 SQLite 表名前缀校验、SQLite authorizer 纵深、语句超时 / 行数 / 字节护栏、批次
//! 事务语义、rusqlite 列 → JSON 转换辅助。
//!
//! ## 为什么这段实现**留在 wasm 核心内**（ADR 0036）
//!
//! `wasm-core-lib-split` 票 07/08 曾把它连同 SQLite 引擎一起搬进
//! `bedcode-sqlite-engine` 能力域 crate，2026-10-05 撤回：这三个 interface 的
//! 权限门、表名前缀纵深、护栏与属主分区**是 wasm_core 自己设计的插件机制**，而
//! 非与机制无关的引擎能力；机制实现与机制真源（授权记录 / 插件键值表 / 权限判定）
//! 分处两个 crate 只会让归属出现两个答案。引擎面（连接管理 + `schema.sql`）同样
//! 回到 `crate::db`——本 crate 整体撤销。
//!
//! ## 端口仍然保留（测试缝，不是架构边界）
//!
//! 三处取值经 [`sqlite_ports::SqlitePorts`] 而非直接摸上下文：
//!
//! - **权限门**：判定与落日志只在宿主一处（安全闸门属 AGENTS §5.1.3 四类薄壳之二），
//!   本域只问结果；
//! - **库句柄**：主库共享连接与插件私有库的懒创建策略归 [`super::sqlite`] 的
//!   `HostSqlitePorts`，本域不复制第二份；
//! - **同步↔异步桥**：宿主那份唯一的 `block_on_async` 经端口调用（见
//!   [`sqlite_ports`] 模块文档）。
//!
//! 保留端口的唯一收益是可测性：域逻辑因此能在不构造完整 `WasmHostContext` 的前提下
//! 用假端口跑护栏与隔离用例（见 [`sqlite_scaffold`]）。
//!
//! 错误串（`"permission denied"` / `"database error: …"` 等）、结构化日志字段、SQL
//! 前缀校验与 authorizer 的判定顺序、护栏常量取值一律不动。
//!
//! ## 零业务代码红线（AGENTS §5.1）
//!
//! 本域只做引擎原语：方言白名单、表名前缀纵深、资源护栏、列 → JSON 形状。**表名
//! 一律按 `plugin_<sanitized-id>_*` 前缀隔离**，域内不解释任何业务表语义（授权
//! 记录 / 密钥 / 会话 …全在插件私有库）。

use crate::system::constants::{
    PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS, PLUGIN_DB_QUERY_MAX_BYTES, PLUGIN_DB_QUERY_MAX_ROWS,
    PLUGIN_DB_STATEMENT_TIMEOUT_SECS,
};
use regex::Regex;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::wasm_core::host_api::sqlite_ports::{block_on, SqlitePorts};
use crate::wasm_core::permission::{PERMISSION_DATABASE_MAIN, PERMISSION_STORAGE};

// ==================== 逻辑层（Component Model 绑定调用） ====================

/// 解析参数绑定 JSON 数组字符串（空串视为空数组）
fn parse_params_json(params_json: &str) -> Result<Vec<serde_json::Value>, String> {
    if params_json.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(params_json).map_err(|e| format!("invalid params JSON array: {}", e))
}

/// SQL 执行超时护栏：连接上安装 SQLite progress handler，运行 `f` 后（含 panic 路径）
/// 经 Drop 守卫自动移除 handler。
///
/// SQLite progress handler 每 `num_ops`（1000）次虚拟机步回调一次；超时即返回 true
/// （中断），后续 `sqlite3_step` 返回 SQLITE_INTERRUPT → 映射为「查询超时」错误。
/// 主库是内核与全部插件共用的单一连接（全局 Mutex），慢查询会阻塞配对/会话配置/
/// 设置等全部内核 DB 读写——本护栏是内核可用性保护（票据 05）。
/// 超时上限不可由插件调整（安全边界）；触发记结构化 warn!（plugin_id 字段）。
pub(crate) fn with_statement_timeout<T>(
    plugin_id: &str,
    conn: &rusqlite::Connection,
    timeout: Duration,
    f: impl FnOnce(&rusqlite::Connection) -> Result<T, String>,
) -> Result<T, String> {
    let deadline = Instant::now();
    let timed_out = Arc::new(AtomicBool::new(false));
    let flag = timed_out.clone();
    conn.progress_handler(
        1000,
        Some(move || {
            if deadline.elapsed() > timeout {
                flag.store(true, Ordering::Relaxed);
                true
            } else {
                false
            }
        }),
    );
    let _guard = ProgressHandlerGuard { conn };
    let result = f(conn);
    if timed_out.load(Ordering::Relaxed) {
        tracing::warn!(
            plugin_id = %plugin_id,
            timeout_secs = timeout.as_secs(),
            "plugin SQL statement timed out and was interrupted"
        );
        Err(format!(
            "database error: statement timed out after {}s (limit: {}s)",
            timeout.as_secs(),
            PLUGIN_DB_STATEMENT_TIMEOUT_SECS
        ))
    } else {
        result
    }
}

/// progress handler 生命周期守卫：作用域结束（含 panic 展开）时移除 handler，
/// 避免超时 handler 残留在共享主库连接上误中断内核自身的 DB 读写
struct ProgressHandlerGuard<'a> {
    conn: &'a rusqlite::Connection,
}

impl Drop for ProgressHandlerGuard<'_> {
    fn drop(&mut self) {
        self.conn.progress_handler(0, None::<fn() -> bool>);
    }
}

// ==================== Main-DB Table Authorization（票 02 / P0-1） ====================

/// 插件在本插件前缀之外**不可见**的表，由 SQLite 引擎在 prepare 阶段逐个动作仲裁。
///
/// 为什么不能只靠 `validate_sql_table_prefix`：那是八个正则模式的尽力而为匹配，
/// 逗号多表（`FROM 自己的表 a, plugin_secrets b`）、引号/方括号标识符、`main.` 库名限定、
/// `ATTACH`、`PRAGMA` 都会漏过去——漏一种写法就是一个无需任何权限即可读穿
/// `plugin_secrets`（明文宿主托管密钥）与全部配对记录的洞。本守卫把边界放到引擎：
/// 语句真正触碰某张表的那一步才仲裁，写法再怎么变都要经过它。
///
/// 正则校验保留，但**不是边界**——它只提供更早、更可读的失败文案。
/// 拒绝走 `Deny`：`prepare` 直接失败（`not authorized`），错误原样回给插件，
/// 不静默改写、不返回空结果。
///
/// 作用域仅覆盖这一次调用：主库是内核与全部插件共用的单一连接（全局 Mutex 串行），
/// 守卫 Drop 即卸载，绝不残留到内核自己的读写上（与 `ProgressHandlerGuard` 同形态）。
fn with_main_db_guards<T>(
    plugin_id: &str,
    conn: &rusqlite::Connection,
    timeout: Duration,
    sql: &str,
    f: impl FnOnce(&rusqlite::Connection) -> Result<T, String>,
) -> Result<T, String> {
    let prefix = main_db_table_prefix(plugin_id);
    let catalog_named = names_schema_catalog(sql);
    conn.authorizer(Some(move |ctx: AuthContext<'_>| {
        authorize_main_db_action(&prefix, catalog_named, &ctx.action)
    }));
    let _guard = AuthorizerGuard { conn };
    with_statement_timeout(plugin_id, conn, timeout, f)
}

/// 表名白名单守卫的卸载器（含 panic 展开路径）
struct AuthorizerGuard<'a> {
    conn: &'a rusqlite::Connection,
}

impl Drop for AuthorizerGuard<'_> {
    fn drop(&mut self) {
        self.conn.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    }
}

/// 单个动作的仲裁：带表名的动作按前缀放行，自省/挂载类一律拒
///
/// 放行的 `Select` / `Transaction` / `Savepoint` / `Function` / `Recursive` / `Reindex`
/// 都不携带跨表访问（真正的读写由同时上报的 `Read`/`Insert`/`Update`/`Delete` 把关）：
/// - `Function`：`ALTER TABLE` 内部会用到 `printf` / `substr` 等内置函数
/// - `Reindex`：`CREATE INDEX` 必然附带一条隐式 Reindex，拒它等于禁建索引
/// - `Transaction`：批次事务由 rusqlite 自己发 BEGIN/COMMIT（插件侧裸事务控制
///   已在 `reject_bare_transaction_control` 拦掉）
///
/// `DropTrigger` 刻意不放行：rusqlite 0.32 该变体只上报触发器名，判不出归属表，
/// 按前缀门会误伤他插件同名对象 → fail-closed 拒绝（触发器创建本就被
/// `reject_bare_transaction_control` 的末 token 启发式挡住，见票 02 Comments）。
fn authorize_main_db_action(prefix: &str, catalog_named: bool, action: &AuthAction<'_>) -> Authorization {
    let name: &str = match action {
        AuthAction::Read { table_name, .. }
        | AuthAction::Update { table_name, .. }
        | AuthAction::Insert { table_name }
        | AuthAction::Delete { table_name }
        | AuthAction::AlterTable { table_name, .. }
        | AuthAction::CreateTable { table_name }
        | AuthAction::CreateTempTable { table_name }
        | AuthAction::CreateVtable { table_name, .. }
        | AuthAction::DropTable { table_name }
        | AuthAction::DropTempTable { table_name }
        | AuthAction::DropVtable { table_name, .. }
        | AuthAction::Analyze { table_name }
        | AuthAction::CreateIndex { table_name, .. }
        | AuthAction::CreateTempIndex { table_name, .. }
        | AuthAction::DropIndex { table_name, .. }
        | AuthAction::DropTempIndex { table_name, .. }
        | AuthAction::CreateTrigger { table_name, .. }
        | AuthAction::CreateTempTrigger { table_name, .. } => table_name,
        // 视图名与表名同处一个名字空间，同样能指到他表 → 按同一前缀门把关
        AuthAction::CreateView { view_name } | AuthAction::DropView { view_name } => view_name,
        AuthAction::Select
        | AuthAction::Transaction { .. }
        | AuthAction::Savepoint { .. }
        | AuthAction::Function { .. }
        | AuthAction::Recursive
        | AuthAction::Reindex { .. } => return Authorization::Allow,
        // PRAGMA（`database_list` 直接把宿主库文件路径交给插件）、ATTACH/DETACH
        // （挂载任意库文件 = 跨库读写）以及未识别的动作码：fail-closed
        _ => return Authorization::Deny,
    };
    if is_schema_catalog(name) {
        // DDL 记账必然读写 sqlite_master / sqlite_sequence；但语句自己点名这些表
        // （读表清单、`UPDATE sqlite_master` 改 rootpage 等）就是越界 → 拒
        return if catalog_named {
            Authorization::Deny
        } else {
            Authorization::Allow
        };
    }
    if name.starts_with(prefix) {
        Authorization::Allow
    } else {
        Authorization::Deny
    }
}

/// 插件在主库的表名前缀（`plugin_{sanitized_id}_`）
fn main_db_table_prefix(plugin_id: &str) -> String {
    let sanitized_id = plugin_id.replace(['.', '-'], "_");
    format!("plugin_{}_", sanitized_id)
}

/// SQLite 内部目录表：DDL 记账必然触达，插件不得在语句里直接点名
///
/// 白名单而非 `sqlite_` 前缀通配：`sqlite_stat1` 之类可被 ANALYZE 写入的内部表
/// 不给放行（fail-closed）。
fn is_schema_catalog(name: &str) -> bool {
    name.eq_ignore_ascii_case("sqlite_master")
        || name.eq_ignore_ascii_case("sqlite_temp_master")
        || name.eq_ignore_ascii_case("sqlite_sequence")
}

/// 语句是否点名了目录表（去注释与字符串字面量后按词元匹配）
///
/// 双引号/反引号/方括号包起来的是**标识符**，保留在扫描范围内——那正是
/// `"sqlite_master"` 这种规避写法的入口；只有单引号字符串与注释可以吞掉。
fn names_schema_catalog(sql: &str) -> bool {
    strip_sql_literals_and_comments(sql)
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(is_schema_catalog)
}

/// 剥掉 `--` 行注释、`/* */` 块注释与单引号字符串字面量
fn strip_sql_literals_and_comments(sql: &str) -> String {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            i += 1;
            while i < chars.len() {
                if chars[i] == '\'' {
                    if chars.get(i + 1) == Some(&'\'') {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push(' ');
            continue;
        }
        if c == '-' && chars.get(i + 1) == Some(&'-') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
            out.push(' ');
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// 主库执行 SQL（权限 + 表名前缀校验 + 超时护栏），返回受影响行数
pub fn db_execute(ports: &dyn SqlitePorts, plugin_id: &str, sql: &str) -> Result<u32, String> {
    if !ports.check_permission(plugin_id, PERMISSION_DATABASE_MAIN, "host_db_execute") {
        return Err("permission denied".to_string());
    }
    reject_bare_transaction_control(sql)?;
    validate_sql_table_prefix(plugin_id, sql).map_err(|e| e.to_string())?;
    let db = ports.main_db();
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    block_on(ports, async {
        let db = db.lock().await;
        with_main_db_guards(plugin_id, db.conn(), timeout, sql, |conn| {
            conn.execute(sql, []).map_err(|e| e.to_string())
        })
    })
    .map(|affected| affected as u32)
    .map_err(|e| format!("database error: {}", e))
}

/// 主库查询（权限 + 表名前缀校验 + 超时护栏），返回行数组 JSON 字符串
pub fn db_query(ports: &dyn SqlitePorts, plugin_id: &str, sql: &str) -> Result<Option<String>, String> {
    if !ports.check_permission(plugin_id, PERMISSION_DATABASE_MAIN, "host_db_query") {
        return Err("permission denied".to_string());
    }
    validate_sql_table_prefix(plugin_id, sql).map_err(|e| e.to_string())?;
    let db = ports.main_db();
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    let value = block_on(ports, async {
        let db = db.lock().await;
        with_main_db_guards(plugin_id, db.conn(), timeout, sql, |conn| {
            query_to_json(plugin_id, conn, sql)
        })
    })
    .map_err(|e| format!("database error: {}", e))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {}", e))
}

/// 插件独立库执行 SQL（权限校验 + 超时护栏，无表名前缀校验）
pub fn plugin_db_execute(ports: &dyn SqlitePorts, plugin_id: &str, sql: &str) -> Result<u32, String> {
    if !ports.check_permission(plugin_id, PERMISSION_STORAGE, "host_plugin_db_execute") {
        return Err("permission denied".to_string());
    }
    reject_bare_transaction_control(sql)?;
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    block_on(ports, async {
        let db_arc = ports
            .plugin_db(plugin_id.to_string())
            .await
            .map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| {
            conn.execute(sql, []).map(|n| n as u32).map_err(|e| e.to_string())
        })
    })
    .map_err(|e| format!("database error: {}", e))
}

/// 插件独立库查询（权限校验 + 超时护栏，无表名前缀校验）
pub fn plugin_db_query(ports: &dyn SqlitePorts, plugin_id: &str, sql: &str) -> Result<Option<String>, String> {
    if !ports.check_permission(plugin_id, PERMISSION_STORAGE, "host_plugin_db_query") {
        return Err("permission denied".to_string());
    }
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    let value = block_on(ports, async {
        let db_arc = ports
            .plugin_db(plugin_id.to_string())
            .await
            .map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| {
            query_to_json(plugin_id, conn, sql)
        })
    })
    .map_err(|e| format!("database error: {}", e))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {}", e))
}

/// 主库执行参数绑定 SQL（权限 + 表名前缀校验 + 超时护栏）
pub fn db_execute_params(
    ports: &dyn SqlitePorts,
    plugin_id: &str,
    sql: &str,
    params_json: &str,
) -> Result<u32, String> {
    if !ports.check_permission(plugin_id, PERMISSION_DATABASE_MAIN, "host_db_execute_params") {
        return Err("permission denied".to_string());
    }
    reject_bare_transaction_control(sql)?;
    validate_sql_table_prefix(plugin_id, sql).map_err(|e| e.to_string())?;
    let params = parse_params_json(params_json)?;
    let db = ports.main_db();
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    block_on(ports, async {
        let db = db.lock().await;
        with_main_db_guards(plugin_id, db.conn(), timeout, sql, |conn| {
            execute_with_params(conn, sql, &params)
        })
    })
    .map(|affected| affected as u32)
    .map_err(|e| format!("database error: {}", e))
}

/// 主库参数绑定查询（权限 + 表名前缀校验 + 超时护栏）
pub fn db_query_params(
    ports: &dyn SqlitePorts,
    plugin_id: &str,
    sql: &str,
    params_json: &str,
) -> Result<Option<String>, String> {
    if !ports.check_permission(plugin_id, PERMISSION_DATABASE_MAIN, "host_db_query_params") {
        return Err("permission denied".to_string());
    }
    validate_sql_table_prefix(plugin_id, sql).map_err(|e| e.to_string())?;
    let params = parse_params_json(params_json)?;
    let db = ports.main_db();
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    let value = block_on(ports, async {
        let db = db.lock().await;
        with_main_db_guards(plugin_id, db.conn(), timeout, sql, |conn| {
            query_with_params_to_json(plugin_id, conn, sql, &params)
        })
    })
    .map_err(|e| format!("database error: {}", e))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {}", e))
}

/// 插件独立库执行参数绑定 SQL（权限校验 + 超时护栏，无表名前缀校验）
pub fn plugin_db_execute_params(
    ports: &dyn SqlitePorts,
    plugin_id: &str,
    sql: &str,
    params_json: &str,
) -> Result<u32, String> {
    if !ports.check_permission(plugin_id, PERMISSION_STORAGE, "host_plugin_db_execute_params") {
        return Err("permission denied".to_string());
    }
    reject_bare_transaction_control(sql)?;
    let params = parse_params_json(params_json)?;
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    block_on(ports, async {
        let db_arc = ports
            .plugin_db(plugin_id.to_string())
            .await
            .map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| {
            execute_with_params(conn, sql, &params).map(|n| n as u32)
        })
    })
    .map_err(|e| format!("database error: {}", e))
}

/// 插件独立库参数绑定查询（权限校验 + 超时护栏，无表名前缀校验）
pub fn plugin_db_query_params(
    ports: &dyn SqlitePorts,
    plugin_id: &str,
    sql: &str,
    params_json: &str,
) -> Result<Option<String>, String> {
    if !ports.check_permission(plugin_id, PERMISSION_STORAGE, "host_plugin_db_query_params") {
        return Err("permission denied".to_string());
    }
    let params = parse_params_json(params_json)?;
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    let value = block_on(ports, async {
        let db_arc = ports
            .plugin_db(plugin_id.to_string())
            .await
            .map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| {
            query_with_params_to_json(plugin_id, conn, sql, &params)
        })
    })
    .map_err(|e| format!("database error: {}", e))?;
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("database error: JSON serialization failed: {}", e))
}

// ==================== 事务批次执行（票据 06）：execute-batch ====================

/// 解析 execute-batch 的 sqls-json（SQL 字符串数组）
fn parse_sqls_json(sqls_json: &str) -> Result<Vec<String>, String> {
    serde_json::from_str::<Vec<String>>(sqls_json).map_err(|e| format!("invalid sqls JSON array: {}", e))
}

/// 事务控制语句白名单检测：拒绝以裸事务控制语句开头/结尾的 execute（票据 06）
///
/// 跨调用裸事务（BEGIN → 多次 execute → COMMIT）期间，连接 Mutex 按语句释放，
/// 内核自己的写入会插进插件事务窗口，插件 ROLLBACK 会一并丢弃内核写入——
/// execute-batch 是唯一受支持的事务入口（单次调用内持有事务）。
///
/// 尽力而为的检测：去除首尾空白与前导注释后取首/末 token 匹配白名单，
/// 覆盖 BEGIN/COMMIT/END/ROLLBACK/SAVEPOINT/RELEASE 常见形态，不做完整 SQL 解析。
pub(crate) fn reject_bare_transaction_control(sql: &str) -> Result<(), String> {
    let mut s = sql.trim();
    // 剥离前导注释（-- 行注释 / /* 块注释），最多剥 8 层防病态输入
    for _ in 0..8 {
        if let Some(rest) = s.strip_prefix("--") {
            s = rest.split_once('\n').map(|(_, after)| after).unwrap_or("").trim_start();
        } else if let Some(rest) = s.strip_prefix("/*") {
            s = rest.split_once("*/").map(|(_, after)| after).unwrap_or("").trim_start();
        } else {
            break;
        }
    }
    let first = s.split_whitespace().next().map(normalize_sql_token);
    let last = s.split_whitespace().next_back().map(normalize_sql_token);
    let is_bare_transaction = matches!(
        first.as_deref(),
        Some("begin" | "commit" | "end" | "rollback" | "savepoint" | "release")
    ) || matches!(last.as_deref(), Some("commit" | "rollback" | "end" | "release"));
    if is_bare_transaction {
        return Err(
            "database error: bare transaction control statements are rejected at execute level \
             (use execute-batch for multi-statement transactions)"
                .to_string(),
        );
    }
    Ok(())
}

/// 归一化 SQL token：去尾分号 + 小写（用于事务控制白名单匹配）
fn normalize_sql_token(t: &str) -> String {
    t.trim_end_matches(';').to_lowercase()
}

/// 事务内顺序执行多语句：任一句失败整体回滚，返回受影响行数合计（票据 06）
///
/// 经 `unchecked_transaction`（rusqlite 0.32 安全 API，运行时检查嵌套，调用方持有
/// 数据库全局锁独占连接，无并发风险）在 `&Connection` 上开启事务；事务对象 Drop 时
/// 未提交自动回滚。超时护栏由外层 `with_statement_timeout` 覆盖整个批次——
/// progress handler 挂在连接上，事务内所有语句的 VM 步受同一超时约束（长事务不会
/// 无限期阻塞内核 DB）。
fn execute_batch_on_conn(conn: &rusqlite::Connection, sqls: &[String]) -> Result<u32, String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("begin transaction: {}", e))?;
    let mut total_affected: u32 = 0;
    for sql in sqls {
        let affected = tx
            .execute(sql.as_str(), [])
            .map_err(|e| format!("execute '{}': {}", sql, e))?;
        total_affected = total_affected
            .checked_add(affected as u32)
            .ok_or_else(|| "database error: execute-batch affected row count overflow".to_string())?;
    }
    tx.commit().map_err(|e| format!("commit transaction: {}", e))?;
    Ok(total_affected)
}

/// 主库事务批次执行（权限 + 表名前缀 + 语句数上限 + 超时护栏）
pub fn db_execute_batch(ports: &dyn SqlitePorts, plugin_id: &str, sqls_json: &str) -> Result<u32, String> {
    if !ports.check_permission(plugin_id, PERMISSION_DATABASE_MAIN, "host_db_execute_batch") {
        return Err("permission denied".to_string());
    }
    let sqls = parse_sqls_json(sqls_json)?;
    if sqls.len() > PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS {
        return Err(format!(
            "database error: execute-batch exceeds {} statements limit",
            PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS
        ));
    }
    for sql in &sqls {
        validate_sql_table_prefix(plugin_id, sql).map_err(|e| e.to_string())?;
    }
    let db = ports.main_db();
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    block_on(ports, async {
        let db = db.lock().await;
        with_main_db_guards(plugin_id, db.conn(), timeout, &sqls.join(";"), |conn| {
            execute_batch_on_conn(conn, &sqls)
        })
    })
    .map_err(|e| format!("database error: {}", e))
}

/// 插件独立库事务批次执行（权限 + 语句数上限 + 超时护栏，无表名前缀校验）
pub fn plugin_db_execute_batch(ports: &dyn SqlitePorts, plugin_id: &str, sqls_json: &str) -> Result<u32, String> {
    if !ports.check_permission(plugin_id, PERMISSION_STORAGE, "host_plugin_db_execute_batch") {
        return Err("permission denied".to_string());
    }
    let sqls = parse_sqls_json(sqls_json)?;
    if sqls.len() > PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS {
        return Err(format!(
            "database error: execute-batch exceeds {} statements limit",
            PLUGIN_DB_EXECUTE_BATCH_MAX_STATEMENTS
        ));
    }
    let timeout = Duration::from_secs(PLUGIN_DB_STATEMENT_TIMEOUT_SECS);
    block_on(ports, async {
        let db_arc = ports
            .plugin_db(plugin_id.to_string())
            .await
            .map_err(|e| e.to_string())?;
        let db = db_arc.lock().await;
        with_statement_timeout(plugin_id, db.conn(), timeout, |conn| execute_batch_on_conn(conn, &sqls))
    })
    .map_err(|e| format!("database error: {}", e))
}

// ==================== 参数绑定辅助 ====================

/// 将 JSON 参数绑定到预编译语句（1-based 索引，rusqlite 真绑定，防注入）
fn bind_json_params(stmt: &mut rusqlite::Statement<'_>, params: &[serde_json::Value]) -> rusqlite::Result<()> {
    for (i, p) in params.iter().enumerate() {
        let idx = i + 1;
        match p {
            serde_json::Value::Null => stmt.raw_bind_parameter(idx, rusqlite::types::Null)?,
            serde_json::Value::Bool(b) => stmt.raw_bind_parameter(idx, *b)?,
            serde_json::Value::Number(n) => {
                // 整数优先；非整数按浮点绑定
                if let Some(iv) = n.as_i64() {
                    stmt.raw_bind_parameter(idx, iv)?;
                } else {
                    stmt.raw_bind_parameter(idx, n.as_f64().unwrap_or(0.0))?;
                }
            }
            serde_json::Value::String(s) => stmt.raw_bind_parameter(idx, s.as_str())?,
            // 数组/对象 fallback：序列化为 JSON 字符串存储
            other => stmt.raw_bind_parameter(idx, serde_json::to_string(other).unwrap_or_default())?,
        }
    }
    Ok(())
}

/// 执行参数绑定 SQL，返回受影响行数
fn execute_with_params(conn: &rusqlite::Connection, sql: &str, params: &[serde_json::Value]) -> Result<usize, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
    bind_json_params(&mut stmt, params).map_err(|e| format!("bind: {}", e))?;
    stmt.raw_execute().map_err(|e| format!("execute: {}", e))
}

/// 结果集护栏：行数 / 序列化字节双上限逐行检查（票据 05）
///
/// 上限常量不可由插件调整（安全边界）；触发记结构化 warn!（plugin_id 字段），
/// 错误消息指明上限并引导插件加 LIMIT 或分批。行数上限先于字节测量判断，
/// 避免对超出行数的额外行做无谓序列化。
fn push_row_capped(
    plugin_id: &str,
    rows_out: &mut Vec<serde_json::Value>,
    total_bytes: &mut usize,
    row: serde_json::Value,
) -> Result<(), String> {
    if rows_out.len() >= PLUGIN_DB_QUERY_MAX_ROWS {
        tracing::warn!(
            plugin_id = %plugin_id,
            limit = PLUGIN_DB_QUERY_MAX_ROWS,
            "plugin query result exceeded row limit"
        );
        return Err(format!(
            "database error: query result exceeds {} rows limit (add LIMIT or batch)",
            PLUGIN_DB_QUERY_MAX_ROWS
        ));
    }
    *total_bytes += serde_json::to_vec(&row)
        .map_err(|e| format!("serialize row to measure size: {}", e))?
        .len();
    if *total_bytes > PLUGIN_DB_QUERY_MAX_BYTES {
        tracing::warn!(
            plugin_id = %plugin_id,
            limit = PLUGIN_DB_QUERY_MAX_BYTES,
            "plugin query result exceeded byte limit"
        );
        return Err(format!(
            "database error: query result exceeds {} bytes limit (add LIMIT or batch)",
            PLUGIN_DB_QUERY_MAX_BYTES
        ));
    }
    rows_out.push(row);
    Ok(())
}

/// 参数绑定查询 → JSON 行数组（含结果集护栏：行数/字节上限）
fn query_with_params_to_json(
    plugin_id: &str,
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[serde_json::Value],
) -> Result<serde_json::Value, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
    bind_json_params(&mut stmt, params).map_err(|e| format!("bind: {}", e))?;

    let column_count = stmt.column_count();
    let column_names: Vec<String> = (0..column_count)
        .map(|i| {
            stmt.column_name(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| format!("col{}", i))
        })
        .collect();

    let mut rows_out: Vec<serde_json::Value> = Vec::new();
    let mut total_bytes: usize = 0;
    let mut rows = stmt.raw_query();
    while let Some(row) = rows.next().map_err(|e| format!("next: {}", e))? {
        let mut map = serde_json::Map::new();
        for (i, col_name) in column_names.iter().enumerate() {
            map.insert(col_name.clone(), column_to_json(row, i));
        }
        push_row_capped(
            plugin_id,
            &mut rows_out,
            &mut total_bytes,
            serde_json::Value::Object(map),
        )?;
    }

    Ok(serde_json::Value::Array(rows_out))
}

/// 执行查询并将结果集转换为 JSON 行数组（含结果集护栏：行数/字节上限）
///
/// 主库与插件库查询共用，消除原先两份重复的列名提取 + query_map 逻辑；
/// 逐行计数而非全量物化，超限立即截断报错（避免无上限结果集拷入 guest 内存
/// 耗尽单次调用 fuel 预算触发 trap 污染 Store）。
pub(crate) fn query_to_json(
    plugin_id: &str,
    conn: &rusqlite::Connection,
    sql: &str,
) -> Result<serde_json::Value, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;

    let column_count = stmt.column_count();
    let column_names: Vec<String> = (0..column_count)
        .map(|i| {
            stmt.column_name(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| format!("col{}", i))
        })
        .collect();

    let mut rows_out: Vec<serde_json::Value> = Vec::new();
    let mut total_bytes: usize = 0;
    let rows = stmt
        .query_map([], |row| {
            let mut map = serde_json::Map::new();
            for (i, col_name) in column_names.iter().enumerate() {
                let value = column_to_json(row, i);
                map.insert(col_name.clone(), value);
            }
            Ok(map)
        })
        .map_err(|e| format!("query_map: {}", e))?;
    for row in rows {
        let map = row.map_err(|e| format!("row: {}", e))?;
        push_row_capped(
            plugin_id,
            &mut rows_out,
            &mut total_bytes,
            serde_json::Value::Object(map),
        )?;
    }

    Ok(serde_json::Value::Array(rows_out))
}

// ==================== SQL Table Name Validation ====================

/// 验证 SQL 语句中的表名是否以插件专属前缀开头
///
/// WASM 插件只能操作 `plugin_{sanitized_id}_` 前缀的表，
/// 防止插件读写宿主或其他插件的数据表
///
/// **本函数不是安全边界**（票 02）：八个正则模式是尽力而为匹配，逗号多表、引号标识符、
/// `main.` 限定、ATTACH、PRAGMA 都可能漏。真正的边界是 `with_main_db_guards` 里
/// 由 SQLite 引擎逐动作回调的表名仲裁。这里保留的价值是**早失败 + 可读文案**
/// （直接报出违规表名，而不是引擎的 `not authorized`）。
pub(crate) fn validate_sql_table_prefix(plugin_id: &str, sql: &str) -> crate::Result<()> {
    let expected_prefix = main_db_table_prefix(plugin_id);

    let table_names = extract_table_names(sql);

    for table in table_names {
        if !table.starts_with(&expected_prefix) {
            return Err(crate::AppError::Plugin(format!(
                "SQL table name '{}' does not match required prefix '{}' for plugin '{}'",
                table, expected_prefix, plugin_id
            )));
        }
    }

    Ok(())
}

/// 从 SQL 语句中提取表名
///
/// 使用正则匹配常见 SQL 关键字后的表名标识符
pub(crate) fn extract_table_names(sql: &str) -> Vec<String> {
    let mut tables = Vec::new();

    let patterns = [
        r#"(?i)\bCREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bINSERT\s+INTO\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bUPDATE\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bDELETE\s+FROM\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bFROM\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bJOIN\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bALTER\s+TABLE\s+[`"\[]?(\w+)[`"\]]?"#,
        r#"(?i)\bDROP\s+TABLE\s+(?:IF\s+EXISTS\s+)?[`"\[]?(\w+)[`"\]]?"#,
        // ALTER TABLE ... RENAME TO 只改目标名，引擎侧的 AlterTable 动作只上报原表名，
        // 漏掉这条就等于「把自己的表改到他前缀下」可自由命名 → 文本层补上
        r#"(?i)\bRENAME\s+TO\s+[`"\[]?(\w+)[`"\]]?"#,
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            for cap in re.captures_iter(sql) {
                if let Some(m) = cap.get(1) {
                    let name = m.as_str().to_string();
                    if !tables.contains(&name) {
                        tables.push(name);
                    }
                }
            }
        }
    }

    tables
}

// ==================== Database Column Conversion ====================

/// 将 rusqlite 行的指定列转换为 serde_json::Value
///
/// 按类型优先级尝试读取：i64 -> f64 -> String -> bool -> blob -> Null
/// rusqlite 的 FromSql 支持 i64/f64/String/bool 等，但不支持 serde_json::Value
fn column_to_json(row: &rusqlite::Row<'_>, col_index: usize) -> serde_json::Value {
    // 先尝试整数
    if let Ok(v) = row.get::<_, i64>(col_index) {
        // 区分整数和浮点数：如果该列实际是 REAL 类型，i64 读取可能截断
        if let Ok(fv) = row.get::<_, f64>(col_index) {
            if (fv as i64) as f64 != fv {
                return serde_json::Value::Number(
                    serde_json::Number::from_f64(fv).unwrap_or(serde_json::Number::from(0)),
                );
            }
        }
        return serde_json::Value::Number(serde_json::Number::from(v));
    }
    // 尝试浮点数
    if let Ok(v) = row.get::<_, f64>(col_index) {
        return serde_json::Value::Number(serde_json::Number::from_f64(v).unwrap_or(serde_json::Number::from(0)));
    }
    // 尝试字符串
    if let Ok(v) = row.get::<_, String>(col_index) {
        return serde_json::Value::String(v);
    }
    // 尝试布尔值
    if let Ok(v) = row.get::<_, bool>(col_index) {
        return serde_json::Value::Bool(v);
    }
    // 尝试 blob（Vec<u8>）— 转为 hex 字符串
    if let Ok(v) = row.get::<_, Vec<u8>>(col_index) {
        use std::fmt::Write;
        let mut hex = String::with_capacity(v.len() * 2);
        for byte in &v {
            write!(hex, "{:02x}", byte).unwrap();
        }
        return serde_json::Value::String(hex);
    }
    // NULL 或无法识别的类型
    serde_json::Value::Null
}

#[cfg(test)]
mod tests;
