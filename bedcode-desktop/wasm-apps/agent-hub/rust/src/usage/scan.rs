//! 扫描异步流：枚举 → 回灌解析
//!
//! `scan-usage` 以 AUTH_KEY 为闸门（未授权降级 auth-required，避免弹窗风暴）
//! → host-process 平台分派枚举各来源根目录（`== 分段 ==` 标记）→
//! `on_process_done` 回灌：并行读文件（v20 host-task `execute-batch`）→
//! 逐文件水位判定 + 解析 + 落库 → opencode SQLite 同步 → 状态推送。

use super::active::compute_active_sessions;
use super::ingest::{
    file_stem, parse_by_adapter, sync_opencode, upsert_session, upsert_watermark,
    watermark_unchanged,
};
use super::schema::ensure_schema;
use super::{auth_granted, emit_and_return, read_state, write_state};
use super::{SCAN_TIMEOUT_MS, SESSION_ROOTS};
use crate::install::now_ms;
use crate::usage_parse::MAX_FILE_BYTES;
use crate::util::{is_windows, sh_quote, shell_invocation};
use crate::{host, pending, PendingRun, DATA_DIR, HOME};
use bedcode_plugin_api::events::ProcessDoneEvent;
use bedcode_plugin_api::host::{HostFs, HostLog, HostProcess};
use bedcode_plugin_api::host::{HostTask, TaskPlan, TaskUnit};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

static RUN_SEQ: AtomicU32 = AtomicU32::new(0);

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
    // 分段 = 内置 JSONL 来源（按当前 home 展开）+ 自定义来源（state 持久化绝对路径）。
    // opencode 的内置条目 kind=sqlite，不进枚举（它由 sync_opencode 单独同步取数）
    let sections = scan_sections(&home, &state);
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

