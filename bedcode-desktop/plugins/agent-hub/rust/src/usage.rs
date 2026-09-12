//! 使用统计与会话日志域（票据 06）
//!
//! 数据流：`scan-usage` 以 AUTH_KEY 为闸门（fs_auth 第三层按路径弹窗，
//! 未授权时扫描会引发弹窗风暴，故整体降级为 auth-required）→ host-process
//! 枚举两家 JSONL（claude `~/.claude/projects`、pi `~/.pi/agent/sessions`，
//! 平台分派 find/dir，沿用 `== 分段 ==` 标记）→ `on_process_done` 回灌：
//! 逐文件读内容（host-fs），按**水位**（`parse_watermark` 表，WIT 无 stat
//! 原语，JSONL append-only 语义以 size 为水位、mtime 存 NULL）跳过未变更
//! 文件 → 适配器解析（usage_parse.rs 纯函数）→ 会话聚合 upsert
//! `usage_session` → 状态持久化 + 事件推送。
//!
//! 会话日志视图（§4.6）与统计共用一次解析：列表/看板走 `usage_session`
//! 聚合表；打开单会话时以同一适配器重解析该文件产出归一事件流 + 原始行
//! （事件流体积不受控，不落库）。`provider_preset` 表按票据 05 预留建表。
//!
//! 幂等：水位未变的文件整文件跳过；水位变更则整文件重解析并按
//! `UNIQUE(adapter, cli_session_id)` 先 UPDATE 后 INSERT（rusqlite 单语句
//! 执行，见 auto-task queue.rs 同款约束；不用 INSERT ON CONFLICT 以免
//! 依赖宿主 SQLite 版本的 upsert 支持）。扫描进行中（status == syncing）
//! 拒绝重入。

use super::{host, is_windows, pending, shell_invocation, PendingRun, AUTH_KEY, DATA_DIR, HOME};
use crate::install::now_ms;
use crate::usage_parse::{
    parse_claude_session, parse_pi_session, ModelUsage, NormalizedEvent, ParsedSession,
    MAX_FILE_BYTES,
};
use bedcode_plugin_api::events::ProcessDoneEvent;
use bedcode_plugin_api::host::{
    HostEvents, HostFs, HostLog, HostPluginDatabase, HostProcess, HostStorage,
};
use bedcode_plugin_api::sql_params;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

/// host-storage 键：使用统计域状态
pub(crate) const USAGE_KEY: &str = "usage";
/// 适配器清单（票据 06：claude / pi；opencode SQLite、codex 归票据 07）
const ADAPTERS: [&str; 2] = ["claude", "pi"];
/// (adapter, 家目录相对段)：JSONL 会话根目录
const SESSION_ROOTS: [(&str, &str); 2] =
    [("claude", ".claude/projects"), ("pi", ".pi/agent/sessions")];
/// 枚举超时：纯文件系统遍历（与 skills 扫描同量级）
const SCAN_TIMEOUT_MS: u64 = 30_000;
/// 按项目 / 按模型汇总表行数上限（看板展示面）
const BREAKDOWN_LIMIT: usize = 20;
/// 原始 JSONL 行回显上限（与事件流同量级防御）
const RAW_LINE_CAP: usize = 5000;
/// 会话明细列表默认分页大小
pub(crate) const PAGE_SIZE: i64 = 50;

static RUN_SEQ: AtomicU32 = AtomicU32::new(0);

// ==================== Schema（幂等建表，单语句逐条执行） ====================

