//! 传输引擎（断点续传的文件上传/下载，移动端）
//!
//! 与桌面端 `plugin/wasm_runtime/host_functions/transfer.rs` 同语义
//! （两端各自实现、不建共享 crate）：
//! - 下载：reqwest GET + `Range: bytes={offset}-` → tokio 流式写文件
//! - 上传：本地文件从 offset seek → reqwest PUT 流式 body（对端 upload session append）
//! - 进度每 500ms 双通道推送：Tauri 事件 `plugin:transfer:progress`
//!   + 消息总线 `transfer:{task_id}`（载荷均为 TransferProgress）
//! - 取消：tokio_util CancellationToken，终态进度回报最终偏移供续传持久化
//!
//! 所有错误结构化回报（Failed(reason) 终态事件），禁止静默失败

use crate::plugin::fs_auth::FsOp;
use crate::plugin::message_bus::MessageBus;
use crate::system::error_boundary::spawn_with_error_boundary;
use bedcode_plugin_api_mobile::{
    TransferDirection, TransferProgress, TransferRequest, TransferState,
};
use futures_util::{FutureExt as _, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;
use tokio_util::sync::CancellationToken;

/// 进度推送间隔（规格：每 500ms）
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
/// 流式 IO 缓冲（规格：256KB–1MB 取中值）
const IO_BUFFER_SIZE: usize = 512 * 1024;

/// 活跃传输任务表（task_id → 取消令牌）
///
/// 任务完成/失败/取消后自行移除条目；cancel 查不到条目视为已完成。
/// 临界区均为同步短操作（insert/get/remove），用 std Mutex：
/// 同步 host fn（start_transfer）可直接取锁，无需 block_in_place + block_on。
/// HashMap::new 非 const fn，经 OnceLock 惰性初始化
static TASKS: std::sync::OnceLock<Mutex<HashMap<String, CancellationToken>>> =
    std::sync::OnceLock::new();

fn tasks() -> &'static Mutex<HashMap<String, CancellationToken>> {
    TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 校验本地路径 fs 授权（下载 = 写授权，上传 = 读授权）
///
/// host_transfer_start 在启动任务前调用；未授权拒绝启动（规格安全模型）
pub async fn check_local_path_authorized(
    plugin_id: &str,
    request: &TransferRequest,
) -> bool {
    let fs_op = match request.direction {
        TransferDirection::Download => FsOp::Write,
        TransferDirection::Upload => FsOp::Read,
    };
    let fs_auth = crate::state::get_plugin_manager().fs_auth().clone();
    if !fs_auth.check(plugin_id, &request.local_path, fs_op).await {
        return false;
    }

    // final_path 是下载完成后的 rename 目标，同样需要写授权校验
    if let Some(ref final_path) = request.final_path {
        if !fs_auth.check(plugin_id, final_path, fs_op).await {
            tracing::error!(
                plugin_id = %plugin_id,
                final_path = %final_path,
                "check_local_path_authorized: final_path not authorized by user"
            );
            return false;
        }
    }

    true
}

/// 启动传输任务（调用前必须已通过 [`check_local_path_authorized`]）
///
/// 返回宿主生成的 task_id；任务后台异步执行，进度/终局经双通道推送
pub fn spawn_transfer(
    request: TransferRequest,
    app_handle: Arc<tauri::AppHandle>,
    bus: Arc<MessageBus>,
) -> String {
    // 用插件预生成的 task_id（bus topic `transfer:{task_id}` 与 Tauri 事件
    // taskId 均以它为准），不再自生成 UUID —— 插件先订阅后启动，进度零丢失
    let task_id = request.task_id.clone();
    let token = CancellationToken::new();

    // 先登记再 spawn：避免 cancel 早于任务注册到达而丢失取消语义
    // （poison 容忍：传输任务 panic 被下方 catch_unwind 截获后锁会中毒，不能连锁 panic）
    let task_id_for_map = task_id.clone();
    let token_for_map = token.clone();
    tasks().lock().unwrap_or_else(|e| e.into_inner()).insert(task_id_for_map, token_for_map);

    let task_id_for_spawn = task_id.clone();
    let panic_task_id = task_id.clone();
    let panic_app = app_handle.clone();
    let panic_bus = bus.clone();
    let panic_token = token.clone();
    tokio::spawn(async move {
        // 截获 run_transfer 内 panic：若仅用 spawn_with_error_boundary，panic 后
        // 任务会永久停在 transferring（宿主无终态回报、插件收不到失败）。
        // 此处额外清理任务表/停 reporter，并推送 Failed 终态——插件任务转失败，
        // 前端任务列表显示失败原因，而不是整个应用崩溃。
        let result = std::panic::AssertUnwindSafe(run_transfer(
            task_id_for_spawn,
            request,
            app_handle,
            bus,
            token,
        ))
        .catch_unwind()
        .await;

        if let Err(panic_err) = result {
            panic_token.cancel();
            tasks().lock().unwrap_or_else(|e| e.into_inner()).remove(&panic_task_id);
            let msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = panic_err.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic payload".to_string()
            };
            tracing::error!(
                task_id = %panic_task_id,
                error = %msg,
                "transfer task panicked; reporting Failed to plugin instead of crashing"
            );
            emit_progress(
                &panic_app,
                &panic_bus,
                &panic_task_id,
                0,
                0,
                0,
                TransferState::Failed(format!("host transfer internal error: {}", msg)),
            );
        }
    });

    task_id
}

