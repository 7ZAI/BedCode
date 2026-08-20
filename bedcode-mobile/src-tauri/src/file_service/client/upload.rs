//! 上传客户端（v2.1：手机推文件给桌面，POST/PUT session 编排）
//!
//! 与桌面 `file_service_controller` 的 upload session 契约对齐：
//! - `POST {mount}/upload` 建会话（可带 batchId 走 v2 批 gating，未批准 → 403）
//! - `PUT {mount}/upload/{sid}?offset=` 从服务端已收偏移 append（offset 不符 → 409）
//! - `GET {mount}/upload/{sid}` 断点重查（服务端已收字节 = 断点真源）
//! - `POST {mount}/upload/{sid}/complete` 原子改名（目标已存在 → 409 duplicate-name）
//! - `DELETE {mount}/upload/{sid}` 取消清理
//!
//! 重试编排（§2.5）：send→append 循环；网络错误 → 重查偏移 → 续传；会话 404
//! （对端清理/重启）→ 重新 POST 建会话再续传；complete 409 duplicate-name →
//! 该文件 rejected（调用方映射为终态失败）。
//!
//! ApiResponse 信封解析：`{ code, message, data }`（camelCase），data 为会话信息。

use super::{endpoint, TransferHandle};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// 上传错误（结构化，供 responder 映射 fail 偏移上报 / 终态分类）
#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    /// HTTP 非 2xx（403 批未批准 / 404 会话不存在 / 500 服务端）
    #[error("upload HTTP {status} for {url}: {message}")]
    Http { status: u16, url: String, message: String },
    /// 请求/流式网络错误（可重试：重查偏移续传）
    #[error("upload network error: {0}")]
    Network(String),
    /// append 偏移不符（409）——先 GET 重查服务端偏移再续传
    #[error("upload offset mismatch: server has {expected} bytes, client sent {got}")]
    OffsetMismatch { expected: u64, got: u64 },
    /// 会话不存在（404，对端清理）——重新 POST 建会话
    #[error("upload session not found: {0}")]
    SessionNotFound(String),
    /// complete 409 duplicate-name → 该文件 rejected（保留 .part，不自动重建）
    #[error("duplicate-name: {0}")]
    DuplicateName(String),
    /// 传输被取消（session 保留，服务端已收字节为续传真源）
    #[error("upload cancelled")]
    Cancelled,
    /// 本地源读取 / 响应解析错误
    #[error("upload io error: {0}")]
    Io(String),
}

impl From<std::io::Error> for UploadError {
    fn from(e: std::io::Error) -> Self {
        UploadError::Io(e.to_string())
    }
}

/// 创建上传会话请求（camelCase wire，与桌面 CreateUploadRequest 逐字一致）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateUploadRequest {
    /// 目标相对路径（相对桌面挂载根）
    pub relative_path: String,
    /// 声明的文件总大小（字节）
    pub size: u64,
    /// v2：所属传输批 ID（ask 策略下 pull 场景由桌面自批准随 intent 下发）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
}

/// 上传会话信息（服务端返回）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadSessionInfo {
    /// 会话 ID
    pub session_id: String,
    /// 服务端已收字节数（断点真源）
    pub received: u64,
}

/// ApiResponse 信封（`{ code, message, data }`）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiEnvelope<T> {
    #[allow(dead_code)]
    code: u16,
    #[allow(dead_code)]
    message: String,
    data: Option<T>,
}

/// complete 结果错误面
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompleteError {
    /// 409 duplicate-name（该文件 rejected）
    DuplicateName(String),
    /// 404 会话已不存在
    SessionNotFound(String),
    /// 其他 HTTP 错误
    Http { status: u16, url: String, message: String },
    /// 网络错误
    Network(String),
}

/// 上传客户端（reqwest）
#[derive(Clone, Default)]
pub struct UploadClient {
    client: reqwest::Client,
}

impl UploadClient {
    /// 创建客户端
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    // ==================== 会话元操作（低层，可独立单测） ====================