/// 建表 + 索引（CREATE IF NOT EXISTS 幂等，可对旧库重跑；每条单独 execute，
/// 宿主 plugin_db_execute 为单语句语义，多语句会被静默截断）
pub(crate) fn ensure_schema(h: &WasmHost) -> anyhow::Result<()> {
    let stmts = [
        // 解析水位：mtime 预留（WIT 无 stat 原语，当前恒 NULL，水位判定用 size）
        "CREATE TABLE IF NOT EXISTS parse_watermark (\
             id INTEGER PRIMARY KEY AUTOINCREMENT,\
             adapter TEXT NOT NULL,\
             source_path TEXT NOT NULL,\
             size INTEGER,\
             mtime INTEGER,\
             parsed_at INTEGER,\
             UNIQUE(adapter, source_path))",
        // 会话聚合（统计查询真源）；source_path / models_json 为 spec 草案
        // 之外的功能列：前者供日志视图定位源文件（§4.6 顶部展示），后者存
        // 每模型明细（按模型维度聚合在 Rust 侧展开，避免会话多模型时失真）
        "CREATE TABLE IF NOT EXISTS usage_session (\
             id INTEGER PRIMARY KEY AUTOINCREMENT,\
             adapter TEXT NOT NULL,\
             cli_session_id TEXT NOT NULL,\
             project TEXT,\
             title TEXT,\
             source_path TEXT,\
             started_at INTEGER,\
             ended_at INTEGER,\
             duration_ms INTEGER,\
             model TEXT,\
             models_json TEXT,\
             tokens_in INTEGER DEFAULT 0,\
             tokens_out INTEGER DEFAULT 0,\
             tokens_cache_read INTEGER DEFAULT 0,\
             tokens_cache_write INTEGER DEFAULT 0,\
             tokens_reasoning INTEGER DEFAULT 0,\
             cost_total REAL,\
             first_seen_at INTEGER,\
             updated_at INTEGER,\
             UNIQUE(adapter, cli_session_id))",
        "CREATE INDEX IF NOT EXISTS idx_usage_session_started ON usage_session(adapter, started_at)",
        "CREATE INDEX IF NOT EXISTS idx_usage_session_project ON usage_session(project)",
        // 供应商预设（票据 05 预留，无 key 列——刻意）
        "CREATE TABLE IF NOT EXISTS provider_preset (\
             id INTEGER PRIMARY KEY AUTOINCREMENT,\
             name TEXT NOT NULL,\
             base_url TEXT,\
             api_style TEXT,\
             models_json TEXT,\
             notes TEXT,\
             created_at INTEGER,\
             updated_at INTEGER)",
    ];
    for stmt in stmts {
        h.plugin_db_execute(stmt)
            .map_err(|e| anyhow::anyhow!("usage: schema execute failed: {e}"))?;
    }
    Ok(())
}

// ==================== 状态（读-改-写） ====================

fn default_state() -> Value {
    let mut adapters = serde_json::Map::new();
    for name in ADAPTERS {
        adapters.insert(
            name.to_string(),
            json!({ "files": 0, "parsed": 0, "skipped": 0, "sessions": 0, "error": null }),
        );
    }
    json!({
        "status": "idle",
        "error": null,
        "syncedAt": null,
        "authGranted": false,
        "adapters": Value::Object(adapters),
    })
}

fn read_state(h: &WasmHost) -> Value {
    let mut state = h
        .storage_get(USAGE_KEY)
        .ok()
        .flatten()
        .filter(|s| s.get("adapters").is_some())
        .unwrap_or_else(default_state);
    // home 每次以运行时值为准（前端项目路径 ~ 折叠用）
    state["home"] = json!(HOME.get().cloned().unwrap_or_default());
    state
}

fn write_state(h: &WasmHost, state: &Value) {
    if let Err(e) = h.storage_set(USAGE_KEY, state) {
        h.log_warn(&format!("usage: persist state failed: {e}"));
    }
}

/// 全量状态推送前端（命令返回值与事件载荷同形）
fn emit_and_return(h: &WasmHost, state: &Value) -> anyhow::Result<Value> {
    h.emit_event("plugin:agent-hub:usage", state);
    Ok(json!({ "state": state }))
}

/// 授权标记是否为 granted（activate 批量申请 / request-auth 命令写入）
fn auth_granted(h: &WasmHost) -> bool {
    h.storage_get(AUTH_KEY)
        .ok()
        .flatten()
        .map(|v| v == json!("granted"))
        .unwrap_or(false)
}

