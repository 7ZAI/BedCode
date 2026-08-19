//! 手机端双向 HTTP client 传输栈（v2.1 服务器归零：全手机发起传输）
//!
//! 服务器归零后手机端不再监听任何 HTTP 端口；大文件数据流全部由手机侧作为
//! **HTTP client 主动发起**，桌面端是唯一 HTTP 服务端（`file_service_controller`）。
//!
//! 模块划分（与 v2 `plugin/transfer.rs` 平级，宿主侧原生实现，非 WASM）：
//! - [`cursor`]：本地已写字节游标（接收端即断点真源，纯函数可单测）
//! - [`download`]：`GET /{mount}/file` + Range 206 → 本地落盘（HEAD 指纹续传比对）
//! - [`upload`]：`POST /{mount}/upload` 建会话 → `PUT` 偏移 append → complete 原子改名
//!   （断点重查 + 409/404 重建，与桌面 UploadSessionManager 契约对齐）
//!
//! 端点发现：HTTP base = `http://{target.address}:{target.port}`（ConnectionManager
//! TargetDevice，零新增消息）；Authorization 复用现有 session_token（桌面 JWT），
//! 与 WS 认证同款凭据，不新增认证机制。
//!
//! 并发：默认 3（1–8 可配置）；每个 transfer 独立 SAF 读/写句柄，不共享文件锁。

pub mod cursor;
pub mod download;
pub mod upload;

pub use cursor::{Cursor, CursorError, CursorStore, StoredCursor};
pub use download::{download_with_retry, DownloadClient, DownloadError, DownloadRequest};
pub use upload::{
    CompleteError, CreateUploadRequest, UploadClient, UploadError, UploadSessionInfo,
};

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use tokio_util::sync::CancellationToken;

/// 解析桌面 HTTP 端点（base + JWT；ConnectionManager target 为地址真源）
///
/// 认证复用现有 session_token（桌面 JWT），与 WS 首消息 reauthenticate 同款，
/// 不新增认证机制；未连接/无 token 时拒绝启动传输。responder 与
/// host_impl/filesrv.rs 共用此实现（两端此前曾逐字重复）。
pub(crate) async fn desktop_http_endpoint() -> Result<(String, String), String> {
    let cm = crate::state::get_connection_manager();
    let target = cm
        .get_target()
        .await
        .ok_or_else(|| "file service client: no connection target".to_string())?;
    let token = crate::state::get_global_token();
    if token.is_empty() {
        return Err("file service client: no session token (reconnect required)".to_string());
    }
    Ok((
        format!("http://{}:{}", target.address, target.port),
        token,
    ))
}

/// 并发上限常量（沿用 01 选型：默认 3，可配置 1–8）
pub const DEFAULT_CONCURRENCY: usize = 3;
/// 并发下限
pub const MIN_CONCURRENCY: usize = 1;
/// 并发上限
pub const MAX_CONCURRENCY: usize = 8;

/// 桌面文件指纹（HEAD `X-File-Size` / `X-File-Mtime`；续传有效性比对）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileFingerprint {
    /// 文件字节数
    pub size: u64,
    /// 修改时间（Unix 秒）
    pub mtime: u64,
}

/// 构建 desktop file_service 端点 URL（挂在 /api/plugins/{plugin}/{mount}/**）
///
/// `path` 为端点相对路径（如 `file?path=<urlencoded>` / `upload`）；
/// base 已含 http:// 前缀且允许尾部斜杠（trim 后拼接）
pub fn endpoint(base: &str, plugin_id: &str, mount_path: &str, path: &str) -> String {
    format!(
        "{}/api/plugins/{}/{}/{}",
        base.trim_end_matches('/'),
        plugin_id,
        mount_path,
        path.trim_start_matches('/')
    )
}

/// 相对路径 → URL 查询参数（保留 `/`，转义 `&?%#+ =` 等保留字符）
///
/// 桌面端 list/file 端点的 `?path=` 取整串字符串（".trim_matches('/')"），
/// 文件名中的保留字符必须转义避免破坏 query 结构；`/` 保留便于阅读与后端解析
pub fn urlencode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// 全局 client 传输游标存储（下载续传按 task_id 分 key，响应器共用）
static CLIENT_CURSORS: OnceLock<CursorStore> = OnceLock::new();

/// 获取全局 client 游标存储（惰性初始化）
pub(crate) fn client_cursor_store() -> &'static CursorStore {
    CLIENT_CURSORS.get_or_init(CursorStore::new)
}

/// 单个传输句柄（取消 + 进度共享，跨任务线程安全）
///
/// 下载/上传客户端函数都接受本句柄：取消经 CancellationToken 中止 HTTP 流，
/// 进度经原子计数器由任务/进度推送器并发读写
#[derive(Clone)]
pub struct TransferHandle {
    /// 任务 ID（intent_id 或 UUID）
    pub task_id: String,
    /// 已传输字节数（含续传偏移）
    pub transferred: Arc<AtomicU64>,
    /// 总字节数（未知为 0）
    pub total: Arc<AtomicU64>,
    token: CancellationToken,
}

impl TransferHandle {
    /// 创建传输句柄（独立取消令牌）
    pub fn new(task_id: String) -> Self {
        Self {
            task_id,
            transferred: Arc::new(AtomicU64::new(0)),
            total: Arc::new(AtomicU64::new(0)),
            token: CancellationToken::new(),
        }
    }

    /// 读取进度 (transferred, total)
    pub fn progress(&self) -> (u64, u64) {
        (
            self.transferred.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }

    /// 设置总大小（下载 HEAD 指纹 / 上传源 metadata 填充）
    pub fn set_total(&self, total: u64) {
        self.total.store(total, Ordering::Relaxed);
    }

    /// 取消传输（HTTP 流立即中止）
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// 是否已取消
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// 取消令牌副本（透传给传输函数内部 select）
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// 增加已传字节数并返回新值
    pub fn add_transferred(&self, n: u64) -> u64 {
        self.transferred.fetch_add(n, Ordering::Relaxed) + n
    }

    /// 已传字节计数器的 Arc 副本（供自身 'static 流式 body 闭包共享）
    pub fn transferred_handle(&self) -> Arc<AtomicU64> {
        self.transferred.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_builds_plugin_mount_url() {
        assert_eq!(
            endpoint("http://192.168.1.5:4455", "com.bedcode.file-transfer", "files", "file?path=movies%2Fa.mp4"),
            "http://192.168.1.5:4455/api/plugins/com.bedcode.file-transfer/files/file?path=movies%2Fa.mp4"
        );
        // base 尾部斜杠容忍
        assert_eq!(
            endpoint("http://192.168.1.5:4455/", "p", "m", "/upload"),
            "http://192.168.1.5:4455/api/plugins/p/m/upload"
        );
    }

    #[test]
    fn transfer_handle_tracks_progress_and_cancel() {
        let h = TransferHandle::new("intent-1".to_string());
        assert_eq!(h.progress(), (0, 0));
        h.set_total(100);
        h.add_transferred(30);
        assert_eq!(h.progress(), (30, 100));
        assert!(!h.is_cancelled());
        h.cancel();
        assert!(h.is_cancelled());
        // 取消后 add 仍容忍
        h.add_transferred(5);
    }
}
