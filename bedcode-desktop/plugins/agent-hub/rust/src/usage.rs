//! 使用统计与会话日志域（票据 06 + 增补：正在使用的项目会话）
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
//! **正在使用的项目会话**（扫描收尾统一重算进 `state.activeSessions`）：
//! claude 读 `~/.claude.json` 配置（`projects[路径].lastStartTime` 最大者
//! 即当前项目，`lastSessionId` 即当前会话，比文件 mtime 权威；该文件不在
//! fs_auth `.claude/` 白名单段内，需随 `auth_dirs` 批量授权，未授权/损坏时
//! 静默回退最新会话）；pi 无配置指针，取各自最新会话（started_at 最大）
//! 兜底。列表接口按 (adapter, cli_session_id) 命中注入 `active: true`。
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

use super::{host, is_windows, path_rejected_for_script, pending, sh_quote, shell_invocation, PendingRun, AUTH_KEY, DATA_DIR, HOME};
use crate::install::now_ms;
use bedcode_plugin_api::host::{HostTask, TaskPlan, TaskUnit};
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
        // 供应商预设（票据 05 预留；api_key 中心凭据列由 providers::ensure_schema
        // 幂等迁移补齐，此处只保证表存在）
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

/// 内置来源（(adapter, 家目录相对段)）→ 家目录绝对路径条目（只读）
fn builtin_sources() -> Vec<Value> {
    let home = HOME.get().cloned().unwrap_or_default();
    SESSION_ROOTS
        .iter()
        .map(|(name, seg)| {
            json!({ "name": name, "path": format!("{home}/{seg}"), "builtin": true })
        })
        .collect()
}

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
        // 日志来源清单（内置只读 + 自定义增删）；旧状态无此键由 read_state 补齐
        "sources": json!(builtin_sources()),
        // 正在使用的项目会话（扫描时计算：claude 配置权威 / 其余最新会话）
        "activeSessions": json!({}),
    })
}

