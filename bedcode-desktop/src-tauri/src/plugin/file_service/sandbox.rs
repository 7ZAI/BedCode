//! 目录沙箱（纯函数，可单测）
//!
//! 宿主强制的路径安全边界，插件无法绕过（规格 4.3 节）：
//! - 挂载时：root canonicalize + 去重取最外层（[`normalize_roots`]）
//! - 请求时：`..` 一律拒绝；最终路径 canonicalize 后必须仍在某 root 前缀内
//!   （防 symlink 逃逸），否则返回错误
//! - 大小写按文件系统实际语义（canonicalize 后比较）
//!
//! 本模块不依赖 tauri / actix，便于在无头环境单测。

use std::path::{Path, PathBuf};

/// 沙箱错误
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SandboxError {
    /// 路径穿越/绝对路径/非法字符被拒绝
    #[error("path traversal rejected: {0}")]
    Traversal(String),
    /// 路径不在（或逃出）任何允许目录根内
    #[error("path outside allowed roots: {0}")]
    OutsideRoots(String),
    /// root 本身非法（不存在/不是目录）
    #[error("invalid root '{path}': {reason}")]
    InvalidRoot {
        /// root 路径
        path: String,
        /// 原因
        reason: String,
    },
    /// 未配置任何 root
    #[error("no allowed roots configured")]
    NoRoots,
}

/// 规范化允许目录根：canonicalize + 存在性/目录校验 + 去重取最外层
///
/// 去重规则：嵌套 root 只保留最外层（外层已包含内层全部权限）；
/// 完全重复的 root 只保留一份。返回的 root 均为 canonicalize 后路径，
/// 供后续 `starts_with` 前缀比较直接复用
pub fn normalize_roots(roots: &[PathBuf]) -> Result<Vec<PathBuf>, SandboxError> {
    if roots.is_empty() {
        return Err(SandboxError::NoRoots);
    }

    let mut normalized: Vec<PathBuf> = Vec::with_capacity(roots.len());
    for root in roots {
        // canonicalize 同时完成存在性校验与 symlink 解析
        let canonical = root.canonicalize().map_err(|e| SandboxError::InvalidRoot {
            path: root.display().to_string(),
            reason: format!("canonicalize failed (must exist): {}", e),
        })?;
        if !canonical.is_dir() {
            return Err(SandboxError::InvalidRoot {
                path: root.display().to_string(),
                reason: "not a directory".to_string(),
            });
        }

        // 已被现有外层 root 包含 → 跳过
        if normalized.iter().any(|existing| canonical.starts_with(existing)) {
            continue;
        }
        // 新 root 更外层 → 移除被它包含的旧 root
        normalized.retain(|existing| !existing.starts_with(&canonical));
        normalized.push(canonical);
    }

    Ok(normalized)
}

/// 清洗挂载点相对路径为安全的分量列表
///
/// 拒绝规则（规格 4.3）：
/// - `..` 分量一律拒绝
/// - 绝对路径（前导 `/` 或 `\`）拒绝
/// - 含 `:` 的分量拒绝（Windows 盘符前缀 / NTFS ADS 流）
/// - `\` 统一按 `/` 处理（移动端客户端一律发正斜杠）
pub fn clean_relative_parts(rel: &str) -> Result<Vec<String>, SandboxError> {
    if rel.starts_with('/') || rel.starts_with('\\') {
        return Err(SandboxError::Traversal(format!(
            "absolute path rejected: {}",
            rel
        )));
    }

    let mut parts = Vec::new();
    for part in rel.replace('\\', "/").split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(SandboxError::Traversal(format!(
                "'..' component rejected in: {}",
                rel
            )));
        }
        if part.contains(':') {
            return Err(SandboxError::Traversal(format!(
                "':' rejected in component '{}' (drive letter / ADS)",
                part
            )));
        }
        parts.push(part.to_string());
    }
    Ok(parts)
}

