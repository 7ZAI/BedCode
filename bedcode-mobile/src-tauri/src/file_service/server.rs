//! 文件服务 HTTP server（移动端独立服务，规格 4.4 / 4.5 节）
//!
//! 移动端文件服务是独立端口的 actix-web 服务（桌面端子路由挂在现有 actix
//! server + JWT 中间件，移动端无此条件）：
//! - 绑定 `0.0.0.0:0`（随机端口，启动后取实际端口经 WS 公告）
//! - 只认 Bearer Token（[`BearerTokenGuard`]），未通过校验一律 401 JSON
//! - 生命周期：首个 mount 时 [`ensure_started`](FileServiceServer::ensure_started)，
//!   末个 unmount 时 [`stop`](FileServiceServer::stop)（ensure_started 幂等）
//!
//! 端点形状与桌面端 `file_service_controller.rs` 一致，但**无 /api 前缀**：
//! - GET    /{plugin_id}/{mount}/list?path=
//! - GET    /{plugin_id}/{mount}/file?path=      （Range 续传 206）
//! - HEAD   /{plugin_id}/{mount}/file?path=      （size+mtime 指纹）
//! - POST   /{plugin_id}/{mount}/upload
//! - PUT    /{plugin_id}/{mount}/upload/{sid}    （web::Payload 流式 append）
//! - GET    /{plugin_id}/{mount}/upload/{sid}    （查询已收字节）
//! - POST   /{plugin_id}/{mount}/upload/{sid}/complete
//! - DELETE /{plugin_id}/{mount}/upload/{sid}
//!
//! 服务面无删除/改名/移动/覆盖端点（规格 8 节）。
//!
//! Android 注意：actix 默认按核数起 worker 线程，服务启动经
//! `tauri::async_runtime::spawn` 交给运行时，不阻塞调用方。

use crate::file_service::auth::BearerTokenGuard;
use crate::file_service::registry::{FileServiceRegistry, MountEntry};
use crate::file_service::upload::UploadSessionError;
use crate::file_service::sandbox;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use actix_web::dev::{ServerHandle, Service as _};
use bedcode_plugin_api_mobile::FileOperation;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// PUT append 累积缓冲下限（规格：256KB–1MB）
const APPEND_FLUSH_THRESHOLD: usize = 512 * 1024;
/// 单个 payload chunk 的大小上限（超过立即 flush，防大 chunk 撑爆缓冲）
const APPEND_MAX_CHUNK: usize = 1024 * 1024;
/// 下载流式读取缓冲（规格：256KB–1MB 取中值）
const DOWNLOAD_CHUNK_SIZE: usize = 512 * 1024;
/// POST /upload JSON body 上限（仅元数据，小值即可）
const CREATE_UPLOAD_BODY_LIMIT: usize = 64 * 1024;

// ==================== DTO ====================

/// 目录条目（浏览列表，过滤 *.part 临时文件）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntryDto {
    /// 文件/目录名
    pub name: String,
    /// 字节数（目录为 0）
    pub size: u64,
    /// 修改时间（Unix 秒；读取失败为 0）
    pub mtime: u64,
    /// 是否目录
    pub is_dir: bool,
}

/// 目录列举响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResponse {
    /// 当前相对路径
    pub path: String,
    /// 条目列表（目录优先，按名称排序）
    pub entries: Vec<FileEntryDto>,
    /// 非空时：列表结果可能被 Android 存储权限过滤（对端应提示用户授权）。
    /// 对端 serde 解析默认忽略未知字段，无此字段的旧对端不受影响
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

/// 创建上传会话请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateUploadRequest {
    /// 目标相对路径（相对挂载根）
    pub relative_path: String,
    /// 声明的文件总大小（字节）
    pub size: u64,
}

/// 上传会话响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadSessionResponse {
    /// 会话 ID
    pub session_id: String,
    /// 服务端已收字节数
    pub received: u64,
}

/// 路径查询参数
#[derive(Debug, Deserialize)]
pub struct PathOnlyQuery {
    /// 相对挂载点的路径（空 = 挂载根）
    #[serde(default)]
    pub path: String,
}

