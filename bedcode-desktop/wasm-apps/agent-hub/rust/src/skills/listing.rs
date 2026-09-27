//! 列举解析 / 内容 hash / frontmatter / 分发判定（纯函数，库内容采集共用）
//!
//! find/dir 列举输出解析与路径归一（`== 分段 ==` 标记）；FNV-1a 内容 hash 供
//! 副本落后比对与组合签名；SKILL.md frontmatter（YAML-lite）提取；
//! 分发状态由比对计数判定（全命中 = distributed / 有缺失或落后 = stale）。

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
pub(super) fn relativize(root: &str, path: &str) -> Option<String> {
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
pub(super) fn group_skills(rels: &[String]) -> Vec<String> {
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
pub(super) fn basename(path: &str) -> Option<String> {
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
pub(super) fn combined_hash(entries: &[(String, Option<String>)]) -> String {
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
pub(super) fn hash_of(content: Option<&str>) -> Option<String> {
    content.map(|c| fnv_hex(fnv1a64(c.as_bytes())))
}
// ==================== SKILL.md frontmatter（纯函数） ====================

/// SKILL.md frontmatter 解析（YAML-lite）：`---` 围栏内 `key: value` 行；
/// allowed-tools 支持单行值与后续缩进 `- item` 列表两种形态（列表以 `, `
/// 拼接）。返回 (name, description, allowed_tools)；无 frontmatter / 字段缺
/// 失返回 None（name 缺失由前端回落目录名）
pub(super) fn parse_frontmatter(content: &str) -> (Option<String>, Option<String>, Option<String>) {
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
pub(super) fn distribution_status(total: usize, missing: usize, stale: usize) -> &'static str {
    if total == 0 {
        return "none";
    }
    if missing == 0 && stale == 0 {
        "distributed"
    } else {
        "stale"
    }
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn distribution_status_rules() {
        assert_eq!(distribution_status(0, 0, 0), "none");
        assert_eq!(distribution_status(3, 0, 0), "distributed");
        assert_eq!(distribution_status(3, 1, 0), "stale");
        assert_eq!(distribution_status(3, 0, 2), "stale");
    }
}