/// 将挂载点相对路径解析为沙箱内的绝对路径（目标必须已存在）
///
/// 用于列举/下载：在任一 root 下找到存在的候选路径后 canonicalize，
/// 校验 canonicalize 结果仍在该 root 前缀内 —— symlink 指向 root 外时
/// 在此被拦截（规格 4.3 第 2 条）。
///
/// 前提：roots 已经过 [`normalize_roots`]（canonicalize + 去重）
pub fn resolve_within_roots(roots: &[PathBuf], rel: &str) -> Result<PathBuf, SandboxError> {
    let parts = clean_relative_parts(rel)?;
    if roots.is_empty() {
        return Err(SandboxError::NoRoots);
    }

    // 根别名：rel 首段等于某 root 的最后一段时，视为浏览该 root 本身。
    // 根目录列表（list path=""）以 root 最后一段作为顶层条目名，前端点击
    // 进入会回传该名；若不解到此映射，会解析成 `root/<同名>`（不存在 → 404）。
    // 多段路径（"别名/sub"）同样剥掉别名段后在该 root 下解析——root 内存在
    // 与基名同名的真实子目录时（"别名/别名"），剥离后恰好命中它，语义一致。
    //
    // 歧义取舍：多 root 同名基名时取第一个（与列表顶层条目语义一致，优先保证
    // 导航可达）；别名 root 下解析不到（不存在/逃逸）时落入下方全段循环，
    // 兼容"该路径恰为其他 root 下的真实路径"的场景
    if let Some(first) = parts.first() {
        for root in roots {
            let is_alias = root
                .file_name()
                .map(|n| n.to_string_lossy().as_ref() == first)
                .unwrap_or(false);
            if !is_alias {
                continue;
            }
            if parts.len() == 1 {
                return Ok(root.clone());
            }
            let mut candidate = root.clone();
            for part in &parts[1..] {
                candidate.push(part);
            }
            if !candidate.exists() {
                break;
            }
            let canonical = candidate.canonicalize().map_err(|e| {
                SandboxError::OutsideRoots(format!(
                    "canonicalize failed for '{}': {}",
                    candidate.display(),
                    e
                ))
            })?;
            if canonical.starts_with(root) {
                return Ok(canonical);
            }
            // 逃逸 root（symlink）→ 交给下方全段循环统一报错
            break;
        }
    }

    let mut escape_detected = false;
    for root in roots {
        let mut candidate = root.clone();
        for part in &parts {
            candidate.push(part);
        }
        if !candidate.exists() {
            continue;
        }

        let canonical = candidate.canonicalize().map_err(|e| {
            SandboxError::OutsideRoots(format!(
                "canonicalize failed for '{}': {}",
                candidate.display(),
                e
            ))
        })?;

        if canonical.starts_with(root) {
            return Ok(canonical);
        }
        // 存在但逃出 root（symlink 逃逸）——记录并继续尝试其他 root，
        // 全部失败后以逃逸错误返回而非"未找到"，便于排查
        escape_detected = true;
    }

    if escape_detected {
        Err(SandboxError::OutsideRoots(format!(
            "'{}' resolves outside allowed roots via symlink",
            rel
        )))
    } else {
        Err(SandboxError::OutsideRoots(format!(
            "'{}' not found in any allowed root",
            rel
        )))
    }
}

/// 解析上传目标路径（最终文件可不存在，父目录必须存在）
///
/// 用于上传 session 创建：新文件本身尚不存在无法 canonicalize，
/// 因此 canonicalize 其父目录并校验仍在 root 内，再拼回文件名。
/// 拒绝以根目录本身为上传目标
pub fn resolve_upload_target_within_roots(
    roots: &[PathBuf],
    rel: &str,
) -> Result<PathBuf, SandboxError> {
    let parts = clean_relative_parts(rel)?;
    if parts.is_empty() {
        return Err(SandboxError::Traversal(
            "upload target must be a file path, not the mount root".to_string(),
        ));
    }
    if roots.is_empty() {
        return Err(SandboxError::NoRoots);
    }

    // 根别名（同 resolve_within_roots）：首段为 root 基名时剥掉，在该 root 下
    // 解析上传目标（前端回传 "别名/sub/file" 形态）。单段别名 = root 本身，
    // 不是合法上传目标，直接拒绝；别名 root 下解析不到则落入下方全段循环
    if let Some(first) = parts.first() {
        for root in roots {
            let is_alias = root
                .file_name()
                .map(|n| n.to_string_lossy().as_ref() == first)
                .unwrap_or(false);
            if !is_alias {
                continue;
            }
            if parts.len() == 1 {
                return Err(SandboxError::Traversal(
                    "upload target must be a file path, not the mount root".to_string(),
                ));
            }
            let mut candidate = root.clone();
            for part in &parts[1..] {
                candidate.push(part);
            }

            let (parent, file_name) = match (candidate.parent(), candidate.file_name()) {
                (Some(p), Some(f)) => (p, f.to_owned()),
                _ => continue,
            };
            if !parent.exists() || !parent.is_dir() {
                // 别名 root 下父目录不存在 → 落入全段循环尝试其他 root
                break;
            }

            let canonical_parent = parent.canonicalize().map_err(|e| {
                SandboxError::OutsideRoots(format!(
                    "canonicalize failed for parent '{}': {}",
                    parent.display(),
                    e
                ))
            })?;

            if canonical_parent.starts_with(root) {
                return Ok(canonical_parent.join(file_name));
            }
            // 逃逸 root（symlink）→ 交给下方全段循环统一报错
            break;
        }
    }

    let mut escape_detected = false;
    for root in roots {
        let mut candidate = root.clone();
        for part in &parts {
            candidate.push(part);
        }

        let (parent, file_name) = match (candidate.parent(), candidate.file_name()) {
            (Some(p), Some(f)) => (p, f.to_owned()),
            _ => continue,
        };
        if !parent.exists() || !parent.is_dir() {
            // 父目录不存在 —— 可能落在其他 root 下，继续尝试
            continue;
        }

        let canonical_parent = parent.canonicalize().map_err(|e| {
            SandboxError::OutsideRoots(format!(
                "canonicalize failed for parent '{}': {}",
                parent.display(),
                e
            ))
        })?;

        if canonical_parent.starts_with(root) {
            return Ok(canonical_parent.join(file_name));
        }
        escape_detected = true;
    }

    if escape_detected {
        Err(SandboxError::OutsideRoots(format!(
            "parent of '{}' escapes allowed roots via symlink",
            rel
        )))
    } else {
        Err(SandboxError::OutsideRoots(format!(
            "no allowed root contains the parent directory of '{}'",
            rel
        )))
    }
}