// ==================== Server ====================

/// 运行中的服务句柄（ServerHandle + 实际端口）
struct RunningServer {
    /// actix Server 控制句柄（stop 用；Server 本身不可 Clone，spawn 前取 handle）
    handle: ServerHandle,
    /// 实际监听端口（bind :0 后取得）
    port: u16,
}

/// 文件服务 HTTP server（随挂载启停，幂等）
pub struct FileServiceServer {
    /// 挂载注册表（handler 共享）
    registry: Arc<FileServiceRegistry>,
    /// Bearer Token 守卫（wrap_fn 校验 + 公告取用）
    token: Arc<BearerTokenGuard>,
    /// 运行状态（None = 未启动）
    running: Mutex<Option<RunningServer>>,
}

impl FileServiceServer {
    /// 创建 server（未启动状态）
    pub fn new(registry: Arc<FileServiceRegistry>) -> Self {
        Self {
            registry,
            token: Arc::new(BearerTokenGuard::new()),
            running: Mutex::new(None),
        }
    }

    /// Token 守卫引用（公告/吊销用）
    pub fn token_guard(&self) -> &Arc<BearerTokenGuard> {
        &self.token
    }

    /// 是否运行中
    pub async fn is_running(&self) -> bool {
        self.running.lock().await.is_some()
    }

    /// 当前监听端口（未运行返回 None）
    pub async fn port(&self) -> Option<u16> {
        self.running.lock().await.as_ref().map(|r| r.port)
    }

