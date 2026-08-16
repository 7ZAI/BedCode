//! 上传会话管理（upload session 模型，规格 4.4 节）
//!
//! 与桌面端 `bedcode-desktop/src-tauri/src/plugin/file_service/upload.rs` 同源
//! （两端同逻辑，按项目惯例各自维护一份）。
//!
//! 流程：POST /upload 创建 session → PUT 从服务端已收偏移 append →
//! POST complete 原子 rename 落位 / DELETE 取消清理。
//!
//! 临时文件放在目标目录内（`.bedcode-upload-{sessionId}.part`），
//! 同卷保证 rename 原子落位，避免跨卷双倍 IO。
//! 「同名即拒」在 session 创建前的策略钩子完成；complete 时目标已存在
//! 视为竞态失败（duplicate-name），保留 .part 供用户决定。

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// 上传临时文件前缀
pub const UPLOAD_PART_PREFIX: &str = ".bedcode-upload-";
/// 上传临时文件后缀
pub const UPLOAD_PART_SUFFIX: &str = ".part";

/// 会话空闲 TTL：24 小时无活动自动清理（规格 4.4，宿主侧可配置见 Notes）
const SESSION_TTL: Duration = Duration::from_secs(24 * 3600);
/// sweeper 扫描间隔：每小时一次
const SWEEP_INTERVAL: Duration = Duration::from_secs(3600);

/// 上传会话错误（controller 据此映射 HTTP 状态码）
#[derive(Debug, thiserror::Error)]
pub enum UploadSessionError {
    /// session 不存在（已过期/已取消/未创建）
    #[error("upload session not found: {0}")]
    NotFound(String),
    /// append 偏移不一致（客户端必须从服务端已收偏移续传）→ HTTP 409
    #[error("offset mismatch: server has {expected} bytes, client sent offset {got}")]
    OffsetMismatch {
        /// 服务端已收字节数
        expected: u64,
        /// 客户端声明的偏移
        got: u64,
    },
    /// complete 时目标文件已存在（竞态）→ HTTP 409 duplicate-name，保留 .part
    #[error("target file already exists: {0}")]
    DuplicateName(PathBuf),
    /// 底层 IO 错误
    #[error("upload session io failed: {0}")]
    Io(#[from] std::io::Error),
}

/// MediaStore 落位结果：区分「同名拒绝」（任务终态失败，.part 保留）与
/// 其他失败（回退私有目录 rename，原 complete 语义）
pub enum PlacementError {
    /// 目标（公共 Download 目录）已存在同名文件
    Duplicate(String),
    /// 其他失败（MediaStore 不可用、IO 错误等）
    Other(String),
}

/// 单个上传会话
pub struct UploadSession {
    /// 会话 ID（UUID v4）
    pub id: String,
    /// 所属插件（session 访问按 plugin + mount 鉴权）
    pub plugin_id: String,
    /// 所属挂载点
    pub mount_path: String,
    /// 最终目标路径（沙箱内绝对路径）
    pub target: PathBuf,
    /// 临时文件路径（目标目录内 `.bedcode-upload-{id}.part`）
    pub tmp: PathBuf,
    /// 客户端声明的总大小（字节，用于进度展示）
    pub size: u64,
    /// 已接收字节数
    pub received: u64,
    /// 最后活动时间（TTL 清理依据）
    pub last_active: Instant,
}

/// 上传会话管理器
pub struct UploadSessionManager {
    sessions: Mutex<HashMap<String, UploadSession>>,
}

impl UploadSessionManager {
    /// 创建空管理器（后台 sweeper 需另行 [`spawn_sweeper`](Self::spawn_sweeper)）
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// 创建会话（不预建临时文件，首个 append 时创建）
    ///
    /// 调用方必须已完成沙箱校验与策略钩子（拒绝发生在写任何字节前）
    pub async fn create(
        &self,
        plugin_id: &str,
        mount_path: &str,
        target: PathBuf,
        size: u64,
    ) -> Result<UploadSession, UploadSessionError> {
        let id = uuid::Uuid::new_v4().to_string();
        let tmp = match target.parent() {
            Some(parent) => parent.join(format!(
                "{}{}{}",
                UPLOAD_PART_PREFIX, id, UPLOAD_PART_SUFFIX
            )),
            None => {
                return Err(UploadSessionError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("upload target '{}' has no parent directory", target.display()),
                )))
            }
        };