/// 判断绝对路径是否位于任一 root 内（canonicalize 后前缀比较）
///
/// 供上传 session 等已持有绝对路径的场景做二次校验
pub fn is_within_roots(roots: &[PathBuf], path: &Path) -> bool {
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => return false,
    };
    roots.iter().any(|root| canonical.starts_with(root))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 创建临时目录结构：base/{a, a/nested, b}
    fn make_tree() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let base = tempfile::tempdir().expect("tempdir");
        let a = base.path().join("a");
        let nested = a.join("nested");
        let b = base.path().join("b");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(nested.join("file.txt"), b"hello").unwrap();
        (base, a, nested, b)
    }

    #[test]
    fn test_normalize_roots_dedup_nested() {
        let (_base, a, nested, b) = make_tree();
        // nested 是 a 的子目录 → 去重后只剩 a 和 b
        let roots = normalize_roots(&[nested.clone(), a.clone(), b.clone()]).unwrap();
        assert_eq!(roots.len(), 2);
        assert!(roots.iter().any(|r| *r == a.canonicalize().unwrap()));
        assert!(roots.iter().any(|r| *r == b.canonicalize().unwrap()));
    }

    #[test]
    fn test_normalize_roots_rejects_missing_or_file() {
        let (_base, _a, nested, _b) = make_tree();
        assert!(matches!(
            normalize_roots(&[nested.join("does-not-exist")]),
            Err(SandboxError::InvalidRoot { .. })
        ));
        // 文件不能作为 root
        assert!(matches!(
            normalize_roots(&[nested.join("file.txt")]),
            Err(SandboxError::InvalidRoot { .. })
        ));
        assert!(matches!(
            normalize_roots(&[]),
            Err(SandboxError::NoRoots)
        ));
    }

    #[test]
    fn test_resolve_within_roots_ok() {
        let (_base, a, _nested, _b) = make_tree();
        let roots = normalize_roots(&[a.clone()]).unwrap();

        let resolved = resolve_within_roots(&roots, "nested/file.txt").unwrap();
        assert_eq!(resolved, a.canonicalize().unwrap().join("nested/file.txt"));

        // 目录本身也可解析
        let dir = resolve_within_roots(&roots, "nested").unwrap();
        assert!(dir.is_dir());

        // 空相对路径解析为 root 自身
        let root_itself = resolve_within_roots(&roots, "").unwrap();
        assert_eq!(root_itself, a.canonicalize().unwrap());
    }

    #[test]
    fn test_resolve_within_roots_root_alias_multi_segment() {
        let (base, a, nested, _b) = make_tree();
        let roots = normalize_roots(&[a.clone()]).unwrap();

        // 前端回传 "a/nested/file.txt"（首段 "a" 是 root 基名）→ 剥别名段解析
        let resolved = resolve_within_roots(&roots, "a/nested/file.txt").unwrap();
        assert_eq!(resolved, a.canonicalize().unwrap().join("nested/file.txt"));

        // 二级目录
        let dir = resolve_within_roots(&roots, "a/nested").unwrap();
        assert_eq!(dir, nested.canonicalize().unwrap());

        // 不存在的二级路径 → 仍报 OutsideRoots（而非拼出 a/a/nested 误判）
        assert!(matches!(
            resolve_within_roots(&roots, "a/nested/nope.txt"),
            Err(SandboxError::OutsideRoots(_))
        ));

        // root 内存在与基名同名的真实子目录："a/a/xyz.txt" 剥离别名后
        // 恰好命中 a/a/xyz.txt，而非解析成 a/a/a/xyz.txt
        let same = a.join("a");
        fs::create_dir_all(&same).unwrap();
        fs::write(same.join("xyz.txt"), b"x").unwrap();
        let resolved = resolve_within_roots(&roots, "a/a/xyz.txt").unwrap();
        assert_eq!(resolved, same.canonicalize().unwrap().join("xyz.txt"));

        let _ = base;
    }

    #[test]
    fn test_resolve_rejects_traversal() {
        let (_base, a, _nested, _b) = make_tree();
        let roots = normalize_roots(&[a.clone()]).unwrap();

        for evil in ["..", "../b", "nested/../../b", "a/../a/../../etc"] {
            assert!(
                matches!(
                    resolve_within_roots(&roots, evil),
                    Err(SandboxError::Traversal(_))
                ),
                "should reject: {}",
                evil
            );
        }
        // 绝对路径拒绝
        assert!(matches!(
            resolve_within_roots(&roots, "/etc"),
            Err(SandboxError::Traversal(_))
        ));
        // Windows 盘符 / NTFS ADS 拒绝
        assert!(matches!(
            resolve_within_roots(&roots, "nested/file.txt:hidden"),
            Err(SandboxError::Traversal(_))
        ));
    }

    #[test]
    fn test_resolve_missing_path_outside_roots() {
        let (_base, a, _nested, _b) = make_tree();
        let roots = normalize_roots(&[a.clone()]).unwrap();
        assert!(matches!(
            resolve_within_roots(&roots, "nested/nope.txt"),
            Err(SandboxError::OutsideRoots(_))
        ));
    }

    #[test]
    fn test_resolve_rejects_symlink_escape() {
        let (base, a, _nested, b) = make_tree();
        let roots = normalize_roots(&[a.clone()]).unwrap();

        // a/link → b（root 外）：解析 link 必须被拒绝
        let link = a.join("link");
        #[cfg(unix)]
        let created = std::os::unix::fs::symlink(&b, &link).is_ok();
        #[cfg(windows)]
        let created = std::os::windows::fs::symlink_dir(&b, &link).is_ok();

        if !created {
            // Windows 无开发者模式/管理员权限时无法创建 symlink，跳过该场景
            eprintln!("skip symlink test: insufficient privileges to create symlink");
            let _ = base; // base 目录清理交给 TempDir Drop
            return;
        }

        assert!(
            matches!(
                resolve_within_roots(&roots, "link"),
                Err(SandboxError::OutsideRoots(_))
            ),
            "symlink escaping root must be rejected"
        );
    }

    #[test]
    fn test_resolve_upload_target_root_alias() {
        let (_base, a, nested, _b) = make_tree();
        let roots = normalize_roots(&[a.clone()]).unwrap();

        // 前端回传 "a/nested/new.bin"（首段是 root 基名）→ 剥别名段解析
        let target = resolve_upload_target_within_roots(&roots, "a/nested/new.bin").unwrap();
        assert_eq!(target, nested.canonicalize().unwrap().join("new.bin"));
        assert!(!target.exists());

        // 单段别名（root 本身）不是合法上传目标
        assert!(matches!(
            resolve_upload_target_within_roots(&roots, "a"),
            Err(SandboxError::Traversal(_))
        ));

        // 别名 root 下父目录不存在 → 拒绝
        assert!(matches!(
            resolve_upload_target_within_roots(&roots, "a/ghost/new.bin"),
            Err(SandboxError::OutsideRoots(_))
        ));
    }

    #[test]
    fn test_resolve_upload_target() {
        let (_base, a, nested, _b) = make_tree();
        let roots = normalize_roots(&[a.clone()]).unwrap();

        // 新文件（不存在）+ 已存在父目录 → 允许
        let target = resolve_upload_target_within_roots(&roots, "nested/new.bin").unwrap();
        assert_eq!(target, nested.canonicalize().unwrap().join("new.bin"));
        assert!(!target.exists());

        // 父目录不存在 → 拒绝
        assert!(matches!(
            resolve_upload_target_within_roots(&roots, "ghost/new.bin"),
            Err(SandboxError::OutsideRoots(_))
        ));

        // 根目录本身不能作为上传目标
        assert!(matches!(
            resolve_upload_target_within_roots(&roots, ""),
            Err(SandboxError::Traversal(_))
        ));

        // 穿越拒绝
        assert!(matches!(
            resolve_upload_target_within_roots(&roots, "../b/evil.bin"),
            Err(SandboxError::Traversal(_))
        ));
    }

    #[test]
    fn test_is_within_roots() {
        let (_base, a, nested, _b) = make_tree();
        let roots = normalize_roots(&[a.clone()]).unwrap();
        assert!(is_within_roots(&roots, &nested.join("file.txt")));
        assert!(!is_within_roots(&roots, &nested.join("missing.txt")));
    }
}