    /// 确保服务已启动（幂等）；返回监听端口
    ///
    /// 首次调用：生成 Bearer Token → bind 0.0.0.0:0 取实际端口 →
    /// `tauri::async_runtime::spawn` 运行服务（actix worker 线程模型，
    /// 不占用调用方任务）
    pub async fn ensure_started(self: &Arc<Self>) -> crate::Result<u16> {
        let mut running = self.running.lock().await;
        if let Some(r) = running.as_ref() {
            return Ok(r.port);
        }

        // token 与服务生命周期绑定：启动即生成（内存态，不落盘）
        self.token.generate();

        let registry = self.registry.clone();
        let token = self.token.clone();
        let http_server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(registry.clone()))
                // POST /upload 仅承载元数据 JSON，显式限制 body 上限
                .app_data(
                    web::JsonConfig::default().limit(CREATE_UPLOAD_BODY_LIMIT),
                )
                // Bearer Token 校验：未通过一律 401 JSON（无例外路由）。
                // 注意：srv 的借用不能带进 async 块（生命周期约束），
                // 先在同步段构造调用 future 再 move 进 async 块；
                // 两个分支用 Either 统一返回类型
                .wrap_fn(|req, srv| {
                    // 请求日志（排查链路用，移动端 file service 无 actix Logger）：
                    // 来源 IP + method + path + 结果（token 本体不落日志）
                    let req_desc = {
                        let ip = req
                            .peer_addr()
                            .map(|a| a.ip().to_string())
                            .unwrap_or_else(|| "?".to_string());
                        format!("{} {} {} {}", ip, req.method(), req.path(), req.query_string())
                    };
                    let req_start = std::time::Instant::now();

                    let authorized = req
                        .headers()
                        .get(actix_web::http::header::AUTHORIZATION)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.strip_prefix("Bearer "))
                        .map(|presented| {
                            let guard = req
                                .app_data::<web::Data<Arc<BearerTokenGuard>>>()
                                .cloned();
                            match guard {
                                Some(g) => g.verify(presented),
                                None => false,
                            }
                        })
                        .unwrap_or(false);

                    if !authorized {
                        tracing::warn!(
                            "file service request REJECTED (missing/invalid bearer token): {}",
                            req_desc
                        );
                        // ServiceRequest → HttpRequest 后才能构造新响应
                        let (req, _payload) = req.into_parts();
                        let resp = HttpResponse::Unauthorized().json(serde_json::json!({
                            "code": 401,
                            "message": "unauthorized: valid Bearer token required",
                        }));
                        return futures_util::future::Either::Left(async move {
                            Ok(actix_web::dev::ServiceResponse::new(req, resp)
                                .map_into_right_body())
                        });
                    }

                    let fut = srv.call(req);
                    futures_util::future::Either::Right(async move {
                        let res = fut.await?;
                        let status = res.response().status().as_u16();
                        tracing::info!(
                            "file service request {} status={} elapsed_ms={}",
                            req_desc,
                            status,
                            req_start.elapsed().as_millis() as u64
                        );
                        Ok(res.map_into_left_body())
                    })
                })
                // token guard 供 wrap_fn 取用（app_data 注册需在 wrap_fn 求值前生效）
                .app_data(web::Data::new(token.clone()))
                .service(
                    web::resource("/{plugin_id}/{mount}/list")
                        .route(web::get().to(list_dir)),
                )
                .service(
                    web::resource("/{plugin_id}/{mount}/file")
                        .route(web::get().to(download_file))
                        .route(web::head().to(head_file)),
                )
                .service(
                    web::resource("/{plugin_id}/{mount}/upload")
                        .route(web::post().to(create_upload)),
                )
                .service(
                    web::resource("/{plugin_id}/{mount}/upload/{sid}")
                        .route(web::put().to(append_upload))
                        .route(web::get().to(query_upload))
                        .route(web::delete().to(cancel_upload)),
                )
                .service(
                    web::resource("/{plugin_id}/{mount}/upload/{sid}/complete")
                        .route(web::post().to(complete_upload)),
                )
        })
        .bind("0.0.0.0:0")
        .map_err(|e| {
            crate::AppError::Internal(format!("file service bind 0.0.0.0:0 failed: {}", e))
        })?;

        // bind :0 → 取内核分配的实际端口（公告给对端）
        let port = http_server
            .addrs()
            .first()
            .map(|a| a.port())
            .ok_or_else(|| {
                crate::AppError::Internal("file service bind returned no address".to_string())
            })?;

        let server = http_server.run();
        // Server 本身不可 Clone：spawn 前取控制句柄（stop 用）
        let handle = server.handle();
        // actix 在 Android 上有独立 worker 线程模型，启动放 tauri async runtime
        tauri::async_runtime::spawn(async move {
            if let Err(e) = server.await {
                tracing::error!("file service server exited with error: {}", e);
            }
        });

        tracing::info!(port = port, "file service server started");
        *running = Some(RunningServer { handle, port });
        Ok(port)
    }

    /// 停止服务并吊销 token（幂等；末个挂载摘除/解配时调用）
    pub async fn stop(&self) {
        let running = self.running.lock().await.take();
        if let Some(running) = running {
            // graceful=false：文件传输任务由对端断点续传兜底，快速释放端口
            running.handle.stop(false).await;
            tracing::info!(port = running.port, "file service server stopped");
        }
        self.token.revoke();
    }
}

// ==================== Helpers ====================

/// 统一错误响应：HTTP 状态码 + JSON body
fn error_response(status: actix_web::http::StatusCode, code: u16, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(serde_json::json!({
        "code": code,
        "message": message,
    }))
}

/// 校验挂载声明了指定操作（未声明 → 403）
fn require_op(entry: &MountEntry, op: FileOperation) -> Result<(), HttpResponse> {
    if entry.operations.contains(&op) {
        Ok(())
    } else {
        Err(error_response(
            actix_web::http::StatusCode::FORBIDDEN,
            403,
            &format!(
                "operation '{:?}' not allowed for mount '{}'",
                op, entry.mount_path
            ),
        ))
    }
}