        let session = UploadSession {
            id: id.clone(),
            plugin_id: plugin_id.to_string(),
            mount_path: mount_path.to_string(),
            target,
            tmp,
            size,
            received: 0,
            last_active: Instant::now(),
        };
        let snapshot = SessionSnapshot::from(&session);
        self.sessions.lock().await.insert(id, session);
        Ok(snapshot.into())
    }

    /// 查询会话（含归属校验：plugin + mount 必须匹配，防止跨插件劫持 session）
    pub async fn get(&self, sid: &str, plugin_id: &str, mount_path: &str) -> Option<UploadSession> {
        let sessions = self.sessions.lock().await;
        sessions.get(sid).and_then(|s| {
            if s.plugin_id == plugin_id && s.mount_path == mount_path {
                Some(SessionSnapshot::from(s).into())
            } else {
                None
            }
        })
    }

    /// 追加数据块（校验 offset == 已收字节数，否则 OffsetMismatch → HTTP 409）
    ///
    /// 返回追加后的已收字节数。数据块先经挂载点的 TransportCipher 解密
    /// 再落盘（调用方负责）。
    ///
    /// 实现说明：管理器级 Mutex 覆盖 append 的文件 IO，单块 ≤1MB 时持锁时间
    /// 在毫秒级；MVP 并发上限 8 任务可接受，后续可按 session 拆锁
    pub async fn append(
        &self,
        sid: &str,
        plugin_id: &str,
        mount_path: &str,
        offset: u64,
        chunk: &[u8],
    ) -> Result<u64, UploadSessionError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(sid)
            .filter(|s| s.plugin_id == plugin_id && s.mount_path == mount_path)
            .ok_or_else(|| UploadSessionError::NotFound(sid.to_string()))?;

        if offset != session.received {
            return Err(UploadSessionError::OffsetMismatch {
                expected: session.received,
                got: offset,
            });
        }

        if !chunk.is_empty() {
            // create + append：首块创建临时文件，后续块顺序追加
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&session.tmp)?;
            file.write_all(chunk)?;
            file.flush()?;
            session.received += chunk.len() as u64;
        }
        session.last_active = Instant::now();
        Ok(session.received)
    }

    /// 完成上传：临时文件原子 rename 到目标名
    ///
    /// 目标已存在 → DuplicateName（保留 .part，由发起方决定重试或放弃）。
    /// rename 成功后移除 session
    pub async fn complete(
        &self,
        sid: &str,
        plugin_id: &str,
        mount_path: &str,
    ) -> Result<PathBuf, UploadSessionError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get(sid)
            .filter(|s| s.plugin_id == plugin_id && s.mount_path == mount_path)
            .ok_or_else(|| UploadSessionError::NotFound(sid.to_string()))?;

        if session.target.exists() {
            return Err(UploadSessionError::DuplicateName(session.target.clone()));
        }
        if !session.tmp.exists() {
            return Err(UploadSessionError::NotFound(format!(
                "{} (temp file missing)",
                sid
            )));
        }

        let target = session.target.clone();
        let tmp = session.tmp.clone();
        // 先移除 session 再 rename：rename 失败时 .part 保留，session 不复活
        // （重复 complete 会返回 NotFound，语义安全）
        let session = sessions.remove(sid).expect("session present after get_mut");
        drop(sessions);

        if let Err(e) = std::fs::rename(&tmp, &target) {
            // 还原 session 便于客户端查询最终状态；.part 保留
            self.sessions.lock().await.insert(sid.to_string(), session);
            return Err(UploadSessionError::Io(e));
        }
        Ok(target)
    }

    /// 完成上传（M2 落位扩展）：先尝试外部落位回调（MediaStore 公共下载），
    /// 失败自动回退 rename 到目标名（私有下载目录，原 complete 语义）
    ///
    /// 目标已存在 → DuplicateName（保留 .part）；临时文件缺失 → NotFound。
    /// 落位回调返回 Err 视为 MediaStore 写入失败（原因仅记录日志），
    /// 回退 rename 成功仍返回 Ok(target)——调用方无需感知回退发生。
    /// 本方法不引入 tauri/SafIo 依赖（回调由调用方注入，保持本模块可独立单测）。
    pub async fn complete_to_media<F>(
        &self,
        sid: &str,
        plugin_id: &str,
        mount_path: &str,
        place: F,
    ) -> Result<PathBuf, UploadSessionError>
    where
        F: FnOnce(&Path, &str) -> Result<(), PlacementError>,
    {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get(sid)
            .filter(|s| s.plugin_id == plugin_id && s.mount_path == mount_path)
            .ok_or_else(|| UploadSessionError::NotFound(sid.to_string()))?;

        if session.target.exists() {
            return Err(UploadSessionError::DuplicateName(session.target.clone()));
        }
        if !session.tmp.exists() {
            return Err(UploadSessionError::NotFound(format!(
                "{} (temp file missing)",
                sid
            )));
        }

        let target = session.target.clone();
        let tmp = session.tmp.clone();
        // 展示名 = 最终目标文件名（临时文件名 `.bedcode-upload-{sid}.part`
        // 对用户无意义，不能作为公共下载目录的文件名）
        let display_name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // 先移除 session：任一落位路径失败（rename 失败）时 .part 保留，
        // session 不复活（与 complete 相同的语义安全）
        let session = sessions.remove(sid).expect("session present after get_mut");
        drop(sessions);

        match place(&tmp, &display_name) {
            // MediaStore 写入成功：删除临时文件（公共目录为唯一副本）
            Ok(()) => {
                if let Err(e) = std::fs::remove_file(&tmp) {
                    // 删失败仅告警：残留 `.part` 由下次挂载的孤儿清理兜底，
                    // 公共目录副本已可用，不阻断成功响应
                    tracing::warn!(
                        session_id = %sid,
                        tmp = %tmp.display(),
                        "complete_to_media: remove temp after media placement failed: {}",
                        e
                    );
                }
                Ok(target)
            }
            // 同名拒绝：任务终态失败（409 duplicate-name），.part 保留，
            // 不回退私有目录（避免覆盖私有目录既有同名文件）
            Err(PlacementError::Duplicate(reason)) => {
                self.sessions.lock().await.insert(sid.to_string(), session);
                tracing::warn!(
                    session_id = %sid,
                    reason = %reason,
                    "complete_to_media: duplicate-name, keeping .part"
                );
                Err(UploadSessionError::DuplicateName(target))
            }
            // MediaStore 其他失败：回退 rename 到私有下载目录（原语义）
            Err(PlacementError::Other(reason)) => {
                tracing::warn!(
                    session_id = %sid,
                    reason = %reason,
                    "complete_to_media: media placement failed, falling back to private rename"
                );
                if let Err(e) = std::fs::rename(&tmp, &target) {
                    // 还原 session 便于客户端查询最终状态；.part 保留
                    self.sessions.lock().await.insert(sid.to_string(), session);
                    return Err(UploadSessionError::Io(e));
                }
                Ok(target)
            }
        }
    }

    /// 取消会话：移除 session 并删除临时文件（删失败仅告警）
    pub async fn cancel(&self, sid: &str, plugin_id: &str, mount_path: &str) -> Result<(), UploadSessionError> {
        let mut sessions = self.sessions.lock().await;
        let matches = sessions
            .get(sid)
            .map(|s| s.plugin_id == plugin_id && s.mount_path == mount_path)
            .unwrap_or(false);
        if !matches {
            return Err(UploadSessionError::NotFound(sid.to_string()));
        }
        let session = sessions.remove(sid).expect("session present after check");
        drop(sessions);

        if session.tmp.exists() {
            if let Err(e) = std::fs::remove_file(&session.tmp) {
                tracing::warn!(
                    session_id = %sid,
                    tmp = %session.tmp.display(),
                    "cancel: failed to remove temp file: {}",
                    e
                );
            }
        }
        Ok(())
    }

    /// 取消某挂载点下的全部会话（卸载/停用时调用）
    pub async fn cancel_for_mount(&self, plugin_id: &str, mount_path: &str) -> usize {
        let ids: Vec<String> = {
            let sessions = self.sessions.lock().await;
            sessions
                .iter()
                .filter(|(_, s)| s.plugin_id == plugin_id && s.mount_path == mount_path)
                .map(|(id, _)| id.clone())
                .collect()
        };
        let mut cancelled = 0;
        for id in ids {
            if self.cancel(&id, plugin_id, mount_path).await.is_ok() {
                cancelled += 1;
            }
        }
        cancelled
    }

    /// 按插件取消单个会话（v2 接收端本地取消；归属校验在内部按 session 记录完成）
    ///
    /// 与 [`cancel`](Self::cancel) 的区别：不需要调用方提供 mount_path——
    /// session 记录自带归属（plugin + mount），宿主命令层只有 plugin_id + session_id。
    pub async fn cancel_for_plugin(&self, sid: &str, plugin_id: &str) -> Result<(), UploadSessionError> {
        let mut sessions = self.sessions.lock().await;
        let matches = sessions
            .get(sid)
            .map(|s| s.plugin_id == plugin_id)
            .unwrap_or(false);
        if !matches {
            return Err(UploadSessionError::NotFound(sid.to_string()));
        }
        let session = sessions.remove(sid).expect("session present after check");
        drop(sessions);

        if session.tmp.exists() {
            if let Err(e) = std::fs::remove_file(&session.tmp) {
                tracing::warn!(
                    session_id = %sid,
                    tmp = %session.tmp.display(),
                    "cancel_for_plugin: failed to remove temp file: {}",
                    e
                );
            }
        }
        Ok(())
    }

    /// 清理超过 TTL 无活动的会话（返回清理数量）
    pub async fn sweep_expired(&self) -> usize {
        let now = Instant::now();
        let expired: Vec<UploadSession> = {
            let mut sessions = self.sessions.lock().await;
            let expired_ids: Vec<String> = sessions
                .iter()
                .filter(|(_, s)| now.duration_since(s.last_active) > SESSION_TTL)
                .map(|(id, _)| id.clone())
                .collect();
            expired_ids
                .into_iter()
                .filter_map(|id| sessions.remove(&id))
                .collect()
        };

        let count = expired.len();
        for session in expired {
            if session.tmp.exists() {
                if let Err(e) = std::fs::remove_file(&session.tmp) {
                    tracing::warn!(
                        tmp = %session.tmp.display(),
                        "sweep: failed to remove expired temp file: {}",
                        e
                    );
                }
            }
        }
        count
    }

    /// 启动后台 sweeper：每小时清理 24 小时无活动的会话
    ///
    /// 必须在 tokio runtime 上下文内调用
    pub fn spawn_sweeper(manager: Arc<UploadSessionManager>) {
        crate::system::error_boundary::spawn_with_error_boundary(
            "upload_session_sweeper",
            async move {
                let mut interval = tokio::time::interval(SWEEP_INTERVAL);
                // 首个 tick 立即完成，跳过以对齐"每小时一次"语义
                interval.tick().await;
                loop {
                    interval.tick().await;
                    let removed = manager.sweep_expired().await;
                    if removed > 0 {
                        tracing::info!(
                            "upload session sweeper removed {} expired session(s)",
                            removed
                        );
                    }
                }
            },
        );
    }

    /// 当前活跃会话数（测试/诊断用）
    #[cfg(test)]
    pub async fn len(&self) -> usize {
        self.sessions.lock().await.len()
    }
}

