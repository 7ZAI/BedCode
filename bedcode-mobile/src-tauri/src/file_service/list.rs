//! 目录列举（v2.1 list 迁移）
//!
//! 手机端不再运行 HTTP server 后，桌面「浏览手机共享目录」改经 WS 控制面：
//! 桌面发 `FileServicePayload::FileListRequest` → 手机本模块列举 → 回
//! `FileListResponse`。本模块自原 `server.rs` 的 `/list` handler 提取为
//! **HTTP 无关**的纯列举函数（同一套 SAF/真实路径/权限 notice 语义），
//! file_service 门面与 handler 复用同一实现，不复制第二份引擎。

use std::path::Path;
use std::sync::Arc;

use bedcode_plugin_api_mobile::FileOperation;

use crate::enums::file_service::ListEntryDto;
use crate::file_service::registry::FileServiceRegistry;
use crate::file_service::saf_tree;
use crate::file_service::upload::is_filtered_listing_name;

/// 列举结果（无 HTTP 语义；错误由调用方映射为 wire error message）
pub struct ListOutcome {
    /// 条目列表（目录优先，按名称排序）
    pub entries: Vec<ListEntryDto>,
    /// 非空时：列表结果可能被 Android 存储权限过滤（对端应提示用户授权）
    pub notice: Option<String>,
}

