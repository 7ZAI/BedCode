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
//! SAF 化（M2，见 [`saf_tree`]）：挂载根含 `content://tree/...` URI 时，
//! list/download 经 SafIo 服务（list_tree 遍历 / 中转复制到私有中转目录，
//! Range 响应从中转副本服务）；upload 目标语义改为下载目录，complete 优先
//! 落 MediaStore 公共下载、失败回退私有目录。
//!
//! Android 注意：actix 默认按核数起 worker 线程，服务启动经
//! `tauri::async_runtime::spawn` 交给运行时，不阻塞调用方。

use crate::file_service::auth::BearerTokenGuard;
use crate::file_service::registry::{FileServiceRegistry, MountEntry};
use crate::file_service::saf_tree;
use crate::file_service::upload::UploadSessionError;
use crate::file_service::{cipher::TransportCipher, sandbox};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use actix_web::dev::{ServerHandle, Service as _};
use bedcode_plugin_api_mobile::FileOperation;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
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
#[derive(Debug, Serialize, Deserialize)]
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
#[derive(Debug, Serialize, Deserialize)]
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
    /// v2：所属传输批 ID（ask 模式强制批上下文；无批 ID 时走 v1 per-file 钩子）
    #[serde(default)]
    pub batch_id: Option<String>,
}

