//! SAF (Storage Access Framework) Uri → 真实文件系统路径解析
//!
//! Android 的 SAF 选择器返回 `content://` Uri（如
//! `content://com.android.externalstorage.documents/tree/primary%3ADownload`），
//! 而宿主文件服务基于真实文件系统路径（`std::fs`）。本模块把 SAF 的
//! (authority, documentId) 解析为宿主可访问的真实路径。
//!
//! 支持：
//! - `com.android.externalstorage.documents`（主存储 `primary:` 与可移动存储卷号）
//! - `com.android.providers.downloads.documents` 的 `raw:` 前缀（直接携带真实路径）
//!
//! 解析失败返回 `None`，调用方降级为手动路径输入。

/// 解析 SAF Uri（authority + documentId）为真实文件系统路径
///
/// - `primary_dir`：主存储根路径（Android 上通常为 `/storage/emulated/0`）
/// - documentId 形如 `primary:Download/foo`（主存储）或 `ABCD-1234:foo`（SD 卡，
///   挂载于 `/storage/<卷号>`）或 `raw:/storage/emulated/0/Download/a.mp4`（下载 provider）
///
/// 解析失败（不支持的 provider、非法 documentId、含 `..` 逃逸段）返回 `None`。
pub fn resolve_saf_path(authority: &str, document_id: &str, primary_dir: &str) -> Option<String> {
    match authority {
        // 外部存储：primary: 为主存储，其余卷号为可移动存储（SD 卡挂载于 /storage/<卷号>）
        "com.android.externalstorage.documents" => {
            let (volume, rest) = document_id.split_once(':')?;
            if volume.is_empty() {
                return None;
            }
            let base = if volume == "primary" {
                primary_dir.trim_end_matches('/').to_string()
            } else {
                format!("/storage/{}", volume)
            };
            // 防御：拒绝 .. 逃逸段；空段折叠
            let mut segments = Vec::new();
            for seg in rest.split('/') {
                if seg == ".." {
                    return None;
                }
                if !seg.is_empty() && seg != "." {
                    segments.push(seg);
                }
            }
            Some(if segments.is_empty() {
                base
            } else {
                format!("{}/{}", base, segments.join("/"))
            })
        }
        // 下载 provider：raw: 前缀直接携带真实路径
        "com.android.providers.downloads.documents" => document_id
            .strip_prefix("raw:")
            .filter(|p| !p.is_empty())
            .map(|p| p.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY_DIR: &str = "/storage/emulated/0";

    /// 主存储根目录（documentId 无剩余路径）
    #[test]
    fn primary_root() {
        assert_eq!(
            resolve_saf_path(
                "com.android.externalstorage.documents",
                "primary:",
                PRIMARY_DIR,
            ),
            Some("/storage/emulated/0".to_string())
        );
        assert_eq!(
            resolve_saf_path(
                "com.android.externalstorage.documents",
                "primary:",
                "/storage/emulated/0/",
            ),
            Some("/storage/emulated/0".to_string())
        );
    }

    /// 主存储一层子目录
    #[test]
    fn primary_subdir() {
        assert_eq!(
            resolve_saf_path(
                "com.android.externalstorage.documents",
                "primary:Download",
                PRIMARY_DIR,
            ),
            Some("/storage/emulated/0/Download".to_string())
        );
    }

    /// 主存储嵌套子目录
    #[test]
    fn primary_nested() {
        assert_eq!(
            resolve_saf_path(
                "com.android.externalstorage.documents",
                "primary:Download/videos/2024",
                PRIMARY_DIR,
            ),
            Some("/storage/emulated/0/Download/videos/2024".to_string())
        );
    }

    /// 空格与特殊字符保持原样
    #[test]
    fn primary_spaces() {
        assert_eq!(
            resolve_saf_path(
                "com.android.externalstorage.documents",
                "primary:My Files/today's notes",
                PRIMARY_DIR,
            ),
            Some("/storage/emulated/0/My Files/today's notes".to_string())
        );
    }

    /// 可移动存储（SD 卡卷号）挂载于 /storage/<卷号>
    #[test]
    fn sd_card_volume() {
        assert_eq!(
            resolve_saf_path(
                "com.android.externalstorage.documents",
                "ABCD-1234:Download",
                PRIMARY_DIR,
            ),
            Some("/storage/ABCD-1234/Download".to_string())
        );
        assert_eq!(
            resolve_saf_path(
                "com.android.externalstorage.documents",
                "ABCD-1234:",
                PRIMARY_DIR,
            ),
            Some("/storage/ABCD-1234".to_string())
        );
    }

    /// 下载 provider 的 raw: 前缀直接携带真实路径
    #[test]
    fn downloads_raw_prefix() {
        assert_eq!(
            resolve_saf_path(
                "com.android.providers.downloads.documents",
                "raw:/storage/emulated/0/Download/a.mp4",
                PRIMARY_DIR,
            ),
            Some("/storage/emulated/0/Download/a.mp4".to_string())
        );
        // 空 raw 路径不解析
        assert_eq!(
            resolve_saf_path(
                "com.android.providers.downloads.documents",
                "raw:",
                PRIMARY_DIR,
            ),
            None
        );
    }

    /// 不支持的 provider（云盘、媒体库等）→ 降级手动输入
    #[test]
    fn unsupported_provider() {
        assert_eq!(
            resolve_saf_path("com.google.android.apps.docs.storage", "doc:abc", PRIMARY_DIR),
            None
        );
        assert_eq!(
            resolve_saf_path("com.android.providers.media.documents", "image:123", PRIMARY_DIR),
            None
        );
        assert_eq!(resolve_saf_path("", "", PRIMARY_DIR), None);
    }

    /// 非法 documentId：无冒号分隔 / 空卷号
    #[test]
    fn malformed_document_id() {
        assert_eq!(
            resolve_saf_path("com.android.externalstorage.documents", "primary", PRIMARY_DIR),
            None
        );
        assert_eq!(
            resolve_saf_path("com.android.externalstorage.documents", ":Download", PRIMARY_DIR),
            None
        );
        assert_eq!(
            resolve_saf_path("com.android.externalstorage.documents", "", PRIMARY_DIR),
            None
        );
    }

    /// 路径逃逸防御：.. 段被拒绝
    #[test]
    fn path_escape_rejected() {
        assert_eq!(
            resolve_saf_path(
                "com.android.externalstorage.documents",
                "primary:Download/../../etc",
                PRIMARY_DIR,
            ),
            None
        );
        assert_eq!(
            resolve_saf_path(
                "com.android.externalstorage.documents",
                "primary:..",
                PRIMARY_DIR,
            ),
            None
        );
    }

    /// 冗余分隔符与 . 段折叠
    #[test]
    fn path_normalization() {
        assert_eq!(
            resolve_saf_path(
                "com.android.externalstorage.documents",
                "primary:Download//foo/./bar",
                PRIMARY_DIR,
            ),
            Some("/storage/emulated/0/Download/foo/bar".to_string())
        );
    }
}
