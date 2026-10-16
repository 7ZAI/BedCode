//! 使用统计看板聚合（一次返回全部分组 + 汇总，前端本地切换指标/维度）
//!
//! # 口径
//!
//! - **时间窗**：`days` 参数（0 = 全量，1..=3650 天）。窗内过滤只落在
//!   `started_at` 上，与会话列表的 `from`/`to` 语义一致；`days` 非法
//!   （负数 / 超上限 / 非整数）一律回落到 0（全量），不静默截成别的窗口。
//! - **token 总量** = 输入 + 输出 + 缓存读 + 缓存写。**推理不另计**：
//!   claude 的 `thinking_tokens` 是 `output_tokens` 的子集（见
//!   `usage_parse::claude`），重复相加会凭空放大总量；推理单列为
//!   「其中推理」维度，只在指标选择里单列，不进构成堆叠。
//! - **按模型**在 Rust 侧展开 `models_json`（会话多模型时按消息级归属，
//!   数字与源数据一致），窗口过滤与 SQL 各分组保持同一条件。
//! - **节奏矩阵**（`byHour`）按宿主本地时区把会话落到 7×24 网格，格内是
//!   token 量（不是会话数）——「什么时候用得多」要看量而非次数。
//!
//! # 为什么按天不按小时画趋势
//!
//! 逐日序列是用户唯一能读出「哪天多」的时间轴；小时级只进节奏矩阵。

use super::schema::ensure_schema;
use super::BREAKDOWN_LIMIT;
use crate::usage_parse::ModelUsage;
use bedcode_plugin_api::host::HostPluginDatabase;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};
use std::collections::HashMap;

/// 时间窗上限（天）：超过即视为非法参数回落全量，防止一条命令拉全历史聚合
const MAX_WINDOW_DAYS: i64 = 3650;

/**
 * 排行排序表达式（**必须重写聚合，不能写裸列名**）
 *
 * SQLite 在 `GROUP BY` 查询的 `ORDER BY` 里遇到与结果别名同名的**输入列**
 * 时，解析到的是**输入列**（组内任意一行的值）而不是 `SUM()` 别名。
 * 写成 `ORDER BY tokens_in + tokens_out DESC` 时排行顺序等同于随机——
 * 「Top 项目 / Top CLI」会毫无章法。此处显式重写聚合，结果稳定。
 */
const RANK_BY_TOKENS: &str = "(COALESCE(SUM(tokens_in), 0) + COALESCE(SUM(tokens_out), 0) \
                            + COALESCE(SUM(tokens_cache_read), 0) + COALESCE(SUM(tokens_cache_write), 0))";

// ==================== 纯函数（可单测） ====================

/// 时间窗天数清洗：`0` = 全量；`1..=MAX_WINDOW_DAYS` 按天；其余（负数 /
/// 超上限 / 缺省）一律回落到 0。
fn sanitize_days(raw: Option<i64>) -> i64 {
    match raw {
        Some(d) if (1..=MAX_WINDOW_DAYS).contains(&d) => d,
        _ => 0,
    }
}

/// 统计作用域 WHERE 子句。
///
/// `require_timed` = true 时额外要求 `started_at IS NOT NULL`（按天 / 节奏
/// 矩阵这类**以时间为主键**的分组必须，否则 `date(NULL)` 会把所有无时间戳
/// 会话并进同一天）。返回 `(where_sql, params)`，params 顺序即 `?` 顺序。
fn scope_where(days: i64, now_ms: i64, require_timed: bool) -> (String, Vec<Value>) {
    let mut clauses: Vec<String> = Vec::new();
    let mut params: Vec<Value> = Vec::new();
    if days > 0 {
        // 窗口按「距今天数」换算：now - days*86400_000，闭区间取 [from, ∞)
        let from = now_ms - days * 86_400_000;
        clauses.push("started_at >= ?".to_string());
        params.push(json!(from));
    }
    if require_timed {
        clauses.push("started_at IS NOT NULL".to_string());
    }
    if clauses.is_empty() {
        return (String::new(), params);
    }
    (format!(" WHERE {}", clauses.join(" AND ")), params)
}

// ==================== 看板聚合 ====================

