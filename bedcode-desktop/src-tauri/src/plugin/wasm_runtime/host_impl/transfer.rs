//! 传输引擎域宿主实现（断点续传的文件上传/下载）
//!
//! 宿主托管实际字节搬运，插件只负责任务编排（规格第 6、7 节）：
//! - 下载：reqwest GET + `Range: bytes={offset}-` → tokio 流式写文件
//! - 上传：本地文件从 offset seek → reqwest PUT 流式 body（对端 upload session append）
//! - 进度每 500ms 双通道推送：Tauri 事件 `plugin:transfer:progress`
//!   + 消息总线 `transfer:{task_id}`（载荷均为 TransferProgress）
//! - 取消：tokio_util CancellationToken，终态进度回报最终偏移供续传持久化
//!
//! 所有错误结构化回报（Failed(reason) 终态事件），禁止静默失败

use crate::plugin::fs_auth::FsOp;
use crate::plugin::message_bus::MessageBus;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};
use crate::system::error_boundary::spawn_with_error_boundary;
use bedcode_plugin_api::permission::PERMISSION_TRANSFER;
use bedcode_plugin_api::{TransferDirection, TransferProgress, TransferRequest, TransferState};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// 进度推送间隔（规格：每 500ms）
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
/// 流式 IO 缓冲（规格：256KB–1MB 取中值）
const IO_BUFFER_SIZE: usize = 512 * 1024;

/// 活跃传输任务表（task_id → 取消令牌）
///
/// 任务完成/失败/取消后自行移除条目；cancel 查不到条目视为已完成。
/// tokio Mutex/HashMap::new 非 const fn，经 OnceLock 惰性初始化
static TASKS: std::sync::OnceLock<Mutex<HashMap<String, CancellationToken>>> =
    std::sync::OnceLock::new();

fn tasks() -> &'static Mutex<HashMap<String, CancellationToken>> {
    TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 传输任务终局
enum Outcome {
    Completed,
    Cancelled,
    /// 失败原因（结构化回报给插件）
    Failed(String),
}

// ==================== 逻辑层（Component Model 绑定调用） ====================

/// 启动传输任务（权限 + fs 授权 + 登记 + spawn），返回 task_id
///
/// 宿主托管实际字节搬运，插件只负责任务编排（规格第 6、7 节）。
/// 本地路径授权按方向判定：下载 = 写授权，上传 = 读授权。
pub(crate) fn transfer_start(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    request_json: &str,
) -> Result<String, String> {
    let request: TransferRequest = serde_json::from_str(request_json)
        .map_err(|e| format!("transfer error: invalid TransferRequest JSON: {}", e))?;

    if !super::check_permission(host_ctx, plugin_id, PERMISSION_TRANSFER, "host_transfer_start") {
        return Err("permission denied".to_string());
    }

    // 本地路径 fs 授权：下载 = 写授权，上传 = 读授权
    let fs_op = match request.direction {
        TransferDirection::Download => FsOp::Write,
        TransferDirection::Upload => FsOp::Read,
    };
    let fs_auth = host_ctx.fs_auth.clone();
    let local_path = request.local_path.clone();
    if !block_on_async(fs_auth.check(plugin_id, &local_path, fs_op)) {
        tracing::error!(
            plugin_id = %plugin_id,
            local_path = %local_path,
            "transfer_start: local path not authorized by user"
        );
        return Err("transfer error: local path not authorized by user".to_string());
    }

    // final_path 是下载完成后的 rename 目标，同样需要写授权校验
    if let Some(ref final_path) = request.final_path {
        if !block_on_async(fs_auth.check(plugin_id, final_path, fs_op)) {
            tracing::error!(
                plugin_id = %plugin_id,
                final_path = %final_path,
                "transfer_start: final_path not authorized by user"
            );
            return Err("transfer error: final_path not authorized by user".to_string());
        }
    }

    // 用插件预生成的 task_id（bus topic `transfer:{task_id}` 与 Tauri 事件
    // taskId 均以它为准），不再自生成 UUID —— 插件先订阅后启动，进度零丢失
    let task_id = request.task_id.clone();
    let token = CancellationToken::new();

    // 先登记再 spawn：避免 cancel 早于任务注册到达而丢失取消语义
    let task_id_for_map = task_id.clone();
    let token_for_map = token.clone();
    block_on_async(async move {
        tasks().lock().await.insert(task_id_for_map, token_for_map);
    });

    let app_handle = host_ctx.app_handle.clone();
    let bus = host_ctx.message_bus.clone();
    let task_id_for_spawn = task_id.clone();
    spawn_with_error_boundary("plugin_transfer_task", async move {
        run_transfer(task_id_for_spawn, request, app_handle, bus, token).await;
    });

    Ok(task_id)
}