/// 当前域状态（前端挂载首拉；状态本身已在推送/落库前以 authGranted 实时化）
pub(crate) fn get_state(h: &WasmHost) -> anyhow::Result<Value> {
    let mut state = read_state(h);
    state["authGranted"] = json!(auth_granted(h));
    Ok(json!({ "state": state }))
}

// ==================== 扫描（异步流：枚举 → 回灌解析） ====================

/// 触发增量扫描：授权闸门 → 枚举两家 JSONL → 回灌逐文件解析。
/// 扫描进行中（status == syncing）拒绝重入。
pub(crate) fn scan(h: &WasmHost) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let mut state = read_state(h);
    state["authGranted"] = json!(auth_granted(h));
    if !auth_granted(h) {
        // 授权被拒不弹窗（fs_auth 第三层会按路径逐个弹窗）：整体降级，
        // 前端呈现 auth-required + 授权入口
        state["status"] = json!("auth-required");
        write_state(h, &state);
        return emit_and_return(h, &state);
    }
    if state["status"] == json!("syncing") {
        return emit_and_return(h, &state);
    }

    let home = HOME
        .get()
        .ok_or_else(|| anyhow::anyhow!("usage: home unavailable"))?;
    let sections: Vec<(&str, String)> = SESSION_ROOTS
        .iter()
        .map(|(name, seg)| (*name, format!("{home}/{seg}")))
        .collect();
    let script = scan_script(&sections, is_windows());
    let (command, args_vec) = shell_invocation(script, is_windows());

    let data_dir = DATA_DIR
        .get()
        .ok_or_else(|| anyhow::anyhow!("usage: data dir unavailable"))?;
    let seq = RUN_SEQ.fetch_add(1, AtomicOrdering::Relaxed);
    let output_path = format!("{data_dir}/runs/usage-scan-{seq}.log");
    let request = json!({
        "command": command,
        "args": args_vec,
        "output_path": output_path,
        "timeout_ms": SCAN_TIMEOUT_MS,
    });
    let run_id = h
        .process_run(&request.to_string())
        .map_err(|e| anyhow::anyhow!("usage: scan spawn failed: {e}"))?;

    pending()
        .lock()
        .map_err(|e| anyhow::anyhow!("usage: poisoned pending map: {e}"))?
        .insert(
            run_id,
            PendingRun {
                kind: "usage-scan".to_string(),
                output_path,
                cli: None,
                source: None,
            },
        );

    state["status"] = json!("syncing");
    state["error"] = json!(null);
    write_state(h, &state);
    h.log_info("usage scan started");
    emit_and_return(h, &state)
}

/// 枚举脚本：分段输出各适配器根目录下的 *.jsonl 文件（平台分派，与
/// skills scan_script 同款 `== 分段 ==` 标记；根目录不存在属常态）
fn scan_script(sections: &[(&str, String)], windows: bool) -> String {
    let mut parts: Vec<String> = vec![];
    for (key, root) in sections {
        if windows {
            parts.push(format!(
                "echo == {key} == & dir /s /b /a:-d \"{root}\\*.jsonl\" 2>nul"
            ));
        } else {
            parts.push(format!(
                "echo '== {key} =='\nfind \"{root}\" -type f -name '*.jsonl' 2>/dev/null\n"
            ));
        }
    }
    if windows {
        parts.join(" & ")
    } else {
        parts.join("")
    }
}

