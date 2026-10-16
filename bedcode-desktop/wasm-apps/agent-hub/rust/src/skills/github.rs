//! GitHub 安装（URL 解析 → trees API → raw 下载）
//!
//! 非流式 host-http 响应体强制 UTF-8（宿主 http.rs），二进制 tarball 不可行
//! → 走 GitHub JSON API（repo 默认分支 + recursive trees 一次拿全量路径）+
//! raw.githubusercontent.com 文本下载；非 UTF-8 文件（图片等）跳过并记录。
//! 不可达时报错落状态，前端提示代理/镜像（v1 仅提示）。

use super::listing::group_skills;
use super::scan::scan;
use super::SKIPPED_SAMPLE_CAP;
use super::{library_root, read_state, write_state};
use crate::install::now_ms;
use bedcode_plugin_api::host::{HostEvents, HostFs, HostHttp, HostLog};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

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
// ==================== 命令：GitHub 安装 ====================
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

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

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
}