impl Default for UploadSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Listing Filter & Orphan Cleanup ====================

/// 判断文件名是否为上传临时文件（`.bedcode-upload-*.part`）
pub fn is_upload_part_name(name: &str) -> bool {
    name.starts_with(UPLOAD_PART_PREFIX) && name.ends_with(UPLOAD_PART_SUFFIX)
}

/// 浏览列表过滤规则：所有 `*.part` 临时文件（含 `.bedcode-upload-*.part`）
pub fn is_filtered_listing_name(name: &str) -> bool {
    name.ends_with(UPLOAD_PART_SUFFIX)
}

/// 扫描清理指定目录树中的孤儿上传临时文件（挂载时对各 root 执行）
///
/// 孤儿来源：宿主异常退出遗留的 .part。返回清理数量
pub fn clean_orphan_parts(roots: &[PathBuf]) -> usize {
    let mut removed = 0;
    for root in roots {
        walk_and_clean(root, &mut removed);
    }
    removed
}

fn walk_and_clean(dir: &Path, removed: &mut usize) {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            tracing::debug!(dir = %dir.display(), "orphan scan: cannot read dir: {}", e);
            return;
        }
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_and_clean(&path, removed);
        } else if is_upload_part_name(&entry.file_name().to_string_lossy()) {
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    *removed += 1;
                    tracing::info!(file = %path.display(), "removed orphan upload temp file");
                }
                Err(e) => {
                    tracing::warn!(
                        file = %path.display(),
                        "failed to remove orphan upload temp file: {}",
                        e
                    );
                }
            }
        }
    }
}