/// 文件修改时间（Unix 秒，读取失败为 0）
fn mtime_unix_secs(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 同步读取目录条目（spawn_blocking 中执行）
///
/// root 失效（删除/移动/权限回收）时 read_dir 失败 → 明确错误（规格 4.3 第 4 条）
fn read_dir_entries(dir: &Path) -> crate::Result<Vec<FileEntryDto>> {
    if !dir.is_dir() {
        return Err(crate::AppError::NotFound(format!(
            "'{}' is not a directory",
            dir.display()
        )));
    }
    let read_dir = std::fs::read_dir(dir).map_err(|e| {
        crate::AppError::Internal(format!(
            "failed to read directory '{}' (root may have been removed or permission revoked): {}",
            dir.display(),
            e
        ))
    })?;

    let mut entries = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(|e| {
            crate::AppError::Internal(format!(
                "failed to read entry in '{}': {}",
                dir.display(),
                e
            ))
        })?;
        let name = entry.file_name().to_string_lossy().to_string();
        // 过滤上传临时文件（*.part），不向对端暴露
        if crate::file_service::upload::is_filtered_listing_name(&name) {
            continue;
        }
        let meta = entry.metadata().map_err(|e| {
            crate::AppError::Internal(format!(
                "failed to read metadata of '{}': {}",
                entry.path().display(),
                e
            ))
        })?;
        entries.push(FileEntryDto {
            name,
            size: meta.len(),
            mtime: mtime_unix_secs(&meta),
            is_dir: meta.is_dir(),
        });
    }
    // 目录优先，按名称排序，保证两端 UI 展示一致
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

/// 判断路径是否需要「所有文件访问权限」（MANAGE_EXTERNAL_STORAGE）
///
/// Android 11+ 分区存储：仅 App 私有目录（`/storage/emulated/0/Android/data/`）
/// 无需任何授权即可读写；其余主存储路径（含 DCIM/Download 等媒体集合，
/// App 未声明 READ_MEDIA_*）的 read_dir 均受 FUSE 过滤，未授权时静默返回
/// 空列表（不报错）。返回 true 且列表结果为空时，对端几乎可以确定是
/// 权限问题而非真空目录。
fn needs_all_files_access(path: &Path) -> bool {
    let p = path.to_string_lossy().replace('\\', "/");
    let normalized = p.trim_end_matches('/').to_lowercase();
    if !normalized.starts_with("/storage/emulated/0") {
        // 其他存储位置（外部 SD 卡等）也会被过滤，但 App 自身私有
        // 目录（/data/user/0/...）不受影响——只对主存储判定，避免误报
        return false;
    }
    !normalized.starts_with("/storage/emulated/0/android/data")
}

/// 解析 Range 头 `bytes=N-` / `bytes=N-M`（仅支持单段）
///
/// 返回 (start, 可选 end)；非法/不支持的形式返回 None（走 200 全量）
fn parse_range_header(value: &str, file_len: u64) -> Option<(u64, Option<u64>)> {
    let spec = value.strip_prefix("bytes=")?;
    // 多段 Range 不支持（插件传输引擎只用单段续传）
    if spec.contains(',') {
        return None;
    }
    let (start_str, end_str) = spec.split_once('-')?;
    let start: u64 = start_str.parse().ok()?;
    if start >= file_len {
        return None;
    }
    let end: Option<u64> = if end_str.is_empty() {
        None
    } else {
        let end = end_str.parse().ok()?;
        if end < start {
            return None;
        }
        Some(end)
    };
    Some((start, end))
}

// ==================== Handlers ====================

/// GET /{plugin_id}/{mount}/list?path= — 目录列举
///
/// path 为空时列举挂载根（多 root 时每个 root 作为顶层条目，
/// 名称取 root 最后一段；失效的 root 跳过并告警）
async fn list_dir(
    registry: web::Data<Arc<FileServiceRegistry>>,
    params: web::Path<(String, String)>,
    query: web::Query<PathOnlyQuery>,
) -> HttpResponse {
    let (plugin_id, mount) = params.into_inner();

    let entry = match registry.get_entry(&plugin_id, &mount).await {
        Ok(e) => e,
        Err(e) => return error_response(actix_web::http::StatusCode::NOT_FOUND, 404, &e.to_string()),
    };
    if let Err(resp) = require_op(&entry, FileOperation::List) {
        return resp;
    }

    let rel = query.path.trim_matches('/').to_string();

    // 挂载根列举：每个允许目录根作为顶层条目
    if rel.is_empty() {
        let mut entries = Vec::new();
        for root in &entry.roots {
            match std::fs::metadata(root) {
                Ok(meta) if meta.is_dir() => {
                    entries.push(FileEntryDto {
                        name: root
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| root.display().to_string()),
                        size: 0,
                        mtime: mtime_unix_secs(&meta),
                        is_dir: true,
                    });
                }
                _ => {
                    // root 失效（规格 4.3）：该 root 下线、其余正常
                    tracing::warn!(
                        plugin_id = %plugin_id,
                        mount = %mount,
                        root = %root.display(),
                        "list: root unavailable, skipped"
                    );
                }
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        return HttpResponse::Ok().json(ListResponse {
            path: String::new(),
            entries,
            notice: None,
        });
    }

    let target = match registry.resolve_sandboxed(&plugin_id, &mount, &rel).await {
        Ok(p) => p,
        Err(e) => return error_response(actix_web::http::StatusCode::NOT_FOUND, 404, &e.to_string()),
    };

    // spawn_blocking 会 move target，权限判定提前计算
    let may_need_all_files_access = needs_all_files_access(&target);

    match tokio::task::spawn_blocking(move || read_dir_entries(&target)).await {
        Ok(Ok(entries)) => {
            // Android 分区存储：未授予「所有文件访问权限」时 read_dir 静默返回空列表
            //（FUSE 过滤，不报错）——空结果 + 路径需要该权限 ≈ 权限问题而非真空目录，
            // 经 notice 告知对端，对端据此提示用户（而非让用户反复刷新）
            let notice = if entries.is_empty() && may_need_all_files_access {
                tracing::warn!(
                    path = %rel,
                    "list: empty result in top-level storage dir; MANAGE_EXTERNAL_STORAGE may not be granted"
                );
                Some("all_files_access_may_be_required".to_string())
            } else {
                None
            };
            HttpResponse::Ok().json(ListResponse { path: rel, entries, notice })
        }
        Ok(Err(e)) => {
            let status = if matches!(e, crate::AppError::NotFound(_)) {
                actix_web::http::StatusCode::NOT_FOUND
            } else {
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
            };
            error_response(status, status.as_u16(), &e.to_string())
        }
        Err(e) => error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            500,
            &format!("list task failed: {}", e),
        ),
    }
}

/// GET /{plugin_id}/{mount}/file?path= — 下载（支持 Range 续传 206）
///
/// 流式读取（512KB 缓冲），字节经挂载点 cipher.encrypt_chunk（MVP 直通）
async fn download_file(
    req: HttpRequest,
    registry: web::Data<Arc<FileServiceRegistry>>,
    params: web::Path<(String, String)>,
    query: web::Query<PathOnlyQuery>,
) -> HttpResponse {
    let (plugin_id, mount) = params.into_inner();

    let entry = match registry.get_entry(&plugin_id, &mount).await {
        Ok(e) => e,
        Err(e) => return error_response(actix_web::http::StatusCode::NOT_FOUND, 404, &e.to_string()),
    };
    if let Err(resp) = require_op(&entry, FileOperation::Download) {
        return resp;
    }

    let rel = query.path.trim_matches('/').to_string();
    let target = match registry.resolve_sandboxed(&plugin_id, &mount, &rel).await {
        Ok(p) => p,
        Err(e) => return error_response(actix_web::http::StatusCode::NOT_FOUND, 404, &e.to_string()),
    };

    let meta = match tokio::fs::metadata(&target).await {
        Ok(m) if m.is_file() => m,
        _ => {
            return error_response(
                actix_web::http::StatusCode::NOT_FOUND,
                404,
                "not a file",
            )
        }
    };
    let file_len = meta.len();

    // Range 解析：仅支持 bytes=N- / bytes=N-M 单段
    let range = req
        .headers()
        .get(actix_web::http::header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| parse_range_header(v, file_len));

    let (start, end) = range.unwrap_or((0, None));
    let end = end.unwrap_or(file_len.saturating_sub(1)).min(file_len.saturating_sub(1));
    let content_len = end - start + 1;

    let file = match tokio::fs::File::open(&target).await {
        Ok(f) => f,
        Err(e) => {
            return error_response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                500,
                &format!("failed to open file '{}': {}", target.display(), e),
            )
        }
    };

    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut file = file;
    if let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await {
        return error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            500,
            &format!("seek failed: {}", e),
        );
    }

    let cipher = entry.cipher.clone();
    // remaining：本响应还需发送的字节数（Range 截断后的长度）。
    // 外层闭包持有 cipher（Arc），每次迭代克隆一份移入 async 块，
    // 避免 FnMut 多次调用时移动捕获变量
    let stream = futures_util::stream::unfold(
        (file, content_len),
        move |(mut file, remaining)| {
            let cipher = cipher.clone();
            async move {
            if remaining == 0 {
                return None;
            }
            let to_read = (DOWNLOAD_CHUNK_SIZE as u64).min(remaining) as usize;
            let mut buf = vec![0u8; to_read];
            match file.read(&mut buf).await {
                Ok(0) => None, // EOF 早于声明长度（文件被截断），终止流
                Ok(n) => {
                    buf.truncate(n);
                    // 加密缝：下载方向文件字节经 cipher 变换后发送（MVP 直通）
                    let chunk = cipher.encrypt_chunk(buf);
                    Some((
                        Ok::<_, std::io::Error>(web::Bytes::from(chunk)),
                        (file, remaining - n as u64),
                    ))
                }
                Err(e) => Some((Err(e), (file, 0))),
            }
            }
        },
    );

    let mut builder = if range.is_some() {
        let mut b = HttpResponse::PartialContent();
        b.insert_header((
            actix_web::http::header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", start, end, file_len),
        ));
        b
    } else {
        HttpResponse::Ok()
    };

    builder
        .insert_header((actix_web::http::header::ACCEPT_RANGES, "bytes"))
        .insert_header((actix_web::http::header::CONTENT_LENGTH, content_len.to_string()))
        .content_type("application/octet-stream")
        .streaming(stream)
}