/// 批量传输请求 DTO（POST /transfer-request 请求体，camelCase）
///
/// 与 crate::file_service::transfer::TransferRequestDto 同构（serde 派生），
/// server 层直接用 SDK 契约类型反序列化，避免双份 DTO 漂移
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequestHttpDto {
    /// 批 ID
    pub batch_id: String,
    /// 批内文件清单
    pub files: Vec<bedcode_plugin_api_mobile::UploadRequestMeta>,
    /// 批内文件总大小（字节）
    pub total_size: u64,
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
#[derive(Debug, Deserialize, Clone)]
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

        // 启动扫描清理 SAF 中转副本（进程崩溃残留；副本可随时从 SAF 重新
        // 生成，见 saf_tree 模块「启动扫描清理」）。懒解析失败（无 app handle）
        // 仅告警，不阻断服务启动
        if let Some(relay_dir) = self.registry.relay_dir().await {
            saf_tree::sweep_relay_dir(&relay_dir);
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
                .service(
                    web::resource("/{plugin_id}/{mount}/transfer-request")
                        .route(web::post().to(create_transfer_request)),
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
/// 名称取 root 最后一段；失效的 root 跳过并告警；SAF 树根以别名
/// （树 document id 末段）作为顶层条目）。非空 path 先试 SAF 根命中
/// （list_tree 遍历，无 needs_all_files_access notice 语义），
/// 未命中走真实路径 read_dir。
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

    // 挂载根列举：真实路径根 + SAF 树根作为顶层条目
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
        // SAF 树根：别名作为顶层条目（is_dir=true；别名与请求路径首段映射，
        // 可导航；授权有效性在遍历时校验，此处不拦截）
        for saf_root in &entry.saf_roots {
            match saf_tree::tree_alias(saf_root) {
                Some(alias) => entries.push(FileEntryDto {
                    name: alias,
                    size: 0,
                    mtime: 0,
                    is_dir: true,
                }),
                None => tracing::warn!(
                    plugin_id = %plugin_id,
                    mount = %mount,
                    root = %saf_root,
                    "list: invalid SAF root skipped"
                ),
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        return HttpResponse::Ok().json(ListResponse {
            path: String::new(),
            entries,
            notice: None,
        });
    }

    // SAF 根命中：list_tree 遍历（替代 std::fs::read_dir）。SAF 授权场景
    // 不触发 needs_all_files_access notice（notice 仅真实路径根的分区存储
    // 过滤语义，见 resolve_sandboxed 分支下方）
    if let Some((tree_uri, parts)) = saf_tree::match_saf_root(&entry.saf_roots, &rel) {
        return list_saf_dir(&registry, &tree_uri, &parts, &rel).await;
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

// ==================== SAF 端点辅助（M2） ====================

/// SAF 解析错误 → HTTP 响应（NotFound → 404，其余 500 带上下文）
fn saf_error_response(err: &crate::AppError) -> HttpResponse {
    let status = if matches!(err, crate::AppError::NotFound(_)) {
        actix_web::http::StatusCode::NOT_FOUND
    } else {
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
    };
    error_response(status, status.as_u16(), &err.to_string())
}

/// SAF 目录列举：walk 到目标目录 → list_tree 子条目（替代 std::fs::read_dir）
///
/// 无 needs_all_files_access notice 语义（SAF 条目经持久化授权，分区存储
/// 过滤不适用）；列表字段与真实路径一致（SAF 条目无 mtime，置 0）。
async fn list_saf_dir(
    registry: &web::Data<Arc<FileServiceRegistry>>,
    tree_uri: &str,
    parts: &[String],
    rel: &str,
) -> HttpResponse {
    let saf = match registry.saf_io().await {
        Some(s) => s,
        None => return error_response(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, 500, "SAF storage unavailable on this platform"),
    };
    let root_doc = match saf_tree::tree_document_id(tree_uri) {
        Some(d) => d,
        None => {
            return error_response(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, 500, &format!("invalid SAF root: {}", tree_uri))
        }
    };
    let target = match saf_tree::walk_to_entry(saf.as_ref(), tree_uri, &root_doc, parts).await {
        Ok(t) => t,
        Err(e) => return saf_error_response(&e),
    };
    if !target.is_dir {
        return error_response(actix_web::http::StatusCode::NOT_FOUND, 404, &format!("'{}' is not a directory", rel));
    }
    let children = match saf.list_tree(tree_uri, &target.document_id) {
        Ok(c) => c,
        Err(e) => {
            return error_response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                500,
                &format!("list SAF tree failed (permission may be revoked): {}", e),
            )
        }
    };
    let mut entries: Vec<FileEntryDto> = children
        .into_iter()
        // 过滤上传/中转临时文件（*.part），与真实路径列表规则一致
        .filter(|c| !crate::file_service::upload::is_filtered_listing_name(&c.name))
        .map(|c| FileEntryDto {
            name: c.name,
            size: if c.is_dir { 0 } else { c.size.max(0) as u64 },
            mtime: 0,
            is_dir: c.is_dir,
        })
        .collect();
    // 目录优先，按名称排序，保证两端 UI 展示一致
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    HttpResponse::Ok().json(ListResponse {
        path: rel.to_string(),
        entries,
        notice: None,
    })
}

/// SAF 下载源解析：walk 到文件条目 → 确保中转副本（不可续）→ 副本路径/大小
///
/// 指纹（HEAD）与 Range（GET）均以副本为准：副本在 TTL 内稳定，续传握手
/// （size/mtime 比对）可命中；副本被清理/重生成后指纹变化，对端判
/// remote-changed 从头重传（语义安全，见 saf_tree 模块）。
async fn resolve_saf_download_source(
    registry: &Arc<FileServiceRegistry>,
    tree_uri: &str,
    parts: &[String],
    rel: &str,
) -> crate::Result<(PathBuf, u64, PathBuf)> {
    let saf = registry.saf_io().await.ok_or_else(|| {
        crate::AppError::Internal("SAF storage unavailable on this platform".to_string())
    })?;
    let root_doc = saf_tree::tree_document_id(tree_uri).ok_or_else(|| {
        crate::AppError::InvalidInput(format!("invalid SAF root: {}", tree_uri))
    })?;
    let source = saf_tree::walk_to_entry(saf.as_ref(), tree_uri, &root_doc, parts).await?;
    if source.is_dir {
        return Err(crate::AppError::NotFound(format!("'{}' is not a file", rel)));
    }
    let relay_dir = registry.relay_dir().await.ok_or_else(|| {
        crate::AppError::Internal("SAF relay dir unavailable".to_string())
    })?;
    let cache_path =
        saf_tree::ensure_relay_copy(saf.as_ref(), &relay_dir, &source.uri).await?;
    let meta = tokio::fs::metadata(&cache_path).await.map_err(|e| {
        crate::AppError::Internal(format!("relay copy metadata failed: {}", e))
    })?;
    Ok((cache_path, meta.len(), relay_dir))
}

/// GET /{plugin_id}/{mount}/file?path= — 下载（支持 Range 续传 206）
///
/// 流式读取（512KB 缓冲），字节经挂载点 cipher.encrypt_chunk（MVP 直通）。
/// SAF 根命中：源经中转复制（不可续）后从中转副本服务（可续）。
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

    // SAF 根命中：中转复制 → Range 从中转副本服务（副本 TTL 内续传命中）
    if let Some((tree_uri, parts)) = saf_tree::match_saf_root(&entry.saf_roots, &rel) {
        let (cache_path, file_len, relay_dir) =
            match resolve_saf_download_source(registry.as_ref(), &tree_uri, &parts, &rel).await
            {
                Ok(v) => v,
                Err(e) => return saf_error_response(&e),
            };

        // Range 解析：仅支持 bytes=N- / bytes=N-M 单段（与真实路径一致）
        let range = req
            .headers()
            .get(actix_web::http::header::RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| parse_range_header(v, file_len));
        let (start, end) = range.unwrap_or((0, None));
        let end = end.unwrap_or(file_len.saturating_sub(1)).min(file_len.saturating_sub(1));
        // 空文件（file_len==0）无 Range 时 end=0，end-start+1=1 与实际空 body
        // 不符 → 对端 reqwest 报「error decoding response body」，0 字节文件
        // 必然下载失败；空文件 content_len 必须为 0
        let content_len = if file_len == 0 { 0 } else { end - start + 1 };

        let file = match tokio::fs::File::open(&cache_path).await {
            Ok(f) => f,
            Err(e) => {
                return error_response(
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    &format!("failed to open relay copy '{}': {}", cache_path.display(), e),
                )
            }
        };

        use tokio::io::AsyncSeekExt;
        let mut file = file;
        if let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await {
            return error_response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                500,
                &format!("seek failed: {}", e),
            );
        }

        let stream = build_download_stream(
            file,
            content_len,
            entry.cipher.clone(),
            Some((relay_dir.clone(), cache_path.clone())),
        );
        // 响应构建即 arm：客户端中断（流未走终止分支）时副本仍按 TTL 清理；
        // 流终止分支会再次 arm（刷新 last_access 滑动续期，见 saf_tree）
        saf_tree::arm_relay_cleanup(&relay_dir, &cache_path);
        return build_download_response(range, start, end, file_len, content_len, stream);
    }

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
    // 空文件（file_len==0）无 Range 时 end=0，end-start+1=1 与实际空 body
    // 不符 → 对端 reqwest 报「error decoding response body」，0 字节文件
    // 必然下载失败；空文件 content_len 必须为 0
    let content_len = if file_len == 0 { 0 } else { end - start + 1 };

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

    use tokio::io::AsyncSeekExt;
    let mut file = file;
    if let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await {
        return error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            500,
            &format!("seek failed: {}", e),
        );
    }

    let stream = build_download_stream(file, content_len, entry.cipher.clone(), None);
    build_download_response(range, start, end, file_len, content_len, stream)
}

/// 流式响应状态：文件 + 剩余字节 + 中转副本清理钩子（SAF 下载用）
struct DownloadStreamState {
    file: tokio::fs::File,
    remaining: u64,
    /// 中转副本清理（服务完成/超时后；relay_dir, cache_path）
    relay_cleanup: Option<(PathBuf, PathBuf)>,
    /// 清理钩子是否已触发（流可能提前终止，仅触发一次）
    cleanup_armed: bool,
}

/// 构建下载流式响应体（真实路径与 SAF 中转副本共用）
///
/// cleanup：流终止（完成/EOF/错误）时触发的中转副本 TTL 清理——副本在
/// 响应结束后延迟清理，续传窗口（TTL 内）命中需文件仍在（见 saf_tree）。
fn build_download_stream(
    file: tokio::fs::File,
    content_len: u64,
    cipher: Arc<dyn TransportCipher>,
    relay_cleanup: Option<(PathBuf, PathBuf)>,
) -> impl futures_util::Stream<Item = Result<web::Bytes, std::io::Error>> {
    futures_util::stream::unfold(
        DownloadStreamState {
            file,
            remaining: content_len,
            relay_cleanup,
            cleanup_armed: false,
        },
        move |mut st| {
            let cipher = cipher.clone();
            async move {
                if st.remaining == 0 {
                    arm_relay_cleanup_once(&mut st);
                    return None;
                }
                let to_read = (DOWNLOAD_CHUNK_SIZE as u64).min(st.remaining) as usize;
                let mut buf = vec![0u8; to_read];
                match st.file.read(&mut buf).await {
                    Ok(0) => {
                        // EOF 早于声明长度（文件被截断），终止流
                        arm_relay_cleanup_once(&mut st);
                        None
                    }
                    Ok(n) => {
                        buf.truncate(n);
                        // 加密缝：下载方向文件字节经 cipher 变换后发送（MVP 直通）
                        let chunk = cipher.encrypt_chunk(buf);
                        st.remaining -= n as u64;
                        Some((
                            Ok::<_, std::io::Error>(web::Bytes::from(chunk)),
                            st,
                        ))
                    }
                    Err(e) => {
                        arm_relay_cleanup_once(&mut st);
                        Some((Err(e), st))
                    }
                }
            }
        },
    )
}

/// 流终止时触发中转副本 TTL 清理（幂等：仅触发一次）
fn arm_relay_cleanup_once(st: &mut DownloadStreamState) {
    if st.cleanup_armed {
        return;
    }
    st.cleanup_armed = true;
    if let Some((relay_dir, cache_path)) = st.relay_cleanup.clone() {
        saf_tree::arm_relay_cleanup(&relay_dir, &cache_path);
    }
}

/// 构造下载响应（Range 206 / 200 全量 + 流式 body）
fn build_download_response<S>(
    range: Option<(u64, Option<u64>)>,
    start: u64,
    end: u64,
    file_len: u64,
    content_len: u64,
    stream: S,
) -> HttpResponse
where
    S: futures_util::Stream<Item = Result<web::Bytes, std::io::Error>> + 'static,
{
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
///
/// SAF 根命中：先确保中转副本（指纹以副本为准，见 resolve_saf_download_source）。
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

    // SAF 根命中：中转副本的 size/mtime 作为指纹（副本 TTL 内稳定）
    if let Some((tree_uri, parts)) = saf_tree::match_saf_root(&entry.saf_roots, &rel) {
        let (cache_path, _file_len, relay_dir) =
            match resolve_saf_download_source(registry.as_ref(), &tree_uri, &parts, &rel).await
            {
                Ok(v) => v,
                Err(e) => return saf_error_response(&e),
            };
        let meta = match tokio::fs::metadata(&cache_path).await {
            Ok(m) if m.is_file() => m,
            _ => {
                return error_response(
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "relay copy missing",
                )
            }
        };
        saf_tree::arm_relay_cleanup(&relay_dir, &cache_path);
        return HttpResponse::Ok()
            .insert_header(("X-File-Size", meta.len().to_string()))
            .insert_header(("X-File-Mtime", mtime_unix_secs(&meta).to_string()))
            .insert_header((actix_web::http::header::CONTENT_LENGTH, meta.len().to_string()))
            .finish();
    }

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
/// 流程（规格 4.2/4.4 + v2 批 gating）：
/// 1. batchId 存在 → 批 gating（approved 免钩子；pending/rejected/not-found → 403 防绕过）
/// 2. batchId 不存在 → 走 v1 per-file 钩子；钩子 ask → 403 batch-context-required（fail-closed）
/// 3. session 创建成功（两条路径都要）→ 本地事件 `filesrv:receiving_started`
///
/// M2：目标语义为下载目录（接收落点；共享目录只读暴露，桌面端推送统一落下载目录）。
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

    // 沙箱解析：父目录必须存在于下载目录内（最终文件尚不存在）。
    // 复用 resolve_upload_target_within_roots 的别名/穿越校验，根 = 下载目录
    let rel = body.relative_path.trim_matches('/').to_string();
    let downloads_dir = match registry.downloads_dir().await {
        Some(d) => d,
        None => {
            return error_response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "downloads dir unavailable",
            )
        }
    };
    let target = match sandbox::resolve_upload_target_within_roots(&[downloads_dir], &rel) {
        Ok(p) => p,
        Err(e) => {
            return error_response(actix_web::http::StatusCode::BAD_REQUEST, 400, &e.to_string())
        }
    };

    // v2 批 gating：带批 ID 的 session 创建免钩子（批已批准即代表用户同意）
    if let Some(ref batch_id) = body.batch_id {
        match registry.check_batch(&plugin_id, &mount, batch_id).await {
            Ok(_batch) => {
                // 批 approved：直接建 session（免 per-file 钩子）
            }
            Err(e) => {
                let (message, status) = match &e {
                    crate::file_service::registry::BatchError::NotFound(m) => {
                        (m.clone(), actix_web::http::StatusCode::FORBIDDEN)
                    }
                    crate::file_service::registry::BatchError::NotApproved(m)
                    | crate::file_service::registry::BatchError::Rejected(m) => {
                        (m.clone(), actix_web::http::StatusCode::FORBIDDEN)
                    }
                    _ => (
                        "batch-not-approved".to_string(),
                        actix_web::http::StatusCode::FORBIDDEN,
                    ),
                };
                tracing::warn!(
                    plugin_id = %plugin_id,
                    mount = %mount,
                    batch_id = %batch_id,
                    error = %message,
                    "create_upload rejected by batch gating"
                );
                return error_response(status, 403, &message);
            }
        }
    } else {
        // 无批 ID：走 v1 per-file 钩子（accept/reject 策略路径）
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
            // ask 模式强制批上下文：钩子返回 ask 的 session 创建一律 403
            //（防绕过 /upload —— 发送方必须走 transfer-request 批流）
            if decision.ask {
                return error_response(
                    actix_web::http::StatusCode::FORBIDDEN,
                    403,
                    "batch-context-required",
                );
            }
            // 同名拒绝返回 409 (Conflict)，对齐桌面 handshake create_session
            // 的 DuplicateName 解析路径（409 → Rejected(duplicate-name) 变秒显
            // 可重设/备远端同名文件）；其他钩子拒绝（invalid-path / 钩子不可用 /
            // 超时）仍返 403，供发起方抖出真实原因。
            if reason == "duplicate-name" {
                return error_response(
                    actix_web::http::StatusCode::CONFLICT,
                    409,
                    "duplicate-name",
                );
            }
            return error_response(actix_web::http::StatusCode::FORBIDDEN, 403, &reason);
        }
    }

    let session = match registry
        .upload_sessions()
        .create(&plugin_id, &mount, target, body.size)
        .await
    {
        Ok(session) => session,
        Err(e) => {
            return error_response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                500,
                &e.to_string(),
            )
        }
    };

    // session 创建成功（钩子路径与批路径都发）：接收端「正在接收」任务 + accept 模式 toast
    registry
        .emit_filesrv_event(
            "filesrv:receiving_started",
            serde_json::json!({
                "sessionId": session.id,
                "batchId": body.batch_id,
                "relativePath": rel,
                "size": body.size,
            }),
        )
        .await;
    // 批内 session 活动刷新（approved 批 24h TTL 依据）
    if let Some(ref batch_id) = body.batch_id {
        registry.touch_batch(batch_id).await;
    }

    HttpResponse::Ok().json(UploadSessionResponse {
        session_id: session.id,
        received: 0,
    })
}