/// 取消传输任务（权限 + 查任务表），任务不存在视为幂等成功
pub(crate) fn transfer_cancel(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    task_id: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_TRANSFER, "host_transfer_cancel") {
        return Err("permission denied".to_string());
    }
    let token = block_on_async(async { tasks().lock().await.get(task_id).cloned() });
    match token {
        Some(token) => {
            tracing::info!(plugin_id = %plugin_id, task_id = %task_id, "transfer cancel requested");
            token.cancel();
        }
        None => {
            tracing::debug!(
                plugin_id = %plugin_id,
                task_id = %task_id,
                "transfer_cancel: task not active (already finished or unknown)"
            );
        }
    }
    Ok(())
}

// ==================== Transfer Task ====================

/// 运行传输任务：进度 reporter + 传输本体，终局推送最终进度后注销任务
async fn run_transfer(
    task_id: String,
    request: TransferRequest,
    app_handle: Option<Arc<tauri::AppHandle>>,
    bus: Arc<MessageBus>,
    token: CancellationToken,
) {
    let transferred = Arc::new(AtomicU64::new(request.offset));
    // 上传总大小：插件 expected_size 恒为 0（见插件 start_single_task），此处
    // 从本地文件 metadata 取真实大小，进度事件携带真实 total → 插件更新
    // task.size → 前端进度条可动；下载沿用插件上报值
    let total = match request.direction {
        TransferDirection::Upload => tokio::fs::metadata(&request.local_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0),
        TransferDirection::Download => request.expected_size,
    };

    // 进度 reporter：每 500ms 推送 Running 进度（含瞬时速率）
    let reporter_token = token.child_token();
    {
        let transferred = transferred.clone();
        let app_handle = app_handle.clone();
        let bus = bus.clone();
        let task_id = task_id.clone();
        // reporter 任务持有自己的令牌副本（cancelled() 借用需随任务 move）
        let reporter_stop = reporter_token.clone();
        spawn_with_error_boundary("plugin_transfer_progress", async move {
            let mut interval = tokio::time::interval(PROGRESS_INTERVAL);
            interval.tick().await; // 首个 tick 立即完成，跳过避免启动即推
            let mut last_bytes = transferred.load(Ordering::Relaxed);
            let mut last_tick = Instant::now();
            loop {
                tokio::select! {
                    _ = reporter_stop.cancelled() => break,
                    _ = interval.tick() => {}
                }
                let now = Instant::now();
                let current = transferred.load(Ordering::Relaxed);
                let elapsed = now.duration_since(last_tick).as_secs_f64();
                let bytes_per_sec = if elapsed > 0.0 {
                    (current.saturating_sub(last_bytes) as f64 / elapsed) as u64
                } else {
                    0
                };
                last_bytes = current;
                last_tick = now;
                emit_progress(
                    &app_handle,
                    &bus,
                    &task_id,
                    current,
                    total,
                    bytes_per_sec,
                    TransferState::Running,
                );
            }
        });
    }

    // 取消立即中断传输 future（下载中断流读取 / 上传丢弃请求体）
    let outcome = tokio::select! {
        _ = token.cancelled() => Outcome::Cancelled,
        result = execute_transfer(&request, transferred.clone(), token.clone()) => match result {
            Ok(()) => Outcome::Completed,
            Err(reason) => Outcome::Failed(reason),
        },
    };

    reporter_token.cancel();
    tasks().lock().await.remove(&task_id);

    // 终局事件（携带最终偏移，插件据此持久化续传点）
    let final_bytes = transferred.load(Ordering::Relaxed);
    let state = match &outcome {
        Outcome::Completed => TransferState::Completed,
        Outcome::Cancelled => TransferState::Cancelled,
        Outcome::Failed(reason) => TransferState::Failed(reason.clone()),
    };
    emit_progress(&app_handle, &bus, &task_id, final_bytes, total, 0, state);

    match &outcome {
        Outcome::Completed => {
            tracing::info!(task_id = %task_id, bytes = final_bytes, "transfer completed");
        }
        Outcome::Cancelled => {
            tracing::info!(task_id = %task_id, bytes = final_bytes, "transfer cancelled");
        }
        Outcome::Failed(reason) => {
            tracing::error!(task_id = %task_id, bytes = final_bytes, reason = %reason, "transfer failed");
        }
    }
}