/// 扫描进程回灌：读列举输出 → 逐文件水位判定 + 解析 + 落库 → 状态推送
pub(crate) fn handle_scan_done(event: &ProcessDoneEvent) -> anyhow::Result<()> {
    let h = host();
    let output_path = {
        let mut map = pending()
            .lock()
            .map_err(|e| anyhow::anyhow!("usage: poisoned pending map: {e}"))?;
        map.remove(&event.run_id).map(|e| e.output_path)
    };
    let Some(output_path) = output_path else {
        // 停用清理后的迟到回调——放行
        return Ok(());
    };

    let mut state = read_state(&h);
    if event.timed_out {
        state["status"] = json!("error");
        state["error"] = json!("scan timed out");
        write_state(&h, &state);
        h.log_warn("usage scan timed out");
        return emit_and_return(&h, &state).map(|_| ());
    }

    // 枚举失败（如 shell 不可用）≠ 无文件：报错态推进
    let Some(output) = h.fs_read(&output_path).ok().flatten() else {
        state["status"] = json!("error");
        state["error"] = json!("scan output unreadable");
        write_state(&h, &state);
        h.log_warn("usage scan output unreadable");
        return emit_and_return(&h, &state).map(|_| ());
    };
    let _ = h.fs_delete(&output_path);

    ensure_schema(&h)?;
    let listing = crate::skills::parse_listing(&output);
    if listing.is_empty() {
        state["error"] = json!(null);
        state["status"] = json!("ok");
        state["syncedAt"] = json!(now_ms(&h).ok());
        write_state(&h, &state);
        h.log_info("usage scan finished: no session files found");
        return emit_and_return(&h, &state).map(|_| ());
    }

    // 按分段（adapter）分组扫描；分段名与 SESSION_ROOTS 键一致
    let mut per_adapter: HashMap<&str, (u32, u32, u32, u32)> = ADAPTERS
        .iter()
        .map(|name| (*name, (0u32, 0u32, 0u32, 0u32))) // files, parsed, skipped, sessions
        .collect();
    let now = now_ms(&h).unwrap_or(0);
    for (section, path) in &listing {
        let Some(slot) = per_adapter.get_mut(section.as_str()) else {
            continue;
        };
        slot.0 += 1;
        // 逐文件水位：内容字节长未变 → 跳过解析（JSONL append-only）
        let content_opt = match h.fs_read(path) {
            Ok(c) => c,
            Err(e) => {
                h.log_warn(&format!("usage: read session file failed: {e}"));
                continue;
            }
        };
        let Some(content) = content_opt else {
            continue; // 枚举与读取间隙被删除
        };
        let size = content.len();
        if watermark_unchanged(&h, section, path, size) {
            slot.2 += 1;
            continue;
        }
        if size > MAX_FILE_BYTES {
            h.log_warn(&format!(
                "usage: session file exceeds cap, skipped, size = {size}"
            ));
            slot.2 += 1;
            continue;
        }
        let Some(parsed) = parse_by_adapter(section, &content) else {
            continue;
        };
        if parsed.skipped_lines > 0 {
            h.log_warn(&format!(
                "usage: skipped unparseable lines, count = {}, adapter = {section}",
                parsed.skipped_lines
            ));
        }
        let sessions = upsert_session(&h, section, path, &parsed, now);
        upsert_watermark(&h, section, path, size, now);
        slot.1 += 1;
        slot.3 += sessions;
    }

    // 汇总各适配器结果进状态
    if let Some(adapters) = state["adapters"].as_object_mut() {
        for (name, (files, parsed, skipped, sessions)) in &per_adapter {
            adapters.insert(
                name.to_string(),
                json!({ "files": files, "parsed": parsed, "skipped": skipped, "sessions": sessions, "error": null }),
            );
        }
    }
    state["error"] = json!(null);
    state["status"] = json!("ok");
    state["syncedAt"] = json!(now_ms(&h).ok());
    write_state(&h, &state);
    h.log_info("usage scan finished");
    emit_and_return(&h, &state).map(|_| ())
}

// ==================== 适配器分派 ====================

/// 按适配器名分派解析（扫描回灌与会话日志打开共用；票据 07 新增适配器
/// 在此与 SESSION_ROOTS/ADAPTERS 同步扩一行）
fn parse_by_adapter(adapter: &str, content: &str) -> Option<ParsedSession> {
    match adapter {
        "claude" => Some(parse_claude_session(content)),
        "pi" => Some(parse_pi_session(content)),
        _ => None,
    }
}

