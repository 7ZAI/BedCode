//! Skills 管理域（票据 04）
//!
//! 真源模型：`~/.agents/skills` 为规范库；编辑 / GitHub 安装 / 本地导入都落
//! 规范库；**分发** = 逐文件复制到各 CLI 私有目录（claude `~/.claude/skills`、
//! pi `~/.pi/agent/skills`），以逐文件内容 hash 比对检测**副本落后**并一键
//! 重新分发（opencode/codex 无 skills 目录约定，不在 v1 分发面）。
//!
//! 目录枚举经 host-process 平台分派（WIT host-fs 无列举原语，spec §3 只用
//! 既有原语）：unix `find -type f` / Windows `dir /s /b /a:-d`，沿用 detect.rs
//! 的 `== 分段 ==` 标记输出。根目录不存在属常态（未装/未初始化），扫描按输出
//! 解析推进、**不以 exit code 判失败**（区别于 detect.rs），仅超时视为失败。
//!
//! 编辑冲突检测用「保存前重读 + 内容比对」：WIT 无 stat 原语（mtime 不可得），
//! 内容比对严格更强（任何外部改动都会反映在内容上）；spec 的 mtime/hash 二选一
//! 取 hash 语义。保存前 diff 预览由前端计算（LCS 行 diff，`utils/diff.ts`）。
//!
//! GitHub 安装：非流式 host-http 响应体强制 UTF-8（宿主 http.rs），二进制
//! tarball 不可行 → 走 GitHub JSON API（repo 默认分支 + recursive trees 一次
//! 拿全量路径）+ raw.githubusercontent.com 文本下载；非 UTF-8 文件（图片等）
//! 跳过并记录。不可达时报错落状态，前端提示代理/镜像（v1 仅提示）。
//!
//! 状态为单一真源（host-storage `skills` 键，读-改-写），每次变更全量 emit
//! `plugin:agent-hub:skills` 推送前端。

use super::{host, is_windows, path_rejected_for_script, pending, sh_quote, shell_invocation, PendingRun, DATA_DIR, HOME};
use crate::install::now_ms;
use bedcode_plugin_api::events::ProcessDoneEvent;
use bedcode_plugin_api::host::{
    HostEvents, HostFs, HostHttp, HostLog, HostPlatform, HostProcess, HostStorage,
};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

/// host-storage 键：Skills 域复合状态
pub(crate) const SKILLS_KEY: &str = "skills";
/// 分发目标白名单（v1：claude / pi 家级私有目录），值为家目录相对段
const TARGET_SEGS: [(&str, &str); 2] = [("claude", ".claude/skills"), ("pi", ".pi/agent/skills")];
/// 规范库家目录相对段
const LIBRARY_SEG: &str = ".agents/skills";
/// 扫描/导入枚举超时：目录列举为纯文件系统遍历，30s 上限
const SCAN_TIMEOUT_MS: u64 = 30_000;
/// GitHub raw 跳过名单回显上限（全部计数在 skippedFiles）
const SKIPPED_SAMPLE_CAP: usize = 20;

static RUN_SEQ: AtomicU32 = AtomicU32::new(0);

// ==================== 状态（读-改-写） ====================

/// 分发目标根目录表：[(目标名, 绝对根)]
fn target_roots(home: &str) -> Vec<(&'static str, String)> {
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

fn read_state(h: &WasmHost) -> Value {
    let home = HOME.get().map(|s| s.as_str()).unwrap_or("");
    h.storage_get(SKILLS_KEY)
        .ok()
        .flatten()
        .filter(|s| s.get("libraryRoot").is_some())
        .unwrap_or_else(|| default_state(home))
}

fn write_state(h: &WasmHost, state: &Value) {
    if let Err(e) = h.storage_set(SKILLS_KEY, state) {
        h.log_warn(&format!("skills: persist state failed: {e}"));
    }
}

/// 全量状态推送前端（命令返回值与事件载荷同形）
fn emit_and_return(h: &WasmHost, state: &Value) -> anyhow::Result<Value> {
    h.emit_event("plugin:agent-hub:skills", state);
    Ok(json!({ "state": state }))
}

fn library_root() -> anyhow::Result<String> {
    HOME.get()
        .map(|home| format!("{home}/{LIBRARY_SEG}"))
        .ok_or_else(|| anyhow::anyhow!("skills: home unavailable"))
}

// ==================== 列举输出解析（纯函数） ====================

/// 解析 find/dir 列举输出 → (分段名, 归一化绝对路径) 列表。
/// `== 分段 ==` 标记与 detect.rs 同风格；路径行统一 `\` → `/` 规范化，
/// 非路径行（报错回显等）忽略。
pub(crate) fn parse_listing(output: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut section = "";
    for line in output.lines() {
        let line = line.trim();
        if let Some(key) = line.strip_prefix("== ").and_then(|s| s.strip_suffix(" ==")) {
            section = key;
            continue;
        }
        if line.is_empty() {
            continue;
        }
        let is_path = line.starts_with('/')
            || (line.len() >= 2
                && line.as_bytes()[0].is_ascii_alphabetic()
                && line.as_bytes()[1] == b':');
        if is_path {
            out.push((section.to_string(), line.replace('\\', "/")));
        }
    }
    out
}

/// 绝对路径剥根前缀 → 相对路径；不属于该根（含大小写不敏感比较，Windows
/// 路径大小写不敏感）返回 None。用 `get` 切片避免多字节路径 panic
pub(crate) fn relativize(root: &str, path: &str) -> Option<String> {
    let prefix = format!("{root}/");
    let head = path.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(&prefix) {
        Some(path[prefix.len()..].to_string())
    } else {
        None
    }
}

/// 相对路径分组为 skill 目录：首段为目录名，仅收录根级含 SKILL.md 的目录
/// （库内游离文件、无 SKILL.md 的目录忽略）
pub(crate) fn group_skills(rels: &[String]) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    for rel in rels {
        if let Some((dir, file)) = rel.split_once('/') {
            if file == "SKILL.md" && !dirs.iter().any(|d| d == dir) {
                dirs.push(dir.to_string());
            }
        }
    }
    dirs.sort();
    dirs
}

/// 从绝对路径取末段（导入 skill 命名：所选目录 basename）
pub(crate) fn basename(path: &str) -> Option<String> {
    let p = path.trim_end_matches('/');
    let name = p.rsplit('/').next().unwrap_or("");
    (!name.is_empty()).then(|| name.to_string())
}

// ==================== 内容 hash（纯函数） ====================

/// FNV-1a 64-bit（wasm 可用的纯函数 hash，用于副本落后比对——非安全用途）
pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn fnv_hex(v: u64) -> String {
    format!("{v:016x}")
}

