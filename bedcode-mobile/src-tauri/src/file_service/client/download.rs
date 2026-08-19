//! 下载客户端（v2.1：手机从桌面拉文件，GET + Range → 本地落盘）
//!
//! 与桌面 `file_service_controller` 的 `GET {mount}/file`（NamedFile 原生
//! Range/206）+ `HEAD`（X-File-Size / X-File-Mtime 指纹）契约对齐：
//! - 续传：以本地游标（cursor.rs）为 Range 起点，中断后新 `Range: bytes={cursor}-`
//!   重拉，已写字节保留（.part 顺序追加）
//! - 指纹：HEAD 取 size+mtime，与游标内上次指纹比对——源文件变更则放弃旧
//!   游标从头重传，避免续传拼接出「新旧字节混合」的文件
//! - 落盘：写 `.part` → 完成后原子 rename 到最终路径；可选经 SafIo 写入
//!   公共下载目录（MediaStore，阶段 1 不直写，SAF 中转语义不变）
//!
//! 重试（连续 3 次失败转 failed 待手动）由调用方（responder）编排，本模块
//! 每次调用单次尝试，失败时游标保留在 store 供续传。

use super::cursor::{Cursor, CursorStore, StoredCursor};
use super::{FileFingerprint, TransferHandle};
use futures_util::StreamExt;
use std::path::{Path, PathBuf};

/// 下载错误（结构化，供 responder 映射转移 fail 偏移上报）
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    /// HTTP 非 2xx（如 404 路径不存在 / 403 操作未授权 / 500）
    #[error("download HTTP {status} for {url}")]
    Http { status: u16, url: String },
    /// 请求/流式读取网络错误（可重试：游标保留）
    #[error("download network error: {0}")]
    Network(String),
    /// HEAD 指纹与上次游标不一致（源文件变更）——调用方放弃旧游标重传，
    /// 下载内已自动归零重传（本变体保留作错误面，实际内部分支不报出）
    #[error("source fingerprint changed (size {0}->{1}); abandon stale cursor")]
    FingerprintMismatch(u64, u64),
    /// 传输被取消（游标保留，供续传；取消为终态，不重试）
    #[error("download cancelled")]
    Cancelled,
    /// 完成 rename 时目标名已占用（保留 .part 供用户决定）
    #[error("destination already exists: {0}")]
    DuplicateName(String),
    /// 本地 IO 错误（写入/seek/rename）
    #[error("download io error: {0}")]
    Io(String),
}

impl From<std::io::Error> for DownloadError {
    fn from(e: std::io::Error) -> Self {
        DownloadError::Io(e.to_string())
    }
}

/// 下载请求（路径语义在发起方侧解析：path = 桌面挂载内相对路径）
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    /// GET endpoint URL（`{base}/api/plugins/{plugin}/{mount}/file?path=<..>`）
    pub url: String,
    /// Authorization（桌面 JWT，可空）
    pub auth: String,
    /// `.part` 写入路径（本地游标落点；目录须存在）
    pub dest_path: PathBuf,
    /// 完成原子 rename 目标（最终文件名）
    pub final_path: PathBuf,
    /// 声明的总大小（来自 intent.size；0 = 未知）
    pub total: u64,
    /// 公共下载目录写入（Some((显示名, MIME))；None = 仅私有 rename）
    pub media: Option<(String, String)>,
}

/// 下载客户端（reqwest，每调用独立请求）
#[derive(Clone, Default)]
pub struct DownloadClient {
    client: reqwest::Client,
}