/// 多维看板聚合（一次返回汇总 + 全部维度分组 + 节奏矩阵）。
///
/// `args.days`：时间窗天数（0 = 全量，见 [`sanitize_days`]）。
pub(crate) fn get_stats(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let empty = json!([]);
    // 宿主时钟经 config 取（wasm 无系统时钟）；取不到按 0 处理——
    // 窗内过滤会退化成「窗口起点为 0」即全量，与 days=0 行为一致，不会空看板
    let now = crate::install::now_ms(h).unwrap_or(0) as i64;
    let days = sanitize_days(args.get("days").and_then(|v| v.as_i64()));
    // 以时间为主键的分组必须再挡一道 NULL
    let (w_all, p_all) = scope_where(days, now, false);
    let (w_timed, p_timed) = scope_where(days, now, true);

    // ---- 汇总（一次扫描出全部标量） ----
    let total = h
        .plugin_db_query_params(
            &format!(
                "SELECT COUNT(*) AS sessions, \
                    COALESCE(SUM(tokens_in), 0) AS tokens_in, \
                    COALESCE(SUM(tokens_out), 0) AS tokens_out, \
                    COALESCE(SUM(tokens_cache_read), 0) AS tokens_cache_read, \
                    COALESCE(SUM(tokens_cache_write), 0) AS tokens_cache_write, \
                    COALESCE(SUM(tokens_reasoning), 0) AS tokens_reasoning, \
                    COALESCE(SUM(duration_ms), 0) AS duration_ms, \
                    SUM(cost_total) AS cost_total, \
                    COUNT(DISTINCT CASE WHEN started_at IS NOT NULL \
                        THEN date(started_at / 1000, 'unixepoch', 'localtime') END) AS active_days, \
                    COUNT(DISTINCT CASE WHEN project IS NOT NULL AND project <> '' THEN project END) AS projects, \
                    COUNT(DISTINCT CASE WHEN model IS NOT NULL AND model <> '' THEN model END) AS models, \
                    MIN(started_at) AS first_at, MAX(started_at) AS last_at \
             FROM usage_session{w_all}"
            ),
            &p_all,
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .unwrap_or(json!({ "sessions": 0, "tokens_in": 0, "tokens_out": 0, "tokens_cache_read": 0, "tokens_cache_write": 0, "tokens_reasoning": 0, "duration_ms": 0, "cost_total": null, "active_days": 0, "projects": 0, "models": 0, "first_at": null, "last_at": null }));

    // ---- 按天（趋势图数据源；必须挡 NULL 时间戳） ----
    let by_day = h
        .plugin_db_query_params(
            &format!(
                "SELECT date(started_at / 1000, 'unixepoch', 'localtime') AS day, \
                    COUNT(*) AS sessions, \
                    COALESCE(SUM(tokens_in), 0) AS tokens_in, \
                    COALESCE(SUM(tokens_out), 0) AS tokens_out, \
                    COALESCE(SUM(tokens_cache_read), 0) AS tokens_cache_read, \
                    COALESCE(SUM(tokens_cache_write), 0) AS tokens_cache_write, \
                    COALESCE(SUM(tokens_reasoning), 0) AS tokens_reasoning, \
                    COALESCE(SUM(duration_ms), 0) AS duration_ms, \
                    SUM(cost_total) AS cost_total \
             FROM usage_session{w_timed} GROUP BY day ORDER BY day"
            ),
            &p_timed,
        )
        .ok()
        .flatten()
        .unwrap_or(empty.clone());

    // ---- 按 CLI（恒存在全部四家，未采集的显示 0 参与占比） ----
    let by_cli = h
        .plugin_db_query_params(
            &format!(
                "SELECT adapter, COUNT(*) AS sessions, \
                    COALESCE(SUM(duration_ms), 0) AS duration_ms, \
                    COALESCE(SUM(tokens_in), 0) AS tokens_in, \
                    COALESCE(SUM(tokens_out), 0) AS tokens_out, \
                    COALESCE(SUM(tokens_cache_read), 0) AS tokens_cache_read, \
                    COALESCE(SUM(tokens_cache_write), 0) AS tokens_cache_write, \
                    COALESCE(SUM(tokens_reasoning), 0) AS tokens_reasoning, \
                    SUM(cost_total) AS cost_total, \
                    MAX(started_at) AS last_at \
             FROM usage_session{w_all} GROUP BY adapter \
             ORDER BY {RANK_BY_TOKENS} DESC"
            ),
            &p_all,
        )
        .ok()
        .flatten()
        .unwrap_or(empty.clone());

    // ---- 按项目（截断展示面；路径全量回传，由前端折叠 ~ 前缀） ----
    let by_project = h
        .plugin_db_query_params(
            &format!(
                "SELECT COALESCE(project, '') AS project, COUNT(*) AS sessions, \
                    COALESCE(SUM(duration_ms), 0) AS duration_ms, \
                    COALESCE(SUM(tokens_in), 0) AS tokens_in, \
                    COALESCE(SUM(tokens_out), 0) AS tokens_out, \
                    COALESCE(SUM(tokens_cache_read), 0) AS tokens_cache_read, \
                    COALESCE(SUM(tokens_cache_write), 0) AS tokens_cache_write, \
                    COALESCE(SUM(tokens_reasoning), 0) AS tokens_reasoning, \
                    SUM(cost_total) AS cost_total, \
                    MAX(started_at) AS last_at \
             FROM usage_session{w_all} GROUP BY project \
             ORDER BY {RANK_BY_TOKENS} DESC"
            ),
            &p_all,
        )
        .ok()
        .flatten()
        .map(|rows| truncate_rows(rows, BREAKDOWN_LIMIT))
        .unwrap_or(empty.clone());

    // ---- 节奏矩阵（7×24，本地时区；格内是 token 量） ----
    let by_hour = h
        .plugin_db_query_params(
            &format!(
                "SELECT CAST(strftime('%w', started_at / 1000, 'unixepoch', 'localtime') AS INTEGER) AS dow, \
                    CAST(strftime('%H', started_at / 1000, 'unixepoch', 'localtime') AS INTEGER) AS hour, \
                    COUNT(*) AS sessions, \
                    COALESCE(SUM(tokens_in + tokens_out + tokens_cache_read + tokens_cache_write), 0) AS tokens \
             FROM usage_session{w_timed} GROUP BY dow, hour"
            ),
            &p_timed,
        )
        .ok()
        .flatten()
        .unwrap_or(empty.clone());

    // ---- 按模型：展开各会话 models_json（消息级归属，不按主导模型摊派） ----
    let mut by_model: HashMap<String, Value> = HashMap::new();
    if let Some(rows) = h
        .plugin_db_query_params(
            &format!("SELECT models_json FROM usage_session{w_all}"),
            &p_all,
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
    {
        for row in rows {
            let Some(raw) = row.get("models_json").and_then(|m| m.as_str()) else {
                continue;
            };
            let Ok(models) = serde_json::from_str::<Vec<ModelUsage>>(raw) else {
                continue;
            };
            for m in models {
                let entry = by_model.entry(m.model.clone()).or_insert_with(|| {
                    json!({
                        "model": m.model,
                        "sessions": 0, "messages": 0,
                        "tokens_in": 0, "tokens_out": 0,
                        "tokens_cache_read": 0, "tokens_cache_write": 0,
                        "tokens_reasoning": 0,
                    })
                });
                entry["sessions"] = json!(entry["sessions"].as_i64().unwrap_or(0) + 1);
                entry["messages"] =
                    json!(entry["messages"].as_i64().unwrap_or(0) + m.messages as i64);
                entry["tokens_in"] =
                    json!(entry["tokens_in"].as_i64().unwrap_or(0) + m.tokens.input);
                entry["tokens_out"] =
                    json!(entry["tokens_out"].as_i64().unwrap_or(0) + m.tokens.output);
                entry["tokens_cache_read"] =
                    json!(entry["tokens_cache_read"].as_i64().unwrap_or(0) + m.tokens.cache_read);
                entry["tokens_cache_write"] =
                    json!(entry["tokens_cache_write"].as_i64().unwrap_or(0) + m.tokens.cache_write);
                entry["tokens_reasoning"] =
                    json!(entry["tokens_reasoning"].as_i64().unwrap_or(0) + m.tokens.reasoning);
            }
        }
    }
    let mut by_model: Vec<Value> = by_model.into_values().collect();
    by_model.sort_by_key(|e| {
        std::cmp::Reverse(
            e["tokens_in"].as_i64().unwrap_or(0)
                + e["tokens_out"].as_i64().unwrap_or(0)
                + e["tokens_cache_read"].as_i64().unwrap_or(0)
                + e["tokens_cache_write"].as_i64().unwrap_or(0),
        )
    });
    by_model.truncate(BREAKDOWN_LIMIT);

    Ok(json!({
        "window": { "days": days, "now": now },
        "total": total,
        "byDay": by_day,
        "byCli": by_cli,
        "byProject": by_project,
        "byModel": by_model,
        "byHour": by_hour,
    }))
}

fn truncate_rows(mut rows: Value, limit: usize) -> Value {
    if let Some(arr) = rows.as_array_mut() {
        arr.truncate(limit);
    }
    rows
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 时间窗清洗契约：0 = 全量；1..=3650 透传；负数 / 超上限 / 缺省 / 字符串
    /// 一律回落到全量（不静默截成别的窗口，也不报错把看板打空）。
    #[test]
    fn sanitize_days_clamps_to_full_range() {
        assert_eq!(sanitize_days(Some(0)), 0, "0 显式表示全量");
        assert_eq!(sanitize_days(Some(7)), 7);
        assert_eq!(sanitize_days(Some(3650)), 3650, "上限边界透传");
        assert_eq!(sanitize_days(Some(-1)), 0, "负数回落全量");
        assert_eq!(sanitize_days(Some(3651)), 0, "超上限回落全量");
        assert_eq!(sanitize_days(None), 0, "缺省即全量");
    }

    /// 作用域 WHERE 契约：
    /// - 全量 + 不要求时间 → 空子句、零参数（SQL 里不留 `WHERE` 尾巴）
    /// - 窗内 → `started_at >= ?` 且参数 = now - days*86400_000
    /// - 要求时间（按天 / 节奏矩阵）→ 追加 IS NOT NULL，且**参数顺序不变**
    ///   （追加的是无参子句，绑定顺序因此稳定）
    #[test]
    fn scope_where_builds_window_and_timed_clauses() {
        // 全量：不带时间的分组（汇总 / CLI / 项目 / 模型）不加任何条件
        let (sql, params) = scope_where(0, 1_000_000, false);
        assert_eq!(sql, "");
        assert!(params.is_empty());

        // 窗内：30 天 = 2_592_000_000ms
        let (sql, params) = scope_where(30, 1_000_000_000, false);
        assert_eq!(sql, " WHERE started_at >= ?");
        assert_eq!(params, vec![json!(1_000_000_000i64 - 2_592_000_000i64)]);

        // 以时间为主键的分组必须再挡 NULL（否则 date(NULL) 全并进同一天）
        let (sql, params) = scope_where(30, 1_000_000_000, true);
        assert_eq!(sql, " WHERE started_at >= ? AND started_at IS NOT NULL");
        assert_eq!(params.len(), 1, "IS NOT NULL 不引入占位符");

        // 全量 + 要求时间：仍然要挡 NULL
        let (sql, params) = scope_where(0, 1_000_000_000, true);
        assert_eq!(sql, " WHERE started_at IS NOT NULL");
        assert!(params.is_empty());
    }

    /// 回归见证：漏掉 IS NOT NULL 时，按天分组会把无时间戳会话并进同一天，
    /// 节奏矩阵也会出现 dow/hour 为 NULL 的幽灵格。
    #[test]
    fn timed_scope_rejects_null_started_at() {
        let (sql, _) = scope_where(0, 0, true);
        assert!(
            sql.contains("started_at IS NOT NULL"),
            "以时间为主键的分组未挡 NULL 时间戳：{sql}"
        );
    }

    /// 截断上限：分组行数超过展示面上限时按原序截断（不重排）。
    #[test]
    fn truncate_rows_keeps_order_and_caps() {
        let rows = json!([{"a": 1}, {"a": 2}, {"a": 3}]);
        assert_eq!(truncate_rows(rows.clone(), 2), json!([{"a": 1}, {"a": 2}]));
        assert_eq!(truncate_rows(rows.clone(), 9), rows, "不足上限时原样返回");
        assert_eq!(truncate_rows(json!(null), 2), json!(null), "非数组原样返回");
    }

    /// 排行排序契约：必须显式重写聚合。
    ///
    /// 回归见证：写成裸列名 `ORDER BY tokens_in + tokens_out DESC` 时，SQLite
    /// 在 `GROUP BY` 查询里把它解析成**输入列**（组内任意一行的值）而非
    /// `SUM()` 别名，排行顺序等同于随机——实机 160 会话库里「Top 项目」
    /// 会把 1.3 亿 tokens 的主项目排到第 4 位。此锁在有人改回裸列名时变红。
    #[test]
    fn rank_order_uses_aggregate_not_bare_column() {
        let expr: String = RANK_BY_TOKENS.split_whitespace().collect();
        assert!(
            expr.starts_with("(COALESCE(SUM(tokens_in),0)+COALESCE(SUM(tokens_out),0)"),
            "排行排序未显式包裹聚合：{RANK_BY_TOKENS}"
        );
        // 四个桶都在（漏一个会让「缓存读为主」的项目排到后面）
        for col in [
            "tokens_in",
            "tokens_out",
            "tokens_cache_read",
            "tokens_cache_write",
        ] {
            assert!(
                expr.contains(&format!("COALESCE(SUM({col}),0)")),
                "排行排序缺 {col} 桶"
            );
        }
        // 反例守门：裸列名形式必须不再出现
        assert!(
            !expr.contains("ORDER BY tokens_in"),
            "排序又退回裸列名（SQLite 会解析成组内任意一行的值）"
        );
    }
}