/// 组合 hash：按 (相对路径, 文件 hash) 排序后逐一喂入；binary 文件（无法以
/// UTF-8 读出）以 "bin" 占位——分发比对时该文件退化为存在性检查
pub(crate) fn combined_hash(entries: &[(String, Option<String>)]) -> String {
    let mut sorted: Vec<&(String, Option<String>)> = entries.iter().collect();
    sorted.sort();
    let mut bytes = Vec::new();
    for (path, hash) in sorted {
        bytes.extend_from_slice(path.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(hash.as_deref().unwrap_or("bin").as_bytes());
        bytes.push(0);
    }
    fnv_hex(fnv1a64(&bytes))
}

/// 单文件 hash：fs_read 成功 → Some(hex)；读取失败（二进制等非 UTF-8 内容）
/// → None（binary）；文件消失（Ok(None)）也归为 None（下次分发按缺失处理）
pub(crate) fn hash_of(content: Option<&str>) -> Option<String> {
    content.map(|c| fnv_hex(fnv1a64(c.as_bytes())))
}

// ==================== SKILL.md frontmatter（纯函数） ====================

/// SKILL.md frontmatter 解析（YAML-lite）：`---` 围栏内 `key: value` 行；
/// allowed-tools 支持单行值与后续缩进 `- item` 列表两种形态（列表以 `, `
/// 拼接）。返回 (name, description, allowed_tools)；无 frontmatter / 字段缺
/// 失返回 None（name 缺失由前端回落目录名）
pub(crate) fn parse_frontmatter(content: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut name = None;
    let mut description = None;
    let mut allowed = None;
    let mut lines = content.lines();
    if lines.next().map(|l| l.trim() == "---").unwrap_or(false) {
        for line in lines {
            let trimmed = line.trim();
            if trimmed == "---" || trimmed == "..." {
                break;
            }
            if trimmed.is_empty() {
                continue;
            }
            if let Some(item) = trimmed.strip_prefix("- ") {
                // 缩进列表项归属最近的 allowed-tools（仅该字段支持列表形态）
                if allowed.is_some() && line.starts_with([' ', '\t']) {
                    let sep = if allowed.as_deref() == Some("") {
                        ""
                    } else {
                        ", "
                    };
                    allowed = Some(format!(
                        "{}{}{}",
                        allowed.unwrap_or_default(),
                        sep,
                        item.trim()
                    ));
                }
                continue;
            }
            let Some((key, value)) = trimmed.split_once(':') else {
                continue;
            };
            let value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            match key.trim() {
                "name" => name = Some(value),
                "description" => description = Some(value),
                "allowed-tools" => allowed = Some(value),
                _ => {}
            }
        }
    }
    (name, description, allowed)
}

// ==================== 分发状态（纯函数） ====================

/// 由比对计数得出分发状态：全命中 = distributed；有缺失/落后 = stale；
/// 无库文件 = none
pub(crate) fn distribution_status(total: usize, missing: usize, stale: usize) -> &'static str {
    if total == 0 {
        return "none";
    }
    if missing == 0 && stale == 0 {
        "distributed"
    } else {
        "stale"
    }
}

// ==================== GitHub 安装（纯函数） ====================

/// GitHub 安装目标（从 URL 解析）
#[derive(Debug, PartialEq)]
pub(crate) struct GithubTarget {
    pub owner: String,
    pub repo: String,
    /// URL 未带 ref 时 None（安装时经 repo API 查 default_branch）
    pub ref_: Option<String>,
    /// 仓库内子目录（`tree/<ref>/<sub...>` 形态）
    pub subdir: Option<String>,
}

/// GitHub 仓库/子目录 URL 解析（纯函数）：
/// - `github.com/<owner>/<repo>`（可选结尾 `/`、`.git` 后缀）
/// - `github.com/<owner>/<repo>/tree/<ref>[/<subdir>]`
///
/// 限制：含 `/` 的分支名不支持（ref 取 tree 后首段）；`blob/`（文件）拒绝。
pub(crate) fn parse_github_url(url: &str) -> Result<GithubTarget, String> {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let mut segments = without_scheme.split('/');
    let host = segments.next().unwrap_or("").trim();
    if !(host.eq_ignore_ascii_case("github.com") || host.eq_ignore_ascii_case("www.github.com")) {
        return Err(format!("not a github.com URL: {url}"));
    }
    let owner = segments.next().unwrap_or("").trim().to_string();
    let mut repo = segments.next().unwrap_or("").trim().to_string();
    if owner.is_empty() || repo.is_empty() {
        return Err(format!("URL must point to owner/repo: {url}"));
    }
    if let Some(stripped) = repo.strip_suffix(".git") {
        repo = stripped.to_string();
    }
    let rest: Vec<&str> = segments.filter(|s| !s.trim().is_empty()).collect();
    match rest.first().map(|s| s.trim()) {
        None => Ok(GithubTarget {
            owner,
            repo,
            ref_: None,
            subdir: None,
        }),
        Some("tree") => {
            let ref_ = rest
                .get(1)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let Some(ref_) = ref_ else {
                return Err(format!("tree URL missing ref: {url}"));
            };
            let subdir: Vec<&str> = rest.iter().skip(2).copied().collect();
            Ok(GithubTarget {
                owner,
                repo,
                ref_: Some(ref_),
                subdir: (!subdir.is_empty()).then(|| subdir.join("/")),
            })
        }
        Some(kind) => Err(format!(
            "unsupported GitHub URL kind `{kind}` (only repo root and tree/)"
        )),
    }
}

/// trees API 响应 → 安装计划 [(skill 目录名, 相对文件路径列表)]（纯函数）
///
/// - subdir 指向的目录必须根级含 SKILL.md（单 skill 安装）
/// - 仓库根：根级 SKILL.md → 单 skill（以 repo 名入库）；否则安装全部
///   「顶层目录直接含 SKILL.md」的 skill（多 skill 仓库，如 anthropics/skills）
/// - 都没有 → Err（前端提示无可安装的 skill）
pub(crate) fn plan_tree(
    tree_json: &str,
    subdir: Option<&str>,
    repo_name: &str,
) -> Result<Vec<(String, Vec<String>)>, String> {
    let tree: Value =
        serde_json::from_str(tree_json).map_err(|e| format!("tree API response invalid: {e}"))?;
    if tree
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Err("tree truncated (repository too large)".to_string());
    }
    let entries = tree
        .get("tree")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "tree API response missing `tree` array".to_string())?;
    let mut blobs: Vec<String> = Vec::new();
    for entry in entries {
        if entry.get("type").and_then(|v| v.as_str()) == Some("blob") {
            if let Some(path) = entry.get("path").and_then(|v| v.as_str()) {
                blobs.push(path.to_string());
            }
        }
    }

    if let Some(subdir) = subdir {
        let prefix = format!("{subdir}/");
        let rels: Vec<String> = blobs
            .iter()
            .filter(|p| p.starts_with(&prefix))
            .map(|p| p[prefix.len()..].to_string())
            .collect();
        if !rels.iter().any(|r| r == "SKILL.md") {
            return Err(format!("no SKILL.md in `{subdir}`"));
        }
        let dir = subdir.rsplit('/').next().unwrap_or(subdir).to_string();
        return Ok(vec![(dir, rels)]);
    }

    if blobs.iter().any(|p| p == "SKILL.md") {
        return Ok(vec![(repo_name.to_string(), blobs)]);
    }
    let dirs = group_skills(
        &blobs
            .iter()
            .map(|p| p.trim_start_matches('/').to_string())
            .collect::<Vec<String>>(),
    );
    if dirs.is_empty() {
        return Err("no SKILL.md found in repository root or top-level dirs".to_string());
    }
    let plan = dirs
        .into_iter()
        .map(|dir| {
            let prefix = format!("{dir}/");
            let rels: Vec<String> = blobs
                .iter()
                .filter(|p| p.starts_with(&prefix))
                .map(|p| p[prefix.len()..].to_string())
                .collect();
            (dir, rels)
        })
        .collect();
    Ok(plan)
}

