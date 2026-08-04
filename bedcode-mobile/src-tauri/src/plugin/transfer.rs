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
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Emitter;
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
    let task_id = uuid::Uuid::new_v4().to_string();
    let token = CancellationToken::new();

    // 先登记再 spawn：避免 cancel 早于任务注册到达而丢失取消语义
    // （host fn 调用方在 block_on 中完成登记后才返回 task_id）
    let task_id_for_map = task_id.clone();
    let token_for_map = token.clone();
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(async move { tasks().lock().await.insert(task_id_for_map, token_for_map) });
    });

    let task_id_for_spawn = task_id.clone();
    spawn_with_error_boundary("plugin_transfer_task", async move {
        run_transfer(task_id_for_spawn, request, app_handle, bus, token).await;
    });

    task_id
}

/// 取消传输任务（任务不存在视为已完成，幂等返回 false）
pub async fn cancel_transfer(task_id: &str) -> bool {
    match tasks().lock().await.get(task_id).cloned() {
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
    let transferred = Arc::new(AtomicU64::new(request.offset));
    let total = request.expected_size;

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

    // ReaderStream 按 IO_BUFFER_SIZE 读块；inspect 中累已传字节（进度 reporter 读取）
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