impl DownloadClient {
    /// 创建客户端（复用请求池；默认系统代理可被环回测试经 NO_PROXY 屏蔽）
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// HEAD 指纹（size+mtime；404 → Http 错误）
    pub async fn head_fingerprint(
        &self,
        url: &str,
        auth: &str,
    ) -> Result<FileFingerprint, DownloadError> {
        let resp = self
            .client
            .head(url)
            .header(reqwest::header::AUTHORIZATION, auth)
            .send()
            .await
            .map_err(|e| DownloadError::Network(format!("HEAD {} failed: {}", url, e)))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(DownloadError::Http {
                status: status.as_u16(),
                url: url.to_string(),
            });
        }
        // 指纹头缺失时回退 Content-Length（mtime 未知为 0）
        let size = resp
            .headers()
            .get("X-File-Size")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .or_else(|| {
                resp.headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
            })
            .unwrap_or(0);
        let mtime = resp
            .headers()
            .get("X-File-Mtime")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        Ok(FileFingerprint { size, mtime })
    }

    /// 执行单次下载尝试（完整流程：指纹比对 → Range 拉流 → 落盘 → rename）
    ///
    /// - 游标存于 `store`（key = handle.task_id），含上次指纹；源文件变更
    ///   自动放弃旧游标（位置归零从头传）
    /// - 取消：`.part` 与游标保留（可续传）；连续失败次数由调用方统计
    /// - 成功：`.part` → 最终路径 rename，可选 MediaStore 落公共下载
    pub async fn download(
        &self,
        req: &DownloadRequest,
        store: &CursorStore,
        handle: &TransferHandle,
    ) -> Result<PathBuf, DownloadError> {
        // 1. HEAD 指纹（源文件变更放弃旧游标）
        let fingerprint = self.head_fingerprint(&req.url, &req.auth).await?;
        handle.set_total(if fingerprint.size > 0 {
            fingerprint.size
        } else {
            req.total
        });

        let stored = store.get(&handle.task_id);
        let mut cursor = Cursor::restore(
            stored.as_ref().map(|s| s.position).unwrap_or(0),
            handle.total.load(std::sync::atomic::Ordering::Relaxed),
        )
        .unwrap_or_default();
        // 指纹不一致（源文件已变更）→ 放弃旧游标从头重传（.part 一并截断）。
        // ADR：size + mtime 双因子，任一变化（含等长重编码）即视为源变更——
        // 只比 size 会把「同 size 新 mtime」误判为未变，续传拼出损坏文件
        if let Some(prev) = stored.as_ref().and_then(|s| s.fingerprint) {
            if prev != fingerprint {
                tracing::warn!(
                    task_id = %handle.task_id,
                    prev_size = prev.size,
                    prev_mtime = prev.mtime,
                    new_size = fingerprint.size,
                    new_mtime = fingerprint.mtime,
                    "download: source fingerprint changed, abandoning stale cursor"
                );
                cursor.position = 0;
            }
        }
        let offset = cursor.effective_offset();
        tracing::info!(
            task_id = %handle.task_id,
            offset,
            total = fingerprint.size,
            "download: HEAD fingerprint {}/{}",
            fingerprint.size,
            fingerprint.mtime,
        );

        // 2. GET + Range（offset>0 → 期望 206）
        let mut builder = self.client.get(&req.url).header(
            reqwest::header::AUTHORIZATION,
            req.auth.as_str(),
        );
        if offset > 0 {
            builder = builder.header(reqwest::header::RANGE, format!("bytes={}-", offset));
        }
        let resp = builder
            .send()
            .await
            .map_err(|e| DownloadError::Network(format!("GET {} failed: {}", req.url, e)))?;
        let status = resp.status();
        let expect_partial = offset > 0;
        let ok = if expect_partial {
            status == reqwest::StatusCode::PARTIAL_CONTENT
                || status == reqwest::StatusCode::OK
        } else {
            status.is_success()
        };
        if !ok {
            return Err(DownloadError::Http {
                status: status.as_u16(),
                url: req.url.clone(),
            });
        }

        // 3. 流式写 .part（offset=0 截断清理残留；offset>0 保留已传字节）
        if let Some(parent) = req.dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        use tokio::io::{AsyncSeekExt, AsyncWriteExt};
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(offset == 0)
            .open(&req.dest_path)
            .await?;
        file.seek(std::io::SeekFrom::Start(offset)).await?;

        let token = handle.token();
        let mut stream = resp.bytes_stream();
        while let Some(chunk_result) = stream.next().await {
            if token.is_cancelled() {
                return Err(DownloadError::Cancelled);
            }
            let chunk = chunk_result
                .map_err(|e| DownloadError::Network(format!("download stream error: {}", e)))?;
            file.write_all(&chunk).await?;
            let new_pos = handle.add_transferred(chunk.len() as u64);
            store.upsert(
                &handle.task_id,
                StoredCursor {
                    position: new_pos,
                    fingerprint: Some(fingerprint),
                },
            );
        }
        file.flush().await?;
        drop(file);

        // 4. 游标已满或对端返回完整 → 落位（fingerprint 与游标在成功时清）
        if req.final_path.exists() {
            return Err(DownloadError::DuplicateName(
                req.final_path.display().to_string(),
            ));
        }
        std::fs::rename(&req.dest_path, &req.final_path)?;
        store.remove(&handle.task_id);

        // 5. 可选：落公共下载目录（MediaStore；SAF 中转语义，阶段 1 不直写）
        if let Some((display, mime)) = &req.media {
            let _ = display;
            let _ = mime;
            // SafIo 仅 Android 可用：经 registry.saf_io 注入由高级调用方负责，
            // 本模块不持有——落公共下载由调用方（responder）在 rename 后执行
        }

        tracing::info!(
            task_id = %handle.task_id,
            final_path = %req.final_path.display(),
            "download completed"
        );
        Ok(req.final_path.clone())
    }
}