/// HEAD /{plugin_id}/{mount}/file?path= — 返回 size+mtime 指纹（续传有效性比对，规格 7.4）
async fn head_file(
    registry: web::Data<Arc<FileServiceRegistry>>,
    params: web::Path<(String, String)>,
    query: web::Query<PathOnlyQuery>,
) -> HttpResponse {
    let (plugin_id, mount) = params.into_inner();

    let entry = match registry.get_entry(&plugin_id, &mount).await {
        Ok(e) => e,
        Err(e) => return error_response(actix_web::http::StatusCode::NOT_FOUND, 404, &e.to_string()),
    };
    if let Err(resp) = require_op(&entry, FileOperation::Download) {
        return resp;
    }

    let rel = query.path.trim_matches('/').to_string();
    let target = match registry.resolve_sandboxed(&plugin_id, &mount, &rel).await {
        Ok(p) => p,
        Err(e) => return error_response(actix_web::http::StatusCode::NOT_FOUND, 404, &e.to_string()),
    };

    let meta = match tokio::fs::metadata(&target).await {
        Ok(m) if m.is_file() => m,
        _ => {
            return error_response(
                actix_web::http::StatusCode::NOT_FOUND,
                404,
                "not a file",
            )
        }
    };

    HttpResponse::Ok()
        .insert_header(("X-File-Size", meta.len().to_string()))
        .insert_header(("X-File-Mtime", mtime_unix_secs(&meta).to_string()))
        .insert_header((actix_web::http::header::CONTENT_LENGTH, meta.len().to_string()))
        .finish()
}