    /// POST 建会话（返回 sessionId + received=0；403 → Http 错误含批 gating 消息）
    pub async fn create_session(
        &self,
        url: &str,
        auth: &str,
        body: &CreateUploadRequest,
    ) -> Result<UploadSessionInfo, UploadError> {
        let resp = self
            .client
            .post(url)
            .header(reqwest::header::AUTHORIZATION, auth)
            .json(body)
            .send()
            .await
            .map_err(|e| UploadError::Network(format!("POST {} failed: {}", url, e)))?;
        parse_json_envelope::<UploadSessionInfo>(resp, url, "create session")
            .await
            .and_then(|info| {
                info.data
                    .ok_or_else(|| UploadError::Io(format!("create session {}: missing data", url)))
            })
    }

    /// GET 断点重查（服务端已收字节；404 → SessionNotFound）
    pub async fn query_session(&self, url: &str, auth: &str) -> Result<UploadSessionInfo, UploadError> {
        let resp = self
            .client
            .get(url)
            .header(reqwest::header::AUTHORIZATION, auth)
            .send()
            .await
            .map_err(|e| UploadError::Network(format!("GET {} failed: {}", url, e)))?;
        parse_json_envelope::<UploadSessionInfo>(resp, url, "query session")
            .await
            .and_then(|info| {
                info.data
                    .ok_or_else(|| UploadError::Io(format!("query session {}: missing data", url)))
            })
    }

