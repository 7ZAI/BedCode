//! Skills 管理域（票据 04）
//!
//! 真源模型：`~/.agents/skills` 为规范库；编辑 / GitHub 安装 / 本地导入都落
//! 规范库；**分发** = 逐文件复制到各 CLI 私有目录（claude `~/.claude/skills`、
//! pi `~/.pi/agent/skills`），以逐文件内容 hash 比对检测**副本落后**并一键
//! 重新分发（opencode/codex 无 skills 目录约定，不在 v1 分发面）。
//!
//! 目录枚举经 host-process 平台分派（WIT host-fs 无列举原语）：unix
//! `find -type f` / Windows `dir /s /b /a:-d`，沿用 detect.rs 的
//! `== 分段 ==` 标记输出。根目录不存在属常态，扫描按输出解析推进、
//! **不以 exit code 判失败**，仅超时视为失败。
//!
//! # 模块结构
//! - [`listing`]：列举解析 / 内容 hash / frontmatter / 分发判定（纯函数）
//! - [`scan`]：规范库扫描（枚举 → 逐 skill 采集 → 分发比对）
//! - [`editor`]：读 / 保存 / 分发命令
//! - [`github`]：GitHub 安装（URL 解析 → trees API → raw 下载）
//! - [`import`]：本地目录导入（pick-folder + 授权 → 枚举 → fs_copy）
//! - 本模块：域状态（host-storage `skills` 键，读-改-写 + 全量推送）+ 命令入口
//!
//! 编辑冲突检测用「保存前重读 + 内容比对」：WIT 无 stat 原语（mtime 不可得），
//! 内容比对严格更强（spec 的 mtime/hash 二选一取 hash 语义）。GitHub 安装走
//! JSON API + raw 文本下载（非流式 host-http 响应体强制 UTF-8，二进制
//! tarball 不可行；非 UTF-8 文件跳过记录，不可达报错提示代理/镜像）。

/// host-storage 键：Skills 域复合状态
pub(crate) const SKILLS_KEY: &str = "skills";
/// 分发目标白名单（v1：claude / pi 家级私有目录），值为家目录相对段
pub(super) const TARGET_SEGS: [(&str, &str); 2] =
    [("claude", ".claude/skills"), ("pi", ".pi/agent/skills")];
/// 规范库家目录相对段
pub(super) const LIBRARY_SEG: &str = ".agents/skills";
/// 扫描/导入枚举超时：目录列举为纯文件系统遍历，30s 上限
pub(super) const SCAN_TIMEOUT_MS: u64 = 30_000;
/// GitHub raw 跳过名单回显上限（全部计数在 skippedFiles）
pub(super) const SKIPPED_SAMPLE_CAP: usize = 20;

pub(super) static RUN_SEQ: AtomicU32 = AtomicU32::new(0);
// ==================== 状态（读-改-写） ====================

/// 分发目标根目录表：[(目标名, 绝对根)]
pub(super) fn target_roots(home: &str) -> Vec<(&'static str, String)> {
    TARGET_SEGS
        .iter()
        .map(|(name, seg)| (*name, format!("{home}/{seg}")))
        .collect()
}

fn default_state(home: &str) -> Value {
    let mut targets = serde_json::Map::new();
    for (name, root) in target_roots(home) {
        targets.insert(name.to_string(), json!({ "root": root, "exists": false }));
    }
    json!({
        "status": "idle",
        "error": null,
        "scannedAt": null,
        "libraryRoot": format!("{home}/{LIBRARY_SEG}"),
        "importing": false,
        "skills": [],
        "targets": Value::Object(targets),
        "github": { "last": null },
        "import": { "last": null },
    })
}

pub(super) fn read_state(h: &WasmHost) -> Value {
    let home = HOME.get().map(|s| s.as_str()).unwrap_or("");
    h.storage_get(SKILLS_KEY)
        .ok()
        .flatten()
        .filter(|s| s.get("libraryRoot").is_some())
        .unwrap_or_else(|| default_state(home))
}

pub(super) fn write_state(h: &WasmHost, state: &Value) {
    if let Err(e) = h.storage_set(SKILLS_KEY, state) {
        h.log_warn(&format!("skills: persist state failed: {e}"));
    }
}

/// 全量状态推送前端（命令返回值与事件载荷同形）
pub(super) fn emit_and_return(h: &WasmHost, state: &Value) -> anyhow::Result<Value> {
    h.emit_event("plugin:agent-hub:skills", state);
    Ok(json!({ "state": state }))
}

pub(super) fn library_root() -> anyhow::Result<String> {
    HOME.get()
        .map(|home| format!("{home}/{LIBRARY_SEG}"))
        .ok_or_else(|| anyhow::anyhow!("skills: home unavailable"))
}
mod editor;
mod github;
mod import;
mod listing;
mod scan;

/// 命令入口面（lib.rs 路由）
pub(crate) use editor::{distribute, read_skill, save_skill};
pub(crate) use github::install_github;
pub(crate) use import::import_local;
pub(crate) use listing::parse_listing;
pub(crate) use scan::scan;

use super::{host, pending, HOME};
use bedcode_plugin_api::events::ProcessDoneEvent;
use bedcode_plugin_api::host::{HostEvents, HostLog, HostStorage};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};
use std::sync::atomic::AtomicU32;

use self::import::handle_import_done;
use self::scan::handle_scan_done;

// ==================== 命令入口 ====================

/// 读取 Skills 域状态（前端挂载时拉取）
pub(crate) fn get_state(h: &WasmHost) -> anyhow::Result<Value> {
    emit_and_return(h, &read_state(h))
}

/// 进程完成统一分派（lib.rs 按前缀路由）：先按 run_id 移除 pending 归属，
/// 再按 kind 分发（迟到/外部回调直接放行）
pub(crate) fn handle_process_done(event: &ProcessDoneEvent, kind: &str) -> anyhow::Result<()> {
    let removed = pending()
        .lock()
        .map_err(|e| anyhow::anyhow!("process-done: poisoned pending map: {e}"))?
        .remove(&event.run_id);
    let Some(entry) = removed else {
        return Ok(());
    };
    match kind {
        "skills-scan" => handle_scan_done(event, &entry.output_path),
        "skills-import" => {
            let src = entry.source.unwrap_or_default();
            handle_import_done(event, &entry.output_path, &src)
        }
        other => {
            host().log_warn(&format!("skills: unknown process kind {other}"));
            Ok(())
        }
    }
}