/// POST /{plugin_id}/{mount}/upload — 创建 upload session
///
/// 流程（规格 4.2/4.4）：沙箱解析目标 → 策略钩子（2s fail-closed，
/// 拒绝发生在写任何字节前）→ 创建 session 返回 {sessionId, received:0}
async fn create_upload(
    registry: web::Data<Arc<FileServiceRegistry>>,
    params: web::Path<(String, String)>,
    body: web::Json<CreateUploadRequest>,
) -> HttpResponse {
    let (plugin_id, mount) = params.into_inner();

    let entry = match registry.get_entry(&plugin_id, &mount).await {
        Ok(e) => e,
        Err(e) => return error_response(actix_web::http::StatusCode::NOT_FOUND, 404, &e.to_string()),
    };
    if let Err(resp) = require_op(&entry, FileOperation::Upload) {
        return resp;
    }

    // 沙箱解析：父目录必须存在于某 root 内（最终文件尚不存在）
    let rel = body.relative_path.trim_matches('/').to_string();
    let target = match sandbox::resolve_upload_target_within_roots(&entry.roots, &rel) {
        Ok(p) => p,
        Err(e) => {
            return error_response(actix_web::http::StatusCode::BAD_REQUEST, 400, &e.to_string())
        }
    };

    // 策略钩子：同名即拒等策略由插件实现；超时/异常 fail-closed
    let meta = bedcode_plugin_api_mobile::UploadRequestMeta {
        relative_path: rel.clone(),
        size: body.size,
    };
    let decision = registry.call_upload_hook(&plugin_id, &mount, &meta).await;
    if !decision.allow {
        let reason = decision
            .reason
            .unwrap_or_else(|| "rejected by upload hook".to_string());
        tracing::info!(
            plugin_id = %plugin_id,
            mount = %mount,
            relative_path = %rel,
            reason = %reason,
            "upload rejected by policy hook"
        );
        return error_response(actix_web::http::StatusCode::FORBIDDEN, 403, &reason);
    }

    match registry
        .upload_sessions()
        .create(&plugin_id, &mount, target, body.size)
        .await
    {
        Ok(session) => HttpResponse::Ok().json(UploadSessionResponse {
            session_id: session.id,
            received: 0,
        }),
        Err(e) => error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            500,
            &e.to_string(),
        ),
    }
}

