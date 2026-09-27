//! opencode SQLite 数据源（票据 07）
//!
//! # 为什么走宿主 `sqlite3` CLI 而不是库
//!
//! 本插件是 WASM 组件，`host-fs` 只提供「读整个文件为字符串」，而
//! `~/.local/share/opencode/opencode.db` 实机 **445MB / 22 张表**——整读进
//! WATM 边界必然爆掉，且 SQLite 是 B-tree 二进制格式，guest 侧无解析器
//! （WASM 目标无法链 rusqlite）。新增 WIT 原语要 ABI bump + 双端同步，
//! 超出本票边界（spec §2 非目标「不动 WIT / ABI」）。
//!
//! 故复用本插件**既有**的 `host-process` 通道（安装域已在跑 `npm install -g`），
//! 以同步 `sqlite3 -json -readonly` 查表。语义与 git 域（terminal-session
//! `file_browse::GitPort`）同款：`process_run_sync` 同步拿 stdout。
//!
//! # 外部依赖与降级
//!
//! 需要宿主 PATH 上有 `sqlite3`（SQLite ≥ 3.38，JSON1 恒开）。缺失 / 版本过旧
//! / 库文件不存在时返回结构化 [`SyncError`]，由 `usage.rs` 写进适配器
//! `error` 字段并由前端走 i18n 呈现——**不静默吞错**（见 spec §8 fail-visible）。
//! `~/.local/share/opencode/` 已在 activate 的批量授权清单内，与另两家同闸门。
//!
//! # 边界（445MB 库不被整读进内存）
//!
//! - 聚合层按 **session 主键 keyset 分页**（`WHERE id > ? ORDER BY id LIMIT n`），
//!   每页至多 [`SESSION_PAGE_ROWS`] 行，全局至多 [`MAX_SESSIONS`] 行；
//! - 事件流联表按 `(message.id, part.id)` keyset 分页，**并按 part 逐类截断**：
//!   原始 `part.data` 单条最大 151KB（工具输出逐字回传），`state.output` 只取
//!   前 [`PART_OUTPUT_CAP`] 字符，其余字段（type/tool/status）体积极小。
//!   截断在 **SQL 侧**用 `substr()` 完成，guest 永远收不到大 blob；
//! - 损坏 blob 用 `json_valid()` 守卫（`json_extract` 遇非法 JSON 会中止整条
//!   查询，不能裸调）。

use crate::usage_parse::{
    parse_opencode_events, parse_opencode_session_row, NormalizedEvent, ParsedSession,
};
use bedcode_plugin_api::host::{HostFs, HostLog, HostProcess};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::Value;

/// 聚合层每页行数（本机 54 行会话 = 1 页即完；万级会话也只跑 20 次）
const SESSION_PAGE_ROWS: usize = 500;
/// 聚合层单次扫描的会话数上限（防御异常大的库把扫描拖成长任务）
pub(crate) const MAX_SESSIONS: usize = 20_000;
/// 事件流每页行数（每行是「一条 part 展平后」的窄行）
const EVENT_PAGE_ROWS: usize = 200;
/// 事件流单会话行数上限（超出截断并标 `truncated`）
pub(crate) const MAX_EVENT_ROWS: usize = 4_000;
/// `part.state.output` 跨边界携带的字符上限（工具输出可达 150KB）
const PART_OUTPUT_CAP: usize = 600;
/// 单次 `sqlite3` 调用的超时（本地只读查询，正常 < 200ms）
const QUERY_TIMEOUT_MS: u64 = 30_000;

/// opencode 库相对家目录的落点（母 spec §9：`~/.local/share/opencode/opencode.db`）
pub(crate) const DB_REL: &str = ".local/share/opencode/opencode.db";

/// 库文件绝对路径（home 由 `usage.rs` 注入，本模块不读全局）
pub(crate) fn db_path(home: &str) -> String {
    format!("{home}/{DB_REL}")
}

/// 同步查询失败原因（前端按 `code` 走 i18n，`detail` 只进日志不进界面）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SyncError {
    /// 宿主 PATH 上没有 `sqlite3`（或无法执行）
    SqliteMissing,
    /// 库文件不存在（opencode 未安装 / 从未跑过）
    DbMissing,
    /// 查询执行失败（含 JSON1 缺失导致的 `json_extract` 报错）
    QueryFailed(String),
}