    /// PUT append：从 source 流式上传，返回服务端最新 received
    ///
    /// 409 offset mismatch → OffsetMismatch；404 → SessionNotFound（重建）；
    /// 取消 → Cancelled（源 fd 未关闭，续传语义由调用方重开）
    pub async fn append_stream(
        &self,
        url: &str,
        auth: &str,
        offset: u64,
        source: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
        handle: &TransferHandle,
    ) -> Result<u64, UploadError> {
        let url_with_offset = format!("{}?offset={}", url, offset);
        let transferred = handle.transferred_handle();
        let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(source).map(move |item| {
            item.map(|bytes| {
                transferred.fetch_add(bytes.len() as u64, std::sync::atomic::Ordering::Relaxed);
                bytes
            })
        }));
        let resp = self
            .client
            .put(&url_with_offset)
            .header(reqwest::header::AUTHORIZATION, auth)
            .body(body)
            .send()
            .await
            .map_err(|e| {
                if handle.is_cancelled() {
                    UploadError::Cancelled
                } else {
                    UploadError::Network(format!("PUT {} failed: {}", url, e))
                }
            })?;
        let status = resp.status().as_u16();
        match status {
            200..=299 => {
                let envelope = parse_json_envelope::<UploadSessionInfo>(resp, &url_with_offset, "append").await?;
                envelope
                    .data
                    .map(|d| d.received)
                    .ok_or_else(|| UploadError::Io(format!("append {}: missing data", url)))
            }
            409 => {
                let msg = resp.text().await.unwrap_or_else(|_| "offset mismatch".to_string());
                // 从错误消息中尝试提取服务端已收字节（警告性；以重查为准）
                let expected = extract_expected_offset(&msg).unwrap_or(0);
                Err(UploadError::OffsetMismatch { expected, got: offset })
            }
            404 => Err(UploadError::SessionNotFound(url.to_string())),
            _ => {
                let msg = resp.text().await.unwrap_or_default();
                Err(UploadError::Http {
                    status,
                    url: url.to_string(),
                    message: msg,
                })
            }
        }
    }

    /// POST complete（原子改名）；409 → DuplicateName，404 → SessionNotFound
    pub async fn complete_session(&self, url: &str, auth: &str) -> Result<(), CompleteError> {
        let resp = self
            .client
            .post(url)
            .header(reqwest::header::AUTHORIZATION, auth)
            .send()
            .await
            .map_err(|e| CompleteError::Network(format!("POST {} failed: {}", url, e)))?;
        match resp.status().as_u16() {
            200..=299 => Ok(()),
            409 => {
                let msg = resp.text().await.unwrap_or_default();
                Err(CompleteError::DuplicateName(msg))
            }
            404 => Err(CompleteError::SessionNotFound(url.to_string())),
            s => {
                let msg = resp.text().await.unwrap_or_default();
                Err(CompleteError::Http {
                    status: s,
                    url: url.to_string(),
                    message: msg,
                })
            }
        }
    }

    /// DELETE 取消会话（清理服务端 .part）
    pub async fn cancel_session(&self, url: &str, auth: &str) -> Result<(), UploadError> {
        let resp = self
            .client
            .delete(url)
            .header(reqwest::header::AUTHORIZATION, auth)
            .send()
            .await
            .map_err(|e| UploadError::Network(format!("DELETE {} failed: {}", url, e)))?;
        if resp.status().is_success() || resp.status().as_u16() == 404 {
            Ok(())
        } else {
            Err(UploadError::Http {
                status: resp.status().as_u16(),
                url: url.to_string(),
                message: resp.text().await.unwrap_or_default(),
            })
        }
    }

    // ==================== 全链路编排（上传文件） ====================

    /// 上传本地/SAF 文件到桌面（完整编排：建会话 → 重查偏移 → append → complete）
    ///
    /// - `source_path`：本地路径或 `content://`（需 `saf` 与 offset 语义）
    /// - `saf`：SAF 桥实现（Android 注入；非 Android / content:// 不可用时传 None）
    /// - 断点真源 = 服务端 session received：网络错误 → 重查偏移续传；
    ///   session 404 → 重建；complete 409 → DuplicateName（终态失败）
    /// - 返回最终可见会话信息（received == size）
    #[allow(clippy::too_many_arguments)]
    pub async fn upload_file(
        &self,
        base: &str,
        plugin_id: &str,
        mount_path: &str,
        create: &CreateUploadRequest,
        auth: &str,
        source_path: &str,
        saf: Option<Arc<dyn crate::plugin::saf_io::SafIo>>,
        handle: &TransferHandle,
    ) -> Result<UploadSessionInfo, UploadError> {
        let token = handle.token();
        let mut session_id: Option<String> = None;
        let mut network_failures = 0u32;
        let max_network_failures = 3u32;
        // 断点历史字节是否已补报（仅首次 query 后上报一次；本轮 append 的实际
        // 传输字节由 append_stream 内部 fetch_add，避免后续重复叠加）
        let mut initial_offset_reported = false;

        loop {
            if token.is_cancelled() {
                return Err(UploadError::Cancelled);
            }

            // 1. 建会话（404 重建触发）
            let sid = match session_id.take() {
                Some(s) => s,
                None => {
                    match self
                        .create_session(&endpoint(base, plugin_id, mount_path, "upload"), auth, create)
                        .await
                    {
                        Ok(info) => info.session_id,
                        Err(UploadError::Network(e)) => {
                            network_failures += 1;
                            if network_failures >= max_network_failures {
                                return Err(UploadError::Network(format!(
                                    "create session failed after {} attempts: {}",
                                    network_failures, e
                                )));
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
            };
            network_failures = 0;

            // 2. 断点重查（服务端 received = 续传真源）
            let query_url = endpoint(base, plugin_id, mount_path, &format!("upload/{}", sid));
            let received = match self.query_session(&query_url, auth).await {
                Ok(info) => info.received,
                Err(UploadError::SessionNotFound(_)) => {
                    // 会话 404：重建
                    session_id = None;
                    continue;
                }
                Err(UploadError::Network(e)) => {
                    network_failures += 1;
                    if network_failures >= max_network_failures {
                        return Err(UploadError::Network(format!(
                            "query session failed after {} attempts: {}",
                            network_failures, e
                        )));
                    }
                    // 网络抖动：服务端 session 仍有效，保留 sid 下一轮重查（否则
                    // 重建空 session 从头传，击穿断点续传）
                    session_id = Some(sid.clone());
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    continue;
                }
                Err(e) => return Err(e),
            };
            handle.set_total(create.size);
            // 进度 = 断点历史（received）+ 本轮 append 内部累计。历史只在首次
            // 补报一次（append_stream 已按实际传输字节 fetch_add，若再按增量
            // 叠加会双计 → 进度超 100%）
            if !initial_offset_reported {
                let base = received.min(create.size);
                if base > 0 {
                    handle.add_transferred(base);
                }
                initial_offset_reported = true;
            }

            // 已收满 → 直接 complete（断点续传命中快路径）
            if received >= create.size {
                match self
                    .complete_session(
                        &endpoint(base, plugin_id, mount_path, &format!("upload/{}/complete", sid)),
                        auth,
                    )
                    .await
                {
                    Ok(()) => {
                        return Ok(UploadSessionInfo {
                            session_id: sid,
                            received,
                        })
                    }
                    Err(CompleteError::DuplicateName(p)) => return Err(UploadError::DuplicateName(p)),
                    Err(CompleteError::SessionNotFound(_)) => {
                        session_id = None;
                        continue;
                    }
                    Err(CompleteError::Http { status, url, message }) => {
                        return Err(UploadError::Http { status, url, message })
                    }
                    Err(CompleteError::Network(e)) => {
                        network_failures += 1;
                        if network_failures >= max_network_failures {
                            return Err(UploadError::Network(format!(
                                "complete failed after {} attempts: {}",
                                network_failures, e
                            )));
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        continue;
                    }
                }
            }

            // 3. 打开源（本地路径或 SAF 流），从 received 续读
            let source = open_source(source_path, received, saf.clone())
                .await
                .map_err(UploadError::Io)?;

            // 4. append 流式上传
            let append_url = endpoint(base, plugin_id, mount_path, &format!("upload/{}", sid));
            match self.append_stream(&append_url, auth, received, source, handle).await {
                Ok(new_received) => {
                    if new_received >= create.size {
                        // 全量收齐 → complete
                        match self
                            .complete_session(
                                &endpoint(base, plugin_id, mount_path, &format!("upload/{}/complete", sid)),
                                auth,
                            )
                            .await
                        {
                            Ok(()) => {
                                return Ok(UploadSessionInfo {
                                    session_id: sid,
                                    received: new_received,
                                })
                            }
                            Err(CompleteError::DuplicateName(p)) => return Err(UploadError::DuplicateName(p)),
                            Err(CompleteError::SessionNotFound(_)) => {
                                session_id = None;
                                continue;
                            }
                            Err(CompleteError::Http { status, url, message }) => {
                                return Err(UploadError::Http { status, url, message })
                            }
                            Err(CompleteError::Network(e)) => {
                                // complete 网络失败：服务端 received 已达 size，
                                // 下一轮走断点重查快路径（received >= size → complete）。
                                // 保留 sid（服务端 session 未丢，重建会从头传）
                                session_id = Some(sid.clone());
                                network_failures += 1;
                                if network_failures >= max_network_failures {
                                    return Err(UploadError::Network(format!(
                                        "complete failed after {} attempts: {}",
                                        network_failures, e
                                    )));
                                }
                                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                            }
                        }
                    } else {
                        // 未收齐（服务端可能截断）→ 继续 append（下一轮重查偏移）
                        session_id = Some(sid);
                    }
                }
                Err(UploadError::OffsetMismatch { .. }) => {
                    // offset 不符：服务端偏移变过 → 循环重查后续传
                    session_id = Some(sid);
                }
                Err(UploadError::SessionNotFound(_)) => {
                    session_id = None;
                }
                Err(UploadError::Cancelled) => return Err(UploadError::Cancelled),
                Err(UploadError::Network(e)) => {
                    network_failures += 1;
                    if network_failures >= max_network_failures {
                        return Err(UploadError::Network(format!(
                            "append failed after {} attempts: {}",
                            network_failures, e
                        )));
                    }
                    // 网络抖动：保留 sid 下一轮重查偏移续传（服务端已收字节不丢）
                    session_id = Some(sid.clone());
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// 从 409 错误消息提取服务端已收字节（"server has N bytes"）
fn extract_expected_offset(msg: &str) -> Option<u64> {
    let lower = msg.to_ascii_lowercase();
    let need = "has";
    let idx = lower.find(need)? + need.len();
    lower[idx..]
        .trim_start()
        .trim_start_matches("bytes:")
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

/// 解析 ApiResponse 信封并映射 HTTP 状态到 UploadError
async fn parse_json_envelope<T: for<'de> Deserialize<'de>>(
    resp: reqwest::Response,
    url: &str,
    op: &str,
) -> Result<ApiEnvelope<T>, UploadError> {
    let status = resp.status().as_u16();
    if !(200..=299).contains(&status) {
        let msg = resp.text().await.unwrap_or_default();
        return Err(UploadError::Http {
            status,
            url: url.to_string(),
            message: msg,
        });
    }
    let text = resp
        .text()
        .await
        .map_err(|e| UploadError::Network(format!("{}: read body failed: {}", op, e)))?;
    serde_json::from_str(&text)
        .map_err(|e| UploadError::Io(format!("{}: invalid ApiResponse JSON from '{}': {}", op, url, e)))
}

/// 打开上传源：本地路径（tokio::fs，可 seek 真续传）或 content:// SAF 流
async fn open_source(
    path: &str,
    offset: u64,
    saf: Option<Arc<dyn crate::plugin::saf_io::SafIo>>,
) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>, String> {
    if path.starts_with("content://") {
        let saf = saf.ok_or_else(|| {
            format!(
                "open SAF stream '{}' failed: SafIo unavailable (SAF is Android-only)",
                path
            )
        })?;
        let handle = saf
            .open_stream(path, offset)
            .map_err(|e| format!("saf_open '{}' failed: {}", path, e))?;
        if handle.effective_offset != offset {
            return Err(format!(
                "SAF stream '{}' not seekable to offset {} (effective {})",
                path, offset, handle.effective_offset
            ));
        }
        let handle_id = handle.handle_id;
        Ok(Box::new(SafStreamReader {
            handle_id,
            saf,
            eof: false,
        }))
    } else {
        let mut file = tokio::fs::File::open(Path::new(path))
            .await
            .map_err(|e| format!("open local file '{}' failed: {}", path, e))?;
        if offset > 0 {
            use tokio::io::AsyncSeekExt;
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|e| format!("seek local file to offset {} failed: {}", offset, e))?;
        }
        Ok(Box::new(file))
    }
}

/// SAF 流直传的 AsyncRead 适配（桥读为同步阻塞，poll_read 内跨桥；EOF 返回 0）
///
/// drop 不关闭句柄：上传失败/取消保留 fd 供任务内顺序续读，成功路径由
/// 调用方显式 close_stream 或 Kotlin 超时清扫兜底（与 plugin/transfer.rs 一致）
struct SafStreamReader {
    handle_id: String,
    saf: std::sync::Arc<dyn crate::plugin::saf_io::SafIo>,
    eof: bool,
}

impl tokio::io::AsyncRead for SafStreamReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if self.eof {
            return std::task::Poll::Ready(Ok(()));
        }
        let capacity = buf.remaining();
        if capacity == 0 {
            return std::task::Poll::Ready(Ok(()));
        }
        match self.saf.read_stream(&self.handle_id, capacity) {
            Ok(data) => {
                if data.is_empty() {
                    self.eof = true;
                    std::task::Poll::Ready(Ok(()))
                } else {
                    let n = data.len().min(capacity);
                    buf.put_slice(&data[..n]);
                    std::task::Poll::Ready(Ok(()))
                }
            }
            Err(e) => std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("saf_read_stream: {}", e),
            ))),
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn disable_proxy_for_loopback() {
        std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
    }

    /// mock 请求（含 path + query + body）
    struct Req {
        method: String,
        path: String,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    }

    impl Req {
        fn header(&self, k: &str) -> Option<&str> {
            self.headers.get(k).map(|s| s.as_str())
        }
        fn query(&self, key: &str) -> Option<String> {
            let (_, q) = self.path.split_once('?')?;
            for pair in q.split('&') {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                if k == key {
                    return Some(v.to_string());
                }
            }
            None
        }
    }

    /// mock 响应（可含 JSON body）
    struct Resp {
        status: u16,
        body: Vec<u8>,
    }
    impl Resp {
        fn json(status: u16, body: impl Into<Vec<u8>>) -> Self {
            Self {
                status,
                body: body.into(),
            }
        }
        fn ok_data(data: serde_json::Value) -> Self {
            let payload = serde_json::json!({ "code": 0, "message": "ok", "data": data });
            Self {
                status: 200,
                body: serde_json::to_vec(&payload).unwrap(),
            }
        }
        fn err(status: u16, msg: &str) -> Self {
            let payload = serde_json::json!({ "code": status, "message": msg });
            Self {
                status,
                body: serde_json::to_vec(&payload).unwrap(),
            }
        }
    }

    /// mock 会话
    #[derive(Default)]
    struct MockSession {
        received: usize,
    }

    /// mock 服务器共享状态
    struct MockState {
        sessions: HashMap<String, MockSession>,
        /// 触发行为："409-once" / "404-query-once" / "404-append-once"
        trigger: Option<String>,
        /// 已触发标记
        triggered: bool,
        /// complete 返回冲突一次
        dup_complete: bool,
    }

    async fn spawn_session_server(state: Arc<Mutex<MockState>>) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let state = state.clone();
                tokio::spawn(async move {
                    let Some(req) = read_request(&mut sock).await else {
                        return;
                    };
                    let resp = route(&req, &state);
                    let reason = match resp.status {
                        200 => "OK",
                        404 => "Not Found",
                        409 => "Conflict",
                        403 => "Forbidden",
                        _ => "OK",
                    };
                    let head = format!(
                        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                        resp.status, reason, resp.body.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(&resp.body).await;
                });
            }
        });
        addr
    }

    /// 读取并解析 HTTP 请求（请求行 + 头 + body：content-length / chunked）
    async fn read_request(sock: &mut tokio::net::TcpStream) -> Option<Req> {
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
        let path = parts.next()?.to_string();
        let mut headers = HashMap::new();
        let mut content_length = 0usize;
        for line in lines {
            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim().to_ascii_lowercase();
                let value = v.trim().to_string();
                if key == "content-length" {
                    content_length = value.parse().unwrap_or(0);
                }
                headers.insert(key, value);
            }
        }
        if headers
            .get("expect")
            .map(|v| v.to_ascii_lowercase().starts_with("100"))
            .unwrap_or(false)
        {
            let _ = sock.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await;
        }
        let mut body = buf.split_off(header_end + 4);
        if headers
            .get("transfer-encoding")
            .map(|v| v.to_ascii_lowercase() == "chunked")
            .unwrap_or(false)
        {
            body = read_chunked(sock, body).await?;
        } else {
            while body.len() < content_length {
                let n = sock.read(&mut tmp).await.ok()?;
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&tmp[..n]);
            }
            body.truncate(content_length);
        }
        Some(Req {
            method,
            path,
            headers,
            body,
        })
    }

    async fn read_chunked(sock: &mut tokio::net::TcpStream, mut buf: Vec<u8>) -> Option<Vec<u8>> {
        let mut tmp = [0u8; 4096];
        let mut out = Vec::new();
        loop {
            let line_end = match buf.windows(2).position(|w| w == b"\r\n") {
                Some(p) => p,
                None => {
                    let n = sock.read(&mut tmp).await.ok()?;
                    if n == 0 {
                        return None;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    continue;
                }
            };
            let line = String::from_utf8_lossy(&buf[..line_end]).to_string();
            buf.drain(..line_end + 2);
            let size = usize::from_str_radix(line.split(';').next().unwrap_or("").trim(), 16).ok()?;
            if size == 0 {
                return Some(out);
            }
            while buf.len() < size + 2 {
                let n = sock.read(&mut tmp).await.ok()?;
                if n == 0 {
                    return None;
                }
                buf.extend_from_slice(&tmp[..n]);
            }
            out.extend_from_slice(&buf[..size]);
            buf.drain(..size + 2);
        }
    }

    /// 路由：/{plugin}/{mount}/upload 三件套
    fn route(req: &Req, state: &Arc<Mutex<MockState>>) -> Resp {
        let path = req.path.split('?').next().unwrap_or("").to_string();
        let mut st = state.lock().unwrap();

        // 触发行为：一次性清除会话（模拟对端清理/重启）
        let sid = path
            .strip_prefix("/api/plugins/p/m/upload/")
            .map(|s| s.trim_end_matches("/complete").to_string());

        if req.method == "POST" && path.ends_with("/upload") {
            // 建会话
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(serde_json::Value::Null);
            if req.header("authorization").is_none() || body.is_null() {
                return Resp::err(403, "batch-context-required");
            }
            let id = format!("s{}", st.sessions.len() + 1);
            st.sessions.insert(id.clone(), MockSession::default());
            return Resp::ok_data(serde_json::json!({ "sessionId": id, "received": 0 }));
        }

        if req.method == "PUT" && path.contains("/upload/") {
            let id = sid.clone().unwrap();
            if !st.triggered && st.trigger.as_deref() == Some("404-append-once") {
                st.triggered = true;
                st.sessions.remove(&id);
                return Resp::err(404, "upload session not found: gone");
            }
            let Some(session) = st.sessions.get_mut(&id) else {
                return Resp::err(404, "upload session not found");
            };
            let offset: u64 = req.query("offset").and_then(|s| s.parse().ok()).unwrap_or(0);
            if offset as usize != session.received {
                return Resp::err(
                    409,
                    &format!(
                        "offset mismatch: server has {} bytes, client sent offset {}",
                        session.received, offset
                    ),
                );
            }
            session.received += req.body.len();
            return Resp::ok_data(serde_json::json!({
                "sessionId": id,
                "received": session.received,
            }));
        }

        if req.method == "GET" && path.contains("/upload/") {
            let id = sid.clone().unwrap();
            if !st.triggered && st.trigger.as_deref() == Some("404-query-once") {
                st.triggered = true;
                st.sessions.remove(&id);
                return Resp::err(404, "upload session not found: gone");
            }
            let Some(session) = st.sessions.get(&id) else {
                return Resp::err(404, "upload session not found");
            };
            return Resp::ok_data(serde_json::json!({
                "sessionId": id,
                "received": session.received,
            }));
        }

        if req.method == "POST" && path.ends_with("/complete") {
            let id = path
                .strip_prefix("/api/plugins/p/m/upload/")
                .and_then(|s| s.strip_suffix("/complete"))
                .unwrap_or("")
                .to_string();
            if st.sessions.get(&id).is_none() {
                return Resp::err(404, "upload session not found");
            }
            if st.dup_complete {
                return Resp::err(409, "duplicate-name");
            }
            st.sessions.remove(&id);
            return Resp::ok_data(serde_json::Value::Null);
        }

        Resp::err(404, "unknown route")
    }

    fn state_with(trigger: Option<&str>) -> Arc<Mutex<MockState>> {
        Arc::new(Mutex::new(MockState {
            sessions: HashMap::new(),
            trigger: trigger.map(|s| s.to_string()),
            triggered: false,
            dup_complete: false,
        }))
    }

    fn base_of(addr: SocketAddr) -> String {
        format!("http://{}", addr)
    }

    fn handle(file: &str) -> TransferHandle {
        TransferHandle::new(format!("up-{}", file))
    }

    #[test]
    fn create_session_parses_envelope() {
        disable_proxy_for_loopback();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = state_with(None);
            let addr = spawn_session_server(state).await;
            let client = UploadClient::new();
            let info = client
                .create_session(
                    &format!("{}/api/plugins/p/m/upload", base_of(addr)),
                    "Bearer tok",
                    &CreateUploadRequest {
                        relative_path: "movies/a.mp4".to_string(),
                        size: 100,
                        batch_id: Some("b1".to_string()),
                    },
                )
                .await
                .unwrap();
            assert_eq!(info.received, 0);
            assert!(!info.session_id.is_empty());
        });
    }

    #[test]
    fn upload_full_flow_creates_appends_completes() {
        disable_proxy_for_loopback();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let data: Vec<u8> = (0..8 * 1024).map(|i| (i % 253) as u8).collect();
            let dir = tempfile::tempdir().unwrap();
            let src = dir.path().join("src.bin");
            std::fs::write(&src, &data).unwrap();

            let state = state_with(None);
            let addr = spawn_session_server(state.clone()).await;
            let client = UploadClient::new();
            let h = handle("full");
            let info = client
                .upload_file(
                    &base_of(addr),
                    "p",
                    "m",
                    &CreateUploadRequest {
                        relative_path: "dst.bin".to_string(),
                        size: data.len() as u64,
                        batch_id: None,
                    },
                    "Bearer tok",
                    src.to_str().unwrap(),
                    None,
                    &h,
                )
                .await
                .unwrap();
            assert_eq!(info.received, data.len() as u64);
            // 服务端完整接收
            let st = state.lock().unwrap();
            assert!(st.sessions.is_empty(), "complete 后会话应移除");
            assert_eq!(h.progress().0, data.len() as u64);
        });
    }

    #[test]
    fn append_stream_rejects_offset_mismatch() {
        disable_proxy_for_loopback();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // 直接测低层 append_stream：服务端已收 5 字节，客户端 offset=0 → 409
            let addr = spawn_session_server(Arc::new(Mutex::new(MockState {
                sessions: HashMap::from([("s1".to_string(), MockSession { received: 5 })]),
                trigger: None,
                triggered: false,
                dup_complete: false,
            })))
            .await;
            let client = UploadClient::new();
            let h = handle("mismatch");
            let source: Box<dyn tokio::io::AsyncRead + Send + Unpin> =
                Box::new(tokio::io::BufReader::new(b"01234".as_slice()));
            let err = client
                .append_stream(
                    &format!("{}/api/plugins/p/m/upload/s1", base_of(addr)),
                    "Bearer tok",
                    0,
                    source,
                    &h,
                )
                .await
                .unwrap_err();
            assert!(
                matches!(err, UploadError::OffsetMismatch { expected: 5, got: 0 }),
                "got: {:?}",
                err
            );
        });
    }

    #[test]
    fn upload_404_rebuilds_session_and_completes() {
        disable_proxy_for_loopback();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // append 首次触发 404（模拟对端清理会话）→ 客户端重建 + 续传完成
            let data: Vec<u8> = b"hello-world".to_vec();
            let dir = tempfile::tempdir().unwrap();
            let src = dir.path().join("src.bin");
            std::fs::write(&src, &data).unwrap();

            let state = state_with(Some("404-append-once"));
            let addr = spawn_session_server(state).await;
            let client = UploadClient::new();
            let h = handle("404");
            let info = client
                .upload_file(
                    &base_of(addr),
                    "p",
                    "m",
                    &CreateUploadRequest {
                        relative_path: "dst.bin".to_string(),
                        size: data.len() as u64,
                        batch_id: None,
                    },
                    "Bearer tok",
                    src.to_str().unwrap(),
                    None,
                    &h,
                )
                .await
                .unwrap();
            assert_eq!(info.received, data.len() as u64);
        });
    }

    #[test]
    fn upload_complete_duplicate_name_rejected() {
        disable_proxy_for_loopback();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let data: Vec<u8> = b"dup-data".to_vec();
            let dir = tempfile::tempdir().unwrap();
            let src = dir.path().join("src.bin");
            std::fs::write(&src, &data).unwrap();

            let addr = spawn_session_server(Arc::new(Mutex::new(MockState {
                sessions: HashMap::new(),
                trigger: None,
                triggered: false,
                dup_complete: true,
            })))
            .await;
            let client = UploadClient::new();
            let h = handle("dup");
            let err = client
                .upload_file(
                    &base_of(addr),
                    "p",
                    "m",
                    &CreateUploadRequest {
                        relative_path: "dst.bin".to_string(),
                        size: data.len() as u64,
                        batch_id: None,
                    },
                    "Bearer tok",
                    src.to_str().unwrap(),
                    None,
                    &h,
                )
                .await
                .unwrap_err();
            assert!(matches!(err, UploadError::DuplicateName(_)), "got: {:?}", err);
        });
    }

    #[test]
    fn extract_expected_offset_parses_message() {
        assert_eq!(
            extract_expected_offset("offset mismatch: server has 123 bytes, client sent offset 0"),
            Some(123)
        );
        assert_eq!(extract_expected_offset("boom"), None);
    }
}