/// 水位判定：同 (adapter, path) 的已记录 size 与当前一致 → 无需重解析
fn watermark_unchanged(h: &WasmHost, adapter: &str, path: &str, size: usize) -> bool {
    h.plugin_db_query_params(
        "SELECT size FROM parse_watermark WHERE adapter = ?1 AND source_path = ?2",
        &sql_params![adapter, path],
    )
    .ok()
    .flatten()
    .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
    .and_then(|row| row.get("size").and_then(|s| s.as_i64()))
    .map(|prev| prev == size as i64)
    .unwrap_or(false)
}

/// 水位落库：先 UPDATE 后 INSERT（同 upsert_session 的单语句兼容策略）
fn upsert_watermark(h: &WasmHost, adapter: &str, path: &str, size: usize, now: u64) {
    let params = sql_params![adapter, path, size as i64, now as i64];
    let affected = h
        .plugin_db_execute_params(
            "UPDATE parse_watermark SET size = ?3, parsed_at = ?4 WHERE adapter = ?1 AND source_path = ?2",
            &params,
        )
        .unwrap_or(0);
    if affected == 0 {
        // INSERT 失败仅影响下轮重解析（水位幂等性降级为「多算一轮」），warn 留痕
        if let Err(e) = h.plugin_db_execute_params(
            "INSERT INTO parse_watermark (adapter, source_path, size, mtime, parsed_at) VALUES (?1, ?2, ?3, NULL, ?4)",
            &params,
        ) {
            h.log_warn(&format!("usage: watermark insert failed: {e}"));
        }
    }
}

/// 会话聚合落库：先 UPDATE 后 INSERT，返回本文件贡献的会话数（空会话——
/// 无 cli_session_id——丢弃，不产生记录）
fn upsert_session(
    h: &WasmHost,
    adapter: &str,
    path: &str,
    parsed: &ParsedSession,
    now: u64,
) -> u32 {
    // 无会话 id 的文件（如仅剩损坏行）不入库，但水位仍推进（内容没变就不会变好）
    if parsed.cli_session_id.is_empty() {
        return 0;
    }
    let started = parsed.started_at.unwrap_or(0);
    let ended = parsed.ended_at.unwrap_or(started);
    let duration = (ended.saturating_sub(started)).max(0);
    let dominant = parsed.dominant_model().unwrap_or("unknown");
    let models_json = serde_json::to_string(&parsed.models).unwrap_or_else(|_| "[]".to_string());
    let cost = parsed.cost_total;
    // UPDATE 与 INSERT 共用同一参数序列（?1..?17，INSERT 的 first_seen_at
    // 与 updated_at 共用 ?17）
    let params = sql_params![
        adapter,
        parsed.cli_session_id,
        parsed.project,
        parsed.title,
        path,
        started,
        ended,
        duration,
        dominant,
        models_json,
        parsed.tokens.input,
        parsed.tokens.output,
        parsed.tokens.cache_read,
        parsed.tokens.cache_write,
        parsed.tokens.reasoning,
        cost,
        now as i64,
    ];

    let affected = h
        .plugin_db_execute_params(
            "UPDATE usage_session SET project = ?3, title = ?4, source_path = ?5, \
                 started_at = ?6, ended_at = ?7, duration_ms = ?8, model = ?9, models_json = ?10, \
                 tokens_in = ?11, tokens_out = ?12, tokens_cache_read = ?13, tokens_cache_write = ?14, \
                 tokens_reasoning = ?15, cost_total = ?16, updated_at = ?17 \
             WHERE adapter = ?1 AND cli_session_id = ?2",
            &params,
        )
        .unwrap_or(0);
    if affected > 0 {
        return 0; // 更新既有行，不新增会话
    }
    let inserted = h
        .plugin_db_execute_params(
            "INSERT INTO usage_session (adapter, cli_session_id, project, title, source_path, \
                 started_at, ended_at, duration_ms, model, models_json, \
                 tokens_in, tokens_out, tokens_cache_read, tokens_cache_write, tokens_reasoning, \
                 cost_total, first_seen_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17)",
            &params,
        )
        .unwrap_or(0);
    if inserted > 0 {
        1
    } else {
        0
    }
}