impl SyncError {
    /// 机器可读 code（前端 i18n 查表键）
    pub(crate) fn code(&self) -> &'static str {
        match self {
            SyncError::SqliteMissing => "sqlite3-missing",
            SyncError::DbMissing => "db-missing",
            SyncError::QueryFailed(_) => "query-failed",
        }
    }
}

/// 变化指纹：`{db 字节数}:{wal 字节数}`。
///
/// 母 spec 写的是「db 文件 mtime 变更触发增量重扫」，但 **`host-fs.stat` 只给
/// `{size, isFile, isDir}`，WIT 无 mtime 原语**（这是 usage.rs 水位注释里
/// 「WIT 无 stat 原语」的同一事实——stat 是 v19 后补的，但仍无 mtime）。
/// 于是取**文件字节数**做指纹：opencode 跑 WAL 模式，主库大小由 checkpoint
/// 节奏决定，故必须把 `-wal` 边车一起纳入，否则增量扫描会漏掉「只写 WAL
/// 未 checkpoint」的新会话。
pub(crate) fn signature(h: &WasmHost, db: &str) -> Option<String> {
    let st = h.fs_stat(db).ok().flatten()?;
    if !st.is_file {
        return None;
    }
    let wal = h
        .fs_stat(&format!("{db}-wal"))
        .ok()
        .flatten()
        .filter(|w| w.is_file)
        .map(|w| w.size)
        .unwrap_or(0);
    Some(format!("{}:{wal}", st.size))
}

/// SQL 字符串字面量转义（单引号加倍）。
///
/// 用处：`WHERE id > '<游标>'` 与 `WHERE session_id = '<id>'`。两个值都来自
/// **本插件自己的库**（不是用户输入），但仍走统一转义——不依赖「上游可信」
/// 这一隐含前提。
pub(crate) fn sql_text_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

// ==================== SQL ====================

/// 聚合层分页 SQL（keyset 分页 + 显式列清单）。
///
/// 列清单**不写 `SELECT *`**：`part`/`message` 之外的表在 opencode 升级后可能
/// 加列，全量取会白搬数据且可能撞上 SQLite 变长列的读取成本。
/// `model` 原样取回（是 JSON **串**，由 guest 侧解析——`json_extract` 遇非 JSON
/// 串会中止整条查询，见模块头「损坏 blob 守卫」）。
fn session_page_sql(after_id: &str) -> String {
    format!(
        "SELECT id, directory, title, cost, tokens_input, tokens_output, \
                tokens_reasoning, tokens_cache_read, tokens_cache_write, \
                model, time_created, time_updated \
         FROM session WHERE id > {} ORDER BY id LIMIT {}",
        sql_text_literal(after_id),
        SESSION_PAGE_ROWS
    )
}

/// 事件流分页 SQL：message × part 联表，`message.data` / `part.data` 在 SQL
/// 侧用 `json_extract` 展平成窄列；`state.output` 按 [`PART_OUTPUT_CAP`] 截断。
///
/// `json_valid()` 守卫：opencode 若写出损坏 blob，裸 `json_extract` 会让整条
/// 查询报错（连带已取到的行一起丢），守卫后该 part 退化为全 NULL 列被跳过。
fn event_page_sql(session_id: &str, after_mid: &str, after_pid: &str) -> String {
    format!(
        "SELECT m.id AS mid, m.time_created AS mts, \
                CASE WHEN json_valid(m.data) THEN json_extract(m.data,'$.role') END AS role, \
                CASE WHEN json_valid(m.data) THEN json_extract(m.data,'$.modelID') END AS model, \
                CASE WHEN json_valid(m.data) THEN json_extract(m.data,'$.tokens.input') END AS t_in, \
                CASE WHEN json_valid(m.data) THEN json_extract(m.data,'$.tokens.output') END AS t_out, \
                CASE WHEN json_valid(m.data) THEN json_extract(m.data,'$.tokens.reasoning') END AS t_reason, \
                CASE WHEN json_valid(m.data) THEN json_extract(m.data,'$.tokens.cache.read') END AS t_cache_read, \
                CASE WHEN json_valid(m.data) THEN json_extract(m.data,'$.tokens.cache.write') END AS t_cache_write, \
                CASE WHEN json_valid(p.data) THEN json_extract(p.data,'$.type') END AS ptype, \
                CASE WHEN json_valid(p.data) THEN json_extract(p.data,'$.text') END AS ptext, \
                CASE WHEN json_valid(p.data) THEN json_extract(p.data,'$.tool') END AS ptool, \
                CASE WHEN json_valid(p.data) THEN json_extract(p.data,'$.state.status') END AS pstatus, \
                CASE WHEN json_valid(p.data) THEN substr(json_extract(p.data,'$.state.output'),1,{PART_OUTPUT_CAP}) END AS poutput, \
                COALESCE(p.id,'') AS pid \
         FROM message m LEFT JOIN part p ON p.message_id = m.id \
         WHERE m.session_id = {sid} \
           AND (m.id > {mid} OR (m.id = {mid} AND COALESCE(p.id,'') > {pid})) \
         ORDER BY m.id, p.id LIMIT {EVENT_PAGE_ROWS}",
        sid = sql_text_literal(session_id),
        mid = sql_text_literal(after_mid),
        pid = sql_text_literal(after_pid),
    )
}