/// 执行传输本体（按方向分发）
async fn execute_transfer(
    request: &TransferRequest,
    transferred: Arc<AtomicU64>,
    token: CancellationToken,
) -> Result<(), String> {
    match request.direction {
        TransferDirection::Download => download(request, transferred, token).await,
        TransferDirection::Upload => upload(request, transferred).await,
    }
}

/// 下载：GET 对端文件（Range 续传）→ tokio 流式写本地文件
async fn download(
    request: &TransferRequest,
    transferred: Arc<AtomicU64>,
    token: CancellationToken,
) -> Result<(), String> {
    use tokio::io::{AsyncSeekExt, AsyncWriteExt};

    let client = reqwest::Client::new();
    let mut builder = client.get(&request.url);
    for (key, value) in &request.headers {
        builder = builder.header(key.as_str(), value.as_str());
    }
    if request.offset > 0 {
        builder = builder.header(
            reqwest::header::RANGE,
            format!("bytes={}-", request.offset),
        );
    }

    let response = builder
        .send()
        .await
        .map_err(|e| format!("GET {} failed: {}", request.url, e))?;

    let status = response.status();
    if !(status.is_success() || status == reqwest::StatusCode::PARTIAL_CONTENT) {
        return Err(format!(
            "GET {} returned HTTP {}",
            request.url,
            status.as_u16()
        ));
    }

    // offset=0 全新写入（truncate 清理残留）；offset>0 保留已传进度，seek 后续写
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(request.offset == 0)
        .open(&request.local_path)
        .await
        .map_err(|e| format!("open local file '{}' failed: {}", request.local_path, e))?;

    file.seek(std::io::SeekFrom::Start(request.offset))
        .await
        .map_err(|e| format!("seek local file to offset {} failed: {}", request.offset, e))?;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if token.is_cancelled() {
            // 外层 select 已处理取消，这里是 future 被 poll 到前的双保险
            return Err("cancelled".to_string());
        }
        let chunk = chunk.map_err(|e| format!("download stream error: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("write local file failed: {}", e))?;
        transferred.fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }

    file.flush()
        .await
        .map_err(|e| format!("flush local file failed: {}", e))?;
    drop(file); // 释放文件句柄，避免 rename 时 Windows 文件锁冲突

    // .part 临时文件下载完成后原子 rename 到最终路径（规格 7.4）
    if let Some(ref final_path) = request.final_path {
        if tokio::fs::try_exists(final_path)
            .await
            .unwrap_or(false)
        {
            // 目标名已被占用 → 保留 .part 供用户决定，回报 duplicate-name
            tracing::warn!(
                part_path = %request.local_path,
                final_path = %final_path,
                "download: final_path already exists, keeping .part file"
            );
            return Err("duplicate-name".to_string());
        }
        tokio::fs::rename(&request.local_path, final_path)
            .await
            .map_err(|e| {
                format!(
                    "rename '{}' -> '{}' failed: {}",
                    request.local_path, final_path, e
                )
            })?;
        tracing::debug!(
            part_path = %request.local_path,
            final_path = %final_path,
            "download: .part renamed to final path"
        );
    }

    Ok(())
}

/// 上传：本地文件从 offset seek → PUT 流式 body 到对端 upload session
///
/// Upload 方向忽略 final_path（仅 Download 方向用于 .part → 最终名原子落位）
async fn upload(request: &TransferRequest, transferred: Arc<AtomicU64>) -> Result<(), String> {
    use tokio::io::AsyncSeekExt;

    let mut file = tokio::fs::File::open(&request.local_path)
        .await
        .map_err(|e| format!("open local file '{}' failed: {}", request.local_path, e))?;

    if request.offset > 0 {
        file.seek(std::io::SeekFrom::Start(request.offset))
            .await
            .map_err(|e| format!("seek local file to offset {} failed: {}", request.offset, e))?;
    }

    // ReaderStream 按 IO_BUFFER_SIZE 读块；inspect 中累计已传字节（进度 reporter 读取）
    let stream = tokio_util::io::ReaderStream::with_capacity(file, IO_BUFFER_SIZE).map(
        move |item| {
            item.inspect(|bytes| {
                transferred.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            })
        },
    );

    let client = reqwest::Client::new();
    let mut builder = client.put(&request.url);
    for (key, value) in &request.headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    let response = builder
        .body(reqwest::Body::wrap_stream(stream))
        .send()
        .await
        .map_err(|e| format!("PUT {} failed: {}", request.url, e))?;

    if !response.status().is_success() {
        return Err(format!(
            "PUT {} returned HTTP {}",
            request.url,
            response.status().as_u16()
        ));
    }
    Ok(())
}

/// 双通道推送进度：Tauri 事件 + 消息总线
///
/// 无头上下文（无 AppHandle）时仅走总线，事件发送失败不影响传输本身
fn emit_progress(
    app_handle: &Option<Arc<tauri::AppHandle>>,
    bus: &MessageBus,
    task_id: &str,
    transferred: u64,
    total: u64,
    bytes_per_sec: u64,
    state: TransferState,
) {
    use tauri::Emitter;

    let progress = TransferProgress {
        task_id: task_id.to_string(),
        transferred,
        total,
        bytes_per_sec,
        state,
    };

    if let Some(handle) = app_handle {
        if let Err(e) = handle.emit("plugin:transfer:progress", &progress) {
            tracing::warn!(task_id = %task_id, "transfer progress emit failed: {}", e);
        }
    }

    let payload = serde_json::to_value(&progress).unwrap_or_default();
    bus.publish(&format!("transfer:{}", task_id), "host", payload);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bedcode_plugin_api::BusMessage;
    use crate::plugin::message_bus::{BusMessageHandler, MessageDispatcher};
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// 禁用系统代理对 loopback 的干扰（Windows 全局代理可能拦截测试请求）
    ///
    /// reqwest Client::new() 走系统代理；测试全连 127.0.0.1，
    /// 需在每次 Client 构建前设置 NO_PROXY。并行测试均设置同一值，幂等无竞态。
    fn disable_proxy_for_loopback() {
        std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
    }

    // ==================== Mock HTTP Server ====================

    /// 极简请求（mock 服务器解析产物）
    struct MockRequest {
        method: String,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    }

    impl MockRequest {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers.get(name).map(|s| s.as_str())
        }
    }

    /// 极简响应
    struct MockResponse {
        status: u16,
        body: Vec<u8>,
    }

    impl MockResponse {
        fn ok(body: impl Into<Vec<u8>>) -> Self {
            Self { status: 200, body: body.into() }
        }

        fn with_status(status: u16, body: impl Into<Vec<u8>>) -> Self {
            Self { status, body: body.into() }
        }
    }

    /// 启动 mock HTTP 服务器（每连接独立任务，响应后关闭连接）
    async fn spawn_mock_server(
        handler: Arc<dyn Fn(MockRequest) -> MockResponse + Send + Sync>,
    ) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else { break };
                let handler = handler.clone();
                tokio::spawn(async move {
                    let Some(req) = read_request(&mut sock).await else { return };
                    let resp = handler(req);
                    let reason = match resp.status {
                        200 => "OK",
                        206 => "Partial Content",
                        500 => "Internal Server Error",
                        _ => "Unknown",
                    };
                    let head = format!(
                        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        resp.status,
                        reason,
                        resp.body.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(&resp.body).await;
                });
            }
        });
        addr
    }

    /// 读取并解析一个 HTTP 请求（请求行 + 头 + Content-Length body）
    async fn read_request(sock: &mut tokio::net::TcpStream) -> Option<MockRequest> {
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
        let _path = parts.next()?;
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
        // reqwest 对未知长度的大 body 会发 Expect: 100-continue，需先应答再读 body
        if headers
            .get("expect")
            .map(|v| v.to_ascii_lowercase().starts_with("100"))
            .unwrap_or(false)
        {
            let _ = sock.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await;
        }
        let mut body = buf.split_off(header_end + 4);
        // wrap_stream 上传体无固定长度 → reqwest 使用 chunked encoding
        if headers.get("transfer-encoding").map(|v| v.to_ascii_lowercase() == "chunked").unwrap_or(false) {
            body = read_chunked_body(sock, body).await?;
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
        Some(MockRequest { method, headers, body })
    }

    /// 读取 chunked 编码的请求体（`{hex-size}\r\n{data}\r\n` 直到 size=0）
    async fn read_chunked_body(
        sock: &mut tokio::net::TcpStream,
        mut buf: Vec<u8>,
    ) -> Option<Vec<u8>> {
        let mut tmp = [0u8; 4096];
        let mut body = Vec::new();
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
            // 支持 `size` 与 `size;ext=...` 两种 chunk 头
            let size = usize::from_str_radix(line.split(';').next().unwrap_or("").trim(), 16)
                .ok()?;
            if size == 0 {
                return Some(body); // 结束 chunk，忽略 trailer
            }
            while buf.len() < size + 2 {
                let n = sock.read(&mut tmp).await.ok()?;
                if n == 0 {
                    return None;
                }
                buf.extend_from_slice(&tmp[..n]);
            }
            body.extend_from_slice(&buf[..size]);
            buf.drain(..size + 2); // 含 chunk 尾部 CRLF
        }
    }

    /// 构造传输请求
    fn make_request(direction: TransferDirection, url: &str, local_path: &str) -> TransferRequest {
        TransferRequest {
            task_id: format!("test-{}", url),
            direction,
            url: url.to_string(),
            headers: HashMap::new(),
            local_path: local_path.to_string(),
            offset: 0,
            expected_size: 0,
            final_path: None,
        }
    }

    /// 已传字节计数器
    fn counter() -> Arc<AtomicU64> {
        Arc::new(AtomicU64::new(0))
    }

    // ==================== download ====================

    #[tokio::test]
    async fn download_full_writes_file_and_counts_bytes() {
        disable_proxy_for_loopback();
        // 128KB 伪随机体，验证流式分块累计
        let body: Vec<u8> = (0..128 * 1024).map(|i| (i % 251) as u8).collect();
        let server_body = body.clone();
        let addr = spawn_mock_server(Arc::new(move |req: MockRequest| {
            assert_eq!(req.method, "GET");
            assert_eq!(req.header("range"), None, "offset=0 不应带 Range");
            MockResponse::ok(server_body.clone())
        }))
        .await;
        let dir = tempdir().unwrap();
        let local = dir.path().join("out.bin");
        let req = make_request(
            TransferDirection::Download,
            &format!("http://{}/file", addr),
            local.to_str().unwrap(),
        );
        let transferred = counter();

        download(&req, transferred.clone(), CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(std::fs::read(&local).unwrap(), body);
        assert_eq!(transferred.load(Ordering::Relaxed), body.len() as u64);
    }

    #[tokio::test]
    async fn download_resume_sends_range_and_appends_to_existing() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(Arc::new(|req: MockRequest| {
            assert_eq!(req.header("range"), Some("bytes=3-"));
            MockResponse::with_status(206, b"def".to_vec())
        }))
        .await;
        let dir = tempdir().unwrap();
        let local = dir.path().join("resume.bin");
        std::fs::write(&local, "abc").unwrap();
        let mut req = make_request(
            TransferDirection::Download,
            &format!("http://{}/file", addr),
            local.to_str().unwrap(),
        );
        req.offset = 3;

        download(&req, counter(), CancellationToken::new()).await.unwrap();

        // offset>0 不 truncate：前缀保留，seek 后续写
        assert_eq!(std::fs::read(&local).unwrap(), b"abcdef");
    }

    #[tokio::test]
    async fn download_fresh_truncates_stale_part() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(Arc::new(|_| MockResponse::ok("hello"))).await;
        let dir = tempdir().unwrap();
        let local = dir.path().join("fresh.bin");
        std::fs::write(&local, "stale-content").unwrap();

        download(
            &make_request(
                TransferDirection::Download,
                &format!("http://{}/file", addr),
                local.to_str().unwrap(),
            ),
            counter(),
            CancellationToken::new(),
        )
        .await
        .unwrap();

        assert_eq!(std::fs::read(&local).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn download_renames_part_to_final_path() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(Arc::new(|_| MockResponse::ok("final-bytes"))).await;
        let dir = tempdir().unwrap();
        let part = dir.path().join("out.part");
        let final_path = dir.path().join("out.txt");
        let mut req = make_request(
            TransferDirection::Download,
            &format!("http://{}/file", addr),
            part.to_str().unwrap(),
        );
        req.final_path = Some(final_path.to_str().unwrap().to_string());

        download(&req, counter(), CancellationToken::new()).await.unwrap();

        assert!(!part.exists(), ".part 应已 rename，不再存在");
        assert_eq!(std::fs::read(&final_path).unwrap(), b"final-bytes");
    }

    #[tokio::test]
    async fn download_duplicate_name_keeps_part() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(Arc::new(|_| MockResponse::ok("bytes"))).await;
        let dir = tempdir().unwrap();
        let part = dir.path().join("dup.part");
        let final_path = dir.path().join("dup.txt");
        std::fs::write(&final_path, "occupied").unwrap();
        let mut req = make_request(
            TransferDirection::Download,
            &format!("http://{}/file", addr),
            part.to_str().unwrap(),
        );
        req.final_path = Some(final_path.to_str().unwrap().to_string());

        let err = download(&req, counter(), CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(err, "duplicate-name");
        assert!(part.exists(), "duplicate-name 时 .part 应保留供用户决定");
    }

    #[tokio::test]
    async fn download_http_error_returns_failure() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(Arc::new(|_| MockResponse::with_status(500, "boom"))).await;
        let dir = tempdir().unwrap();
        let local = dir.path().join("err.bin");

        let err = download(
            &make_request(
                TransferDirection::Download,
                &format!("http://{}/file", addr),
                local.to_str().unwrap(),
            ),
            counter(),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();

        assert!(err.contains("HTTP 500"), "got: {}", err);
    }

    #[tokio::test]
    async fn download_cancelled_token_aborts() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(Arc::new(|_| MockResponse::ok(vec![0u8; 64 * 1024]))).await;
        let dir = tempdir().unwrap();
        let local = dir.path().join("cancel.bin");
        let token = CancellationToken::new();
        token.cancel();

        let err = download(
            &make_request(
                TransferDirection::Download,
                &format!("http://{}/file", addr),
                local.to_str().unwrap(),
            ),
            counter(),
            token,
        )
        .await
        .unwrap_err();

        assert_eq!(err, "cancelled");
    }

    #[tokio::test]
    async fn download_carries_extra_headers() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(Arc::new(|req: MockRequest| {
            assert_eq!(req.header("authorization"), Some("Bearer tok-1"));
            MockResponse::ok("x")
        }))
        .await;
        let dir = tempdir().unwrap();
        let local = dir.path().join("hdr.bin");
        let mut req = make_request(
            TransferDirection::Download,
            &format!("http://{}/file", addr),
            local.to_str().unwrap(),
        );
        req.headers
            .insert("Authorization".to_string(), "Bearer tok-1".to_string());

        download(&req, counter(), CancellationToken::new()).await.unwrap();
    }

    // ==================== upload ====================

    #[tokio::test]
    async fn upload_streams_local_file_body() {
        disable_proxy_for_loopback();
        // 256KB 伪随机体，覆盖多块流式读取
        let data: Vec<u8> = (0..256 * 1024).map(|i| (i % 253) as u8).collect();
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let addr = spawn_mock_server({
            let received = received.clone();
            Arc::new(move |req: MockRequest| {
                assert_eq!(req.method, "PUT");
                received.lock().unwrap().extend_from_slice(&req.body);
                MockResponse::ok("")
            })
        })
        .await;
        let dir = tempdir().unwrap();
        let local = dir.path().join("src.bin");
        std::fs::write(&local, &data).unwrap();

        upload(
            &make_request(
                TransferDirection::Upload,
                &format!("http://{}/upload/sid", addr),
                local.to_str().unwrap(),
            ),
            counter(),
        )
        .await
        .unwrap();

        assert_eq!(*received.lock().unwrap(), data);
    }

    #[tokio::test]
    async fn upload_resume_seeks_to_offset() {
        disable_proxy_for_loopback();
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let addr = spawn_mock_server({
            let received = received.clone();
            Arc::new(move |req: MockRequest| {
                received.lock().unwrap().extend_from_slice(&req.body);
                MockResponse::ok("")
            })
        })
        .await;
        let dir = tempdir().unwrap();
        let local = dir.path().join("src.bin");
        std::fs::write(&local, "abcdef").unwrap();
        let mut req = make_request(
            TransferDirection::Upload,
            &format!("http://{}/upload/sid", addr),
            local.to_str().unwrap(),
        );
        req.offset = 2;

        upload(&req, counter()).await.unwrap();

        // seek 到 offset 后只发送尾部（续传语义）
        assert_eq!(*received.lock().unwrap(), b"cdef");
    }

    #[tokio::test]
    async fn upload_http_error_returns_failure() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(Arc::new(|_| MockResponse::with_status(500, "boom"))).await;
        let dir = tempdir().unwrap();
        let local = dir.path().join("src.bin");
        std::fs::write(&local, "data").unwrap();

        let err = upload(
            &make_request(
                TransferDirection::Upload,
                &format!("http://{}/upload/sid", addr),
                local.to_str().unwrap(),
            ),
            counter(),
        )
        .await
        .unwrap_err();

        assert!(err.contains("HTTP 500"), "got: {}", err);
    }

    // ==================== dispatch ====================

    #[tokio::test]
    async fn execute_transfer_dispatches_by_direction() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(Arc::new(|req: MockRequest| {
            if req.method == "GET" {
                MockResponse::ok("downloaded")
            } else {
                MockResponse::ok("")
            }
        }))
        .await;
        let dir = tempdir().unwrap();
        let dl = dir.path().join("dl.bin");
        let up = dir.path().join("up.bin");
        std::fs::write(&up, "uploaded").unwrap();
        let token = CancellationToken::new();

        execute_transfer(
            &make_request(
                TransferDirection::Download,
                &format!("http://{}/file", addr),
                dl.to_str().unwrap(),
            ),
            counter(),
            token.clone(),
        )
        .await
        .unwrap();
        execute_transfer(
            &make_request(
                TransferDirection::Upload,
                &format!("http://{}/upload/sid", addr),
                up.to_str().unwrap(),
            ),
            counter(),
            token.clone(),
        )
        .await
        .unwrap();

        assert_eq!(std::fs::read(&dl).unwrap(), b"downloaded");
    }

    // ==================== run_transfer（无头上下文 AppHandle=None） ====================

    /// 测试投递器：publish 要求 dispatcher 已注入，静态订阅者实际不经 dispatcher
    struct NoopDispatcher;

    impl MessageDispatcher for NoopDispatcher {
        fn dispatch_to_wasm(&self, _plugin_id: &str, _msg: &BusMessage) -> anyhow::Result<()> {
            Ok(())
        }

        fn is_activated(&self, _plugin_id: &str) -> bool {
            true
        }
    }

    /// 静态订阅者：把收到的消息转发到 mpsc 通道
    struct ChannelHandler(tokio::sync::mpsc::UnboundedSender<BusMessage>);

    impl BusMessageHandler for ChannelHandler {
        fn on_message(&self, msg: &BusMessage) -> anyhow::Result<()> {
            let _ = self.0.send(msg.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn run_transfer_completed_emits_terminal_progress() {
        disable_proxy_for_loopback();
        let body = vec![7u8; 32 * 1024];
        let server_body = body.clone();
        let addr = spawn_mock_server(Arc::new(move |_| MockResponse::ok(server_body.clone()))).await;
        let dir = tempdir().unwrap();
        let local = dir.path().join("run.bin");
        let mut req = make_request(
            TransferDirection::Download,
            &format!("http://{}/file", addr),
            local.to_str().unwrap(),
        );
        req.expected_size = body.len() as u64;

        let bus = Arc::new(MessageBus::new());
        bus.set_dispatcher(Arc::new(NoopDispatcher)).await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        bus.subscribe_static("test-plugin", "transfer:task-run-1", Box::new(ChannelHandler(tx)))
            .await;

        let task_id = "task-run-1".to_string();
        run_transfer(task_id.clone(), req, None, bus.clone(), CancellationToken::new()).await;

        // 终态消息：Completed + 最终偏移（插件据此持久化续传点）
        let msg = rx.recv().await.expect("应收到终态进度消息");
        assert_eq!(msg.topic, format!("transfer:{}", task_id));
        let progress: TransferProgress = serde_json::from_value(msg.payload).unwrap();
        assert_eq!(progress.state, TransferState::Completed);
        assert_eq!(progress.transferred, body.len() as u64);
        assert_eq!(progress.total, body.len() as u64);

        // 任务完成/失败后应自行注销（cancel 查不到 = 已完成）
        assert!(!tasks().lock().await.contains_key(&task_id));
    }

    #[tokio::test]
    async fn run_transfer_failed_emits_failure_terminal() {
        disable_proxy_for_loopback();
        let addr = spawn_mock_server(Arc::new(|_| MockResponse::with_status(500, "boom"))).await;
        let dir = tempdir().unwrap();
        let local = dir.path().join("run-err.bin");

        let bus = Arc::new(MessageBus::new());
        bus.set_dispatcher(Arc::new(NoopDispatcher)).await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        bus.subscribe_static("test-plugin", "transfer:task-run-2", Box::new(ChannelHandler(tx)))
            .await;

        let task_id = "task-run-2".to_string();
        run_transfer(
            task_id.clone(),
            make_request(
                TransferDirection::Download,
                &format!("http://{}/file", addr),
                local.to_str().unwrap(),
            ),
            None,
            bus.clone(),
            CancellationToken::new(),
        )
        .await;

        let msg = rx.recv().await.expect("应收到失败终态消息");
        let progress: TransferProgress = serde_json::from_value(msg.payload).unwrap();
        assert!(
            matches!(progress.state, TransferState::Failed(_)),
            "应为 Failed 终态，got: {:?}",
            progress.state
        );
        assert!(!tasks().lock().await.contains_key(&task_id));
    }
}