// ==================== 看板聚合 ====================

/// 四维看板聚合（一次返回全部分组，前端本地切换维度）：
/// 按天 / 按 CLI 用 SQL GROUP BY；按模型在 Rust 侧展开 models_json
/// （会话多模型时按消息级归属，数字与源数据一致）；按项目 SQL 聚合。
pub(crate) fn get_stats(h: &WasmHost) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let empty = json!([]);

    // 总量
    let total = h
        .plugin_db_query(
            "SELECT COUNT(*) AS sessions, \
                    COALESCE(SUM(tokens_in), 0) AS tokens_in, \
                    COALESCE(SUM(tokens_out), 0) AS tokens_out, \
                    COALESCE(SUM(tokens_cache_read), 0) AS tokens_cache_read, \
                    COALESCE(SUM(tokens_cache_write), 0) AS tokens_cache_write, \
                    COALESCE(SUM(tokens_reasoning), 0) AS tokens_reasoning, \
                    COALESCE(SUM(duration_ms), 0) AS duration_ms, \
                    SUM(cost_total) AS cost_total \
             FROM usage_session",
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .unwrap_or(json!({ "sessions": 0, "tokens_in": 0, "tokens_out": 0, "tokens_cache_read": 0, "tokens_cache_write": 0, "tokens_reasoning": 0, "duration_ms": 0, "cost_total": null }));

    // 按天（宿主本地时区日切；每日 tokens 为堆叠条数据源）
    let by_day = h
        .plugin_db_query(
            "SELECT date(started_at / 1000, 'unixepoch', 'localtime') AS day, \
                    COUNT(*) AS sessions, \
                    COALESCE(SUM(tokens_in), 0) AS tokens_in, \
                    COALESCE(SUM(tokens_out), 0) AS tokens_out \
             FROM usage_session WHERE started_at IS NOT NULL \
             GROUP BY day ORDER BY day",
        )
        .ok()
        .flatten()
        .unwrap_or(empty.clone());

    // 按 CLI
    let by_cli = h
        .plugin_db_query(
            "SELECT adapter, COUNT(*) AS sessions, \
                    COALESCE(SUM(duration_ms), 0) AS duration_ms, \
                    COALESCE(SUM(tokens_in), 0) AS tokens_in, \
                    COALESCE(SUM(tokens_out), 0) AS tokens_out, \
                    COALESCE(SUM(tokens_cache_read), 0) AS tokens_cache_read, \
                    COALESCE(SUM(tokens_cache_write), 0) AS tokens_cache_write, \
                    COALESCE(SUM(tokens_reasoning), 0) AS tokens_reasoning, \
                    SUM(cost_total) AS cost_total \
             FROM usage_session GROUP BY adapter ORDER BY tokens_in + tokens_out DESC",
        )
        .ok()
        .flatten()
        .unwrap_or(empty.clone());

    // 按项目（截断展示面）
    let by_project = h
        .plugin_db_query(
            "SELECT COALESCE(project, '') AS project, COUNT(*) AS sessions, \
                    COALESCE(SUM(duration_ms), 0) AS duration_ms, \
                    COALESCE(SUM(tokens_in), 0) AS tokens_in, \
                    COALESCE(SUM(tokens_out), 0) AS tokens_out \
             FROM usage_session GROUP BY project ORDER BY tokens_in + tokens_out DESC",
        )
        .ok()
        .flatten()
        .map(|rows| truncate_rows(rows, BREAKDOWN_LIMIT))
        .unwrap_or(empty.clone());

    // 按模型：展开各会话 models_json（消息级归属，不按主导模型摊派）
    let mut by_model: HashMap<String, Value> = HashMap::new();
    if let Some(rows) = h
        .plugin_db_query("SELECT models_json FROM usage_session")
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
                    json!({ "model": m.model, "sessions": 0, "messages": 0, "tokens_in": 0, "tokens_out": 0 })
                });
                entry["sessions"] = json!(entry["sessions"].as_i64().unwrap_or(0) + 1);
                entry["messages"] =
                    json!(entry["messages"].as_i64().unwrap_or(0) + m.messages as i64);
                entry["tokens_in"] =
                    json!(entry["tokens_in"].as_i64().unwrap_or(0) + m.tokens.input);
                entry["tokens_out"] =
                    json!(entry["tokens_out"].as_i64().unwrap_or(0) + m.tokens.output);
            }
        }
    }
    let mut by_model: Vec<Value> = by_model.into_values().collect();
    by_model.sort_by_key(|e| {
        std::cmp::Reverse(
            e["tokens_in"].as_i64().unwrap_or(0) + e["tokens_out"].as_i64().unwrap_or(0),
        )
    });
    by_model.truncate(BREAKDOWN_LIMIT);

    Ok(json!({
        "total": total,
        "byDay": by_day,
        "byCli": by_cli,
        "byProject": by_project,
        "byModel": by_model,
    }))
}