// ==================== 同步执行 ====================

/// 跑一条只读查询，返回 `sqlite3 -json` 解析出的行数组。
///
/// 错误判别顺序：**先看 db 是否存在**（`fs_stat`）再看进程结果——库不存在时
/// `sqlite3` 的 stderr 是 `unable to open database file`，与「SQL 写错」同形，
/// 直接归 `QueryFailed` 会让前端把「没装 opencode」说成「查询失败」。
fn query(h: &WasmHost, db: &str, sql: &str) -> Result<Vec<Value>, SyncError> {
    let exists = h
        .fs_stat(db)
        .ok()
        .flatten()
        .map(|s| s.is_file)
        .unwrap_or(false);
    if !exists {
        return Err(SyncError::DbMissing);
    }
    let request = serde_json::json!({
        "command": "sqlite3",
        "args": ["-json", "-readonly", db, sql],
        "timeout_ms": QUERY_TIMEOUT_MS,
    });
    let r = HostProcess::process_run_sync(h, &request.to_string())
        .map_err(|e| SyncError::QueryFailed(e.message))?;
    if r.timed_out {
        return Err(SyncError::QueryFailed(
            "opencode sqlite query timed out".into(),
        ));
    }
    if r.exit_code != Some(0) {
        let stderr = r.stderr.trim().to_string();
        // 命令不存在：sh 会以 127 退出且 stderr 含 not found
        if r.exit_code == Some(127) || stderr.contains("not found") {
            return Err(SyncError::SqliteMissing);
        }
        return Err(SyncError::QueryFailed(stderr));
    }
    // sqlite3 -json 对零行输出空串（不是 `[]`）
    let stdout = r.stdout.trim();
    if stdout.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Value>(stdout)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .ok_or_else(|| SyncError::QueryFailed("opencode sqlite returned non-array JSON".into()))
}

// ==================== 聚合层 ====================

/// 拉取全库会话（keyset 分页累积）。
///
/// 超过 [`MAX_SESSIONS`] 即停止并记日志（不是静默截断——调用方把「已封顶」
/// 写进适配器状态，用户能看到少了哪些）。
pub(crate) fn collect_sessions(h: &WasmHost, db: &str) -> Result<Vec<ParsedSession>, SyncError> {
    let mut out: Vec<ParsedSession> = Vec::new();
    let mut after = String::new();
    let mut capped = false;
    while out.len() < MAX_SESSIONS {
        let rows = query(h, db, &session_page_sql(&after))?;
        if rows.is_empty() {
            break;
        }
        for row in &rows {
            let parsed = parse_opencode_session_row(row);
            if parsed.cli_session_id.is_empty() {
                continue;
            }
            out.push(parsed);
        }
        // 游标推进到本页最后一行（rows 已按 id 升序）
        match rows
            .last()
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
        {
            Some(last) => after = last.to_string(),
            // 拿不到末行 id = 查询形态变了（不该发生）：停手避免死循环
            None => {
                return Err(SyncError::QueryFailed(
                    "opencode session page lacks id column".into(),
                ))
            }
        }
        if rows.len() < SESSION_PAGE_ROWS {
            break;
        }
    }
    if out.len() >= MAX_SESSIONS {
        capped = true;
    }
    if capped {
        h.log_warn(&format!(
            "usage: opencode session import capped at {MAX_SESSIONS} rows; raise MAX_SESSIONS to import more"
        ));
    }
    Ok(out)
}

// ==================== 事件流 ====================

