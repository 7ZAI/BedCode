//! 平台缝实现：Android SAF 共享访问 / MediaStore 落点提升 / 内置共享条目。
//!
//! `PeerPlatformSeam` 三处差异面都只是参数注入（peer-net 引擎已预留接缝），
//! 能力域自身零平台代码：
//!
//! - `shared_saf_access`：SAF 共享目录访问（仅 Android；dev 窗口无此形态返回
//!   None，注册表若存在 saf 条目会按 read-failed 拒绝，防御性兜底）；
//! - `file_landing`：落点提升钩子（私有下载目录中的完整文件尽力写入
//!   MediaStore.Downloads，失败保留私有副本 = 回退语义）；
//! - `builtin_shared_dirs`：内置免授权共享条目（app 私有下载目录，解析失败仅
//!   跳过条目，不阻断启动）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bedcode_server_peer_net::engine_ports::PeerPlatformSeam;

use super::ports::downloads_dir_cache;

// ==================== 平台缝（SAF / 落点提升 / 内置共享条目） ====================

/// 移动平台缝：三处差异面都只是参数注入（peer-net 引擎已预留接缝），
/// 能力域自身零平台代码
pub(crate) struct MobilePlatformSeam;

impl PeerPlatformSeam for MobilePlatformSeam {
    /// Android SAF 共享目录访问（无 SAF 平台返回 None——dev 窗口无此形态，
    /// 注册表若存在 saf 条目会按 read-failed 拒绝，防御性兜底）
    fn shared_saf_access(&self) -> Option<Arc<dyn bedcode_peer_net::SharedSafAccess>> {
        #[cfg(target_os = "android")]
        {
            Some(Arc::new(android_saf_bridge::SafSharedAccess::new(
                crate::plugin::saf_io::default_saf_io(),
            )))
        }
        #[cfg(not(target_os = "android"))]
        {
            None
        }
    }

    /// 落点提升钩子：私有下载目录中的完整文件尽力写入 MediaStore.Downloads
    /// （下载目录不可用时与集中化前一致：不提升）
    fn file_landing(&self) -> Option<Arc<dyn bedcode_peer_net::FileLanding>> {
        downloads_dir_cache().map(|_| {
            let io = crate::plugin::saf_io::default_saf_io();
            Arc::new(MediaLanding { saf: io }) as Arc<dyn bedcode_peer_net::FileLanding>
        })
    }

    /// 内置免授权共享条目：app 私有下载目录（解析失败仅跳过条目，不阻断启动）
    fn builtin_shared_dirs(&self) -> Vec<(String, PathBuf)> {
        downloads_dir_cache()
            .map(|dir| vec![("下载".to_string(), PathBuf::from(dir))])
            .unwrap_or_default()
    }
}

// ==================== 平台缝：SAF 桥（Android） ====================
#[cfg(target_os = "android")]
mod android_saf_bridge {
    use super::*;
    use crate::file_service::saf_tree;
    use crate::plugin::saf_io::SafEntry;
    use bedcode_peer_net::SeqReader;

    /// SAF 共享目录访问适配器：把既有 SafIo（Kotlin 桥）映射到 crate 的
    /// `SharedSafAccess` 缝。相对路径解析用 list_tree 逐层下降（与
    /// file_service/saf_tree 的 walk 同构，此处同步化以匹配 trait 签名）。
    pub(crate) struct SafSharedAccess {
        io: Arc<dyn crate::plugin::saf_io::SafIo>,
    }

    impl SafSharedAccess {
        pub(crate) fn new(io: Arc<dyn crate::plugin::saf_io::SafIo>) -> Self {
            Self { io }
        }

        /// 同步逐层下降到 rel 指向的条目（parts 已过 crate 路径清洗）
        fn walk(&self, tree_uri: &str, root_doc_id: &str, parts: &[String]) -> bedcode_peer_net::Result<SafEntry> {
            use bedcode_peer_net::{saf_not_found, PeerNetError};

            if parts.is_empty() {
                return Ok(self.root_entry(tree_uri, root_doc_id));
            }
            let mut doc_id = root_doc_id.to_string();
            let mut current = self.root_entry(tree_uri, root_doc_id);
            for part in parts {
                let children = self.io.list_tree(tree_uri, &doc_id).map_err(|e| {
                    tracing::warn!(tree_uri, doc_id = %doc_id, error = %e, "saf walk list_tree failed");
                    PeerNetError::TransferSession {
                        role: "saf",
                        detail: format!("list_tree failed (permission may be revoked): {e}"),
                    }
                })?;
                let child = children
                    .into_iter()
                    .find(|c| &c.name == part)
                    .ok_or_else(|| saf_not_found(format!("'{part}' in tree {tree_uri}")))?;
                doc_id = child.document_id.clone();
                current = child;
            }
            Ok(current)
        }