/// 目录列举（挂载根或挂载内相对路径）
///
/// `rel` 为空 → 挂载根顶层条目（每 root 一个顶层条目 + SAF 树根别名）；
/// 非空 → 先试 SAF 根命中（list_tree 遍历），未命中走真实路径 read_dir。
///
/// 错误语义（与旧 `/list` HTTP 一致，wire 经 error message 透传）：
/// - mount 不存在 / mount 未声明 List 操作 / resolve 越界 → Err
/// - 目录读取失败 → Err
pub async fn list_entries(
    registry: &Arc<FileServiceRegistry>,
    plugin_id: &str,
    mount_path: &str,
    rel: &str,
) -> crate::Result<ListOutcome> {
    let entry = registry.get_entry(plugin_id, mount_path).await?;
    if !entry.operations.contains(&FileOperation::List) {
        return Err(crate::AppError::InvalidInput(format!(
            "operation 'list' not allowed for mount '{}'",
            entry.mount_path
        )));
    }

    let rel = rel.trim_matches('/').to_string();

    // 挂载根列举：真实路径根 + SAF 树根作为顶层条目
    if rel.is_empty() {
        let mut entries = Vec::new();
        for root in &entry.roots {
            match std::fs::metadata(root) {
                Ok(meta) if meta.is_dir() => {
                    entries.push(ListEntryDto {
                        name: root
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| root.display().to_string()),
                        size: 0,
                        mtime: mtime_unix_secs(&meta),
                        is_dir: true,
                    });
                }
                _ => {
                    // root 失效：该 root 下线、其余正常
                    tracing::warn!(
                        plugin_id = %plugin_id,
                        mount = %mount_path,
                        root = %root.display(),
                        "list: root unavailable, skipped"
                    );
                }
            }
        }
        // SAF 树根：别名作为顶层条目（可导航；授权有效性在遍历时校验）
        for saf_root in &entry.saf_roots {
            match saf_tree::tree_alias(saf_root) {
                Some(alias) => entries.push(ListEntryDto {
                    name: alias,
                    size: 0,
                    mtime: 0,
                    is_dir: true,
                }),
                None => tracing::warn!(
                    plugin_id = %plugin_id,
                    mount = %mount_path,
                    root = %saf_root,
                    "list: invalid SAF root skipped"
                ),
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        return Ok(ListOutcome { entries, notice: None });
    }

    // SAF 根命中：list_tree 遍历（无 needs_all_files_access notice 语义）
    if let Some((tree_uri, parts)) = saf_tree::match_saf_root(&entry.saf_roots, &rel) {
        return list_saf_dir(registry, &tree_uri, &parts, &rel).await;
    }

    let target = registry.resolve_sandboxed(plugin_id, mount_path, &rel).await?;

    // spawn_blocking 会 move target，权限判定提前计算
    let may_need_all_files_access = needs_all_files_access(&target);

    match tokio::task::spawn_blocking(move || read_dir_entries(&target)).await {
        Ok(Ok(entries)) => {
            // Android 分区存储：未授权时 read_dir 静默返回空列表（不报错）——
            // 空结果 + 需要该权限 ≈ 权限问题而非真空目录，经 notice 告知对端
            let notice = if entries.is_empty() && may_need_all_files_access {
                tracing::warn!(
                    path = %rel,
                    "list: empty result in top-level storage dir; MANAGE_EXTERNAL_STORAGE may not be granted"
                );
                Some("all_files_access_may_be_required".to_string())
            } else {
                None
            };
            Ok(ListOutcome { entries, notice })
        }
        Ok(Err(e)) => Err(e),
        Err(e) => Err(crate::AppError::Internal(format!(
            "list task failed: {}",
            e
        ))),
    }
}

/// SAF 目录列举：walk 到目标目录 → list_tree 子条目
///
/// 无 needs_all_files_access notice 语义（SAF 条目经持久化授权，分区存储
/// 过滤不适用）；列表字段与真实路径一致（SAF 条目无 mtime，置 0）。
async fn list_saf_dir(
    registry: &Arc<FileServiceRegistry>,
    tree_uri: &str,
    parts: &[String],
    rel: &str,
) -> crate::Result<ListOutcome> {
    let saf = registry.saf_io().await.ok_or_else(|| {
        crate::AppError::Internal("SAF storage unavailable on this platform".to_string())
    })?;
    let root_doc = saf_tree::tree_document_id(tree_uri)
        .ok_or_else(|| crate::AppError::InvalidInput(format!("invalid SAF root: {}", tree_uri)))?;
    let target = saf_tree::walk_to_entry(saf.as_ref(), tree_uri, &root_doc, parts).await?;
    if !target.is_dir {
        return Err(crate::AppError::NotFound(format!(
            "'{}' is not a directory",
            rel
        )));
    }
    let children = saf.list_tree(tree_uri, &target.document_id)?;
    let mut entries: Vec<ListEntryDto> = children
        .into_iter()
        // 过滤上传/中转临时文件（*.part），与真实路径列表规则一致
        .filter(|c| !is_filtered_listing_name(&c.name))
        .map(|c| ListEntryDto {
            name: c.name,
            size: if c.is_dir { 0 } else { c.size.max(0) as u64 },
            mtime: 0,
            is_dir: c.is_dir,
        })
        .collect();
    // 目录优先，按名称排序，保证两端 UI 展示一致
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(ListOutcome {
        entries,
        notice: None,
    })
}

/// 文件修改时间（Unix 秒，读取失败为 0）
fn mtime_unix_secs(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 同步读取目录条目（spawn_blocking 中执行）
///
/// root 失效（删除/移动/权限回收）时 read_dir 失败 → 明确错误
fn read_dir_entries(dir: &Path) -> crate::Result<Vec<ListEntryDto>> {
    if !dir.is_dir() {
        return Err(crate::AppError::NotFound(format!(
            "'{}' is not a directory",
            dir.display()
        )));
    }
    let read_dir = std::fs::read_dir(dir).map_err(|e| {
        crate::AppError::Internal(format!(
            "failed to read directory '{}' (root may have been removed or permission revoked): {}",
            dir.display(),
            e
        ))
    })?;

    let mut entries = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(|e| {
            crate::AppError::Internal(format!(
                "failed to read entry in '{}': {}",
                dir.display(),
                e
            ))
        })?;
        let name = entry.file_name().to_string_lossy().to_string();
        // 过滤上传临时文件（*.part），不向对端暴露
        if is_filtered_listing_name(&name) {
            continue;
        }
        let meta = entry.metadata().map_err(|e| {
            crate::AppError::Internal(format!(
                "failed to read metadata of '{}': {}",
                entry.path().display(),
                e
            ))
        })?;
        entries.push(ListEntryDto {
            name,
            size: meta.len(),
            mtime: mtime_unix_secs(&meta),
            is_dir: meta.is_dir(),
        });
    }
    // 目录优先，按名称排序，保证两端 UI 展示一致
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

/// 判断路径是否需要「所有文件访问权限」（MANAGE_EXTERNAL_STORAGE）
///
/// Android 11+ 分区存储：仅 App 私有目录无需授权；其余主存储路径的 read_dir
/// 受 FUSE 过滤，未授权时静默返回空列表（不报错）。返回 true 且列表为空时，
/// 对端几乎可以确定是权限问题而非真空目录。
fn needs_all_files_access(path: &Path) -> bool {
    let p = path.to_string_lossy().replace('\\', "/");
    let normalized = p.trim_end_matches('/').to_lowercase();
    if !normalized.starts_with("/storage/emulated/0") {
        return false;
    }
    !normalized.starts_with("/storage/emulated/0/android/data")
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_dir_entries_filters_part_files() {
        let dir = std::env::temp_dir().join(format!("bedcode-list-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("keep.txt"), b"x").unwrap();
        std::fs::write(dir.join(".bedcode-upload-abc.part"), b"y").unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();

        let entries = read_dir_entries(&dir).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // .part 被过滤；目录优先
        assert!(names.contains(&"keep.txt"));
        assert!(names.contains(&"sub"));
        assert!(!names.iter().any(|n| n.contains(".part")));
        assert_eq!(names.first().copied(), Some("sub"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_mtime_unix_secs_missing_meta_is_zero() {
        // 不存在的文件 metadata 不可得，不 panic（由调用方负责错误路径）
        assert!(mtime_unix_secs(&std::fs::metadata("no-such-file").unwrap_or_else(|e| {
            // 构造一个有效 metadata：临时文件
            let dir = std::env::temp_dir();
            std::fs::metadata(dir).unwrap()
        })) > 0);
    }

    #[test]
    fn test_needs_all_files_access() {
        assert!(!needs_all_files_access(Path::new("/data/user/0/com.bedcode.mobile/files")));
        assert!(needs_all_files_access(Path::new("/storage/emulated/0/DCIM/Camera")));
        assert!(!needs_all_files_access(Path::new("/storage/emulated/0/android/data/pkg")));
        assert!(!needs_all_files_access(Path::new("/sdcard0/Download")));
    }
}