fn truncate_rows(mut rows: Value, limit: usize) -> Value {
    if let Some(arr) = rows.as_array_mut() {
        arr.truncate(limit);
    }
    rows
}

// ==================== 会话列表（统计明细 + 日志主从共用） ====================

/// 分页会话列表（started_at 倒序）；adapter 过滤可选
pub(crate) fn list_sessions(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let offset = args
        .get("offset")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0);
    let limit = args
        .get("limit")
        .and_then(|v| v.as_i64())
        .filter(|l| *l > 0 && *l <= 200)
        .unwrap_or(PAGE_SIZE);
    let adapter = args.get("adapter").and_then(|v| v.as_str()).unwrap_or("");

    let (rows, total) = if adapter.is_empty() {
        (
            h.plugin_db_query_params(
                "SELECT id, adapter, cli_session_id, project, title, started_at, ended_at, \
                        duration_ms, model, tokens_in, tokens_out, tokens_cache_read, \
                        tokens_cache_write, tokens_reasoning, cost_total \
                 FROM usage_session ORDER BY started_at DESC LIMIT ?1 OFFSET ?2",
                &sql_params![limit, offset],
            ),
            h.plugin_db_query("SELECT COUNT(*) AS n FROM usage_session"),
        )
    } else {
        (
            h.plugin_db_query_params(
                "SELECT id, adapter, cli_session_id, project, title, started_at, ended_at, \
                        duration_ms, model, tokens_in, tokens_out, tokens_cache_read, \
                        tokens_cache_write, tokens_reasoning, cost_total \
                 FROM usage_session WHERE adapter = ?3 ORDER BY started_at DESC LIMIT ?1 OFFSET ?2",
                &sql_params![limit, offset, adapter],
            ),
            h.plugin_db_query_params(
                "SELECT COUNT(*) AS n FROM usage_session WHERE adapter = ?1",
                &sql_params![adapter],
            ),
        )
    };
    let rows = rows.ok().flatten().unwrap_or(json!([]));
    let total = total
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|row| row.get("n").and_then(|n| n.as_i64()))
        .unwrap_or(0);
    Ok(json!({ "sessions": rows, "total": total, "offset": offset, "limit": limit }))
}

// ==================== 会话日志视图（事件流 + 原始行） ====================