        fn root_entry(&self, tree_uri: &str, root_doc_id: &str) -> SafEntry {
            SafEntry {
                name: saf_tree::tree_alias(tree_uri).unwrap_or_else(|| tree_uri.to_string()),
                is_dir: true,
                size: 0,
                mime: String::new(),
                uri: tree_uri.to_string(),
                document_id: root_doc_id.to_string(),
            }
        }
    }

    impl bedcode_peer_net::SharedSafAccess for SafSharedAccess {
        fn list_dir(&self, tree_uri: &str, rel: &str) -> bedcode_peer_net::Result<Vec<bedcode_peer_net::DirEntry>> {
            use bedcode_peer_net::{resolve_rel_path, saf_not_found, PeerNetError};

            let root_doc_id = saf_tree::tree_document_id(tree_uri).ok_or_else(|| PeerNetError::TransferSession {
                role: "saf",
                detail: format!("invalid saf tree uri: {tree_uri}"),
            })?;
            let parts = resolve_rel_path(rel).ok_or_else(|| saf_not_found(rel))?;
            let target = self.walk(tree_uri, &root_doc_id, &parts)?;
            if !target.is_dir {
                return Err(saf_not_found(format!("'{rel}' is not a directory")));
            }
            let children =
                self.io
                    .list_tree(tree_uri, &target.document_id)
                    .map_err(|e| PeerNetError::TransferSession {
                        role: "saf",
                        detail: format!("list_tree failed (permission may be revoked): {e}"),
                    })?;
            Ok(children
                .into_iter()
                .map(|c| bedcode_peer_net::DirEntry::new(c.name, c.is_dir, c.size.max(0) as u64))
                .collect())
        }

        fn open_read(&self, tree_uri: &str, rel: &str) -> bedcode_peer_net::Result<Arc<dyn SeqReader>> {
            use bedcode_peer_net::{resolve_rel_path, saf_not_found, PeerNetError};

            let root_doc_id = saf_tree::tree_document_id(tree_uri).ok_or_else(|| PeerNetError::TransferSession {
                role: "saf",
                detail: format!("invalid saf tree uri: {tree_uri}"),
            })?;
            let parts = resolve_rel_path(rel).ok_or_else(|| saf_not_found(rel))?;
            let entry = self.walk(tree_uri, &root_doc_id, &parts)?;
            if entry.is_dir {
                return Err(saf_not_found(format!("'{rel}' is a directory")));
            }
            let handle = self
                .io
                .open_stream(&entry.uri, 0)
                .map_err(|e| PeerNetError::TransferSession {
                    role: "saf",
                    detail: format!("open stream for '{rel}' failed: {e}"),
                })?;
            // 大小真源：条目元数据优先（OpenableColumns.SIZE），缺失回退流探测值
            let size = if entry.size > 0 { entry.size as u64 } else { handle.size };
            Ok(Arc::new(SafSeqReader {
                io: Arc::clone(&self.io),
                uri: entry.uri,
                handle_id: handle.handle_id,
                size,
            }))
        }
    }

    /// SAF 流顺序读取器：定位读 = seek_stream + read_stream（Kotlin 句柄复用），
    /// Drop 时尽力关闭底层 fd
    struct SafSeqReader {
        io: Arc<dyn crate::plugin::saf_io::SafIo>,
        uri: String,
        handle_id: String,
        size: u64,
    }

    impl SeqReader for SafSeqReader {
        fn size(&self) -> u64 {
            self.size
        }

        fn read_at(&self, offset: u64, cap: usize) -> std::io::Result<Vec<u8>> {
            self.io.seek_stream(&self.handle_id, offset).map_err(io_err)?;
            self.io.read_stream(&self.handle_id, cap).map_err(io_err)
        }
    }

    impl Drop for SafSeqReader {
        fn drop(&mut self) {
            if let Err(e) = self.io.close_stream(&self.handle_id) {
                tracing::debug!(handle_id = %self.handle_id, uri = %self.uri, "close saf stream failed: {e}");
            }
        }
    }

    fn io_err(e: crate::AppError) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
    }
} // mod android_saf_bridge

// ==================== 平台缝：落点提升（MediaStore 回退语义） ====================

/// 落地钩子：私有下载目录中的完整文件尽力写入 MediaStore.Downloads 公共下载
///
/// 成功 → 删除私有副本（公共目录为唯一副本，对齐 legacy upload 语义）；
/// 失败 → 保留私有副本（即「回退私有目录」）。阻塞桥接由 KotlinSafIo 内部承担
/// （scoped-thread 惯例）。
struct MediaLanding {
    saf: Arc<dyn crate::plugin::saf_io::SafIo>,
}