/// PUT /{plugin_id}/{mount}/upload/{sid} — 从 web::Payload 流式 append
///
/// 不走 Json extractor（无大小上限）；按 512KB 缓冲累积后写入，
/// offset 与服务端已收不一致时返回 409（客户端应先 GET 查询续传点）
async fn append_upload(
    registry: web::Data<Arc<FileServiceRegistry>>,
    params: web::Path<(String, String, String)>,
    mut payload: web::Payload,
) -> HttpResponse {
    let (plugin_id, mount, sid) = params.into_inner();
    let sessions = registry.upload_sessions().clone();

    // 归属校验 + 初始偏移（续传握手依赖此值）
    let mut offset = match sessions.get(&sid, &plugin_id, &mount).await {
        Some(session) => session.received,
        None => {
            return error_response(
                actix_web::http::StatusCode::NOT_FOUND,
                404,
                "upload session not found",
            )
        }
    };

    // 加密缝：网络字节经挂载点 cipher 解密后落盘（MVP 直通）
    let entry = match registry.get_entry(&plugin_id, &mount).await {
        Ok(e) => e,
        Err(e) => {
            return error_response(actix_web::http::StatusCode::NOT_FOUND, 404, &e.to_string())
        }
    };

    let mut buffer: Vec<u8> = Vec::with_capacity(APPEND_FLUSH_THRESHOLD);
    let flush_result: Result<(), UploadSessionError> = async {
        while let Some(chunk) = payload.next().await {
            let chunk = chunk.map_err(|e| {
                UploadSessionError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    format!("payload read failed: {}", e),
                ))
            })?;
            buffer.extend_from_slice(&chunk);

            // 达到缓冲下限或单 chunk 过大时 flush，保持内存占用有界
            if buffer.len() >= APPEND_FLUSH_THRESHOLD || buffer.len() >= APPEND_MAX_CHUNK {
                let plain = entry.cipher.decrypt_chunk(std::mem::take(&mut buffer));
                offset = sessions.append(&sid, &plugin_id, &mount, offset, &plain).await?;
            }
        }
        if !buffer.is_empty() {
            let plain = entry.cipher.decrypt_chunk(std::mem::take(&mut buffer));
            offset = sessions.append(&sid, &plugin_id, &mount, offset, &plain).await?;
        }
        Ok(())
    }
    .await;

    match flush_result {
        Ok(()) => HttpResponse::Ok().json(UploadSessionResponse {
            session_id: sid,
            received: offset,
        }),
        Err(UploadSessionError::OffsetMismatch { expected, got }) => error_response(
            actix_web::http::StatusCode::CONFLICT,
            409,
            &format!(
                "offset mismatch: server has {} bytes, client sent offset {}",
                expected, got
            ),
        ),
        Err(UploadSessionError::NotFound(id)) => error_response(
            actix_web::http::StatusCode::NOT_FOUND,
            404,
            &format!("upload session not found: {}", id),
        ),
        Err(e) => error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            500,
            &e.to_string(),
        ),
    }
}