/// 打开单个会话：按 usage_session.source_path 读源文件，用同一适配器
/// 解析为归一事件流 + 原始 JSONL 行（一次读盘双消费）。
pub(crate) fn read_session(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let id = args
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("read-session: id required"))?;
    let row = h
        .plugin_db_query_params(
            "SELECT id, adapter, cli_session_id, project, title, started_at, ended_at, \
                    duration_ms, model, tokens_in, tokens_out, tokens_cache_read, \
                    tokens_cache_write, tokens_reasoning, cost_total, source_path \
             FROM usage_session WHERE id = ?1",
            &sql_params![id],
        )
        .map_err(|e| anyhow::anyhow!("read-session: query failed: {e}"))?
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .ok_or_else(|| anyhow::anyhow!("read-session: session {id} not found"))?;
    let adapter = row
        .get("adapter")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .to_string();
    let source_path = row
        .get("source_path")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    // 源文件可能已被外部清理：会话元数据仍在（列表可展示），事件流报错呈现
    let content = h
        .fs_read(&source_path)
        .map_err(|e| anyhow::anyhow!("read-session: source unreadable: {e}"))?
        .unwrap_or_default();

    let parsed = parse_by_adapter(&adapter, &content)
        .ok_or_else(|| anyhow::anyhow!("read-session: unknown adapter {adapter}"))?;

    // 原始行视图：与事件流同源，上限独立计（超出截断并标注）
    let raw_truncated = content.lines().count() > RAW_LINE_CAP;
    let raw: Vec<&str> = content.lines().take(RAW_LINE_CAP).collect();

    let events: Vec<Value> = parsed.events.iter().map(event_to_json).collect();
    Ok(json!({
        "session": row,
        "events": events,
        "eventsTruncated": parsed.events_truncated,
        "raw": raw,
        "rawTruncated": raw_truncated,
        "skippedLines": parsed.skipped_lines,
    }))
}

fn event_to_json(e: &NormalizedEvent) -> Value {
    json!({
        "ts": e.ts,
        "role": e.role,
        "text": e.text,
        "model": e.model,
        "tokens": e.tokens.map(|t| json!({
            "input": t.input, "output": t.output, "cacheRead": t.cache_read,
            "cacheWrite": t.cache_write, "reasoning": t.reasoning,
        })),
    })
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 枚举脚本：unix find -name 过滤 jsonl + 分段标记；Windows dir /s /b 通配
    #[test]
    fn scan_script_platform_shapes() {
        let sections = vec![
            ("claude", "/home/u/.claude/projects".to_string()),
            ("pi", "/home/u/.pi/agent/sessions".to_string()),
        ];
        let unix = scan_script(&sections, false);
        assert!(unix.contains("== claude =="));
        assert!(unix.contains("== pi =="));
        assert!(unix.contains("find \"/home/u/.claude/projects\" -type f -name '*.jsonl'"));
        assert!(unix.contains("2>/dev/null"));

        let win = scan_script(&sections, true);
        assert!(win.contains("dir /s /b /a:-d \"/home/u/.claude/projects\\*.jsonl\" 2>nul"));
        assert!(win.contains(" & "));
    }

    /// 状态默认形状：两适配器槽位齐备（opencode/codex 票据 07 补入）
    #[test]
    fn default_state_shape() {
        let s = default_state();
        assert_eq!(s["status"], "idle");
        assert!(s["adapters"]["claude"].is_object());
        assert!(s["adapters"]["pi"].is_object());
        assert!(s["adapters"].get("opencode").is_none());
    }

    /// 事件 → wire 形状：token 明细 camelCase、ts 透传
    #[test]
    fn event_wire_shape() {
        let e = NormalizedEvent {
            ts: Some(1),
            role: "assistant",
            text: "t".to_string(),
            model: Some("m".to_string()),
            tokens: Some(crate::usage_parse::TokenUsage {
                input: 1,
                output: 2,
                cache_read: 3,
                cache_write: 4,
                reasoning: 5,
            }),
        };
        let v = event_to_json(&e);
        assert_eq!(v["role"], "assistant");
        assert_eq!(v["tokens"]["cacheRead"], 3);
        assert_eq!(v["tokens"]["reasoning"], 5);
        let no_tokens = event_to_json(&NormalizedEvent {
            ts: None,
            role: "user",
            text: "t".to_string(),
            model: None,
            tokens: None,
        });
        assert!(no_tokens["tokens"].is_null());
    }
}