/// 枚举分段构造（纯函数，可测）：内置 JSONL 根目录 + 自定义来源目录
///
/// **内置条目一律不取**（它们已在 [`SESSION_ROOTS`] 里按 home 展开），
/// 且 `kind == "sqlite"` 的条目不取——opencode 的库文件不是 `*.jsonl`，
/// 走 [`sync_opencode`] 同步查表而非文件枚举。
fn scan_sections(home: &str, state: &Value) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = SESSION_ROOTS
        .iter()
        .map(|(name, seg)| (name.to_string(), format!("{home}/{seg}")))
        .collect();
    if let Some(arr) = state.get("sources").and_then(|s| s.as_array()) {
        for src in arr {
            if src
                .get("builtin")
                .and_then(|b| b.as_bool())
                .unwrap_or(false)
            {
                continue;
            }
            if src.get("kind").and_then(|k| k.as_str()) == Some("sqlite") {
                continue;
            }
            let name = src.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let path = src.get("path").and_then(|p| p.as_str()).unwrap_or("");
            if !name.is_empty() && !path.is_empty() {
                sections.push((name.to_string(), path.to_string()));
            }
        }
    }
    sections
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

    // 按分段（来源名）分组汇总；分段名与状态 sources 条目名一致（内置 + 自定义）。
    // **SQLite 源（opencode）不入此累加器**：它不是文件枚举源，混进去会让
    // files/parsed 口径失真——它的槽位由 sync_opencode 单独写。
    let mut per_adapter: HashMap<String, (u32, u32, u32, u32)> = HashMap::new();
    if let Some(arr) = state.get("sources").and_then(|s| s.as_array()) {
        for src in arr {
            if src.get("kind").and_then(|k| k.as_str()) == Some("sqlite") {
                continue;
            }
            if let Some(name) = src.get("name").and_then(|n| n.as_str()) {
                per_adapter.insert(name.to_string(), (0u32, 0u32, 0u32, 0u32));
            }
        }
    }
    let now = now_ms(&h).unwrap_or(0);

    // 会话文件读内容：v20 host-task `execute-batch` **并行**（读是主流开销——
    // 大量 JSONL 日志文件逐个串行 fs_read 各占一次宿主调用；池线程真并发替代）。
    // **分片读→处理→释放（2026-09-28 OOM trap 修复）**：wasm 线性内存硬顶
    // 256MB，全量 listing（实机 430MB）一次驻留 + 解码副本必然 rust_oom abort；
    // 逐片读（≤ [`SCAN_SLICE_FILES`] 文件）→ 逐文件解析落库 → 释放再读下一片，
    // 峰值内存 = 单片内容。宿主单元结果 >1MB 截断（truncated）的大文件在
    // 逐文件循环里**串行全量重读**（`fs_read` 无截断），逐文件即释放不累积。
    // 解析与 DB 写保持串行：SQLite 连接单 Mutex，水位/upsert 依赖顺序处理。
    for slice in scan_slices(&listing) {
        let read_results = parallel_read(&h, slice);
        for (idx, (section, path)) in slice.iter().enumerate() {
            let Some(slot) = per_adapter.get_mut(section.as_str()) else {
                continue;
            };
            slot.0 += 1;
            // 逐文件水位：内容字节长未变 → 跳过解析（JSONL append-only）
            let content_opt = match &read_results[idx] {
                ReadOutcome::Content(opt) => opt.clone(),
                ReadOutcome::Truncated => {
                    // 大文件（宿主单元结果 >1MB 截断，产物非完整 JSON）：串行全量重读
                    match h.fs_read(path) {
                        Ok(opt) => opt,
                        Err(e) => {
                            h.log_warn(&format!("usage: read session file failed: {}", e.message));
                            continue;
                        }
                    }
                }
                ReadOutcome::Failed(e) => {
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
    }

    // opencode（SQLite 源，票 07）：不进上面的文件枚举，在 JSONL 回灌后
    // **同步**补一轮。失败隔离：任何错误只置 opencode 槽位 error 态
    // （带机器可读 code 供前端 i18n），另两家结果不受影响。
    sync_opencode(&h, &mut state, now);

    // 汇总各适配器结果进状态（per_adapter 不含 opencode，故不会覆写其槽位）
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

/// 扫描分片粒度（文件数/片）——**wasm 线性内存纪律（2026-09-28 OOM trap 修复）**：
/// 插件线性内存硬顶 256MB（宿主 `MAX_PLUGIN_MEMORY_BYTES`），而宿主
/// `execute-batch` 一次性返回整片全部单元结果，插件解码还会产生 2 份副本
/// （raw + parsed/results + 解码内容）。单片 32 文件 × 单元结果上限（~1MB）× 3
/// ≈ 96MB 峰值，安全余量充足；同时远低于宿主单 plan 单元数上限 256。
/// 全量 listing（实机 430MB）一次驻留必然 OOM。
const SCAN_SLICE_FILES: usize = 32;

/// 单文件读取结果（execute-batch 单元结果 → 调用方逐文件处理）
#[derive(Clone, Debug)]
enum ReadOutcome {
    /// 成功；None = 枚举与读取间隙被删除（文件不存在）
    Content(Option<String>),
    /// 宿主 task `fs.read` 单元结果被截断（结果 JSON 编码 >1MB，宿主按
    /// `PLUGIN_TASK_UNIT_RESULT_MAX_BYTES` 截断成**非完整 JSON**）——必须由调用方
    /// 经串行 [`WasmHost::fs_read`] 全量重读（该路径无截断）
    Truncated,
    /// 宿主读失败（批次级失败 / 权限缺失 / 响应损坏 / 单元级错误）
    Failed(String),
}

/// 分片：listing → 每片 ≤ [`SCAN_SLICE_FILES`] 的连续切片（保持原序）。
/// 纯函数（可测）：分片粒度是 OOM 修复的回归锁——单片「读→处理→释放」
/// 保证峰值内存 = 单片内容，与全量 listing 大小解耦。
fn scan_slices(listing: &[(String, String)]) -> Vec<&[(String, String)]> {
    listing.chunks(SCAN_SLICE_FILES).collect()
}

/// 并行读**一片**会话文件内容（v20 host-task `execute-batch`：`fs.read` 单元池线程
/// 真并发）。返回按入参顺序的结果。
///
/// **内存纪律**：wasm 线性内存 256MB 硬顶，宿主响应整片返回 + 解码副本——
/// 全量一次提交（此前 200 单元/片 × ~660KB 平均 ≈ 500MB 峰值）直接
/// `rust_oom` abort trap。调用方必须经 [`scan_slices`] 分片，逐片读→处理→释放。
/// 单元结果 >1MB 被宿主截断（`truncated` 标记，产物非完整 JSON）→
/// [`ReadOutcome::Truncated`]，由调用方逐文件串行全量重读（不累积驻留）。
/// 批次级失败（宿主拒绝 plan / 权限缺失 / 响应损坏）整体降级为逐条
/// [`ReadOutcome::Failed`]——调用方按原串行路径的 `log_warn + continue` 语义
/// 处理，不抛致命错误。
fn parallel_read(h: &WasmHost, slice: &[(String, String)]) -> Vec<ReadOutcome> {
    let units: Vec<TaskUnit> = slice
        .iter()
        .enumerate()
        .map(|(i, (_, path))| TaskUnit::fs_read(&format!("r{i}"), path))
        .collect();
    let raw = match HostTask::execute_batch(h, &TaskPlan::new(units).to_json()) {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("usage: batch read failed: {}", e.message);
            return (0..slice.len())
                .map(|_| ReadOutcome::Failed(msg.clone()))
                .collect();
        }
    };
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => {
            let msg = "usage: batch read: invalid host response".to_string();
            return (0..slice.len())
                .map(|_| ReadOutcome::Failed(msg.clone()))
                .collect();
        }
    };
    let results = parsed["results"].as_array().cloned().unwrap_or_default();
    (0..slice.len())
        .map(|i| decode_read_outcome(results.get(i)))
        .collect()
}

/// 单条单元结果 → [`ReadOutcome`]（纯函数，可测）：
/// - `truncated` 标记 → [`ReadOutcome::Truncated`]（截断产物非完整 JSON，禁解码）
/// - `ok == false` → [`ReadOutcome::Failed`]（单元级错误原样透传）
/// - `ok == true` 无 value → [`ReadOutcome::Content(None)`]（文件不存在）
/// - `ok == true` 有 value → 解码 `Option<String>`（value 是 JSON 编码字符串）
fn decode_read_outcome(entry: Option<&serde_json::Value>) -> ReadOutcome {
    let Some(entry) = entry else {
        return ReadOutcome::Failed("usage: batch read: missing unit result".to_string());
    };
    if entry["truncated"] == true {
        return ReadOutcome::Truncated;
    }
    if entry["ok"] != true {
        let err = entry["error"]
            .as_str()
            .unwrap_or("unknown batch unit error")
            .to_string();
        return ReadOutcome::Failed(err);
    }
    // fs.read 原返回 Option<String>：value 字段缺失 = None（文件不存在）；
    // 存在 = JSON 编码字符串（`"内容"`）
    match entry.get("value").and_then(|v| v.as_str()) {
        None => ReadOutcome::Content(None),
        Some(v) => match serde_json::from_str::<Option<String>>(v) {
            Ok(opt) => ReadOutcome::Content(opt),
            Err(e) => ReadOutcome::Failed(format!("usage: decode read result failed: {e}")),
        },
    }
}

// ==================== Tests（纯函数单测） ====================

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
        let quoted = scan_script(&[("q".to_string(), "/tmp/it's here".to_string())], false);
        assert!(quoted.contains("find '/tmp/it'\\''s here' -type f -name '*.jsonl'"));
    }

    /// 分片回归锁（2026-09-28 实机 bug 两个）：① 852 单元一次 execute-batch
    /// 被宿主拒绝（`plan exceeds max units per plan (852 > 256)`）→ 零扫描；
    /// ② 全量 430MB 内容一次驻留 + 解码副本 → wasm 线性内存 256MB 硬顶
    /// rust_oom abort trap（`on_process_done`）。断言：listing 切成多片、
    /// 每片 ≤ [`SCAN_SLICE_FILES`]（既是内存包络也远低于宿主 plan 上限 256）、
    /// 原序覆盖无丢失。
    #[test]
    fn scan_slices_bound_memory_and_plan_units() {
        // 空列表 → 无分片
        assert!(scan_slices(&[]).is_empty());

        // 边界：恰超一片 → 两片，尺寸 32 + 1
        let boundary: Vec<(String, String)> = (0..(SCAN_SLICE_FILES + 1))
            .map(|i| (format!("s{i}"), format!("/tmp/b{i}.jsonl")))
            .collect();
        let slices = scan_slices(&boundary);
        assert_eq!(slices.len(), 2);
        assert_eq!(slices[0].len(), SCAN_SLICE_FILES);
        assert_eq!(slices[1].len(), 1);

        // 实机规模（852 文件）：多片、每片不越内存包络（同时 < 宿主 plan 上限 256）
        let bulk: Vec<(String, String)> = (0..852)
            .map(|i| (format!("s{i}"), format!("/tmp/f{i}.jsonl")))
            .collect();
        let slices = scan_slices(&bulk);
        assert!(slices.len() > 1);
        for slice in &slices {
            assert!(!slice.is_empty());
            assert!(
                slice.len() <= SCAN_SLICE_FILES,
                "单片 {} 文件越分片粒度 {}",
                slice.len(),
                SCAN_SLICE_FILES
            );
            assert!(
                slice.len() <= 256,
                "单片 {} 文件越宿主 plan 单元上限",
                slice.len()
            );
        }

        // 原序覆盖：完整、无丢失、无乱序（调用方按此序逐文件回填）
        let flattened: Vec<&(String, String)> = slices.iter().copied().flatten().collect();
        assert_eq!(flattened.len(), bulk.len());
        for (i, item) in flattened.iter().enumerate() {
            assert_eq!(item.0, bulk[i].0);
            assert_eq!(item.1, bulk[i].1);
        }
    }

    /// 单元结果 → [`ReadOutcome`] 映射契约：
    /// truncated 标记优先（截断产物非完整 JSON，禁解码）；失败原样透传；
    /// value 缺失 = None；正常解码；缺失条目显性失败。
    #[test]
    fn decode_read_outcome_maps_unit_results() {
        // truncated 标记 → Truncated（即使 ok=true 也不解码）
        let truncated =
            json!({ "id": "r0", "ok": true, "value": "\"……[truncated]", "truncated": true });
        assert!(matches!(
            decode_read_outcome(Some(&truncated)),
            ReadOutcome::Truncated
        ));

        // 单元级失败 → Failed（错误原样透传）
        let unit_err =
            json!({ "id": "r1", "ok": false, "error": "fs error: file read failed: denied" });
        assert!(matches!(
            decode_read_outcome(Some(&unit_err)),
            ReadOutcome::Failed(e) if e == "fs error: file read failed: denied"
        ));

        // 成功但无 value（文件不存在）→ Content(None)
        let none = json!({ "id": "r2", "ok": true });
        assert!(matches!(
            decode_read_outcome(Some(&none)),
            ReadOutcome::Content(None)
        ));

        // 成功且有 value → 解码 Option<String>（value 是 JSON 编码字符串）
        let some = json!({ "id": "r3", "ok": true, "value": "\"hello\\n\"" });
        assert!(matches!(
            decode_read_outcome(Some(&some)),
            ReadOutcome::Content(Some(s)) if s == "hello\n"
        ));

        // 缺失条目 → Failed（防越界静默）
        assert!(matches!(decode_read_outcome(None), ReadOutcome::Failed(_)));
    }

    /// 枚举分段只含 JSONL 目录：opencode 的 SQLite 库不进文件枚举
    #[test]
    fn scan_sections_excludes_sqlite_source() {
        let state = json!({
            "sources": [
                { "name": "claude", "path": "/home/u/.claude/projects", "kind": "jsonl", "builtin": true },
                { "name": "codex", "path": "/home/u/.codex/sessions", "kind": "jsonl", "builtin": true },
                { "name": "opencode", "path": "/home/u/.local/share/opencode/opencode.db", "kind": "sqlite", "builtin": true },
                { "name": "my-logs", "path": "/data/logs", "kind": "jsonl", "builtin": false },
            ]
        });
        let sections = scan_sections("/home/u", &state);
        let names: Vec<&str> = sections.iter().map(|(n, _)| n.as_str()).collect();
        // 三内置 JSONL 根 + 一个自定义目录；opencode 不在其中
        assert_eq!(names, vec!["claude", "codex", "pi", "my-logs"]);
        assert!(!sections.iter().any(|(_, p)| p.contains("opencode.db")));

        // 自定义来源声明为 sqlite 也不进文件枚举（kind 是权威判据）
        let custom_sqlite = json!({
            "sources": [{ "name": "x-db", "path": "/data/x.db", "kind": "sqlite", "builtin": false }]
        });
        assert!(!scan_sections("/home/u", &custom_sqlite)
            .iter()
            .any(|(n, _)| n == "x-db"));
    }
}