/// GET /{plugin_id}/{mount}/upload/{sid} — 查询 session 状态（已收字节，续传握手）
async fn query_upload(
    registry: web::Data<Arc<FileServiceRegistry>>,
    params: web::Path<(String, String, String)>,
) -> HttpResponse {
    let (plugin_id, mount, sid) = params.into_inner();
    match registry.upload_sessions().get(&sid, &plugin_id, &mount).await {
        Some(session) => HttpResponse::Ok().json(UploadSessionResponse {
            session_id: session.id,
            received: session.received,
        }),
        None => error_response(
            actix_web::http::StatusCode::NOT_FOUND,
            404,
            "upload session not found",
        ),
    }
}

/// POST /{plugin_id}/{mount}/upload/{sid}/complete — 原子 rename 落位
///
/// 目标已存在 → 409 duplicate-name（保留 .part，规格 7.4）
async fn complete_upload(
    registry: web::Data<Arc<FileServiceRegistry>>,
    params: web::Path<(String, String, String)>,
) -> HttpResponse {
    let (plugin_id, mount, sid) = params.into_inner();
    match registry
        .upload_sessions()
        .complete(&sid, &plugin_id, &mount)
        .await
    {
        Ok(target) => {
            tracing::info!(
                plugin_id = %plugin_id,
                mount = %mount,
                target = %target.display(),
                "upload completed"
            );
            HttpResponse::Ok().json(serde_json::json!({ "code": 0, "message": "ok" }))
        }
        Err(UploadSessionError::DuplicateName(_)) => error_response(
            actix_web::http::StatusCode::CONFLICT,
            409,
            "duplicate-name",
        ),
        Err(UploadSessionError::NotFound(id)) => error_response(
            actix_web::http::StatusCode::NOT_FOUND,
            404,
            &format!("upload session not found: {}", id),
        ),
        Err(e) => error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            500,
            &e.to_string(),
        ),
    }
}

/// DELETE /{plugin_id}/{mount}/upload/{sid} — 取消（清理临时文件）
async fn cancel_upload(
    registry: web::Data<Arc<FileServiceRegistry>>,
    params: web::Path<(String, String, String)>,
) -> HttpResponse {
    let (plugin_id, mount, sid) = params.into_inner();
    match registry
        .upload_sessions()
        .cancel(&sid, &plugin_id, &mount)
        .await
    {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "code": 0, "message": "ok" })),
        Err(UploadSessionError::NotFound(id)) => error_response(
            actix_web::http::StatusCode::NOT_FOUND,
            404,
            &format!("upload session not found: {}", id),
        ),
        Err(e) => error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            500,
            &e.to_string(),
        ),
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_range_header() {
        assert_eq!(parse_range_header("bytes=0-", 100), Some((0, None)));
        assert_eq!(parse_range_header("bytes=50-", 100), Some((50, None)));
        assert_eq!(parse_range_header("bytes=10-19", 100), Some((10, Some(19))));
        // start 超出文件长度 → None（退化为 200 全量；真实客户端应重新握手）
        assert_eq!(parse_range_header("bytes=100-", 100), None);
        assert_eq!(parse_range_header("bytes=200-", 100), None);
        // 非法格式 → None
        assert_eq!(parse_range_header("bytes=abc-", 100), None);
        assert_eq!(parse_range_header("bytes=10-5", 100), None);
        assert_eq!(parse_range_header("bytes=0-1,2-3", 100), None);
        assert_eq!(parse_range_header("chars=0-", 100), None);
    }
}