/// raw 文件下载 URL（路径段最小百分号编码，保留 `/`）
pub(crate) fn raw_file_url(owner: &str, repo: &str, ref_: &str, path: &str) -> String {
    let encoded: Vec<String> = path
        .split('/')
        .map(|seg| {
            let mut out = String::new();
            for b in seg.bytes() {
                match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        out.push(b as char)
                    }
                    _ => out.push_str(&format!("%{b:02X}")),
                }
            }
            out
        })
        .collect();
    format!(
        "https://raw.githubusercontent.com/{owner}/{repo}/{ref_}/{}",
        encoded.join("/")
    )
}

// ==================== 扫描脚本（双平台分派） ====================

/// 目录列举脚本：分段标记输出各根的文件清单。unix `find -type f`；
/// Windows `dir /s /b /a:-d`（/a:-d 排除目录项，dir /s /b 会混入目录行）。
/// 根缺失属常态：错误回显走 stderr 抑制，exit code 不作判据（超时除外）。
///
/// 安全：unix 根路径经 `sh_quote` 单引号转义（import 目录是用户可控输入，
/// 未经转义直接插双引号即可被 `$()` / 反引号注入命令）；Windows 保留双引号
/// 包裹（cmd 引号内 & | < > 按字面处理），`"` 与 `%` 已在 import_local 入口
/// 拒绝（见 `path_rejected_for_script`）。
pub(crate) fn scan_script(sections: &[(&str, String)], windows: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (key, root) in sections {
        if windows {
            parts.push(format!(
                "echo == {key} == & dir /s /b /a:-d \"{root}\" 2>nul"
            ));
        } else {
            parts.push(format!(
                "echo '== {key} =='\nfind {} -type f 2>/dev/null\n",
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

// ==================== 异步流：规范库扫描 ====================

/// 触发全量扫描（枚举三个根 → 逐 skill 读内容/frontmatter/hash → 分发比对）。
/// 扫描进行中（status == scanning）拒绝重入。
pub(crate) fn scan(h: &WasmHost) -> anyhow::Result<Value> {
    let home = HOME
        .get()
        .ok_or_else(|| anyhow::anyhow!("skills: home unavailable"))?;
    let mut state = read_state(h);
    if state["status"] == json!("scanning") {
        return emit_and_return(h, &state);
    }

    let mut sections = vec![("lib", format!("{home}/{LIBRARY_SEG}"))];
    sections.extend(target_roots(home));
    let script = scan_script(&sections, is_windows());
    let (command, args_vec) = shell_invocation(script, is_windows());

    let data_dir = DATA_DIR
        .get()
        .ok_or_else(|| anyhow::anyhow!("skills: data dir unavailable"))?;
    let seq = RUN_SEQ.fetch_add(1, AtomicOrdering::Relaxed);
    let output_path = format!("{data_dir}/runs/skills-scan-{seq}.log");
    let request = json!({
        "command": command,
        "args": args_vec,
        "output_path": output_path,
        "timeout_ms": SCAN_TIMEOUT_MS,
    });
    let run_id = h
        .process_run(&request.to_string())
        .map_err(|e| anyhow::anyhow!("skills: scan spawn failed: {e}"))?;

    pending()
        .lock()
        .map_err(|e| anyhow::anyhow!("skills: poisoned pending map: {e}"))?
        .insert(
            run_id,
            PendingRun {
                kind: "skills-scan".to_string(),
                output_path,
                cli: None,
                source: None,
            },
        );

    state["status"] = json!("scanning");
    state["error"] = json!(null);
    write_state(h, &state);
    h.log_info("skills scan started");
    emit_and_return(h, &state)
}

/// 扫描进程回灌：读列举输出 → 按 skill 归组 → 读 SKILL.md/frontmatter +
/// 逐文件 hash → 分发状态比对 → 持久化推送
pub(crate) fn handle_scan_done(event: &ProcessDoneEvent, output_path: &str) -> anyhow::Result<()> {
    let h = host();
    let mut state = read_state(&h);

    if event.timed_out {
        state["status"] = json!("error");
        state["error"] = json!("scan timed out");
        write_state(&h, &state);
        h.log_warn("skills scan timed out");
        return emit_and_return(&h, &state).map(|_| ());
    }

    let output = h
        .fs_read(output_path)
        .map_err(|e| anyhow::anyhow!("skills: read scan output failed: {e}"))?
        .unwrap_or_default();
    let _ = h.fs_delete(output_path);

    let listing = parse_listing(&output);
    let home = HOME.get().map(|s| s.as_str()).unwrap_or("");

    // 各目标根存在性：列举出任意文件即存在（fs_exists 兜底，授权被拒时列举为空）
    let mut targets = target_roots(home);
    if let Some(obj) = state["targets"].as_object_mut() {
        for (name, root) in &mut targets {
            let found = listing
                .iter()
                .any(|(sec, p)| sec == name && relativize(root, p).is_some());
            let exists = found || h.fs_exists(root).unwrap_or(false);
            obj.insert(name.to_string(), json!({ "root": root, "exists": exists }));
        }
    }

    // 规范库归组 + 逐 skill 采集
    let lib_root = format!("{home}/{LIBRARY_SEG}");
    let mut lib_rels: Vec<String> = Vec::new();
    for (sec, path) in &listing {
        if sec == "lib" {
            if let Some(rel) = relativize(&lib_root, path) {
                lib_rels.push(rel);
            }
        }
    }
    let mut skills: Vec<Value> = Vec::new();
    for dir in group_skills(&lib_rels) {
        let rels: Vec<&String> = lib_rels
            .iter()
            .filter(|r| r.starts_with(&format!("{dir}/")))
            .collect();
        let skill_md_rel = format!("{dir}/SKILL.md");
        let skill_md_path = format!("{lib_root}/{skill_md_rel}");
        let md_content = h.fs_read(&skill_md_path).ok().flatten();
        let (name, description, allowed_tools) =
            parse_frontmatter(md_content.as_deref().unwrap_or(""));

        let mut entries: Vec<(String, Option<String>)> = Vec::new();
        let mut files: Vec<Value> = Vec::new();
        let dir_prefix_len = dir.len() + 1;
        for rel in &rels {
            let path = format!("{lib_root}/{rel}");
            let content = h.fs_read(&path).ok().flatten();
            let hash = hash_of(content.as_deref());
            let file_rel = &rel[dir_prefix_len..];
            entries.push((file_rel.to_string(), hash.clone()));
            files.push(json!({ "path": file_rel, "hash": hash }));
        }
        let hash = combined_hash(&entries);

        let mut entry = json!({
            "dir": dir,
            "path": format!("{lib_root}/{dir}"),
            "name": name.unwrap_or_else(|| dir.clone()),
            "description": description,
            "allowedTools": allowed_tools,
            "files": files,
            "hash": hash,
            "error": if md_content.is_some() { Value::Null } else { json!("SKILL.md unreadable") },
            "distribution": {},
        });
        recompute_distribution(&h, home, &mut entry);
        skills.push(entry);
    }

    state["status"] = json!("ready");
    state["error"] = json!(null);
    state["scannedAt"] = json!(now_ms(&h).unwrap_or(0));
    state["skills"] = json!(skills);
    write_state(&h, &state);
    h.log_info(&format!(
        "skills scan done (count = {})",
        state["skills"].as_array().map(|a| a.len()).unwrap_or(0)
    ));
    emit_and_return(&h, &state).map(|_| ())
}

/// 重算单个 skill 的分发状态（库侧文件 hash 与各目标逐文件比对）：
/// hash 可比 → 内容比对；binary（hash null）→ 存在性比对
fn recompute_distribution(h: &WasmHost, home: &str, entry: &mut Value) {
    let Some(files) = entry["files"].as_array().cloned() else {
        return;
    };
    let dir = entry["dir"].as_str().unwrap_or("").to_string();
    let mut distribution = serde_json::Map::new();
    for (tname, troot) in target_roots(home) {
        let mut missing_files: Vec<String> = Vec::new();
        let mut stale_files: Vec<String> = Vec::new();
        for f in &files {
            let rel = f["path"].as_str().unwrap_or("");
            let target_path = format!("{troot}/{dir}/{rel}");
            match f["hash"].as_str() {
                Some(lib_hash) => match h.fs_read(&target_path) {
                    Ok(Some(content)) => {
                        if hash_of(Some(&content)).as_deref() != Some(lib_hash) {
                            stale_files.push(rel.to_string());
                        }
                    }
                    Ok(None) => missing_files.push(rel.to_string()),
                    // 目标为二进制而库侧可读：内容必然不同 → 落后
                    Err(_) => stale_files.push(rel.to_string()),
                },
                // 库侧 binary：仅存在性比对
                None => {
                    if !h.fs_exists(&target_path).unwrap_or(false) {
                        missing_files.push(rel.to_string());
                    }
                }
            }
        }
        let status = distribution_status(files.len(), missing_files.len(), stale_files.len());
        distribution.insert(
            tname.to_string(),
            json!({
                "status": status,
                "missingFiles": missing_files,
                "staleFiles": stale_files,
            }),
        );
    }
    entry["distribution"] = Value::Object(distribution);
}

// ==================== 命令：读 / 保存 / 分发 ====================

/// 读取 skill 的 SKILL.md 内容（编辑器装载）
pub(crate) fn read_skill(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let dir = args
        .get("dir")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("read-skill: missing dir"))?;
    // dir 为库根下单段目录名：拒绝路径分隔符，防拼接逃逸
    if dir.is_empty() || dir.contains('/') || dir.contains('\\') || dir.contains("..") {
        return Err(anyhow::anyhow!("read-skill: invalid dir"));
    }
    let lib_root = library_root()?;
    let path = format!("{lib_root}/{dir}/SKILL.md");
    let content = h
        .fs_read(&path)
        .map_err(|e| anyhow::anyhow!("read-skill: read failed: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("read-skill: SKILL.md missing for {dir}"))?;
    let (name, description, allowed_tools) = parse_frontmatter(&content);
    Ok(json!({
        "dir": dir,
        "name": name.unwrap_or_else(|| dir.to_string()),
        "description": description,
        "allowedTools": allowed_tools,
        "content": content,
    }))
}

/// 保存 SKILL.md：保存前重读做冲突检测（baseContent 与磁盘现状比对）；
/// 冲突时返回磁盘内容（force = 用户确认覆盖）。成功后重算该 skill 的
/// frontmatter/hash/分发状态并推送。
pub(crate) fn save_skill(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let dir = args
        .get("dir")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("save-skill: missing dir"))?;
    if dir.is_empty() || dir.contains('/') || dir.contains('\\') || dir.contains("..") {
        return Err(anyhow::anyhow!("save-skill: invalid dir"));
    }
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("save-skill: missing content"))?;
    let base = args
        .get("baseContent")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

    let lib_root = library_root()?;
    let path = format!("{lib_root}/{dir}/SKILL.md");
    let current = h
        .fs_read(&path)
        .map_err(|e| anyhow::anyhow!("save-skill: read failed: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("save-skill: SKILL.md missing for {dir}"))?;

    if !force && current != base {
        h.log_warn(&format!("save-skill: conflict detected (dir = {dir})"));
        return Ok(json!({ "saved": false, "conflict": true, "current": current }));
    }

    h.fs_write(&path, content)
        .map_err(|e| anyhow::anyhow!("save-skill: write failed: {e}"))?;

    // 重算该 skill 条目（frontmatter 可能随编辑变化）+ 分发状态
    let home = HOME.get().map(|s| s.as_str()).unwrap_or("").to_string();
    let mut state = read_state(h);
    if let Some(skills) = state["skills"].as_array_mut() {
        if let Some(entry) = skills.iter_mut().find(|s| s["dir"] == json!(dir)) {
            let (name, description, allowed_tools) = parse_frontmatter(content);
            entry["name"] = json!(name.unwrap_or_else(|| dir.to_string()));
            entry["description"] = json!(description);
            entry["allowedTools"] = json!(allowed_tools);
            let files = entry["files"].as_array().cloned().unwrap_or_default();
            let mut entries: Vec<(String, Option<String>)> = Vec::new();
            let mut new_files: Vec<Value> = Vec::new();
            for f in &files {
                let rel = f["path"].as_str().unwrap_or("").to_string();
                let hash = if rel == "SKILL.md" {
                    hash_of(Some(content))
                } else {
                    let c = h.fs_read(&format!("{lib_root}/{dir}/{rel}")).ok().flatten();
                    hash_of(c.as_deref())
                };
                entries.push((rel.clone(), hash.clone()));
                new_files.push(json!({ "path": rel, "hash": hash }));
            }
            entry["files"] = json!(new_files);
            entry["hash"] = json!(combined_hash(&entries));
            recompute_distribution(h, &home, entry);
        }
    }
    write_state(h, &state);
    h.log_info(&format!("save-skill: saved (dir = {dir})"));
    emit_and_return(h, &state)
}

/// 分发（重新分发同路径）：库侧逐文件 fs_copy 到目标（fs_copy 自动建父目录，
/// 字节级复制支持二进制）。逐文件容错：失败的文件记录在 errors，成功后重算
/// 分发状态。targets 缺省 = 全部白名单目标。
pub(crate) fn distribute(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let dir = args
        .get("dir")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("distribute: missing dir"))?;
    if dir.is_empty() || dir.contains('/') || dir.contains('\\') || dir.contains("..") {
        return Err(anyhow::anyhow!("distribute: invalid dir"));
    }
    let wanted: Vec<String> = match args.get("targets").and_then(|v| v.as_array()) {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        None => TARGET_SEGS.iter().map(|(n, _)| n.to_string()).collect(),
    };
    for t in &wanted {
        if !TARGET_SEGS.iter().any(|(n, _)| n == t) {
            return Err(anyhow::anyhow!("distribute: unknown target {t}"));
        }
    }

    let home = HOME.get().map(|s| s.as_str()).unwrap_or("").to_string();
    let lib_root = library_root()?;
    let mut state = read_state(h);
    let Some(entry) = state["skills"]
        .as_array()
        .and_then(|skills| skills.iter().find(|s| s["dir"] == json!(dir)))
        .cloned()
    else {
        return Err(anyhow::anyhow!(
            "distribute: unknown skill {dir}（请先扫描）"
        ));
    };
    let files = entry["files"].as_array().cloned().unwrap_or_default();

    let mut copied = 0u32;
    let mut errors: Vec<String> = Vec::new();
    for (tname, troot) in target_roots(&home) {
        if !wanted.contains(&tname.to_string()) {
            continue;
        }
        for f in &files {
            let rel = f["path"].as_str().unwrap_or("");
            let src = format!("{lib_root}/{dir}/{rel}");
            let dst = format!("{troot}/{dir}/{rel}");
            match h.fs_copy(&src, &dst) {
                Ok(()) => copied += 1,
                Err(e) => errors.push(format!("{tname}/{rel}: {e}")),
            }
        }
    }

    // 重算该 skill 分发状态（复制后库/目标内容一致的部分转为 distributed）
    if let Some(skills) = state["skills"].as_array_mut() {
        if let Some(entry) = skills.iter_mut().find(|s| s["dir"] == json!(dir)) {
            recompute_distribution(h, &home, entry);
        }
    }
    write_state(h, &state);
    h.log_info(&format!(
        "distribute done (dir = {dir}, copied = {copied}, errors = {})",
        errors.len()
    ));
    let mut result = emit_and_return(h, &state)?;
    result["copied"] = json!(copied);
    result["errors"] = json!(errors);
    Ok(result)
}

// ==================== 命令：GitHub 安装 ====================

fn http_get(h: &WasmHost, url: &str, accept_json: bool) -> Result<(u16, String), String> {
    let mut headers = json!({ "User-Agent": "bedcode-agent-hub" });
    if accept_json {
        headers["Accept"] = json!("application/vnd.github+json");
    }
    let request = json!({ "method": "GET", "url": url, "headers": headers });
    let resp = h
        .http_fetch(&request)
        .map_err(|e| format!("http failed: {e}"))?
        .ok_or_else(|| "empty response".to_string())?;
    let status = resp.get("status").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
    let body = resp
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok((status, body))
}

/// 从仓库 URL 安装 skill（同步命令，多次 host-http 往返）：解析 URL →
/// 定位 ref（缺省查 default_branch）→ recursive trees 一次拿全量路径 →
/// 逐文件 raw 下载（非 UTF-8 跳过记录）→ fs_write 入规范库（fs_write 自动
/// 建父目录）。已存在同名 skill 时需 overwrite 确认。完成后触发重扫描。
pub(crate) fn install_github(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let url = args
        .get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("install-github: missing url"))?;
    let overwrite = args
        .get("overwrite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let target = match parse_github_url(url) {
        Ok(t) => t,
        Err(e) => return finish_github(h, false, &[], 0, &[], &e),
    };

    // ref 缺省 → repo API 查 default_branch
    let ref_ = match &target.ref_ {
        Some(r) => r.clone(),
        None => {
            let api = format!(
                "https://api.github.com/repos/{}/{}",
                target.owner, target.repo
            );
            match http_get(h, &api, true) {
                Ok((200, body)) => match serde_json::from_str::<Value>(&body) {
                    Ok(meta) => meta
                        .get("default_branch")
                        .and_then(|v| v.as_str())
                        .unwrap_or("main")
                        .to_string(),
                    Err(e) => {
                        return finish_github(
                            h,
                            false,
                            &[],
                            0,
                            &[],
                            &format!("repo API response invalid: {e}"),
                        )
                    }
                },
                Ok((status, _)) => {
                    let msg =
                        format!("GitHub repo API returned {status} (unreachable? rate limited?)");
                    return finish_github(h, false, &[], 0, &[], &msg);
                }
                Err(e) => return finish_github(h, false, &[], 0, &[], &e),
            }
        }
    };

    // recursive trees：一次拿全量路径
    let tree_url = format!(
        "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
        target.owner,
        target.repo,
        ref_.replace('/', "%2F")
    );
    let (_status, tree_body) = match http_get(h, &tree_url, true) {
        Ok(r) => r,
        Err(e) => return finish_github(h, false, &[], 0, &[], &e),
    };
    let plan = match plan_tree(&tree_body, target.subdir.as_deref(), &target.repo) {
        Ok(p) => p,
        Err(e) => return finish_github(h, false, &[], 0, &[], &e),
    };

    // 已存在同名 skill：无 overwrite 确认时返回名单（前端两击确认后重试）
    let lib_root = match library_root() {
        Ok(r) => r,
        Err(e) => return finish_github(h, false, &[], 0, &[], &e.to_string()),
    };
    let existing: Vec<String> = plan
        .iter()
        .map(|(dir, _)| dir.clone())
        .filter(|dir| h.fs_exists(&format!("{lib_root}/{dir}")).unwrap_or(false))
        .collect();
    if !existing.is_empty() && !overwrite {
        h.log_info(&format!(
            "install-github: existing skills need overwrite confirm (count = {})",
            existing.len()
        ));
        return Ok(json!({ "installed": false, "exists": existing }));
    }

    let mut installed: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut skipped_total = 0u32;
    for (dir, rels) in &plan {
        for rel in rels {
            let sub_prefix = target
                .subdir
                .as_deref()
                .map(|s| format!("{s}/"))
                .unwrap_or_default();
            let raw = raw_file_url(
                &target.owner,
                &target.repo,
                &ref_,
                &format!("{sub_prefix}{rel}"),
            );
            match http_get(h, &raw, false) {
                Ok((200, body)) => {
                    let dst = format!("{lib_root}/{dir}/{rel}");
                    match h.fs_write(&dst, &body) {
                        Ok(()) => {}
                        Err(e) => {
                            if skipped.len() < SKIPPED_SAMPLE_CAP {
                                skipped.push(format!("{dir}/{rel}"));
                            }
                            skipped_total += 1;
                            h.log_debug(&format!("install-github: write failed {dst}: {e}"));
                        }
                    }
                }
                // 非 UTF-8（图片等二进制）/ 非 200：跳过记录，不中断整批
                Ok((status, _)) => {
                    skipped_total += 1;
                    if skipped.len() < SKIPPED_SAMPLE_CAP {
                        skipped.push(format!("{dir}/{rel} (HTTP {status})"));
                    }
                }
                Err(e) => {
                    skipped_total += 1;
                    if skipped.len() < SKIPPED_SAMPLE_CAP {
                        skipped.push(format!("{dir}/{rel}"));
                    }
                    h.log_debug(&format!("install-github: fetch {raw} failed: {e}"));
                }
            }
        }
        installed.push(dir.clone());
    }

    h.log_info(&format!(
        "install-github done (installed = {}, skipped_files = {skipped_total})",
        installed.len()
    ));
    finish_github(h, true, &installed, skipped_total, &skipped, "")
}

/// GitHub 安装结果落状态 + 推送；成功后触发重扫描，失败时返回错误摘要
fn finish_github(
    h: &WasmHost,
    ok: bool,
    installed: &[String],
    skipped_files: u32,
    skipped: &[String],
    error: &str,
) -> anyhow::Result<Value> {
    let mut state = read_state(h);
    state["github"]["last"] = json!({
        "ok": ok,
        "installed": installed,
        "skippedFiles": skipped_files,
        "skipped": skipped,
        "error": if error.is_empty() { Value::Null } else { json!(error) },
        "at": now_ms(h).unwrap_or(0),
    });
    write_state(h, &state);
    h.emit_event("plugin:agent-hub:skills", &state);
    if ok {
        // 新库内容需重扫描（枚举 + 分发比对走 host-process，异步）
        if let Err(e) = scan(h) {
            h.log_warn(&format!("install-github: post-install rescan failed: {e}"));
        }
        Ok(
            json!({ "installed": true, "names": installed, "skippedFiles": skipped_files, "skipped": skipped }),
        )
    } else {
        Ok(json!({ "installed": false, "error": error }))
    }
}

// ==================== 异步流：本地目录导入 ====================

/// 本地目录导入：无 path 时先 pick-folder（同步对话框）+ 一次批量授权；
/// 目标同名 skill 已存在且未确认覆盖时返回 exists 名单。确认后 spawn 枚举
/// 进程（异步），回灌时逐文件 fs_copy 入规范库。
pub(crate) fn import_local(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    let lib_root = library_root()?;
    let path = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => {
            let picked = h
                .platform_pick_folder()
                .map_err(|e| anyhow::anyhow!("import: pick folder failed: {e}"))?;
            if picked.is_empty() {
                return Ok(json!({ "picked": false }));
            }
            picked
        }
    };
    // 规范化分隔符（Windows 反斜杠路径）后取 basename 作为入库目录名
    let normalized = path.replace('\\', "/");
    if path_rejected_for_script(&path) {
        return Err(anyhow::anyhow!(
            "import: path contains characters unsupported by scan scripts"
        ));
    }
    let Some(name) = basename(&normalized) else {
        return Err(anyhow::anyhow!(
            "import: cannot derive skill name from {path}"
        ));
    };
    if name.contains("..") {
        return Err(anyhow::anyhow!("import: invalid skill name {name}"));
    }

    // 所选目录通常不在批量授权列：一次弹窗授权（记住前缀后幂等）
    let granted = h
        .fs_request_auth(std::slice::from_ref(&path))
        .map_err(|e| anyhow::anyhow!("import: auth failed: {e}"))?;
    if !granted {
        return Ok(json!({ "picked": true, "auth": false, "path": path, "name": name }));
    }

    let exists = h
        .fs_exists(&format!("{lib_root}/{name}"))
        .map_err(|e| anyhow::anyhow!("import: exists check failed: {e}"))?;
    let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
    if exists && !force {
        return Ok(
            json!({ "picked": true, "auth": true, "path": path, "name": name, "exists": true }),
        );
    }

    start_import(h, &path, &name)
}

/// spawn 导入枚举进程（`== src ==` 单分段列举源目录全量文件）
fn start_import(h: &WasmHost, source: &str, name: &str) -> anyhow::Result<Value> {
    let script = scan_script(&[("src", source.to_string())], is_windows());
    let (command, args_vec) = shell_invocation(script, is_windows());
    let data_dir = DATA_DIR
        .get()
        .ok_or_else(|| anyhow::anyhow!("import: data dir unavailable"))?;
    let seq = RUN_SEQ.fetch_add(1, AtomicOrdering::Relaxed);
    let output_path = format!("{data_dir}/runs/skills-import-{seq}.log");
    let request = json!({
        "command": command,
        "args": args_vec,
        "output_path": output_path,
        "timeout_ms": SCAN_TIMEOUT_MS,
    });
    let run_id = h
        .process_run(&request.to_string())
        .map_err(|e| anyhow::anyhow!("import: spawn failed: {e}"))?;
    pending()
        .lock()
        .map_err(|e| anyhow::anyhow!("skills: poisoned pending map: {e}"))?
        .insert(
            run_id,
            PendingRun {
                kind: "skills-import".to_string(),
                output_path,
                cli: None,
                source: Some(format!("{source}\n{name}")),
            },
        );

    let mut state = read_state(h);
    state["importing"] = json!(true);
    write_state(h, &state);
    h.log_info(&format!("skills import started (name = {name})"));
    emit_and_return(h, &state)
}

/// 导入进程回灌：解析源目录文件清单 → 逐文件 fs_copy 入规范库（保留相对
/// 路径）→ 结果落状态 → 触发重扫描
pub(crate) fn handle_import_done(
    event: &ProcessDoneEvent,
    output_path: &str,
    source: &str,
) -> anyhow::Result<()> {
    let h = host();
    let mut state = read_state(&h);
    state["importing"] = json!(false);

    let (src_dir, name) = source
        .split_once('\n')
        .ok_or_else(|| anyhow::anyhow!("skills: import pending entry malformed"))?;

    let fail = |state: &mut Value, error: String| -> anyhow::Result<()> {
        state["import"]["last"] = json!({
            "ok": false, "name": name, "fileCount": 0, "error": error, "at": now_ms(&h).unwrap_or(0),
        });
        write_state(&h, state);
        h.log_warn(&format!("skills import failed (name = {name})"));
        emit_and_return(&h, state).map(|_| ())
    };

    if event.timed_out {
        return fail(&mut state, "import timed out".to_string());
    }
    let output = h
        .fs_read(output_path)
        .map_err(|e| anyhow::anyhow!("skills: read import output failed: {e}"))?
        .unwrap_or_default();
    let _ = h.fs_delete(output_path);

    let src_normalized = src_dir.replace('\\', "/");
    let mut rels: Vec<String> = Vec::new();
    for (sec, path) in parse_listing(&output) {
        if sec != "src" {
            continue;
        }
        if let Some(rel) = relativize(&src_normalized, &path) {
            rels.push(rel);
        }
    }
    if !rels.iter().any(|r| r == "SKILL.md") {
        return fail(&mut state, "selected directory has no SKILL.md".to_string());
    }

    let lib_root = library_root()?;
    let mut copied = 0u32;
    let mut errors: Vec<String> = Vec::new();
    for rel in &rels {
        // fs_copy 字节级复制（支持二进制），目标父目录自动创建
        match h.fs_copy(
            &format!("{src_dir}/{rel}"),
            &format!("{lib_root}/{name}/{rel}"),
        ) {
            Ok(()) => copied += 1,
            Err(e) => errors.push(format!("{rel}: {e}")),
        }
    }
    let ok = errors.is_empty();
    state["import"]["last"] = json!({
        "ok": ok,
        "name": name,
        "fileCount": copied,
        "error": if ok { Value::Null } else { json!(errors.join("; ")) },
        "at": now_ms(&h).unwrap_or(0),
    });
    write_state(&h, &state);
    h.log_info(&format!(
        "skills import done (name = {name}, copied = {copied}, errors = {})",
        errors.len()
    ));
    emit_and_return(&h, &state)?;

    // 导入内容需重扫描刷新库列表与分发状态（尽力而为）
    if let Err(e) = scan(&h) {
        h.log_warn(&format!("skills: post-import rescan failed: {e}"));
    }
    Ok(())
}

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

// ==================== Tests（纯函数单测，双平台形态覆盖） ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== frontmatter ====================

    /// 完整 frontmatter：name/description/allowed-tools 单行值
    #[test]
    fn frontmatter_full() {
        let md = "---\nname: ctx7\ndescription: Fetch docs fast\nallowed-tools: Fetch, Grep\n---\n\n# Body\n";
        let (name, desc, allowed) = parse_frontmatter(md);
        assert_eq!(name.as_deref(), Some("ctx7"));
        assert_eq!(desc.as_deref(), Some("Fetch docs fast"));
        assert_eq!(allowed.as_deref(), Some("Fetch, Grep"));
    }

    /// allowed-tools 缩进列表形态（多行 `- item`）
    #[test]
    fn frontmatter_allowed_tools_list() {
        let md = "---\nname: x\ndescription: y\nallowed-tools:\n  - Read\n  - Grep\n---\nbody";
        let (_, _, allowed) = parse_frontmatter(md);
        assert_eq!(allowed.as_deref(), Some("Read, Grep"));
    }

    /// 无 frontmatter / 缺字段：返回 None（name 由前端回落目录名）
    #[test]
    fn frontmatter_missing() {
        assert_eq!(parse_frontmatter("# plain markdown"), (None, None, None));
        let (name, desc, _) = parse_frontmatter("---\ndescription: only desc\n---\n");
        assert_eq!(name, None);
        assert_eq!(desc.as_deref(), Some("only desc"));
    }

    /// 引号包裹的值被剥离
    #[test]
    fn frontmatter_quoted_values() {
        let md = "---\nname: \"quoted\"\ndescription: 'single'\n---\n";
        let (name, desc, _) = parse_frontmatter(md);
        assert_eq!(name.as_deref(), Some("quoted"));
        assert_eq!(desc.as_deref(), Some("single"));
    }

    // ==================== 列举解析 / 归组 ====================

    /// unix find 输出：分段 + 相对化 + 归组；库外游离文件忽略
    #[test]
    fn listing_unix_grouping() {
        let out = "== lib ==\n/home/u/.agents/skills/ctx7/SKILL.md\n/home/u/.agents/skills/ctx7/ref.md\n/home/u/.agents/skills/loose.txt\n/home/u/.agents/skills/notaskill/a.md\n== claude ==\n/home/u/.claude/skills/ctx7/SKILL.md\n";
        let listing = parse_listing(out);
        let lib_rels: Vec<String> = listing
            .iter()
            .filter(|(s, _)| s == "lib")
            .filter_map(|(_, p)| relativize("/home/u/.agents/skills", p))
            .collect();
        assert_eq!(lib_rels.len(), 4);
        let dirs = group_skills(&lib_rels);
        assert_eq!(dirs, vec!["ctx7".to_string()]);
    }

    /// Windows dir 输出：反斜杠规范化 + 根前缀大小写不敏感
    #[test]
    fn listing_windows() {
        let out = "== lib ==\nC:\\Users\\u\\.agents\\skills\\ctx7\\SKILL.md\n== pi ==\n";
        let listing = parse_listing(out);
        let rel = relativize("C:/Users/u/.agents/skills", &listing[0].1);
        assert_eq!(rel.as_deref(), Some("ctx7/SKILL.md"));
        // 大小写变体根（Windows 大小写不敏感）
        let rel = relativize(
            "c:/users/u/.agents/skills",
            "C:/Users/u/.agents/skills/ctx7/SKILL.md",
        );
        assert_eq!(rel.as_deref(), Some("ctx7/SKILL.md"));
    }

    /// 前缀部分重名的根不误判（.agents/skills2 ≠ .agents/skills）
    #[test]
    fn relativize_prefix_must_include_separator() {
        assert_eq!(
            relativize("/home/u/.agents/skills", "/home/u/.agents/skills2/a.md"),
            None
        );
        assert_eq!(relativize("/home/u/r", "/home/u/root/a.md"), None);
    }

    /// basename：常规路径 / 尾斜杠 / 空段
    #[test]
    fn basename_extraction() {
        assert_eq!(
            basename("/home/u/Downloads/my-skill").as_deref(),
            Some("my-skill")
        );
        assert_eq!(basename("C:/Users/u/skill/").as_deref(), Some("skill"));
        assert_eq!(basename("/"), None);
    }

    // ==================== hash ====================

    /// FNV-1a 64 已知向量（空串 basis / "a"）+ 稳定性
    #[test]
    fn fnv_known_vectors() {
        assert_eq!(format!("{:016x}", fnv1a64(b"")), "cbf29ce484222325");
        assert_eq!(fnv_hex(fnv1a64(b"a")), "af63dc4c8601ec8c");
        assert_eq!(fnv1a64(b"abc"), fnv1a64(b"abc"));
        assert_ne!(fnv1a64(b"abc"), fnv1a64(b"abd"));
    }

    /// 组合 hash：与条目顺序无关；binary 占位参与区分
    #[test]
    fn combined_hash_order_independent() {
        let a = vec![
            ("SKILL.md".to_string(), Some("aa".to_string())),
            ("img.png".to_string(), None),
        ];
        let b = vec![
            ("img.png".to_string(), None),
            ("SKILL.md".to_string(), Some("aa".to_string())),
        ];
        assert_eq!(combined_hash(&a), combined_hash(&b));
        let c = vec![
            ("SKILL.md".to_string(), Some("aa".to_string())),
            ("img.png".to_string(), Some("bb".to_string())),
        ];
        assert_ne!(combined_hash(&a), combined_hash(&c));
    }

    // ==================== 分发状态 ====================

    #[test]
    fn distribution_status_rules() {
        assert_eq!(distribution_status(0, 0, 0), "none");
        assert_eq!(distribution_status(3, 0, 0), "distributed");
        assert_eq!(distribution_status(3, 1, 0), "stale");
        assert_eq!(distribution_status(3, 0, 2), "stale");
    }

    // ==================== GitHub URL / tree 计划 ====================

    #[test]
    fn github_url_forms() {
        let t = parse_github_url("https://github.com/anthropics/skills").unwrap();
        assert_eq!(t.owner, "anthropics");
        assert_eq!(t.repo, "skills");
        assert_eq!(t.ref_, None);
        assert_eq!(t.subdir, None);

        let t = parse_github_url("github.com/o/r.git/").unwrap();
        assert_eq!(t.repo, "r");

        let t = parse_github_url("https://github.com/o/r/tree/main").unwrap();
        assert_eq!(t.ref_.as_deref(), Some("main"));
        assert_eq!(t.subdir, None);

        let t = parse_github_url("https://github.com/o/r/tree/v1.2/sub/dir").unwrap();
        assert_eq!(t.ref_.as_deref(), Some("v1.2"));
        assert_eq!(t.subdir.as_deref(), Some("sub/dir"));

        assert!(parse_github_url("https://github.com/o/r/blob/main/SKILL.md").is_err());
        assert!(parse_github_url("https://gitlab.com/o/r").is_err());
        assert!(parse_github_url("https://github.com/only-owner").is_err());
        assert!(parse_github_url("https://github.com/o/r/tree").is_err());
    }

    /// tree 计划：仓库根直接含 SKILL.md → 单 skill（repo 名入库）
    #[test]
    fn plan_tree_root_skill() {
        let tree = r#"{"sha":"x","tree":[
            {"path":"SKILL.md","type":"blob"},
            {"path":"refs/api.md","type":"blob"},
            {"path":"scripts","type":"tree"}]}"#;
        let plan = plan_tree(tree, None, "my-skill").unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].0, "my-skill");
        assert!(plan[0].1.contains(&"SKILL.md".to_string()));
        assert!(plan[0].1.contains(&"refs/api.md".to_string()));
    }

    /// 多 skill 仓库：顶层目录各自含 SKILL.md → 全部安装；无任何 SKILL.md → Err
    #[test]
    fn plan_tree_multi_skill() {
        let tree = r#"{"tree":[
            {"path":"ctx7/SKILL.md","type":"blob"},
            {"path":"ctx7/a.md","type":"blob"},
            {"path":"grill/SKILL.md","type":"blob"},
            {"path":"README.md","type":"blob"}]}"#;
        let plan = plan_tree(tree, None, "repo").unwrap();
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].0, "ctx7");
        assert_eq!(plan[1].0, "grill");

        let empty = r#"{"tree":[{"path":"README.md","type":"blob"}]}"#;
        assert!(plan_tree(empty, None, "repo").is_err());
    }

    /// 子目录安装：必须根级含 SKILL.md；truncated 拒绝
    #[test]
    fn plan_tree_subdir() {
        let tree = r#"{"tree":[
            {"path":"skills/ctx7/SKILL.md","type":"blob"},
            {"path":"skills/ctx7/x.md","type":"blob"},
            {"path":"other/SKILL.md","type":"blob"}]}"#;
        let plan = plan_tree(tree, Some("skills/ctx7"), "repo").unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].0, "ctx7");
        assert_eq!(plan[0].1, vec!["SKILL.md".to_string(), "x.md".to_string()]);

        let noskill = r#"{"tree":[{"path":"d/a.md","type":"blob"}]}"#;
        assert!(plan_tree(noskill, Some("d"), "repo").is_err());

        let truncated = r#"{"tree":[],"truncated":true}"#;
        assert!(plan_tree(truncated, None, "repo").is_err());
    }

    /// raw URL：路径段百分号编码（空格）、`/` 保留
    #[test]
    fn raw_url_encoding() {
        assert_eq!(
            raw_file_url("o", "r", "main", "my skill/a b.md"),
            "https://raw.githubusercontent.com/o/r/main/my%20skill/a%20b.md"
        );
        assert_eq!(
            raw_file_url("o", "r", "main", "plain.md"),
            "https://raw.githubusercontent.com/o/r/main/plain.md"
        );
    }

    // ==================== 扫描脚本 ====================

    /// 双平台形态：unix find 分段；Windows dir /a:-d + 2>nul
    #[test]
    fn scan_script_platforms() {
        let sections = vec![("lib", "/home/u/.agents/skills".to_string())];
        let unix = scan_script(&sections, false);
        assert!(unix.contains("== lib =="));
        assert!(unix.contains("find '/home/u/.agents/skills' -type f 2>/dev/null"));

        let win = scan_script(&sections, true);
        assert!(win.contains("echo == lib =="));
        assert!(win.contains("dir /s /b /a:-d \"/home/u/.agents/skills\" 2>nul"));
    }

    /// 注入防护：import 来源路径进单引号后仅作为 find 参数；单引号转义不逃逸
    #[test]
    fn scan_script_quotes_import_root() {
        let unix = scan_script(&[("src", "/tmp/$(rm -rf ~);x".to_string())], false);
        // $() 序列整体被单引号包裹：脚本输出逐字节比对，证明序列未逃逸
        assert_eq!(
            unix,
            "echo '== src =='\nfind '/tmp/$(rm -rf ~);x' -type f 2>/dev/null\n"
        );

        let quoted = scan_script(&[("src", "/tmp/a'b".to_string())], false);
        assert!(quoted.contains("find '/tmp/a'\\''b' -type f 2>/dev/null"));
    }
}