fn read_state(h: &WasmHost) -> Value {
    let mut state = h
        .storage_get(USAGE_KEY)
        .ok()
        .flatten()
        .filter(|s| s.get("adapters").is_some())
        .unwrap_or_else(default_state);
    // 票 06 旧状态无 sources：增量注入内置来源（新能力对旧状态兼容）
    if !state.get("sources").is_some() {
        state["sources"] = json!(builtin_sources());
    }
    // 票 06 增补前旧状态无 activeSessions：默认空映射（下次扫描触发计算）
    if !state.get("activeSessions").is_some() {
        state["activeSessions"] = json!({});
    }
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
    // 分段 = 内置来源（按当前 home 展开）+ 自定义来源（state 持久化绝对路径）
    let mut sections: Vec<(String, String)> = SESSION_ROOTS
        .iter()
        .map(|(name, seg)| (name.to_string(), format!("{home}/{seg}")))
        .collect();
    if let Some(arr) = state.get("sources").and_then(|s| s.as_array()) {
        for src in arr {
            if src.get("builtin").and_then(|b| b.as_bool()).unwrap_or(false) {
                continue;
            }
            let name = src.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let path = src.get("path").and_then(|p| p.as_str()).unwrap_or("");
            if !name.is_empty() && !path.is_empty() {
                sections.push((name.to_string(), path.to_string()));
            }
        }
    }
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

/// 枚举脚本：分段输出各来源根目录下的 *.jsonl 文件（平台分派，与
/// skills scan_script 同款 `== 分段 ==` 标记；根目录不存在属常态）
///
/// 安全：unix 根路径经 `sh_quote` 单引号转义（自定义来源路径是用户可控输入，
/// 未经转义直接插双引号即可被 `$()` / 反引号注入命令）；Windows 根路径保留
/// 双引号包裹（cmd 引号内 & | < > 按字面处理），`"` 与 `%` 已在
/// add_source 入口拒绝（见 `path_rejected_for_script`）。
fn scan_script(sections: &[(String, String)], windows: bool) -> String {
    let mut parts: Vec<String> = vec![];
    for (key, root) in sections {
        if windows {
            parts.push(format!(
                "echo == {key} == & dir /s /b /a:-d \"{root}\\*.jsonl\" 2>nul"
            ));
        } else {
            parts.push(format!(
                "echo '== {key} =='\nfind {} -type f -name '*.jsonl' 2>/dev/null\n",
                sh_quote(root)
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

    // 按分段（来源名）分组汇总；分段名与状态 sources 条目名一致（内置 + 自定义）
    let mut per_adapter: HashMap<String, (u32, u32, u32, u32)> = HashMap::new();
    if let Some(arr) = state.get("sources").and_then(|s| s.as_array()) {
        for src in arr {
            if let Some(name) = src.get("name").and_then(|n| n.as_str()) {
                per_adapter.insert(name.to_string(), (0u32, 0u32, 0u32, 0u32));
            }
        }
    }
    let now = now_ms(&h).unwrap_or(0);

    // 会话文件读内容：v20 host-task `execute-batch` **并行**（读是主流开销——
    // 大量 JSONL 日志文件逐个串行 fs_read 各占一次宿主调用；池线程真并发替代）。
    // 解析与 DB 写保持串行：SQLite 连接单 Mutex，水位/upsert 依赖顺序处理。
    let read_results = parallel_read(&h, &listing);

    for (idx, (section, path)) in listing.iter().enumerate() {
        let Some(slot) = per_adapter.get_mut(section.as_str()) else {
            continue;
        };
        slot.0 += 1;
        // 逐文件水位：内容字节长未变 → 跳过解析（JSONL append-only）
        let content_opt = match &read_results[idx] {
            Ok(opt) => opt.clone(),
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
        let Some(mut parsed) = parse_by_adapter(section, &content) else {
            continue;
        };
        if parsed.cli_session_id.is_empty() {
            // 无会话头（未知格式自定义来源）：以文件名兜底，保证可入库与详情定位
            if let Some(stem) = file_stem(path) {
                parsed.cli_session_id = format!("file:{stem}");
                if parsed.title.is_none() {
                    parsed.title = Some(stem.to_string());
                }
            }
        }
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
    // 正在使用的项目会话：每次扫描后统一重算（claude 读 ~/.claude.json 配置，
    // 其余适配器取各自最新会话；配置不可读时回退最新会话）
    state["activeSessions"] = json!(compute_active_sessions(&h));
    state["error"] = json!(null);
    state["status"] = json!("ok");
    state["syncedAt"] = json!(now_ms(&h).ok());
    write_state(&h, &state);
    h.log_info("usage scan finished");
    emit_and_return(&h, &state).map(|_| ())
}

/// 并行读全部会话文件内容（v20 host-task `execute-batch`：`fs.read` 单元池线程
/// 真并发）。返回按入参顺序的结果：`Ok(Some(content))` / `Ok(None)`（枚举与读取
/// 间隙被删除）/ `Err`（宿主读失败，文案与 [`WasmHost::fs_read`] 同源）。
/// 批次级失败（宿主拒绝 plan / 权限缺失 / 响应损坏）整体降级为逐条 Err——
/// 调用方按原串行路径的 `log_warn + continue` 语义处理，不抛致命错误。
fn parallel_read(
    h: &WasmHost,
    listing: &[(String, String)],
) -> Vec<Result<Option<String>, String>> {
    if listing.is_empty() {
        return Vec::new();
    }
    let units: Vec<TaskUnit> = listing
        .iter()
        .enumerate()
        .map(|(i, (_, path))| TaskUnit::fs_read(&format!("r{i}"), path))
        .collect();
    let raw = match HostTask::execute_batch(h, &TaskPlan::new(units).to_json()) {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("usage: batch read failed: {}", e.message);
            return (0..listing.len()).map(|_| Err(msg.clone())).collect();
        }
    };
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => {
            let msg = "usage: batch read: invalid host response".to_string();
            return (0..listing.len()).map(|_| Err(msg.clone())).collect();
        }
    };
    let results = parsed["results"].as_array().cloned().unwrap_or_default();
    let mut out = Vec::with_capacity(listing.len());
    for i in 0..listing.len() {
        let entry = results.get(i).cloned().unwrap_or(serde_json::Value::Null);
        if entry["ok"] == true {
            // fs.read 原返回 Option<String>：value 字段缺失 = None（文件不存在）；
            // 存在 = JSON 编码字符串（`"内容"`）
            match entry.get("value").and_then(|v| v.as_str()) {
                None => out.push(Ok(None)),
                Some(v) => match serde_json::from_str::<Option<String>>(v) {
                    Ok(opt) => out.push(Ok(opt)),
                    Err(e) => out.push(Err(format!("usage: decode read result failed: {e}"))),
                },
            }
        } else {
            let err = entry["error"]
                .as_str()
                .unwrap_or("unknown batch unit error")
                .to_string();
            out.push(Err(err));
        }
    }
    out
}

// ==================== 适配器分派 ====================

/// 按适配器名分派解析（扫描回灌与会话日志打开共用；内置格式直连解析器，
/// 自定义来源走内容嗅探，未知格式退化为「仅原始行会话」）
fn parse_by_adapter(adapter: &str, content: &str) -> Option<ParsedSession> {
    match adapter {
        "claude" => Some(parse_claude_session(content)),
        "pi" => Some(parse_pi_session(content)),
        _ => Some(parse_sniffed_session(content)),
    }
}

/// 自定义来源格式嗅探：前 64 行内顶层 `"type":"message"` → pi 解析器；
/// 顶层 `"type":"assistant"/"user"` → claude 解析器；其余未知格式退化为
/// 空事件会话（cli_session_id 由调用方按文件名补足，详情仅原始行可读）。
fn parse_sniffed_session(content: &str) -> ParsedSession {
    let head: Vec<&str> = content.lines().take(64).collect();
    let has_top_type = |kind: &str| {
        head.iter()
            .any(|l| l.contains(&format!("\"type\":\"{kind}\"")))
    };
    if has_top_type("message") {
        return parse_pi_session(content);
    }
    if has_top_type("assistant") || has_top_type("user") {
        return parse_claude_session(content);
    }
    ParsedSession::default()
}

/// 路径文件名兜底会话 id/标题（去 .jsonl 后缀；未知/损坏路径返回 None）
fn file_stem(path: &str) -> Option<&str> {
    let base = path.rsplit(['/', '\\']).next()?;
    let stem = base.strip_suffix(".jsonl").unwrap_or(base);
    if stem.is_empty() {
        None
    } else {
        Some(stem)
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

// ==================== 正在使用的项目会话（扫描配置 + 最新回退） ====================

/// claude 配置解析（纯函数，可测）：读 `~/.claude.json` 的 `projects` 映射
/// （key=项目绝对路径 → { lastSessionId, lastStartTime }），返回最后启动
/// 时间（epoch 毫秒）最大的项目 → (项目路径, lastSessionId)。
/// 缺键/非数字的条目跳过；无有效条目返回 None。
fn claude_active_from_config(projects: &Value) -> Option<(String, String)> {
    let obj = projects.as_object()?;
    let mut best: Option<(String, String, i64)> = None;
    for (proj, meta) in obj {
        let sid = meta.get("lastSessionId").and_then(|v| v.as_str());
        let ts = meta.get("lastStartTime").and_then(|v| v.as_i64());
        match (sid, ts) {
            (Some(sid), Some(ts)) if best.as_ref().map_or(true, |(_, _, t)| ts > *t) => {
                best = Some((proj.clone(), sid.to_string(), ts));
            }
            _ => {}
        }
    }
    best.map(|(proj, sid, _)| (proj, sid))
}

/// 正在使用的项目会话（键=适配器 → { project, session_id }，无则 null）：
/// claude 优先读 `~/.claude.json` 配置（lastStartTime 最大者为当前项目会话，
/// 比文件 mtime 权威；未授权/损坏时跳过并回退）；pi 无配置指针，取各自
/// 最新会话（started_at 最大）兜底。
fn compute_active_sessions(h: &WasmHost) -> Value {
    let mut active = serde_json::Map::new();
    for adapter in ADAPTERS {
        // 回退：该适配器已入库的最新会话
        let mut entry = h
            .plugin_db_query_params(
                "SELECT project, cli_session_id FROM usage_session \
                 WHERE adapter = ?1 ORDER BY started_at DESC, id DESC LIMIT 1",
                &sql_params![adapter],
            )
            .ok()
            .flatten()
            .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
            .and_then(|row| {
                let project = row
                    .get("project")
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string());
                row.get("cli_session_id")
                    .and_then(|c| c.as_str())
                    .map(|sid| json!({ "project": project, "session_id": sid.to_string() }))
            })
            .unwrap_or(Value::Null);
        if adapter == "claude" {
            if let Some(home) = HOME.get() {
                let claude_json = format!("{home}/.claude.json");
                match h.fs_read(&claude_json) {
                    Ok(Some(content)) => match serde_json::from_str::<Value>(&content) {
                        Ok(cfg) => {
                            if let Some((proj, sid)) =
                                cfg.get("projects").and_then(claude_active_from_config)
                            {
                                entry = json!({ "project": proj, "session_id": sid });
                            }
                        }
                        Err(e) => h.log_warn(&format!(
                            "usage: parse {claude_json} failed, fallback to newest session: {e}"
                        )),
                    },
                    // 未授权（fs_auth 拒绝）或文件缺失：回退最新会话，不中断扫描
                    Ok(None) | Err(_) => {}
                }
            }
        }
        active.insert(adapter.to_string(), entry);
    }
    Value::Object(active)
}

/// 列表行注入 active 标记（纯函数，可测）：命中 `activeSessions[adapter]
/// .session_id` 的行置 `active: true`；无标记/不匹配保持原样。
fn mark_active_rows(rows: &mut Value, active: &Value) {
    let Some(arr) = rows.as_array_mut() else {
        return;
    };
    for row in arr.iter_mut() {
        let adapter = row.get("adapter").and_then(|v| v.as_str()).unwrap_or("");
        let sid = row.get("cli_session_id").and_then(|v| v.as_str()).unwrap_or("");
        if adapter.is_empty() || sid.is_empty() {
            continue;
        }
        let hit = active
            .get(adapter)
            .and_then(|e| e.as_object())
            .and_then(|e| e.get("session_id"))
            .and_then(|v| v.as_str())
            .map(|a| a == sid)
            .unwrap_or(false);
        if hit {
            row["active"] = json!(true);
        }
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

/// 分页会话列表（started_at 倒序）；多条件查询：adapter / 关键词 / 时间范围
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
    let q = args
        .get("q")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let from = args.get("from").and_then(|v| v.as_i64());
    let to = args.get("to").and_then(|v| v.as_i64());

    // 动态 WHERE：占位符按参数数组顺序编号（宿主按 1-based 顺序绑定）
    let mut clauses: Vec<String> = vec![];
    let mut params: Vec<serde_json::Value> = vec![];
    if !adapter.is_empty() {
        clauses.push("adapter = ?".to_string());
        params.push(serde_json::Value::String(adapter.to_string()));
    }
    if !q.is_empty() {
        let like = format!("%{q}%");
        clauses.push("(title LIKE ? OR project LIKE ? OR cli_session_id LIKE ?)".to_string());
        params.push(serde_json::Value::String(like.clone()));
        params.push(serde_json::Value::String(like.clone()));
        params.push(serde_json::Value::String(like));
    }
    if let Some(f) = from {
        clauses.push("started_at >= ?".to_string());
        params.push(serde_json::json!(f));
    }
    if let Some(t) = to {
        clauses.push("started_at <= ?".to_string());
        params.push(serde_json::json!(t));
    }
    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    // LIST 的 LIMIT/OFFSET 占位符排在 WHERE 参数之后
    let mut list_params = params.clone();
    list_params.push(serde_json::json!(limit));
    list_params.push(serde_json::json!(offset));
    let mut rows = h
        .plugin_db_query_params(
            &format!(
                "SELECT id, adapter, cli_session_id, project, title, started_at, ended_at, \
                        duration_ms, model, tokens_in, tokens_out, tokens_cache_read, \
                        tokens_cache_write, tokens_reasoning, cost_total \
                 FROM usage_session{where_sql} ORDER BY started_at DESC LIMIT ? OFFSET ?"
            ),
            &list_params,
        )
        .ok()
        .flatten()
        .unwrap_or(json!([]));

    // 正在使用的项目会话标记（扫描时存 state，查询时按适配器+会话 id 注入）
    mark_active_rows(&mut rows, &read_state(h)["activeSessions"]);
    let total = h
        .plugin_db_query_params(
            &format!("SELECT COUNT(*) AS n FROM usage_session{where_sql}"),
            &params,
        )
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

// ==================== 日志来源管理（内置只读 + 自定义增删） ====================

/// 来源清单 + 各适配器扫描计数（state 持久化；内置条目只读）
pub(crate) fn list_sources(h: &WasmHost) -> anyhow::Result<Value> {
    let state = read_state(h);
    let mut out: Vec<Value> = vec![];
    if let Some(arr) = state.get("sources").and_then(|s| s.as_array()) {
        for src in arr {
            let mut s = src.clone();
            if let Some(name) = s.get("name").and_then(|n| n.as_str()) {
                if let Some(stat) = state.get("adapters").and_then(|a| a.get(name)) {
                    s["scan"] = stat.clone();
                }
            }
            out.push(s);
        }
    }
    Ok(json!({ "sources": out }))
}

/// 来源名合法性：小写字母开头，字母/数字/连字符，≤ 32
fn is_valid_source_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && name.len() <= 32
}

/// 添加自定义来源：名称 + 目录（绝对路径或 ~/ 开头）入态，随后由前端引导扫描
pub(crate) fn add_source(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let raw_path = args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() || raw_path.is_empty() {
        return Err(anyhow::anyhow!("add-source: name and path required"));
    }
    if !is_valid_source_name(&name) {
        return Err(anyhow::anyhow!(
            "add-source: invalid name (lowercase letters / digits / hyphen)"
        ));
    }
    // ~ 展开为绝对路径（与 builtin path 展示形态一致，便于去重）
    let path = if let Some(rest) = raw_path.strip_prefix("~/") {
        let home = HOME
            .get()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("add-source: home unavailable"))?;
        format!("{home}/{rest}")
    } else {
        raw_path
    };
    if !path.starts_with('/') {
        return Err(anyhow::anyhow!(
            "add-source: path must be absolute (or start with ~/)"
        ));
    }
    if path_rejected_for_script(&path) {
        return Err(anyhow::anyhow!(
            "add-source: path contains characters unsupported by scan scripts"
        ));
    }

    let mut state = read_state(h);
    let dup = state
        .get("sources")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter().any(|s| {
                s.get("name").and_then(|n| n.as_str()) == Some(name.as_str())
                    || s.get("path").and_then(|p| p.as_str()) == Some(path.as_str())
            })
        })
        .unwrap_or(false);
    if dup {
        return Err(anyhow::anyhow!(
            "add-source: name or path already registered"
        ));
    }
    // 适配器槽位 + 来源条目
    if let Some(adapters) = state.get_mut("adapters").and_then(|a| a.as_object_mut()) {
        adapters.insert(
            name.clone(),
            json!({ "files": 0, "parsed": 0, "skipped": 0, "sessions": 0, "error": null }),
        );
    }
    state["sources"] = {
        let mut arr = state
            .get("sources")
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default();
        arr.push(json!({ "name": name.clone(), "path": path, "builtin": false }));
        json!(arr)
    };
    write_state(h, &state);
    emit_and_return(h, &state)
}

/// 删除自定义来源（内置只读拒绝）；移出来源清单并清理适配器槽位
pub(crate) fn remove_source(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        return Err(anyhow::anyhow!("remove-source: name required"));
    }
    let mut state = read_state(h);
    let builtin = state
        .get("sources")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter().any(|s| {
                s.get("name").and_then(|n| n.as_str()) == Some(name.as_str())
                    && s.get("builtin").and_then(|b| b.as_bool()) == Some(true)
            })
        })
        .unwrap_or(false);
    if builtin {
        return Err(anyhow::anyhow!(
            "remove-source: builtin sources cannot be removed"
        ));
    }
    state["sources"] = json!(state
        .get("sources")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|s| s.get("name").and_then(|n| n.as_str()) != Some(name.as_str()))
                .cloned()
                .collect::<Vec<Value>>()
        })
        .unwrap_or_default());
    if let Some(adapters) = state.get_mut("adapters").and_then(|a| a.as_object_mut()) {
        adapters.remove(&name);
    }
    write_state(h, &state);
    emit_and_return(h, &state)
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
            ("claude".to_string(), "/home/u/.claude/projects".to_string()),
            ("pi".to_string(), "/home/u/.pi/agent/sessions".to_string()),
        ];
        let unix = scan_script(&sections, false);
        assert!(unix.contains("== claude =="));
        assert!(unix.contains("== pi =="));
        assert!(unix.contains("find '/home/u/.claude/projects' -type f -name '*.jsonl'"));
        assert!(unix.contains("2>/dev/null"));

        let win = scan_script(&sections, true);
        assert!(win.contains("dir /s /b /a:-d \"/home/u/.claude/projects\\*.jsonl\" 2>nul"));
        assert!(win.contains(" & "));
    }

    /// 注入防护：恶意路径（$() 命令替换 / 分号链）进单引号后仅作为 find 参数；
    /// 单引号路径经 '\'' 转义后不逃逸包裹
    #[test]
    fn scan_script_quotes_malicious_roots() {
        let sections = vec![(
            "evil".to_string(),
            "/tmp/a;$(touch /tmp/pwned);b".to_string(),
        )];
        let unix = scan_script(&sections, false);
        // $() 序列整体被单引号包裹：脚本输出逐字节比对，证明序列未逃逸
        assert_eq!(
            unix,
            "echo '== evil =='\nfind '/tmp/a;$(touch /tmp/pwned);b' -type f -name '*.jsonl' 2>/dev/null\n"
        );

        // 路径含单引号：'\'' 转义后不逃逸包裹，整体仍为单引号字符串
        let quoted = scan_script(
            &[("q".to_string(), "/tmp/it's here".to_string())],
            false,
        );
        assert!(quoted.contains("find '/tmp/it'\\''s here' -type f -name '*.jsonl'"));
    }

    /// 状态默认形状：两内置适配器槽位 + 内置来源清单齐备（自定义来源待添加）
    #[test]
    fn default_state_shape() {
        let s = default_state();
        assert_eq!(s["status"], "idle");
        assert!(s["adapters"]["claude"].is_object());
        assert!(s["adapters"]["pi"].is_object());
        assert!(s["adapters"].get("opencode").is_none());
        let sources = s["sources"].as_array().expect("sources array");
        assert_eq!(sources.len(), 2);
        assert!(sources.iter().all(|x| x["builtin"] == json!(true)));
        assert!(sources.iter().any(|x| x["name"] == json!("claude")));
        // 正在使用的项目会话：默认空映射（扫描收尾重算）
        assert!(s["activeSessions"].as_object().map(|o| o.is_empty()).unwrap_or(false));
    }

    /// claude 配置解析：取 lastStartTime 最大项目；缺键/非数字条目跳过
    #[test]
    fn claude_active_picks_latest_project() {
        let projects = json!({
            "/home/u/old": { "lastSessionId": "s-1", "lastStartTime": 1000 },
            "/home/u/new": { "lastSessionId": "s-2", "lastStartTime": 3000 },
            // 缺 lastStartTime / lastSessionId 的条目跳过，不中断
            "/home/u/broken-a": { "lastSessionId": "s-3" },
            "/home/u/broken-b": { "lastStartTime": 5000 },
        });
        let got = claude_active_from_config(&projects);
        assert_eq!(got, Some(("/home/u/new".to_string(), "s-2".to_string())));
        // 空/非对象输入 → None
        assert_eq!(claude_active_from_config(&json!([])), None);
        assert_eq!(claude_active_from_config(&json!({})), None);
    }

    /// 列表注入 active 标记：按 (adapter, cli_session_id) 命中；不匹配行不动
    #[test]
    fn mark_active_rows_by_adapter_session() {
        let mut rows = json!([
            { "adapter": "claude", "cli_session_id": "s-1", "title": "a" },
            { "adapter": "claude", "cli_session_id": "s-2", "title": "b" },
            { "adapter": "pi", "cli_session_id": "s-1", "title": "c" },
            { "adapter": "custom", "cli_session_id": "s-9", "title": "d" },
        ]);
        let active = json!({
            "claude": { "project": "/home/u/new", "session_id": "s-2" },
            "pi": null,
        });
        mark_active_rows(&mut rows, &active);
        let arr = rows.as_array().expect("array");
        // 命中标记仅限同适配器同会话 id（claude/s-1 不标记；pi 条目为 null 不标记）
        assert!(arr[0].get("active").is_none());
        assert_eq!(arr[1]["active"], json!(true));
        assert!(arr[2].get("active").is_none());
        assert!(arr[3].get("active").is_none());
        // 空 activeSessions → 无标记
        let mut rows2 = json!([{ "adapter": "claude", "cli_session_id": "s-2" }]);
        mark_active_rows(&mut rows2, &json!({}));
        assert!(rows2[0].get("active").is_none());
    }

    /// 来源名合法性：小写字母开头 / 字母数字连字符 / 超长拒绝
    #[test]
    fn source_name_validation() {
        assert!(is_valid_source_name("opencode"));
        assert!(is_valid_source_name("my-logs2"));
        assert!(!is_valid_source_name(""));
        assert!(!is_valid_source_name("MyLog"));
        assert!(!is_valid_source_name("2logs"));
        assert!(!is_valid_source_name("logs!/x"));
        assert!(!is_valid_source_name(&"a".repeat(40)));
    }

    /// 文件名兜底：去 .jsonl 后缀、合法 stem、损坏路径 None
    #[test]
    fn session_file_stem() {
        assert_eq!(file_stem("/a/b/2024-01-01.jsonl"), Some("2024-01-01"));
        assert_eq!(file_stem("C:\\x\\y.jsonl"), Some("y"));
        assert_eq!(file_stem("/a/.jsonl"), None);
        assert_eq!(file_stem(""), None);
    }

    /// 格式嗅探：pi / claude 标记命中各自解析器，未知格式退化为空事件会话
    #[test]
    fn sniffed_session_formats() {
        // pi：顶层 type=message
        let pi = parse_sniffed_session(
            "{\"type\":\"session\",\"id\":\"s1\"}\n{\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}}\n",
        );
        assert_eq!(pi.cli_session_id, "s1");
        assert!(pi.events.len() >= 1);

        // claude：顶层 type=user / assistant（真实数据 user content 为字符串）
        let claude = parse_sniffed_session(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"},\"timestamp\":\"2026-01-01T00:00:00Z\"}\n",
        );
        assert!(claude.events.len() >= 1);

        // 未知格式：无事件、无会话 id（由调用方按文件名兜底）
        let unknown = parse_sniffed_session("[not-json-line]\n{\"foo\":1}\n");
        assert!(unknown.events.is_empty());
        assert!(unknown.cli_session_id.is_empty());
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