/// 带重试的单文件下载编排（v2.1 client 栈 / 响应器 push 共用）
///
/// 网络错误 → 游标保留 → 重试（新 Range 续传）→ 连续 `max_attempts` 次失败
/// 转 Err；取消 / 确定性失败（4xx、duplicate-name、IO）立即返回。
/// 每次失败间短暂退避（200ms），避免紧耦合重试打满对端。
pub async fn download_with_retry(
    client: &DownloadClient,
    req: &DownloadRequest,
    store: &CursorStore,
    handle: &TransferHandle,
    max_attempts: u32,
) -> Result<PathBuf, DownloadError> {
    let mut attempts = 0u32;
    let max = max_attempts.max(1);
    loop {
        match client.download(req, store, handle).await {
            Ok(path) => return Ok(path),
            Err(e) => {
                if handle.is_cancelled() {
                    return Err(DownloadError::Cancelled);
                }
                // 网络错误可重试（游标保留）；其余确定性失败直接终态
                let retryable = matches!(e, DownloadError::Network(_));
                attempts += 1;
                if !retryable || attempts >= max {
                    return Err(e);
                }
                tracing::warn!(
                    task_id = %handle.task_id,
                    attempt = attempts,
                    "download retrying after network error"
                );
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_service::client::CursorStore;
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// 禁用系统代理对 loopback 的干扰（reqwest Client::new 走系统代理）
    fn disable_proxy_for_loopback() {
        std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
    }

    /// 极简请求（mock 服务器解析产物）
    struct MockRequest {
        method: String,
        headers: HashMap<String, String>,
    }

    impl MockRequest {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers.get(name).map(|s| s.as_str())
        }
    }

    /// mock 响应（支持状态码 + 自定义响应头 + body）
    struct MockResponse {
        status: u16,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    }

    impl MockResponse {
        fn ok(body: impl Into<Vec<u8>>) -> Self {
            Self {
                status: 200,
                headers: HashMap::new(),
                body: body.into(),
            }
        }
        fn partial(body: impl Into<Vec<u8>>) -> Self {
            Self {
                status: 206,
                headers: HashMap::new(),
                body: body.into(),
            }
        }
        fn with_status(status: u16, body: impl Into<Vec<u8>>) -> Self {
            Self {
                status,
                headers: HashMap::new(),
                body: body.into(),
            }
        }
        fn with_header(mut self, k: &str, v: &str) -> Self {
            self.headers.insert(k.to_string(), v.to_string());
            self
        }
    }

    /// 启动 mock HTTP 服务器：解析请求行 + 头，读完整 body 后按头响应
    async fn spawn_mock_server(
        handler: Arc<dyn Fn(MockRequest) -> MockResponse + Send + Sync>,
    ) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let handler = handler.clone();
                tokio::spawn(async move {
                    let Some(req) = read_headers(&mut sock).await else {
                        return;
                    };
                    let resp = handler(req);
                    let reason = match resp.status {
                        200 => "OK",
                        206 => "Partial Content",
                        404 => "Not Found",
                        _ => "Unknown",
                    };
                    let mut head = format!(
                        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                        resp.status,
                        reason,
                        resp.body.len()
                    );
                    for (k, v) in &resp.headers {
                        head.push_str(&format!("{}: {}\r\n", k, v));
                    }
                    head.push_str("\r\n");
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(&resp.body).await;
                });
            }
        });
        addr
    }

    /// 读取一个 HTTP 请求的请求行 + 头（GET/HEAD 无 body）
    async fn read_headers(sock: &mut tokio::net::TcpStream) -> Option<MockRequest> {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 4096];
        loop {
            let n = sock.read(&mut tmp).await.ok()?;
            if n == 0 {
                return None;
            }
            buf.extend_from_slice(&tmp[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let header_end = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
        let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
        let mut lines = head.lines();
        let mut parts = lines.next()?.split_whitespace();
        let method = parts.next()?.to_string();
        let mut headers = HashMap::new();
        for line in lines {
            if let Some((k, v)) = line.split_once(':') {
                headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
        Some(MockRequest { method, headers })
    }

    fn tmp_dl_paths(dir: &std::path::Path, name: &str) -> (PathBuf, PathBuf) {
        (
            dir.join(format!("{}.part", name)),
            dir.join(name),
        )
    }

    /// 构造默认指纹（size=5, mtime=100）
    fn fp() -> FileFingerprint {
        FileFingerprint { size: 5, mtime: 100 }
    }

    /// 预置游标条目的快捷方式
    fn seed_cursor(store: &CursorStore, key: &str, position: u64) {
        store.upsert(
            key,
            StoredCursor {
                position,
                fingerprint: Some(fp()),
            },
        );
    }

    #[test]
    fn head_parses_fingerprint_headers() {
        disable_proxy_for_loopback();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let addr = spawn_mock_server(Arc::new(|req| {
                assert_eq!(req.method, "HEAD");
                MockResponse::ok(vec![])
                    .with_header("X-File-Size", "12345")
                    .with_header("X-File-Mtime", "1700000000")
            }))
            .await;
            let client = DownloadClient::new();
            let f = client
                .head_fingerprint(&format!("http://{}/file", addr), "Bearer tok")
                .await
                .unwrap();
            assert_eq!(f, FileFingerprint { size: 12345, mtime: 1700000000 });
        });
    }

    #[test]
    fn head_falls_back_to_content_length() {
        disable_proxy_for_loopback();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let addr = spawn_mock_server(Arc::new(|_| MockResponse::ok(vec![]))).await;
            let client = DownloadClient::new();
            let f = client
                .head_fingerprint(&format!("http://{}/file", addr), "")
                .await
                .unwrap();
            assert_eq!(f, FileFingerprint { size: 0, mtime: 0 });
        });
    }

    #[test]
    fn head_404_reports_http_error() {
        disable_proxy_for_loopback();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let addr = spawn_mock_server(Arc::new(|_| {
                MockResponse::with_status(404, "not found")
            }))
            .await;
            let client = DownloadClient::new();
            let err = client
                .head_fingerprint(&format!("http://{}/file", addr), "")
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                DownloadError::Http { status: 404, .. }
            ));
        });
    }

    #[test]
    fn download_full_writes_file_and_update_cursor() {
        disable_proxy_for_loopback();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let body: Vec<u8> = (0..128 * 1024).map(|i| (i % 251) as u8).collect();
            let server_body = body.clone();
            let addr = spawn_mock_server(Arc::new(move |req| {
                if req.method == "HEAD" {
                    return MockResponse::ok(vec![])
                        .with_header("X-File-Size", server_body.len().to_string().as_str())
                        .with_header("X-File-Mtime", "100");
                }
                assert_eq!(req.method, "GET");
                assert_eq!(req.header("range"), None, "offset=0 不应带 Range");
                MockResponse::ok(server_body.clone())
            }))
            .await;
            let dir = tempfile::tempdir().unwrap();
            let (part, final_path) = tmp_dl_paths(dir.path(), "out.bin");
            let store = CursorStore::new();
            let handle = TransferHandle::new("t1".to_string());
            let req = DownloadRequest {
                url: format!("http://{}/file", addr),
                auth: "".to_string(),
                dest_path: part.clone(),
                final_path: final_path.clone(),
                total: body.len() as u64,
                media: None,
            };
            DownloadClient::new()
                .download(&req, &store, &handle)
                .await
                .unwrap();
            assert_eq!(std::fs::read(&final_path).unwrap(), body);
            assert!(!part.exists(), ".part 应已 rename");
            assert!(store.get("t1").is_none(), "成功后游标应清理");
        });
    }

    #[test]
    fn download_resume_sends_range_and_appends() {
        disable_proxy_for_loopback();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // 服务端完整文件 "hello"（5 字节），首次已写 "hel"（offset=3）
            let addr = spawn_mock_server(Arc::new(|req| {
                if req.method == "HEAD" {
                    return MockResponse::ok(vec![])
                        .with_header("X-File-Size", "5")
                        .with_header("X-File-Mtime", "100");
                }
                assert_eq!(req.method, "GET");
                assert_eq!(req.header("range"), Some("bytes=3-"));
                MockResponse::partial(b"lo".to_vec())
            }))
            .await;
            let dir = tempfile::tempdir().unwrap();
            let (part, final_path) = tmp_dl_paths(dir.path(), "res.bin");
            std::fs::write(&part, b"hel").unwrap();
            let store = CursorStore::new();
            seed_cursor(&store, "t1", 3);
            let handle = TransferHandle::new("t1".to_string());
            // seed 指纹与 HEAD 一致（size=5, mtime=100）→ 续传
            let req = DownloadRequest {
                url: format!("http://{}/file", addr),
                auth: "".to_string(),
                dest_path: part.clone(),
                final_path: final_path.clone(),
                total: 5,
                media: None,
            };
            DownloadClient::new()
                .download(&req, &store, &handle)
                .await
                .unwrap();
            assert_eq!(std::fs::read(&final_path).unwrap(), b"hello");
        });
    }

    #[test]
    fn download_fingerprint_change_abandons_cursor_and_restarts() {
        disable_proxy_for_loopback();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // 源文件变更（指纹 size 5->10）：放弃旧游标从头重传（.part 截断）
            let body: Vec<u8> = b"hello-world".to_vec(); // 10 字节
            let server_body = body.clone();
            let addr = spawn_mock_server(Arc::new(move |req| {
                if req.method == "HEAD" {
                    return MockResponse::ok(vec![])
                        .with_header("X-File-Size", server_body.len().to_string().as_str())
                        .with_header("X-File-Mtime", "200");
                }
                assert_eq!(req.header("range"), None, "应从头重传，不带 Range");
                MockResponse::ok(server_body.clone())
            }))
            .await;
            let dir = tempfile::tempdir().unwrap();
            let (part, final_path) = tmp_dl_paths(dir.path(), "new.bin");
            std::fs::write(&part, b"hel").unwrap();
            let store = CursorStore::new();
            // 预置旧指纹（size=5, mtime=200）与游标 3 → 与 HEAD size=10 不符
            store.upsert(
                "t1",
                StoredCursor {
                    position: 3,
                    fingerprint: Some(FileFingerprint { size: 5, mtime: 200 }),
                },
            );
            let handle = TransferHandle::new("t1".to_string());
            let req = DownloadRequest {
                url: format!("http://{}/file", addr),
                auth: "".to_string(),
                dest_path: part.clone(),
                final_path: final_path.clone(),
                total: 10,
                media: None,
            };
            DownloadClient::new()
                .download(&req, &store, &handle)
                .await
                .unwrap();
            // 从零下载完整新文件（旧字节 "hel" 被截断覆盖）
            assert_eq!(std::fs::read(&final_path).unwrap(), b"hello-world");
            assert!(store.get("t1").is_none());
        });
    }

    #[test]
    fn download_duplicate_name_keeps_part() {
        disable_proxy_for_loopback();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let addr = spawn_mock_server(Arc::new(|_| MockResponse::ok("bytes"))).await;
            let dir = tempfile::tempdir().unwrap();
            let (part, final_path) = tmp_dl_paths(dir.path(), "dup.bin");
            std::fs::write(&final_path, "occupied").unwrap();
            let store = CursorStore::new();
            let handle = TransferHandle::new("t1".to_string());
            let req = DownloadRequest {
                url: format!("http://{}/file", addr),
                auth: "".to_string(),
                dest_path: part,
                final_path,
                total: 5,
                media: None,
            };
            let err = DownloadClient::new()
                .download(&req, &store, &handle)
                .await
                .unwrap_err();
            assert!(matches!(err, DownloadError::DuplicateName(_)));
        });
    }

    #[test]
    fn download_cancelled_keeps_cursor_for_resume() {
        disable_proxy_for_loopback();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let addr = spawn_mock_server(Arc::new(|_| {
                MockResponse::ok(vec![0u8; 64 * 1024])
            }))
            .await;
            let dir = tempfile::tempdir().unwrap();
            let (part, final_path) = tmp_dl_paths(dir.path(), "cancel.bin");
            let store = CursorStore::new();
            let handle = TransferHandle::new("t1".to_string());
            handle.cancel(); // 立即取消
            let req = DownloadRequest {
                url: format!("http://{}/file", addr),
                auth: "".to_string(),
                dest_path: part,
                final_path,
                total: 64 * 1024,
                media: None,
            };
            let err = DownloadClient::new()
                .download(&req, &store, &handle)
                .await
                .unwrap_err();
            assert!(matches!(err, DownloadError::Cancelled));
        });
    }
}