/// 取消传输任务（任务不存在视为已完成，幂等返回 false）
pub async fn cancel_transfer(task_id: &str) -> bool {
    match tasks().lock().unwrap_or_else(|e| e.into_inner()).get(task_id).cloned() {
        Some(token) => {
            token.cancel();
            true
        }
        None => false,
    }
}

// ==================== Transfer Task ====================

/// 传输任务终局
enum Outcome {
    Completed,
    Cancelled,
    /// 失败原因（结构化回报给插件）
    Failed(String),
}

/// 运行传输任务：进度 reporter + 传输本体，终局推送最终进度后注销任务
async fn run_transfer(
    task_id: String,
    request: TransferRequest,
    app_handle: Arc<tauri::AppHandle>,
    bus: Arc<MessageBus>,
    token: CancellationToken,
) {
    // M3 SAF 流直传：content:// 源经 SafIo 桥读（Android 才有）；state 未
    // manage（测试/非 Android）时为 None，upload 按普通路径处理或明确报错
    let saf = {
        use tauri::Manager;
        app_handle
            .try_state::<crate::plugin::saf_io::SafIoState>()
            .map(|s| s.inner().0.clone())
    };
    let transferred = Arc::new(AtomicU64::new(request.offset));
    // 上传总大小由 upload() 打开源后填充（File metadata / SAF statSize），
    // 插件侧 expected_size 恒为 0（见 start_single_task），此处用 Arc 让
    // reporter 与传输本体并发安全地读取真实总量；下载沿用插件上报值
    let total = Arc::new(AtomicU64::new(request.expected_size));

    // 进度 reporter：每 500ms 推送 Running 进度（含瞬时速率）
    let reporter_token = token.child_token();
    {
        let transferred = transferred.clone();
        let total = total.clone();
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
                    total.load(Ordering::Relaxed),
                    bytes_per_sec,
                    TransferState::Running,
                );
            }
        });
    }

    // 取消立即中断传输 future（下载中断流读取 / 上传丢弃请求体）
    let outcome = tokio::select! {
        _ = token.cancelled() => Outcome::Cancelled,
        result = execute_transfer(&request, transferred.clone(), total.clone(), token.clone(), saf) => {
            match result {
                Ok(()) => Outcome::Completed,
                // 取消竞态兜底：token 已取消（含流内检查回报 "cancelled" 被
                // reqwest 包装为通用发送错误的场景）→ 一律落 Cancelled，
                // 避免取消被误报为 Failed
                Err(reason) if reason == "cancelled" || token.is_cancelled() => {
                    Outcome::Cancelled
                }
                Err(reason) => Outcome::Failed(reason),
            }
        }
    };

    reporter_token.cancel();
    tasks().lock().unwrap_or_else(|e| e.into_inner()).remove(&task_id);

    // 终局事件（携带最终偏移，插件据此持久化续传点）
    let final_bytes = transferred.load(Ordering::Relaxed);
    let state = match &outcome {
        Outcome::Completed => TransferState::Completed,
        Outcome::Cancelled => TransferState::Cancelled,
        Outcome::Failed(reason) => TransferState::Failed(reason.clone()),
    };
    emit_progress(
        &app_handle,
        &bus,
        &task_id,
        final_bytes,
        total.load(Ordering::Relaxed),
        0,
        state,
    );

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
    total: Arc<AtomicU64>,
    token: CancellationToken,
    saf: Option<Arc<dyn crate::plugin::saf_io::SafIo>>,
) -> Result<(), String> {
    match request.direction {
        TransferDirection::Download => download(request, transferred, token).await,
        TransferDirection::Upload => upload(request, transferred, total, token, saf).await,
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
    drop(file); // 释放文件句柄，避免 rename 时 Android/Linux 文件锁冲突

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

/// 上传本地读取源（M3：上传 SAF 流直传的 IO 半边抽象）
///
/// - 普通路径：tokio::fs（原有语义，可 seek 真续传）
/// - content:// URI：SAF 流句柄（Kotlin 桥 base64 跨桥读），seek 语义由
///   SafIo::open_stream 的 offset 参数承载（可 seek 真续传 / pipe 流保 fd
///   顺序续读 / 跨任务全量重传，见 spec M3 续传策略）
enum UploadSource {
    /// 本地文件（tokio 异步 IO）
    File(tokio::fs::File),
    /// SAF 流直传句柄（同步桥，poll_read 内阻塞跨桥）
    Saf(Box<SafStreamReader>),
}

/// SAF 流直传的 AsyncRead 适配：每次 poll_read 同步经 SafIo::read_stream 读
///
/// poll 上下文阻塞与 Kotlin 桥一致（block_in_place + block_on 已在
/// KotlinSafIo::read_stream 内部完成）；EOF 后返回 0 不再跨桥。
/// drop 不关闭句柄——任务内断线重连依赖 fd 保留（spec M3），关闭由
/// upload 成功路径显式 close_stream 或 Kotlin 超时清扫兜底。
struct SafStreamReader {
    handle_id: String,
    saf: Arc<dyn crate::plugin::saf_io::SafIo>,
    eof: bool,
    /// 取消令牌：桥读为同步阻塞（block_in_place + block_on），先查令牌再
    /// 跨桥，取消无需等下一次读返回（select 无法在被阻塞的 poll 内运行）
    token: CancellationToken,
}

impl tokio::io::AsyncRead for SafStreamReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = &mut *self;
        // 取消优先：被取消后立即中断流（reqwest 据此中止 PUT），不等桥读返回
        if this.token.is_cancelled() {
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "cancelled",
            )));
        }
        if this.eof {
            return std::task::Poll::Ready(Ok(()));
        }
        // ReadBuf::initialize_unfilled 需要 &mut [u8]；先取容量再读入
        let capacity = buf.remaining();
        if capacity == 0 {
            return std::task::Poll::Ready(Ok(()));
        }
        match this.saf.read_stream(&this.handle_id, capacity) {
            Ok(data) => {
                if data.is_empty() {
                    // EOF（Kotlin 侧 read 返回 -1 → 空 base64）
                    this.eof = true;
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

/// 上传：本地文件从 offset seek → PUT 流式 body 到对端 upload session
///
/// Upload 方向忽略 final_path（仅 Download 方向用于 .part → 最终名原子落位）
///
/// M3：local_path 为 content:// URI 时走 SAF 流直传（saf 参数为桥实现，
/// Android 注入；非 Android 无 SAF 概念）。续传策略：
/// - open_stream 返回 effective_offset == request.offset → 正常上传
/// - 不等（pipe 流不可 seek 且无活跃句柄）→ 回报 not-seekable-resume，
///   插件重建 session 全量重传（fd 保留，重传时 offset=0 强制重开从头）
/// - 上传成功显式 close_stream 释放 fd；失败/取消不 close（任务内保 fd
///   顺序续读，spec M3）
///
/// 取消语义：token 传入并下沉到读侧（File 的 map / SafStreamReader poll
/// 前置检查），被取消时流回报 "cancelled" 错误，reqwest 立即中止 PUT ——
/// 不依赖外层 select 恰好能 poll 到 token（同步桥读不会让 send future
/// 让出 select，快局域网上可能整个上传期间都无法观察取消）。
async fn upload(
    request: &TransferRequest,
    transferred: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    token: CancellationToken,
    saf: Option<Arc<dyn crate::plugin::saf_io::SafIo>>,
) -> Result<(), String> {
    use tokio::io::AsyncSeekExt;

    let local_path = &request.local_path;
    let saf_handle_id: Option<String>;
    let source: UploadSource = if local_path.starts_with("content://") {
        // SAF 流直传（M3）：open 即带 offset（可 seek 真续传 / pipe 流从头）
        let saf = saf.clone().ok_or_else(|| {
            format!(
                "open SAF stream '{}' failed: SafIo unavailable (SAF is Android-only)",
                local_path
            )
        })?;
        let handle = saf
            .open_stream(local_path, request.offset)
            .map_err(|e| format!("saf_open '{}' failed: {}", local_path, e))?;
        if handle.effective_offset != request.offset {
            // pipe 流（不可 seek）且无活跃句柄：无法从断点续读，全量重传
            // 由插件重建 session 触发（Kotlin 侧 offset=0 重开会强制重开 fd）
            return Err("not-seekable-resume".to_string());
        }
        saf_handle_id = Some(handle.handle_id.clone());
        // 总大小：safOpen 即得 statSize（pipe 流为 0 = 未知，进度条退化为偏移量）
        total.store(handle.size, Ordering::Relaxed);
        UploadSource::Saf(Box::new(SafStreamReader {
            handle_id: handle.handle_id,
            saf,
            eof: false,
            token,
        }))
    } else {
        saf_handle_id = None;
        let mut file = tokio::fs::File::open(local_path)
            .await
            .map_err(|e| format!("open local file '{}' failed: {}", local_path, e))?;
        // 总大小：本地文件 metadata（进度条需要真实总量，插件侧恒传 0）
        if let Ok(meta) = file.metadata().await {
            total.store(meta.len(), Ordering::Relaxed);
        }
        if request.offset > 0 {
            file.seek(std::io::SeekFrom::Start(request.offset))
                .await
                .map_err(|e| format!("seek local file to offset {} failed: {}", request.offset, e))?;
        }
        UploadSource::File(file)
    };

    let client = reqwest::Client::new();
    let mut builder = client.put(&request.url);
    for (key, value) in &request.headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    // 统一为 Box<dyn Stream>：File（tokio 异步）与 SafStreamReader（同步桥）
    // 均为 AsyncRead，ReaderStream 包装后 Item 类型一致
    let body_stream: Box<
        dyn futures_util::Stream<Item = std::result::Result<bytes::Bytes, std::io::Error>>
            + Send
            + Unpin,
    > = match source {
        UploadSource::File(file) => Box::new(
            tokio_util::io::ReaderStream::with_capacity(file, IO_BUFFER_SIZE).map(move |item| {
                item.inspect(|bytes| {
                    transferred.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                })
            }),
        ),
        UploadSource::Saf(reader) => Box::new(
            tokio_util::io::ReaderStream::with_capacity(reader, IO_BUFFER_SIZE).map(move |item| {
                item.inspect(|bytes| {
                    transferred.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                })
            }),
        ),
    };

    let response = builder
        .body(reqwest::Body::wrap_stream(body_stream))
        .send()
        .await
        .map_err(|e| format!("PUT {} failed: {}", request.url, e))?;

    if !response.status().is_success() {
        // 失败不 close：fd 保留供任务内续读（spec M3）
        return Err(format!(
            "PUT {} returned HTTP {}",
            request.url,
            response.status().as_u16()
        ));
    }

    // 成功：显式释放 SAF 句柄（后续重传会是全新会话，从头或 seek）
    if let (Some(handle_id), Some(saf)) = (saf_handle_id, saf) {
        if let Err(e) = saf.close_stream(&handle_id) {
            tracing::warn!(
                handle_id = %handle_id,
                "upload: close_stream failed (leak bounded by Kotlin sweep): {}",
                e
            );
        }
    }
    Ok(())
}

/// 双通道推送进度：Tauri 事件 + 消息总线
///
/// 事件发送失败不影响传输本身（前端 UI 丢进度由总线兜底）
fn emit_progress(
    app_handle: &tauri::AppHandle,
    bus: &MessageBus,
    task_id: &str,
    transferred: u64,
    total: u64,
    bytes_per_sec: u64,
    state: TransferState,
) {
    let progress = TransferProgress {
        task_id: task_id.to_string(),
        transferred,
        total,
        bytes_per_sec,
        state,
    };

    if let Err(e) = app_handle.emit("plugin:transfer:progress", &progress) {
        tracing::warn!(task_id = %task_id, "transfer progress emit failed: {}", e);
    }

    let payload = serde_json::to_value(&progress).unwrap_or_default();
    bus.publish(&format!("transfer:{}", task_id), "host", payload);
}

#[cfg(test)]
mod tests {
    use super::*;
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

    /// 总大小计数器（upload 填充真实总量用）
    fn total_counter() -> Arc<AtomicU64> {
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
            total_counter(),
            CancellationToken::new(),
            None,
        )
        .await
        .unwrap();

        assert_eq!(*received.lock().unwrap(), data);
    }

    #[tokio::test]
    async fn upload_fills_total_from_file_metadata() {
        disable_proxy_for_loopback();
        // 进度条修复：真实路径上传须把文件大小写入 total（插件 expected_size 恒 0）
        let data: Vec<u8> = vec![7u8; 123_456];
        let addr = spawn_mock_server(Arc::new(|_| MockResponse::ok(""))).await;
        let dir = tempdir().unwrap();
        let local = dir.path().join("src.bin");
        std::fs::write(&local, &data).unwrap();
        let total = total_counter();

        upload(
            &make_request(
                TransferDirection::Upload,
                &format!("http://{}/upload/sid", addr),
                local.to_str().unwrap(),
            ),
            counter(),
            total.clone(),
            CancellationToken::new(),
            None,
        )
        .await
        .unwrap();

        assert_eq!(total.load(Ordering::Relaxed), data.len() as u64);
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

        upload(&req, counter(), total_counter(), CancellationToken::new(), None)
            .await
            .unwrap();

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
            total_counter(),
            CancellationToken::new(),
            None,
        )
        .await
        .unwrap_err();

        assert!(err.contains("HTTP 500"), "got: {}", err);
    }

    // ==================== M3：SAF 流直传（fake 注入） ====================

    /// SAF 流直传测试 fake：按 uri/offset 语义模拟 Kotlin safOpen/safRead
    ///
    /// - open_stream 记录请求 offset，返回 effective_offset（可按配置偏离，
    ///   模拟 pipe 流不可 seek）
    /// - read_stream 按顺序吐出 DATA 的字节切片（512B/次），EOF 后空
    /// - close_stream 记录调用（成功路径必须关闭句柄）
    struct FakeSafIo {
        /// 全量源数据（read_stream 的取数源）
        data: Vec<u8>,
        /// 实际生效偏移（默认与请求一致；偏离模拟 pipe 流跨任务重传场景）
        effective_offset: std::sync::Mutex<Option<u64>>,
        /// 请求过 open_stream 的 (uri, offset)
        opened: std::sync::Mutex<Vec<(String, u64)>>,
        /// 被 close 的句柄
        closed: std::sync::Mutex<Vec<String>>,
        /// read_stream 已读字节数（fake 内部游标）
        cursor: std::sync::Mutex<u64>,
    }

    impl FakeSafIo {
        fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                effective_offset: std::sync::Mutex::new(None),
                opened: std::sync::Mutex::new(Vec::new()),
                closed: std::sync::Mutex::new(Vec::new()),
                cursor: std::sync::Mutex::new(0),
            }
        }
    }

    impl crate::plugin::saf_io::SafIo for FakeSafIo {
        fn list_tree(&self, _t: &str, _d: &str) -> crate::Result<Vec<crate::plugin::saf_io::SafEntry>> {
            Ok(vec![])
        }
        fn read_to_cache(&self, _u: &str, _d: &str) -> crate::Result<crate::plugin::saf_io::SafCopyHandle> {
            unreachable!()
        }
        fn copy_status(&self, _c: &str) -> crate::Result<crate::plugin::saf_io::SafCopyStatus> {
            unreachable!()
        }
        fn cancel_copy(&self, _c: &str) -> crate::Result<()> {
            unreachable!()
        }
        fn cleanup_stale_copies(&self) -> crate::Result<()> {
            Ok(())
        }
        fn check_authorized(&self, _t: &str) -> crate::Result<bool> {
            Ok(true)
        }
        fn write_media_downloads(&self, _s: &str, _d: &str, _m: &str) -> crate::Result<()> {
            Ok(())
        }
        fn open_stream(
            &self,
            uri: &str,
            offset: u64,
        ) -> crate::Result<crate::plugin::saf_io::SafStreamHandle> {
            self.opened.lock().unwrap().push((uri.to_string(), offset));
            let effective = self.effective_offset.lock().unwrap().unwrap_or(offset);
            // 模拟 Kotlin Os.lseek：可 seek 时句柄游标定位到 effective_offset
            // （pipe 流从头，effective_offset=0）
            *self.cursor.lock().unwrap() = effective;
            Ok(crate::plugin::saf_io::SafStreamHandle {
                handle_id: "stream-1".to_string(),
                effective_offset: effective,
                seekable: self.effective_offset.lock().unwrap().is_none(),
                size: self.data.len() as u64,
            })
        }
        fn read_stream(&self, _h: &str, len: usize) -> crate::Result<Vec<u8>> {
            let mut cursor = self.cursor.lock().unwrap();
            let start = *cursor as usize;
            if start >= self.data.len() {
                return Ok(Vec::new()); // EOF
            }
            let end = (start + len).min(self.data.len());
            *cursor = end as u64;
            Ok(self.data[start..end].to_vec())
        }
        fn seek_stream(&self, _h: &str, _o: u64) -> crate::Result<()> {
            Ok(())
        }
        fn close_stream(&self, h: &str) -> crate::Result<()> {
            self.closed.lock().unwrap().push(h.to_string());
            Ok(())
        }
        fn stream_seekable(&self, _u: &str) -> crate::Result<bool> {
            Ok(true)
        }
        fn save_to_document(&self, _s: &str, _n: &str, _m: &str) -> crate::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn upload_saf_content_uri_streams_via_bridge_and_closes_on_success() {
        disable_proxy_for_loopback();
        // 256KB 伪随机体，覆盖多块跨桥读取（单块 512KB > 数据量，一轮读完）
        let data: Vec<u8> = (0..256 * 1024).map(|i| (i % 251) as u8).collect();
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
        let saf = Arc::new(FakeSafIo::new(data.clone()));
        let uri = "content://tree/root/document/f1";

        upload(
            &make_request(TransferDirection::Upload, &format!("http://{}/upload/sid", addr), uri),
            counter(),
            total_counter(),
            CancellationToken::new(),
            Some(saf.clone()),
        )
        .await
        .unwrap();

        // body 与源数据逐字节一致（无中转复制，流直传）
        assert_eq!(*received.lock().unwrap(), data);
        // 打开时 offset=0
        assert_eq!(saf.opened.lock().unwrap().as_slice(), &[(uri.to_string(), 0)]);
        // 成功后必须显式 close（否则 fd 泄漏）
        assert_eq!(saf.closed.lock().unwrap().as_slice(), &["stream-1".to_string()]);
    }

    #[tokio::test]
    async fn upload_saf_fills_total_from_handle_size() {
        disable_proxy_for_loopback();
        // 进度条修复：SAF 流直传须把 Kotlin statSize 写入 total
        let data: Vec<u8> = (0..256 * 1024).map(|i| (i % 251) as u8).collect();
        let addr = spawn_mock_server(Arc::new(|_| MockResponse::ok(""))).await;
        let saf = Arc::new(FakeSafIo::new(data.clone()));
        let total = total_counter();

        upload(
            &make_request(
                TransferDirection::Upload,
                &format!("http://{}/upload/sid", addr),
                "content://tree/root/document/f1",
            ),
            counter(),
            total.clone(),
            CancellationToken::new(),
            Some(saf.clone()),
        )
        .await
        .unwrap();

        assert_eq!(total.load(Ordering::Relaxed), data.len() as u64);
    }

    #[tokio::test]
    async fn upload_saf_seekable_resume_seeks_at_open() {
        disable_proxy_for_loopback();
        // 续传语义：open_stream 收到请求 offset，fake 的游标起点与之一致
        // （真机由 Kotlin Os.lseek 完成）；只发送断点之后的字节
        let data: Vec<u8> = (0..4096).map(|i| (i % 253) as u8).collect();
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let addr = spawn_mock_server({
            let received = received.clone();
            Arc::new(move |req: MockRequest| {
                received.lock().unwrap().extend_from_slice(&req.body);
                MockResponse::ok("")
            })
        })
        .await;
        let saf = Arc::new(FakeSafIo::new(data.clone()));
        let uri = "content://tree/root/document/f1";
        let mut req = make_request(TransferDirection::Upload, &format!("http://{}/upload/sid", addr), uri);
        req.offset = 1024;

        upload(
            &req,
            counter(),
            total_counter(),
            CancellationToken::new(),
            Some(saf.clone()),
        )
        .await
        .unwrap();

        assert_eq!(*received.lock().unwrap(), data[1024..]);
        assert_eq!(saf.opened.lock().unwrap().as_slice(), &[(uri.to_string(), 1024)]);
        assert_eq!(saf.closed.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn upload_saf_pipe_stream_not_seekable_reports_resume_error() {
        disable_proxy_for_loopback();
        // pipe 流（不可 seek）跨任务续传：Kotlin 只能从头打开，
        // effective_offset=0 ≠ 请求 offset=10 → 宿主回报 not-seekable-resume，
        // 插件重建 session 全量重传；句柄不 close（fd 保留供重传）
        let saf = Arc::new(FakeSafIo::new(b"0123456789abcdef".to_vec()));
        *saf.effective_offset.lock().unwrap() = Some(0);
        let uri = "content://pipe/stream/s1";
        let mut req = make_request(TransferDirection::Upload, "http://127.0.0.1:1/upload/sid", uri);
        req.offset = 10;

        let err = upload(
            &req,
            counter(),
            total_counter(),
            CancellationToken::new(),
            Some(saf.clone()),
        )
        .await
        .unwrap_err();

        assert_eq!(err, "not-seekable-resume");
        assert!(saf.closed.lock().unwrap().is_empty(), "失败不得 close（fd 保留续读）");
    }

    #[tokio::test]
    async fn upload_saf_without_io_reports_platform_error() {
        disable_proxy_for_loopback();
        // 非 Android 平台（saf=None）遇到 content:// 源：明确错误而非静默
        // 退化（dev 窗口无 SAF 概念）
        let uri = "content://tree/root/document/f1";
        let err = upload(
            &make_request(TransferDirection::Upload, "http://127.0.0.1:1/upload/sid", uri),
            counter(),
            total_counter(),
            CancellationToken::new(),
            None,
        )
        .await
        .unwrap_err();
        assert!(err.contains("SafIo unavailable"), "got: {}", err);
    }

    #[tokio::test]
    async fn upload_saf_cancelled_token_aborts_before_bridge_read() {
        disable_proxy_for_loopback();
        // 取消修复：SAF 直传的桥读是同步阻塞，poll 前置 token 检查保证
        // 取消即时中断流。reqwest 会把流中断错误包装为通用发送错误，
        // 终态归一（Cancelled）由 run_transfer 的 token.is_cancelled() 兜底
        let data: Vec<u8> = vec![9u8; 2 * 1024 * 1024];
        let addr = spawn_mock_server(Arc::new(|_| MockResponse::ok(""))).await;
        let saf = Arc::new(FakeSafIo::new(data));
        let token = CancellationToken::new();
        token.cancel();

        let err = upload(
            &make_request(
                TransferDirection::Upload,
                &format!("http://{}/upload/sid", addr),
                "content://tree/root/document/f1",
            ),
            counter(),
            total_counter(),
            token,
            Some(saf.clone()),
        )
        .await
        .unwrap_err();

        // 关键断言：取消后立即失败（而非继续跨桥读 / 挂起）；错误被包装
        assert!(err.contains("failed"), "got: {}", err);
        // 取消后不得有任何字节被读出（poll 前置检查，不跨桥）
        assert!(saf.closed.lock().unwrap().is_empty());
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
            total_counter(),
            token.clone(),
            None,
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
            total_counter(),
            token.clone(),
            None,
        )
        .await
        .unwrap();

        assert_eq!(std::fs::read(&dl).unwrap(), b"downloaded");
    }
}