// ==================== Snapshot (返回值克隆) ====================

/// 会话快照（对外返回用，避免持有锁内引用）
#[derive(Debug, Clone)]
struct SessionSnapshot {
    id: String,
    plugin_id: String,
    mount_path: String,
    target: PathBuf,
    tmp: PathBuf,
    size: u64,
    received: u64,
    last_active: Instant,
}

impl From<&UploadSession> for SessionSnapshot {
    fn from(s: &UploadSession) -> Self {
        Self {
            id: s.id.clone(),
            plugin_id: s.plugin_id.clone(),
            mount_path: s.mount_path.clone(),
            target: s.target.clone(),
            tmp: s.tmp.clone(),
            size: s.size,
            received: s.received,
            last_active: s.last_active,
        }
    }
}

impl From<SessionSnapshot> for UploadSession {
    fn from(s: SessionSnapshot) -> Self {
        Self {
            id: s.id,
            plugin_id: s.plugin_id,
            mount_path: s.mount_path,
            target: s.target,
            tmp: s.tmp,
            size: s.size,
            received: s.received,
            last_active: s.last_active,
        }
    }
}

// ==================== Tests ====================
//
// 会话状态机为纯 tokio/标准库逻辑（无 tauri 依赖），可在任意平台单测。
// Windows 上主 crate 的 cargo test 因 tauri cdylib 限制无法运行
// （见 server/app.rs 尾部注释），但本模块测试不引入任何 tauri 符号。

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Runtime::new().unwrap()
    }

    fn target_in(dir: &Path, name: &str) -> PathBuf {
        dir.join(name)
    }

    #[test]
    fn test_create_append_complete_lifecycle() {
        let rt = runtime();
        rt.block_on(async {
            let base = tempfile::tempdir().unwrap();
            let manager = UploadSessionManager::new();

            let target = target_in(base.path(), "movie.mp4");
            let session = manager
                .create("com.test.plugin", "files", target.clone(), 10)
                .await
                .unwrap();
            assert_eq!(session.received, 0);
            assert!(session.tmp.file_name().unwrap().to_string_lossy().starts_with(UPLOAD_PART_PREFIX));

            // append 从头开始
            let received = manager
                .append(&session.id, "com.test.plugin", "files", 0, b"01234")
                .await
                .unwrap();
            assert_eq!(received, 5);

            // offset 不一致 → OffsetMismatch
            let err = manager
                .append(&session.id, "com.test.plugin", "files", 0, b"xx")
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                UploadSessionError::OffsetMismatch { expected: 5, got: 0 }
            ));

            // 续传正确偏移
            let received = manager
                .append(&session.id, "com.test.plugin", "files", 5, b"56789")
                .await
                .unwrap();
            assert_eq!(received, 10);

            // complete → 原子 rename
            let final_path = manager
                .complete(&session.id, "com.test.plugin", "files")
                .await
                .unwrap();
            assert_eq!(final_path, target);
            assert!(target.exists());
            assert!(!session.tmp.exists());
            assert_eq!(std::fs::read(&target).unwrap(), b"0123456789");

            // session 已移除
            assert!(manager.get(&session.id, "com.test.plugin", "files").await.is_none());
        });
    }

    #[test]
    fn test_complete_duplicate_name_keeps_part() {
        let rt = runtime();
        rt.block_on(async {
            let base = tempfile::tempdir().unwrap();
            let manager = UploadSessionManager::new();

            let target = target_in(base.path(), "dup.txt");
            std::fs::write(&target, b"existing").unwrap();

            let session = manager
                .create("p", "m", target.clone(), 4)
                .await
                .unwrap();
            manager.append(&session.id, "p", "m", 0, b"data").await.unwrap();

            let err = manager.complete(&session.id, "p", "m").await.unwrap_err();
            assert!(matches!(err, UploadSessionError::DuplicateName(_)));
            // .part 保留供用户决定；session 仍在可查询
            assert!(session.tmp.exists());
            assert!(manager.get(&session.id, "p", "m").await.is_some());
        });
    }

    #[test]
    fn test_complete_to_media_success_deletes_temp() {
        let rt = runtime();
        rt.block_on(async {
            let base = tempfile::tempdir().unwrap();
            let manager = UploadSessionManager::new();

            let target = target_in(base.path(), "movie.mp4");
            let session = manager
                .create("p", "m", target.clone(), 4)
                .await
                .unwrap();
            manager.append(&session.id, "p", "m", 0, b"data").await.unwrap();

            // MediaStore 落位成功：临时文件删除、目标不产生（副本在公共目录）
            let placed = manager
                .complete_to_media(&session.id, "p", "m", |tmp, name| {
                    assert_eq!(name, "movie.mp4");
                    assert!(tmp.exists());
                    Ok(())
                })
                .await
                .unwrap();
            assert_eq!(placed, target);
            assert!(!session.tmp.exists());
            assert!(!target.exists());
            // session 已移除
            assert!(manager.get(&session.id, "p", "m").await.is_none());
        });
    }

    #[test]
    fn test_complete_to_media_fallback_renames_to_private() {
        let rt = runtime();
        rt.block_on(async {
            let base = tempfile::tempdir().unwrap();
            let manager = UploadSessionManager::new();

            let target = target_in(base.path(), "movie.mp4");
            let session = manager
                .create("p", "m", target.clone(), 4)
                .await
                .unwrap();
            manager.append(&session.id, "p", "m", 0, b"data").await.unwrap();

            // MediaStore 写入失败（如 API<29）→ 回退 rename（私有目录落点）
            let placed = manager
                .complete_to_media(&session.id, "p", "m", |_tmp, _name| {
                    Err(PlacementError::Other("requires API 29+".to_string()))
                })
                .await
                .unwrap();
            assert_eq!(placed, target);
            assert!(target.exists());
            assert_eq!(std::fs::read(&target).unwrap(), b"data");
        });
    }

    #[test]
    fn test_complete_to_media_fallback_rename_failure_restores_session() {
        let rt = runtime();
        rt.block_on(async {
            let base = tempfile::tempdir().unwrap();
            let manager = UploadSessionManager::new();

            // 手工构造 session：tmp 存活于独立目录，target 父目录不存在
            // → MediaStore 回退 rename 必然失败 → session 还原（.part 保留）
            let ghost = base.path().join("ghost");
            let target = ghost.join("x.bin");
            let tmp_dir = base.path().join("tmp-alive");
            std::fs::create_dir_all(&tmp_dir).unwrap();
            let tmp = tmp_dir.join("t.part");
            std::fs::write(&tmp, b"abc").unwrap();
            let session = UploadSession {
                id: "s1".to_string(),
                plugin_id: "p".to_string(),
                mount_path: "m".to_string(),
                target: target.clone(),
                tmp: tmp.clone(),
                size: 3,
                received: 3,
                last_active: Instant::now(),
            };
            manager.sessions.lock().await.insert(session.id.clone(), session);

            let err = manager
                .complete_to_media("s1", "p", "m", |_tmp, _name| {
                    Err(PlacementError::Other("media write failed".to_string()))
                })
                .await
                .unwrap_err();
            assert!(matches!(err, UploadSessionError::Io(_)));
            // 失败后 session 还原、.part 保留（客户端可查询/重试）
            assert!(manager.get("s1", "p", "m").await.is_some());
            assert!(tmp.exists());
        });
    }

    #[test]
    fn test_complete_to_media_duplicate_name_keeps_part() {
        let rt = runtime();
        rt.block_on(async {
            let base = tempfile::tempdir().unwrap();
            let manager = UploadSessionManager::new();

            let target = target_in(base.path(), "dup.txt");
            std::fs::write(&target, b"occupied").unwrap();

            let session = manager
                .create("p", "m", target.clone(), 4)
                .await
                .unwrap();
            manager.append(&session.id, "p", "m", 0, b"data").await.unwrap();

            let err = manager
                .complete_to_media(&session.id, "p", "m", |_tmp, _name| Ok(()))
                .await
                .unwrap_err();
            assert!(matches!(err, UploadSessionError::DuplicateName(_)));
            assert!(session.tmp.exists());
            assert!(manager.get(&session.id, "p", "m").await.is_some());
        });
    }

    #[test]
    fn test_complete_to_media_duplicate_place_error_keeps_part_no_fallback() {
        let rt = runtime();
        rt.block_on(async {
            let base = tempfile::tempdir().unwrap();
            let manager = UploadSessionManager::new();

            // 私有下载目录已存在同名目标（place 返回 Duplicate）→ 不得回退
            // rename 覆盖私有副本；.part 保留、session 还原（客户端可查询）
            let target = target_in(base.path(), "dup.mp4");
            std::fs::write(&target, b"existing-private").unwrap();
            let session = manager
                .create("p", "m", target.clone(), 4)
                .await
                .unwrap();
            manager.append(&session.id, "p", "m", 0, b"data").await.unwrap();

            let err = manager
                .complete_to_media(&session.id, "p", "m", |_tmp, _name| {
                    Err(PlacementError::Duplicate("duplicate-name".to_string()))
                })
                .await
                .unwrap_err();
            assert!(matches!(err, UploadSessionError::DuplicateName(_)));
            // 私有副本未被覆盖、.part 保留、session 还原
            assert_eq!(std::fs::read(&target).unwrap(), b"existing-private");
            assert!(session.tmp.exists());
            assert!(manager.get(&session.id, "p", "m").await.is_some());
        });
    }

    #[test]
    fn test_cancel_removes_temp_file() {
        let rt = runtime();
        rt.block_on(async {
            let base = tempfile::tempdir().unwrap();
            let manager = UploadSessionManager::new();

            let session = manager
                .create("p", "m", target_in(base.path(), "gone.bin"), 3)
                .await
                .unwrap();
            manager.append(&session.id, "p", "m", 0, b"abc").await.unwrap();
            assert!(session.tmp.exists());

            manager.cancel(&session.id, "p", "m").await.unwrap();
            assert!(!session.tmp.exists());
            assert!(manager.get(&session.id, "p", "m").await.is_none());

            // 重复取消 → NotFound
            assert!(matches!(
                manager.cancel(&session.id, "p", "m").await.unwrap_err(),
                UploadSessionError::NotFound(_)
            ));
        });
    }

    #[test]
    fn test_ownership_isolation() {
        let rt = runtime();
        rt.block_on(async {
            let base = tempfile::tempdir().unwrap();
            let manager = UploadSessionManager::new();

            let session = manager
                .create("plugin-a", "files", target_in(base.path(), "x.bin"), 1)
                .await
                .unwrap();

            // 其他插件/挂载点无法访问该 session
            assert!(manager.get(&session.id, "plugin-b", "files").await.is_none());
            assert!(manager.get(&session.id, "plugin-a", "other").await.is_none());
            assert!(manager
                .append(&session.id, "plugin-b", "files", 0, b"z")
                .await
                .is_err());
        });
    }

    #[test]
    fn test_cancel_for_mount() {
        let rt = runtime();
        rt.block_on(async {
            let base = tempfile::tempdir().unwrap();
            let manager = Arc::new(UploadSessionManager::new());

            for i in 0..3 {
                let s = manager
                    .create("p", "m", target_in(base.path(), &format!("f{}", i)), 1)
                    .await
                    .unwrap();
                manager.append(&s.id, "p", "m", 0, b"x").await.unwrap();
            }
            // 另一个挂载点的 session 不受影响
            let other = manager
                .create("p", "other", target_in(base.path(), "keep"), 1)
                .await
                .unwrap();

            let cancelled = manager.cancel_for_mount("p", "m").await;
            assert_eq!(cancelled, 3);
            assert!(manager.get(&other.id, "p", "other").await.is_some());
        });
    }

    #[test]
    fn test_orphan_part_cleanup_and_listing_filter() {
        let base = tempfile::tempdir().unwrap();
        let sub = base.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();

        let orphan = sub.join(format!("{}abc{}", UPLOAD_PART_PREFIX, UPLOAD_PART_SUFFIX));
        let normal = sub.join("keep.txt");
        let foreign_part = sub.join("other.part");
        std::fs::write(&orphan, b"junk").unwrap();
        std::fs::write(&normal, b"keep").unwrap();
        std::fs::write(&foreign_part, b"junk2").unwrap();

        let removed = clean_orphan_parts(&[base.path().to_path_buf()]);
        assert_eq!(removed, 1);
        assert!(!orphan.exists());
        assert!(normal.exists());
        // 非 bedcode 前缀的 .part 不在孤儿清理范围（可能是用户文件），但浏览列表仍过滤
        assert!(foreign_part.exists());

        assert!(is_filtered_listing_name(".bedcode-upload-x.part"));
        assert!(is_filtered_listing_name("anything.part"));
        assert!(!is_filtered_listing_name("movie.mp4"));
        assert!(is_upload_part_name(".bedcode-upload-x.part"));
        assert!(!is_upload_part_name("anything.part"));
    }
}
