//! 传输引擎域 Host Functions（断点续传的文件上传/下载）
//!
//! 宿主托管实际字节搬运，插件只负责任务编排（规格第 6、7 节）：
//! - 下载：reqwest GET + `Range: bytes={offset}-` → tokio 流式写文件
//! - 上传：本地文件从 offset seek → reqwest PUT 流式 body（对端 upload session append）
//! - 进度每 500ms 双通道推送：Tauri 事件 `plugin:transfer:progress`
//!   + 消息总线 `transfer:{task_id}`（载荷均为 TransferProgress）
//! - 取消：tokio_util CancellationToken，终态进度回报最终偏移供续传持久化
//!
//! 所有错误结构化回报（Failed(reason) 终态事件），禁止静默失败

use super::memory::{read_wasm_string_consume, write_result_to_out_ptr, write_wasm_string};
use crate::plugin::fs_auth::FsOp;
use crate::plugin::message_bus::MessageBus;
use crate::plugin::wasm_runtime::{block_on_async, WasmPluginState};
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

/// 传输引擎：启动传输任务
///
/// 参数：(req_ptr, req_len, out_ptr) — req 为 TransferRequest JSON
/// 返回：0 成功（task_id 写入 out_ptr），-1 失败（权限/fs 授权/参数错误）
pub(super) fn host_transfer_start(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    req_ptr: u32,
    req_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let req_str = match read_wasm_string_consume(&mut caller, req_ptr, req_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_transfer_start: failed to read request");
            return -1;
        }
    };

    let request: TransferRequest = match serde_json::from_str(&req_str) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_transfer_start: invalid TransferRequest JSON");
            return -1;
        }
    };

    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_TRANSFER, "host_transfer_start")
    {
        return -1;
    }

    // 本地路径 fs 授权：下载 = 写授权，上传 = 读授权
    let fs_op = match request.direction {
        TransferDirection::Download => FsOp::Write,
        TransferDirection::Upload => FsOp::Read,
    };
    let fs_auth = host_ctx.fs_auth.clone();
    let local_path = request.local_path.clone();
    let plugin_id_for_auth = plugin_id.clone();
    if !block_on_async(fs_auth.check(&plugin_id_for_auth, &local_path, fs_op)) {
        tracing::error!(
            plugin_id = %plugin_id,
            local_path = %local_path,
            "host_transfer_start: local path not authorized by user"
        );
        return -1;
    }

    // final_path 是下载完成后的 rename 目标，同样需要写授权校验
    if let Some(ref final_path) = request.final_path {
        if !block_on_async(fs_auth.check(&plugin_id_for_auth, final_path, fs_op)) {
            tracing::error!(
                plugin_id = %plugin_id,
                final_path = %final_path,
                "host_transfer_start: final_path not authorized by user"
            );
            return -1;
        }
    }

    let task_id = uuid::Uuid::new_v4().to_string();
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

    match write_wasm_string(&mut caller, &task_id) {
        Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_transfer_start: failed to write task_id");
            -1
        }
    }
}

/// 传输引擎：取消传输任务
///
/// 参数：(task_ptr, task_len)
/// 返回：0 成功；任务不存在（已完成/未知）也返回 0（幂等），记录 debug 日志
pub(super) fn host_transfer_cancel(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    task_ptr: u32,
    task_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let task_id = match read_wasm_string_consume(&mut caller, task_ptr, task_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_transfer_cancel: failed to read task id");
            return -1;
        }
    };

    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_TRANSFER, "host_transfer_cancel")
    {
        return -1;
    }

    let token = block_on_async(async { tasks().lock().await.get(&task_id).cloned() });
    match token {
        Some(token) => {
            tracing::info!(plugin_id = %plugin_id, task_id = %task_id, "transfer cancel requested");
            token.cancel();
        }
        None => {
            tracing::debug!(
                plugin_id = %plugin_id,
                task_id = %task_id,
                "host_transfer_cancel: task not active (already finished or unknown)"
            );
        }
    }
    0
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