/// 单会话事件流（联表 keyset 分页累积）
pub(crate) fn session_events(
    h: &WasmHost,
    db: &str,
    session_id: &str,
) -> Result<(Vec<NormalizedEvent>, bool), SyncError> {
    let mut rows: Vec<Value> = Vec::new();
    let (mut after_mid, mut after_pid) = (String::new(), String::new());
    let mut truncated = false;
    while rows.len() < MAX_EVENT_ROWS {
        let page = query(h, db, &event_page_sql(session_id, &after_mid, &after_pid))?;
        if page.is_empty() {
            break;
        }
        for row in page.iter() {
            if rows.len() >= MAX_EVENT_ROWS {
                truncated = true;
                break;
            }
            rows.push(row.clone());
        }
        let last = page.last().expect("page non-empty");
        let Some(mid) = last.get("mid").and_then(|v| v.as_str()) else {
            return Err(SyncError::QueryFailed(
                "opencode event page lacks mid column".into(),
            ));
        };
        after_mid = mid.to_string();
        after_pid = last
            .get("pid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if page.len() < EVENT_PAGE_ROWS {
            break;
        }
    }
    Ok((parse_opencode_events(&rows), truncated))
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// SQL 字符串字面量转义：单引号加倍，杜绝游标/会话 id 越出引号
    #[test]
    fn sql_text_literal_escapes_quotes() {
        assert_eq!(sql_text_literal("ses_abc"), "'ses_abc'");
        // 反例：含单引号时必须加倍，否则 SQL 结构被破坏
        assert_eq!(sql_text_literal("a'b"), "'a''b'");
        // 注释注入尝试被中和（值仍在引号内）
        assert_eq!(sql_text_literal("x' OR '1'='1"), "'x'' OR ''1''=''1'");
    }

    /// 聚合分页 SQL：keyset 游标 + 显式列清单（不 SELECT *）
    #[test]
    fn session_page_sql_uses_keyset_and_explicit_columns() {
        let sql = session_page_sql("ses_prev");
        assert!(sql.contains("WHERE id > 'ses_prev'"));
        assert!(sql.contains("ORDER BY id"));
        assert!(sql.contains(&format!("LIMIT {SESSION_PAGE_ROWS}")));
        // 显式列：归一 schema 需要的 12 列齐备
        for col in [
            "id",
            "directory",
            "title",
            "cost",
            "tokens_input",
            "tokens_output",
            "tokens_reasoning",
            "tokens_cache_read",
            "tokens_cache_write",
            "model",
            "time_created",
            "time_updated",
        ] {
            assert!(sql.contains(col), "aggregate SQL missing column {col}");
        }
        assert!(
            !sql.contains("SELECT *"),
            "禁止 SELECT *（升级加列会白搬数据）"
        );
        // 首屏游标为空串 → 取全表开头
        let first = session_page_sql("");
        assert!(first.contains("WHERE id > ''"));
    }

    /// 事件分页 SQL：联表游标 + json_valid 守卫 + output 截断
    #[test]
    fn event_page_sql_keys_and_bounds_payload() {
        let sql = event_page_sql("ses_1", "msg_a", "prt_b");
        // 游标按 (mid, pid) 复合推进（同一 message 的后续 part 不能漏）
        assert!(sql.contains("WHERE m.session_id = 'ses_1'"));
        assert!(
            sql.contains("(m.id > 'msg_a' OR (m.id = 'msg_a' AND COALESCE(p.id,'') > 'prt_b'))")
        );
        assert!(sql.contains("ORDER BY m.id, p.id"));
        // 大 blob 必须在 SQL 侧截断
        assert!(sql.contains(&format!(
            "substr(json_extract(p.data,'$.state.output'),1,{PART_OUTPUT_CAP})"
        )));
        assert!(!sql.contains("SELECT *"));
        // json_extract 必带 json_valid 守卫（损坏 blob 会中止整条查询）
        let extracts = sql.matches("json_extract(").count();
        let guards = sql.matches("json_valid(").count();
        assert_eq!(
            extracts, guards,
            "每个 json_extract 都必须被 json_valid 守卫（否则一条坏 blob 拖垮整页）"
        );
    }

    /// 错误分类：三种成因各有独立 code（前端按 code 走 i18n）
    #[test]
    fn sync_error_codes_are_distinct() {
        assert_eq!(SyncError::SqliteMissing.code(), "sqlite3-missing");
        assert_eq!(SyncError::DbMissing.code(), "db-missing");
        assert_eq!(SyncError::QueryFailed("boom".into()).code(), "query-failed");
    }

    /// 库路径：家目录拼接（母 spec §9 落点）
    #[test]
    fn db_path_under_local_share() {
        assert_eq!(
            db_path("/home/u"),
            "/home/u/.local/share/opencode/opencode.db"
        );
    }
}