/// POST /{plugin_id}/{mount}/transfer-request — 批量传输请求（v2）
///
/// 钩子三路分流（规格 14.2）：
/// - allow → 200 { batchId, decision: "approved" }（批记录 approved，可立即建 session）
/// - ask → 202 { batchId, decision: "pending" }（批记录 pending + 本地事件）
/// - deny → 403（message 含 reason，如 policy-denied；不建批、无任务无记录）
///
/// 钩子超时/插件异常/挂载不存在一律 fail-closed deny（复用上传钩子 2s 超时语义）
async fn create_transfer_request(
    registry: web::Data<Arc<FileServiceRegistry>>,
    params: web::Path<(String, String)>,
    body: web::Json<TransferRequestHttpDto>,
) -> HttpResponse {
    let (plugin_id, mount) = params.into_inner();

    let entry = match registry.get_entry(&plugin_id, &mount).await {
        Ok(e) => e,
        Err(e) => return error_response(actix_web::http::StatusCode::NOT_FOUND, 404, &e.to_string()),
    };
    if let Err(resp) = require_op(&entry, FileOperation::Upload) {
        return resp;
    }
    // 沙箱不需要：仅元数据（批钩子按接收策略分流，字节写前还有批 gating + 落位校验）

    let dto = crate::file_service::transfer::TransferRequestDto {
        batch_id: body.batch_id.clone(),
        files: body.files.clone(),
        total_size: body.total_size,
    };
    match registry
        .create_transfer_request(&plugin_id, &mount, &dto)
        .await
    {
        Ok(crate::file_service::transfer::BatchDecision::Approved) => {
            HttpResponse::Ok().json(serde_json::json!({
                "batchId": body.batch_id,
                "decision": "approved",
            }))
        }
        Ok(crate::file_service::transfer::BatchDecision::Pending) => {
            HttpResponse::Accepted().json(serde_json::json!({
                "batchId": body.batch_id,
                "decision": "pending",
            }))
        }
        Err(e) => {
            let (message, status) = match &e {
                crate::file_service::registry::BatchError::Denied(m)
                | crate::file_service::registry::BatchError::HookFailed(m)
                | crate::file_service::registry::BatchError::GatingDenied(m) => {
                    (m.clone(), actix_web::http::StatusCode::FORBIDDEN)
                }
                _ => (
                    "policy-denied".to_string(),
                    actix_web::http::StatusCode::FORBIDDEN,
                ),
            };
            tracing::info!(
                plugin_id = %plugin_id,
                mount = %mount,
                batch_id = %body.batch_id,
                error = %message,
                "transfer request denied (fail-closed)"
            );
            error_response(status, 403, &message)
        }
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

/// POST /{plugin_id}/{mount}/upload/{sid}/complete — 落位（M2：MediaStore 优先）
///
/// 落位顺序：MediaStore 公共下载写入成功 → 删除临时文件（公共目录唯一副本）；
/// 写入失败（含 API<29 设备）→ 回退 rename 到私有下载目录（原 complete 语义）。
/// 目标（私有回退路径）已存在 → 409 duplicate-name（保留 .part，规格 7.4）。
async fn complete_upload(
    registry: web::Data<Arc<FileServiceRegistry>>,
    params: web::Path<(String, String, String)>,
) -> HttpResponse {
    let (plugin_id, mount, sid) = params.into_inner();
    let sessions = registry.upload_sessions();

    let result = match registry.saf_io().await {
        // MediaStore 落位：成功删临时文件，失败回退 rename（回退由
        // complete_to_media 内部完成，见 upload.rs）
        Some(saf) => {
            sessions
                .complete_to_media(&sid, &plugin_id, &mount, |tmp, display_name| {
                    saf.write_media_downloads(&tmp.to_string_lossy(), display_name, "")
                        .map_err(|e| {
                            // 同名拒绝（Kotlin 侧 MediaStore 预检返回）→ 终态失败
                            // 不回退私有；其余失败回退私有 rename（原语义）
                            let msg = e.to_string();
                            if msg.contains("duplicate-name") {
                                crate::file_service::upload::PlacementError::Duplicate(msg)
                            } else {
                                crate::file_service::upload::PlacementError::Other(msg)
                            }
                        })
                })
                .await
        }
        // SafIo 未注入（非 Android）：直接 rename 落位（原语义）
        None => sessions.complete(&sid, &plugin_id, &mount).await,
    };

    match result {
        Ok(target) => {
            tracing::info!(
                plugin_id = %plugin_id,
                mount = %mount,
                target = %target.display(),
                "upload completed"
            );
            // 接收任务终态事件（前端接收 tab 归档）
            registry
                .emit_filesrv_event(
                    "filesrv:receiving_done",
                    serde_json::json!({ "sessionId": sid, "state": "completed" }),
                )
                .await;
            HttpResponse::Ok().json(serde_json::json!({ "code": 0, "message": "ok" }))
        }
        Err(UploadSessionError::DuplicateName(_)) => {
            // 409 竞态：该文件 rejected(duplicate-name)，批内其他文件不受影响
            registry
                .emit_filesrv_event(
                    "filesrv:receiving_done",
                    serde_json::json!({
                        "sessionId": sid,
                        "state": "failed",
                        "reason": "duplicate-name",
                    }),
                )
                .await;
            error_response(
                actix_web::http::StatusCode::CONFLICT,
                409,
                "duplicate-name",
            )
        }
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
        Ok(()) => {
            // 发送方取消：接收任务终态事件（接收 tab 归档为 cancelled）
            registry
                .emit_filesrv_event(
                    "filesrv:receiving_done",
                    serde_json::json!({ "sessionId": sid, "state": "cancelled" }),
                )
                .await;
            HttpResponse::Ok().json(serde_json::json!({ "code": 0, "message": "ok" }))
        }
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

    /// 从 HttpResponse 提取响应体字节（测试辅助）
    async fn body_bytes(resp: HttpResponse) -> actix_web::web::Bytes {
        actix_web::body::to_bytes(resp.into_body()).await.unwrap()
    }
    use crate::file_service::cipher::PassthroughCipher;
    use crate::file_service::registry::HookTarget;
    use crate::plugin::saf_io::{SafCopyHandle, SafCopyStatus, SafEntry, SafIo, SafStreamHandle};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const TREE: &str = "content://tree/primary%3ADownload";
    const ROOT_DOC: &str = "primary:Download";

    /// 端点测试用 fake SafIo：内存树 + 文件内容 + 可配置 MediaStore 落位结果
    ///
    /// 覆盖端点 cache 中转编排（list_tree 遍历 / 中转复制 / 落点回退），
    /// spec「测试注入 fake」；read_to_cache 直接落盘文件内容（模拟 Kotlin
    /// 复制完成），copy_status 恒为已终态成功。
    struct FakeSaf {
        /// (tree_uri, document_id) → 子条目
        tree: HashMap<(String, String), Vec<SafEntry>>,
        /// 文件条目 URI → 内容
        files: HashMap<String, Vec<u8>>,
        /// MediaStore 落位是否成功
        media_ok: bool,
        /// 落位调用记录（src, display_name）
        media_writes: std::sync::Mutex<Vec<(String, String)>>,
        /// 已启动的中转复制计数
        copies_started: AtomicUsize,
        /// 中转复制 staging 目录（模拟 Kotlin bedcode_uploads）
        staging: PathBuf,
    }

    impl SafIo for FakeSaf {
        fn list_tree(&self, tree_uri: &str, document_id: &str) -> crate::Result<Vec<SafEntry>> {
            Ok(self
                .tree
                .get(&(tree_uri.to_string(), document_id.to_string()))
                .cloned()
                .unwrap_or_default())
        }

        fn read_to_cache(&self, uri: &str, dest_name: &str) -> crate::Result<SafCopyHandle> {
            let dest_path = self.staging.join(dest_name);
            if let Some(content) = self.files.get(uri) {
                std::fs::create_dir_all(&self.staging).unwrap();
                std::fs::write(&dest_path, content).unwrap();
            }
            self.copies_started.fetch_add(1, Ordering::SeqCst);
            Ok(SafCopyHandle {
                copy_id: format!("copy-{}", dest_name),
                dest_path: dest_path.to_string_lossy().into_owned(),
            })
        }

        fn copy_status(&self, _copy_id: &str) -> crate::Result<SafCopyStatus> {
            Ok(SafCopyStatus {
                copy_id: String::new(),
                done: 0,
                total: 0,
                finished: true,
                cancelled: false,
                error: None,
                dest_path: String::new(),
            })
        }

        fn cancel_copy(&self, _copy_id: &str) -> crate::Result<()> {
            Ok(())
        }

        fn cleanup_stale_copies(&self) -> crate::Result<()> {
            Ok(())
        }

        fn check_authorized(&self, _tree_uri: &str) -> crate::Result<bool> {
            Ok(true)
        }

        fn write_media_downloads(
            &self,
            src_path: &str,
            display_name: &str,
            _mime_type: &str,
        ) -> crate::Result<()> {
            self.media_writes
                .lock()
                .unwrap()
                .push((src_path.to_string(), display_name.to_string()));
            if self.media_ok {
                Ok(())
            } else {
                Err(crate::AppError::Plugin("requires API 29+".to_string()))
            }
        }

        // M3 流直传 / 保存到…：server.rs 端点测试未覆盖（编排在插件侧），
        // 全部返回默认成功/不可用错误以通过 trait 编译
        fn open_stream(&self, _u: &str, offset: u64) -> crate::Result<SafStreamHandle> {
            Ok(SafStreamHandle {
                handle_id: "stream-1".to_string(),
                effective_offset: offset,
                seekable: true,
                size: 0,
            })
        }

        fn read_stream(&self, _h: &str, _len: usize) -> crate::Result<Vec<u8>> {
            Ok(Vec::new())
        }

        fn seek_stream(&self, _h: &str, _o: u64) -> crate::Result<()> {
            Ok(())
        }

        fn close_stream(&self, _h: &str) -> crate::Result<()> {
            Ok(())
        }

        fn stream_seekable(&self, _u: &str) -> crate::Result<bool> {
            Ok(true)
        }

        fn save_to_document(&self, _s: &str, _n: &str, _m: &str) -> crate::Result<()> {
            Ok(())
        }
    }

    impl FakeSaf {
        fn new(media_ok: bool, staging: PathBuf) -> Self {
            Self {
                tree: HashMap::new(),
                files: HashMap::new(),
                media_ok,
                media_writes: std::sync::Mutex::new(Vec::new()),
                copies_started: AtomicUsize::new(0),
                staging,
            }
        }

        /// 构建标准树：根含 sub/ 目录与 a.txt，sub/ 含 b.bin
        fn with_standard_tree(mut self) -> Self {
            self.tree.insert(
                (TREE.to_string(), ROOT_DOC.to_string()),
                vec![
                    SafEntry {
                        name: "sub".to_string(),
                        is_dir: true,
                        size: 0,
                        mime: String::new(),
                        uri: format!("{}/document/sub", TREE),
                        document_id: "sub-doc".to_string(),
                    },
                    SafEntry {
                        name: "a.txt".to_string(),
                        is_dir: false,
                        size: 11,
                        mime: "text/plain".to_string(),
                        uri: format!("{}/document/f1", TREE),
                        document_id: "f1".to_string(),
                    },
                ],
            );
            self.tree.insert(
                (TREE.to_string(), "sub-doc".to_string()),
                vec![SafEntry {
                    name: "b.bin".to_string(),
                    is_dir: false,
                    size: 2,
                    mime: "application/octet-stream".to_string(),
                    uri: format!("{}/document/f2", TREE),
                    document_id: "f2".to_string(),
                }],
            );
            self.files.insert(format!("{}/document/f1", TREE), b"hello world".to_vec());
            self.files.insert(format!("{}/document/f2", TREE), b"hi".to_vec());
            self
        }
    }

    /// 构造带 SAF 根的挂载条目（List/Download/Upload 全操作）
    fn saf_mount_entry() -> MountEntry {
        MountEntry {
            plugin_id: "com.test".to_string(),
            mount_path: "files".to_string(),
            roots: Vec::new(),
            saf_roots: vec![TREE.to_string()],
            operations: vec![
                FileOperation::List,
                FileOperation::Download,
                FileOperation::Upload,
            ],
            hook: HookTarget::None,
            cipher: Arc::new(PassthroughCipher),
        }
    }

    /// 挂载注册表并注入 fake（返回 fake 引用供断言）
    async fn mounted_registry(
        saf: FakeSaf,
        relay_dir: &Path,
    ) -> (Arc<FileServiceRegistry>, Arc<FakeSaf>) {
        let saf = Arc::new(saf);
        let registry = FileServiceRegistry::with_saf_io(saf.clone());
        registry.set_relay_dir(relay_dir.to_path_buf()).await;
        registry.insert_entry_for_test(saf_mount_entry()).await;
        (registry, saf)
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Runtime::new().unwrap()
    }

    /// 收集下载流全部字节（流 Item 为 Result，出错即 panic）
    async fn collect_stream<S>(stream: S) -> Vec<web::Bytes>
    where
        S: futures_util::Stream<Item = Result<web::Bytes, std::io::Error>>,
    {
        stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .map(|c| c.expect("下载流不应出错"))
            .collect()
    }

    #[test]
    fn list_dir_lists_saf_root_and_traverses_tree() {
        let rt = runtime();
        rt.block_on(async {
            let base = tempfile::tempdir().unwrap();
            let relay = base.path().join("relay");
            let (registry, _saf) = mounted_registry(
                FakeSaf::new(true, base.path().join("staging")).with_standard_tree(),
                &relay,
            )
            .await;
            let data = web::Data::new(registry.clone());
            let path = || web::Path::from(("com.test".to_string(), "files".to_string()));

            // 挂载根：SAF 树根以别名「Download」作为顶层条目
            let resp = list_dir(
                data.clone(),
                path(),
                web::Query::from_query("").unwrap(),
            )
            .await;
            assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
            let body: ListResponse = serde_json::from_slice(&body_bytes(resp).await).unwrap();
            assert_eq!(body.entries.len(), 1);
            assert_eq!(body.entries[0].name, "Download");
            assert!(body.entries[0].is_dir);
            assert!(body.notice.is_none(), "SAF 列表不触发 needs_all_files_access notice");

            // 树根：sub（目录）+ a.txt（11 字节）
            let resp = list_dir(
                data.clone(),
                path(),
                web::Query::from_query("path=Download").unwrap(),
            )
            .await;
            assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
            let body: ListResponse = serde_json::from_slice(&body_bytes(resp).await).unwrap();
            assert_eq!(body.entries.len(), 2);
            assert!(body.entries[0].is_dir);
            assert_eq!(body.entries[0].name, "sub");
            assert!(!body.entries[1].is_dir);
            assert_eq!(body.entries[1].name, "a.txt");
            assert_eq!(body.entries[1].size, 11);

            // 子目录：b.bin
            let resp = list_dir(
                data.clone(),
                path(),
                web::Query::from_query("path=Download/sub").unwrap(),
            )
            .await;
            assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
            let body: ListResponse = serde_json::from_slice(&body_bytes(resp).await).unwrap();
            assert_eq!(body.entries.len(), 1);
            assert_eq!(body.entries[0].name, "b.bin");

            // 缺失路径 → 404
            let resp = list_dir(
                data.clone(),
                path(),
                web::Query::from_query("path=Download/missing").unwrap(),
            )
            .await;
            assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);

            // 对文件路径执行列表 → 404（不是目录）
            let resp = list_dir(
                data,
                path(),
                web::Query::from_query("path=Download/a.txt").unwrap(),
            )
            .await;
            assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
        });
    }

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

    // ==================== 纯逻辑补充测试 ====================

    #[test]
    fn needs_all_files_access_classifies_paths() {
        // 主存储（/storage/emulated/0）下除 Android/data 私有目录外均需授权
        assert!(needs_all_files_access(Path::new("/storage/emulated/0/DCIM")));
        assert!(needs_all_files_access(Path::new("/storage/emulated/0")));
        // 尾斜杠归一化后仍判定主存储
        assert!(needs_all_files_access(Path::new("/storage/emulated/0/")));
        // App 私有目录（Android/data）无需授权
        assert!(!needs_all_files_access(Path::new(
            "/storage/emulated/0/Android/data/com.bedcode.mobile"
        )));
        // 小写变体同样识别（路径统一转小写判定）
        assert!(!needs_all_files_access(Path::new(
            "/storage/emulated/0/android/data"
        )));
        // Android/obb 非 data 子目录，FUSE 同样过滤
        assert!(needs_all_files_access(Path::new(
            "/storage/emulated/0/Android/obb"
        )));
        // 非主存储位置不判定（避免误报）：外部 SD 卡、App 私有 /data 目录
        assert!(!needs_all_files_access(Path::new("/sdcard/foo")));
        assert!(!needs_all_files_access(Path::new(
            "/data/user/0/com.bedcode.mobile/files"
        )));
        // 反斜杠分隔（异常输入防御）统一后仍命中主存储
        assert!(needs_all_files_access(Path::new(
            "/storage/emulated/0\\DCIM"
        )));
    }

    #[test]
    fn read_dir_entries_sorts_dirs_first_and_filters_part_files() {
        // 排序规则：目录优先 → 名称升序；*.part 上传临时文件对端不可见
        let base = tempfile::tempdir().unwrap();
        std::fs::create_dir(base.path().join("dir_z")).unwrap();
        std::fs::create_dir(base.path().join("dir_a")).unwrap();
        std::fs::write(base.path().join("b.txt"), b"hello").unwrap();
        std::fs::write(base.path().join("z.bin"), b"zz").unwrap();
        std::fs::write(base.path().join("pending.part"), b"tmp").unwrap();

        let entries = read_dir_entries(base.path()).expect("目录读取应成功");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // .part 被过滤；目录（按名）在前，文件（按名）在后
        assert_eq!(names, vec!["dir_a", "dir_z", "b.txt", "z.bin"]);
        assert!(entries[0].is_dir && entries[1].is_dir);
        assert!(!entries[2].is_dir && !entries[3].is_dir);
        // 文件大小与目录占位值
        assert_eq!(entries[2].size, 5);
        assert_eq!(entries[0].size, 0);
        // 真实目录 mtime 非 0（Unix 秒）
        assert!(entries[0].mtime > 0);
    }

    #[test]
    fn read_dir_entries_rejects_non_directory() {
        // 根失效/路径不是目录时返回明确 NotFound（规格 4.3 第 4 条）
        let base = tempfile::tempdir().unwrap();
        let file = base.path().join("a.txt");
        std::fs::write(&file, b"x").unwrap();
        let err = read_dir_entries(&file).expect_err("非目录应报错");
        assert!(matches!(err, crate::AppError::NotFound(_)));
        // 不存在的路径同样 NotFound
        let missing = base.path().join("missing");
        assert!(matches!(
            read_dir_entries(&missing),
            Err(crate::AppError::NotFound(_))
        ));
    }

    #[test]
    fn build_download_response_range_returns_206_with_headers() {
        // Range 命中 → 206 + Content-Range + Accept-Ranges + 段长度
        let stream = futures_util::stream::empty::<Result<web::Bytes, std::io::Error>>();
        let resp = build_download_response(Some((10, Some(19))), 10, 19, 100, 10, stream);
        assert_eq!(resp.status(), actix_web::http::StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            resp.headers().get(actix_web::http::header::CONTENT_RANGE).unwrap(),
            "bytes 10-19/100"
        );
        assert_eq!(
            resp.headers().get(actix_web::http::header::CONTENT_LENGTH).unwrap(),
            "10"
        );
        assert_eq!(
            resp.headers().get(actix_web::http::header::ACCEPT_RANGES).unwrap(),
            "bytes"
        );
        assert_eq!(
            resp.headers().get(actix_web::http::header::CONTENT_TYPE).unwrap(),
            "application/octet-stream"
        );
    }

    #[test]
    fn build_download_response_full_returns_200_without_content_range() {
        // 无 Range（或解析失败）→ 200 全量，不带 Content-Range
        let stream = futures_util::stream::empty::<Result<web::Bytes, std::io::Error>>();
        let resp = build_download_response(None, 0, 99, 100, 100, stream);
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        assert!(resp.headers().get(actix_web::http::header::CONTENT_RANGE).is_none());
        assert_eq!(
            resp.headers().get(actix_web::http::header::CONTENT_LENGTH).unwrap(),
            "100"
        );
    }

    #[tokio::test]
    async fn build_download_stream_serves_exact_bytes_and_stops_at_early_eof() {
        use futures_util::StreamExt;
        let base = tempfile::tempdir().unwrap();
        let file_path = base.path().join("payload.bin");
        std::fs::write(&file_path, b"0123456789").unwrap();

        // content_len 与实际一致：单块输出全文（512KB 缓冲 > 10 字节）
        let file = tokio::fs::File::open(&file_path).await.unwrap();
        let stream = build_download_stream(file, 10, Arc::new(PassthroughCipher), None);
        let chunks = collect_stream(stream).await;
        assert_eq!(chunks.len(), 1);
        assert_eq!(&chunks[0][..], b"0123456789");

        // content_len 超过实际文件（对端声称更大、文件被截断）：
        // EOF 提前到达 → 流终止，绝不发送缺失字节
        let file = tokio::fs::File::open(&file_path).await.unwrap();
        let stream = build_download_stream(file, 100, Arc::new(PassthroughCipher), None);
        let chunks = collect_stream(stream).await;
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, 10);
    }

    #[tokio::test]
    async fn build_download_stream_splits_at_chunk_boundary() {
        use futures_util::StreamExt;
        let base = tempfile::tempdir().unwrap();
        let file_path = base.path().join("big.bin");
        let data = vec![0xABu8; DOWNLOAD_CHUNK_SIZE + 100];
        std::fs::write(&file_path, &data).unwrap();

        let file = tokio::fs::File::open(&file_path).await.unwrap();
        let stream = build_download_stream(
            file,
            data.len() as u64,
            Arc::new(PassthroughCipher),
            None,
        );
        let chunks = collect_stream(stream).await;
        // 首块 512KB、末块余量，切分边界精确
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), DOWNLOAD_CHUNK_SIZE);
        assert_eq!(chunks[1].len(), 100);
        assert_eq!(chunks[0][0], 0xAB);
        assert_eq!(chunks[1][99], 0xAB);
    }

    #[tokio::test]
    async fn build_download_stream_with_relay_cleanup_completes() {
        // SAF 下载路径携带中转副本清理钩子：流正常结束不 panic，数据完整
        use futures_util::StreamExt;
        let base = tempfile::tempdir().unwrap();
        let relay = base.path().join("relay");
        let cache = relay.join("cache.bin");
        std::fs::create_dir_all(&relay).unwrap();
        std::fs::write(&cache, b"relay-data").unwrap();

        let file = tokio::fs::File::open(&cache).await.unwrap();
        let stream = build_download_stream(
            file,
            10,
            Arc::new(PassthroughCipher),
            Some((relay.clone(), cache.clone())),
        );
        let chunks = collect_stream(stream).await;
        assert_eq!(&chunks[0][..], b"relay-data");
    }

    #[test]
    fn require_op_enforces_mount_operations() {
        // 已声明操作放行；未声明一律 403（含响应体 code）
        let entry = saf_mount_entry();
        assert!(require_op(&entry, FileOperation::List).is_ok());
        assert!(require_op(&entry, FileOperation::Download).is_ok());
        assert!(require_op(&entry, FileOperation::Upload).is_ok());

        let mut restricted = saf_mount_entry();
        restricted.operations = vec![FileOperation::List];
        let err = require_op(&restricted, FileOperation::Download).expect_err("未声明操作应 403");
        assert_eq!(err.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    // ==================== v2 批端点（transfer-request / upload gating） ====================

    /// 端点级批测试注册表：预置挂载（None 钩子）+ 下载目录，可注入批记录
    async fn batch_test_registry() -> Arc<FileServiceRegistry> {
        let registry = FileServiceRegistry::with_saf_io(crate::plugin::saf_io::default_saf_io());
        // 沙箱 starts_with 比较区分大小写：根必须 canonicalize（Windows Temp 的
        // 用户目录大小写可能与 canonical 不同，非 canonical 根会误判逃逸）
        let base = std::env::temp_dir().canonicalize().unwrap();
        let downloads = base.join(format!("ft-batch-test-{}", uuid::Uuid::new_v4()));
        // 沙箱解析要求父目录存在：预创建下载目录
        std::fs::create_dir_all(&downloads).unwrap();
        registry.set_downloads_dir(downloads).await;
        registry
            .insert_entry_for_test(MountEntry {
                plugin_id: "p1".to_string(),
                mount_path: "files".to_string(),
                roots: vec![],
                saf_roots: vec![],
                operations: vec![FileOperation::List, FileOperation::Download, FileOperation::Upload],
                hook: HookTarget::None,
                cipher: Arc::new(PassthroughCipher),
            })
            .await;
        registry
    }

    #[tokio::test]
    async fn transfer_request_endpoint_none_hook_returns_403() {
        // POST /transfer-request：无钩子挂载 → fail-closed 403（不建批）
        let registry = batch_test_registry().await;
        let body = web::Json(TransferRequestHttpDto {
            batch_id: "b1".to_string(),
            files: vec![bedcode_plugin_api_mobile::UploadRequestMeta {
                relative_path: "a.txt".to_string(),
                size: 10,
            }],
            total_size: 10,
        });
        let resp = create_transfer_request(
            web::Data::new(registry.clone()),
            web::Path::from(("p1".to_string(), "files".to_string())),
            body,
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
        let text = String::from_utf8_lossy(&body_bytes(resp).await).to_string();
        assert!(text.contains("no upload hook"));
        // 拒绝不建批（批表仍空：钩子 deny 路径不建记录）
        assert!(registry.mount_count().await == 1);
    }

    #[tokio::test]
    async fn create_upload_with_approved_batch_skips_hook_and_creates_session() {
        // 带 batchId 且批已批准：免钩子直接建 session（200 + sessionId）
        let registry = batch_test_registry().await;
        registry
            .insert_batch_for_test(crate::file_service::transfer::TransferBatch {
                batch_id: "b-approved".to_string(),
                plugin_id: "p1".to_string(),
                mount_path: "files".to_string(),
                files: vec![],
                total_size: 0,
                state: crate::file_service::transfer::BatchState::Approved,
                created_at: std::time::Instant::now(),
                last_active: std::time::Instant::now(),
                approval_timeout: std::time::Duration::from_secs(60),
            })
            .await;
        let resp = create_upload(
            web::Data::new(registry.clone()),
            web::Path::from(("p1".to_string(), "files".to_string())),
            web::Json(CreateUploadRequest {
                relative_path: "a.txt".to_string(),
                size: 10,
                batch_id: Some("b-approved".to_string()),
            }),
        )
        .await;
        let status = resp.status();
        let body = String::from_utf8_lossy(&body_bytes(resp).await).to_string();
        assert_eq!(
            status,
            actix_web::http::StatusCode::OK,
            "response body: {}",
            body
        );
        assert!(body.contains("sessionId"));
    }

    #[tokio::test]
    async fn create_upload_with_pending_batch_returns_403() {
        // ask 模式防绕过：批 pending → 403 batch-not-approved
        let registry = batch_test_registry().await;
        registry
            .insert_batch_for_test(crate::file_service::transfer::TransferBatch {
                batch_id: "b-pending".to_string(),
                plugin_id: "p1".to_string(),
                mount_path: "files".to_string(),
                files: vec![],
                total_size: 0,
                state: crate::file_service::transfer::BatchState::Pending,
                created_at: std::time::Instant::now(),
                last_active: std::time::Instant::now(),
                approval_timeout: std::time::Duration::from_secs(60),
            })
            .await;
        let resp = create_upload(
            web::Data::new(registry),
            web::Path::from(("p1".to_string(), "files".to_string())),
            web::Json(CreateUploadRequest {
                relative_path: "a.txt".to_string(),
                size: 10,
                batch_id: Some("b-pending".to_string()),
            }),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
        let text = String::from_utf8_lossy(&body_bytes(resp).await).to_string();
        assert!(text.contains("batch-not-approved"));
    }

    #[tokio::test]
    async fn create_upload_with_unknown_batch_returns_403() {
        // 批不存在（含他插件批）→ 403 batch-not-found（不泄露存在性）
        let registry = batch_test_registry().await;
        let resp = create_upload(
            web::Data::new(registry),
            web::Path::from(("p1".to_string(), "files".to_string())),
            web::Json(CreateUploadRequest {
                relative_path: "a.txt".to_string(),
                size: 10,
                batch_id: Some("ghost".to_string()),
            }),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
        let text = String::from_utf8_lossy(&body_bytes(resp).await).to_string();
        assert!(text.contains("batch-not-found"));
    }

    #[tokio::test]
    async fn create_upload_without_batch_none_hook_returns_403() {
        // 无 batchId + 无钩子挂载 → per-file 钩子 deny → 403（v1 fail-closed）
        let registry = batch_test_registry().await;
        let resp = create_upload(
            web::Data::new(registry),
            web::Path::from(("p1".to_string(), "files".to_string())),
            web::Json(CreateUploadRequest {
                relative_path: "a.txt".to_string(),
                size: 10,
                batch_id: None,
            }),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn error_response_serializes_code_and_message() {
        // 统一错误响应形状：HTTP 状态码 + JSON {code, message}
        let resp = error_response(actix_web::http::StatusCode::BAD_REQUEST, 400, "bad input");
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
        let body: serde_json::Value = serde_json::from_slice(&body_bytes(resp).await).unwrap();
        assert_eq!(body["code"], 400);
        assert_eq!(body["message"], "bad input");
    }

    #[test]
    fn mtime_unix_secs_reports_file_mtime() {
        // 真实文件 mtime 应为非零 Unix 秒（读取失败才返回 0）
        let base = tempfile::tempdir().unwrap();
        let file = base.path().join("m.txt");
        std::fs::write(&file, b"x").unwrap();
        let meta = std::fs::metadata(&file).unwrap();
        assert!(mtime_unix_secs(&meta) > 0);
    }
}