impl bedcode_peer_net::FileLanding for MediaLanding {
    fn landed(&self, final_path: &Path, display_name: &str) {
        let src = final_path.to_string_lossy().into_owned();
        match self
            .saf
            .write_media_downloads(&src, display_name, mime_type_for(display_name))
        {
            Ok(()) => match std::fs::remove_file(final_path) {
                Ok(()) => tracing::info!(
                    display_name = %display_name,
                    "pulled file promoted to MediaStore.Downloads (private copy removed)"
                ),
                Err(e) => tracing::warn!(
                    path = %final_path.display(),
                    error = %e,
                    "media promote ok but private copy cleanup failed"
                ),
            },
            Err(e) => {
                tracing::info!(
                    path = %final_path.display(),
                    error = %e,
                    "MediaStore write failed, keeping private copy (fallback)"
                );
            }
        }
    }
}

/// 展示名 → MIME（轻量扩展名映射；未知走 octet-stream）
fn mime_type_for(name: &str) -> &'static str {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "txt" | "md" => "text/plain",
        "json" => "application/json",
        "apk" => "application/vnd.android.package-archive",
        _ => "application/octet-stream",
    }
}

// ==================== 测试（MediaStore 回退语义） ====================

#[cfg(test)]
mod share_landing_tests {
    use super::*;
    use crate::plugin::saf_io::{SafCopyHandle, SafCopyStatus, SafEntry, SafStreamHandle};
    // trait 方法调用需要 trait 在作用域内
    use bedcode_peer_net::FileLanding as _;

    /// fake SafIo：write_media_downloads 结果可编程（None = 成功），其余方法不可达
    struct FakeSaf {
        fail_with: Option<String>,
    }

    impl crate::plugin::saf_io::SafIo for FakeSaf {
        fn list_tree(&self, _t: &str, _d: &str) -> crate::Result<Vec<SafEntry>> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn read_to_cache(&self, _u: &str, _n: &str) -> crate::Result<SafCopyHandle> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn copy_status(&self, _c: &str) -> crate::Result<SafCopyStatus> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn cancel_copy(&self, _c: &str) -> crate::Result<()> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn cleanup_stale_copies(&self) -> crate::Result<()> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn check_authorized(&self, _t: &str) -> crate::Result<bool> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn write_media_downloads(&self, _src: &str, _name: &str, _mime: &str) -> crate::Result<()> {
            match &self.fail_with {
                Some(reason) => Err(crate::AppError::Plugin(reason.clone())),
                None => Ok(()),
            }
        }
        fn open_stream(&self, _u: &str, _o: u64) -> crate::Result<SafStreamHandle> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn read_stream(&self, _h: &str, _l: usize) -> crate::Result<Vec<u8>> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn seek_stream(&self, _h: &str, _o: u64) -> crate::Result<()> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn close_stream(&self, _h: &str) -> crate::Result<()> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn stream_seekable(&self, _u: &str) -> crate::Result<bool> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn save_to_document(&self, _s: &str, _n: &str, _m: &str) -> crate::Result<()> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
    }

    fn make_private_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"pulled-bytes").expect("write private file");
        path
    }

    /// MediaStore 写入成功 → 私有副本删除（公共目录为唯一副本）
    #[test]
    fn media_promote_success_removes_private_copy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = make_private_file(dir.path(), "report.pdf");

        let landing = MediaLanding {
            saf: Arc::new(FakeSaf { fail_with: None }),
        };
        landing.landed(&file, "report.pdf");

        assert!(
            !file.exists(),
            "private copy must be removed after successful MediaStore promotion"
        );
    }

    /// MediaStore 写入失败（如 API<29 / 存储不可用）→ 私有副本保留 = 回退语义
    #[test]
    fn media_promote_failure_keeps_private_copy_fallback() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = make_private_file(dir.path(), "clip.mp4");

        let landing = MediaLanding {
            saf: Arc::new(FakeSaf {
                fail_with: Some("media_store unavailable (api < 29)".to_string()),
            }),
        };
        landing.landed(&file, "clip.mp4");

        assert!(
            file.exists(),
            "fallback semantics: private copy must survive failed promotion"
        );
        assert_eq!(std::fs::read(&file).expect("intact"), b"pulled-bytes");
    }

    #[test]
    fn mime_mapping_covers_common_types_with_octet_stream_fallback() {
        assert_eq!(mime_type_for("photo.JPG"), "image/jpeg");
        assert_eq!(mime_type_for("video.mp4"), "video/mp4");
        assert_eq!(mime_type_for("notes.txt"), "text/plain");
        assert_eq!(mime_type_for("app.apk"), "application/vnd.android.package-archive");
        assert_eq!(mime_type_for("noext"), "application/octet-stream");
    }
}
