//! 插件文件服务 HTTP 端点（规格 4.4 节）
//!
//! 挂载在 /api/plugins/{plugin_id}/{mount}/**（注意是**复数** plugins，
//! 与单数 /api/plugin 的动态端点代理区分），自动经过 /api scope 的 JWT
//! 中间件 —— 移动端持已认证的 JWT 访问，零额外认证开发（规格 4.5）。
//!
//! 端点：
//! - GET  {mount}/list?path=                 目录列举（沙箱校验）
//! - GET  {mount}/file?path=                 下载（NamedFile 原生 Range/206）
//! - HEAD {mount}/file?path=                 size+mtime 指纹（续传有效性比对）
//! - POST {mount}/upload                     创建 upload session（策略钩子 fail-closed）
//! - PUT  {mount}/upload/{sid}               流式 append（web::Payload，绕开 Json extractor 限制）
//! - GET  {mount}/upload/{sid}               查询已收字节（续传握手）
//! - POST {mount}/upload/{sid}/complete      原子 rename 落位
//! - DELETE {mount}/upload/{sid}             取消清理
//!
//! 大文件 PUT 与超时：HttpServer 的 client_request_timeout 仅约束首个请求
//! **头部**的读取（actix-http 在请求头解析完成后即清除 head timer），
//! 对流式 body 无影响，无需为大文件调整 NetworkConfig。
//! 服务面无删除/改名/移动/覆盖端点（规格 8 节）。

use crate::plugin::file_service::upload::UploadSessionError;
use crate::plugin::file_service::{sandbox, MountEntry};
use crate::server::dtos::ApiResponse;
use crate::system::app_context::AppContext;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use bedcode_plugin_api::FileOperation;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// PUT append 累积缓冲下限（规格：256KB–1MB）
const APPEND_FLUSH_THRESHOLD: usize = 512 * 1024;
/// 单个 payload chunk 的大小上限（超过立即 flush，防大 chunk 撑爆缓冲）
const APPEND_MAX_CHUNK: usize = 1024 * 1024;

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
}

/// 创建上传会话请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateUploadRequest {
    /// 目标相对路径（相对挂载根）
    pub relative_path: String,
    /// 声明的文件总大小（字节）
    pub size: u64,
    /// v2：所属传输批 ID（ask 模式批准后免钩子创建；缺省走 v1 per-file 钩子）
    #[serde(default)]
    pub batch_id: Option<String>,
}

/// 批量传输请求体（v2，POST /transfer-request）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequestBody {
    /// 批 ID（发送方生成，UUID）
    pub batch_id: String,
    /// 批内文件清单
    pub files: Vec<bedcode_plugin_api::UploadRequestMeta>,
    /// 批总大小（字节）
    pub total_size: u64,
}

/// 批量传输请求响应（v2：200 approved / 202 pending）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequestResponse {
    /// 批 ID
    pub batch_id: String,
    /// 批状态："approved" | "pending"
    pub decision: String,
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

/// 上传进度查询
#[derive(Debug, Deserialize)]
pub struct PathOnlyQuery {
    /// 相对挂载点的路径（空 = 挂载根）
    #[serde(default)]
    pub path: String,
}

// ==================== Helpers ====================

/// 获取文件服务注册表（未初始化 → 503）
fn registry() -> Option<Arc<crate::plugin::file_service::FileServiceRegistry>> {
    AppContext::try_global().map(|ctx| ctx.file_service().clone())
}

/// 统一错误响应：HTTP 状态码 + ApiResponse 风格 body
fn error_response(status: actix_web::http::StatusCode, code: u16, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(ApiResponse::<()>::error(code, message))
}

/// 获取挂载条目（不存在 → 404）
async fn get_mount(
    plugin_id: &str,
    mount: &str,
) -> Result<MountEntry, HttpResponse> {
    let Some(registry) = registry() else {
        return Err(error_response(
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            503,
            "file service not initialized",
        ));
    };
    registry.get_entry(plugin_id, mount).await.map_err(|e| {
        error_response(
            actix_web::http::StatusCode::NOT_FOUND,
            404,
            &e.to_string(),
        )
    })
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
        if crate::plugin::file_service::upload::is_filtered_listing_name(&name) {
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

// ==================== Handlers ====================

/// GET {mount}/list?path= — 目录列举
///
/// path 为空时列举挂载根（多 root 时每个 root 作为顶层条目，
/// 名称取 root 最后一段；失效的 root 跳过并告警）
pub async fn list_dir(
    params: web::Path<(String, String)>,
    query: web::Query<PathOnlyQuery>,
) -> HttpResponse {
    let (plugin_id, mount) = params.into_inner();

    let entry = match get_mount(&plugin_id, &mount).await {
        Ok(e) => e,
        Err(resp) => return resp,
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
        return HttpResponse::Ok().json(ApiResponse::ok_with_data(ListResponse {
            path: String::new(),
            entries,
        }));
    }

    let registry = match registry() {
        Some(r) => r,
        None => {
            return error_response(
                actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
                503,
                "file service not initialized",
            )
        }
    };

    let target = match registry.resolve_sandboxed(&plugin_id, &mount, &rel).await {
        Ok(p) => p,
        Err(e) => {
            return error_response(
                actix_web::http::StatusCode::NOT_FOUND,
                404,
                &e.to_string(),
            )
        }
    };

    match tokio::task::spawn_blocking(move || read_dir_entries(&target)).await {
        Ok(Ok(entries)) => HttpResponse::Ok().json(ApiResponse::ok_with_data(ListResponse {
            path: rel,
            entries,
        })),
        Ok(Err(e)) => {
            let code = if matches!(e, crate::AppError::NotFound(_)) {
                404
            } else {
                500
            };
            error_response(
                actix_web::http::StatusCode::from_u16(code).unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
                code,
                &e.to_string(),
            )
        }
        Err(e) => error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            500,
            &format!("list task failed: {}", e),
        ),
    }
}

/// GET {mount}/file?path= — 下载（actix-files NamedFile 原生 Range/206）
///
/// 加密缝说明：NamedFile 直通文件字节，未来接入 TransportCipher 时
/// 此端点需替换为经 cipher.encrypt_chunk 包装的流式 body
pub async fn download_file(
    req: HttpRequest,
    params: web::Path<(String, String)>,
    query: web::Query<PathOnlyQuery>,
) -> HttpResponse {
    let (plugin_id, mount) = params.into_inner();

    let entry = match get_mount(&plugin_id, &mount).await {
        Ok(e) => e,
        Err(resp) => return resp,
    };
    if let Err(resp) = require_op(&entry, FileOperation::Download) {
        return resp;
    }

    let rel = query.path.trim_matches('/').to_string();
    let Some(registry) = registry() else {
        return error_response(
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            503,
            "file service not initialized",
        );
    };

    let target = match registry.resolve_sandboxed(&plugin_id, &mount, &rel).await {
        Ok(p) => p,
        Err(e) => {
            return error_response(
                actix_web::http::StatusCode::NOT_FOUND,
                404,
                &e.to_string(),
            )
        }
    };
    if !target.is_file() {
        return error_response(
            actix_web::http::StatusCode::NOT_FOUND,
            404,
            "not a file",
        );
    }

    match actix_files::NamedFile::open(&target) {
        Ok(named_file) => named_file.respond_to(&req),
        Err(e) => error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            500,
            &format!("failed to open file '{}': {}", target.display(), e),
        ),
    }
}

/// HEAD {mount}/file?path= — 返回 size+mtime 指纹（续传有效性比对，规格 7.4）
pub async fn head_file(
    params: web::Path<(String, String)>,
    query: web::Query<PathOnlyQuery>,
) -> HttpResponse {
    let (plugin_id, mount) = params.into_inner();

    let entry = match get_mount(&plugin_id, &mount).await {
        Ok(e) => e,
        Err(resp) => return resp,
    };
    if let Err(resp) = require_op(&entry, FileOperation::Download) {
        return resp;
    }

    let rel = query.path.trim_matches('/').to_string();
    let Some(registry) = registry() else {
        return error_response(
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            503,
            "file service not initialized",
        );
    };

    let target = match registry.resolve_sandboxed(&plugin_id, &mount, &rel).await {
        Ok(p) => p,
        Err(e) => {
            return error_response(
                actix_web::http::StatusCode::NOT_FOUND,
                404,
                &e.to_string(),
            )
        }
    };

    let meta = match std::fs::metadata(&target) {
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

/// POST {mount}/upload — 创建 upload session
///
/// 流程（规格 4.2/4.4）：沙箱解析目标 → 策略钩子（2s fail-closed，
/// 拒绝发生在写任何字节前）→ 创建 session 返回 {sessionId, received:0}
pub async fn create_upload(
    params: web::Path<(String, String)>,
    body: web::Json<CreateUploadRequest>,
) -> HttpResponse {
    let (plugin_id, mount) = params.into_inner();

    let entry = match get_mount(&plugin_id, &mount).await {
        Ok(e) => e,
        Err(resp) => return resp,
    };
    if let Err(resp) = require_op(&entry, FileOperation::Upload) {
        return resp;
    }

    let Some(registry) = registry() else {
        return error_response(
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            503,
            "file service not initialized",
        );
    };

    // 沙箱解析：接收落点优先使用 downloads_dir（spec 方向模型：接收上传
    // 不落共享 roots，专设下载目录，与移动端 MediaStore.Downloads 对称）。
    // 旧插件未传 downloads_dir 时回退到 roots 沙箱语义保后兼容。
    //
    // downloads_dir 为插件 resolve_download_dir 给出的绝对路径（用户设置或
    // HomeDir/Downloads）。复用 roots 沙箱解析器：canonicalize 父目录 +
    // starts_with 校验可拦截 downloads_dir 内 symlink/junction 指向外部的
    // 逃逸（Windows 用户态无需管理员即可建 junction），父目录必须存在
    // （不存在即 400 拒绝，避免创建 session 后流式写入中途才失败）。
    // 单根时首段等于基名的别名剥除语义与共享 roots 一致，无害。
    let rel = body.relative_path.trim_matches('/').to_string();
    let target = if let Some(ref downloads_dir) = entry.downloads_dir {
        let canonical_dir = match std::fs::canonicalize(downloads_dir) {
            Ok(p) => p,
            Err(e) => {
                return error_response(
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    &format!(
                        "downloads_dir '{}' not accessible: {}",
                        downloads_dir.display(),
                        e
                    ),
                )
            }
        };
        if !canonical_dir.is_dir() {
            return error_response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                500,
                &format!("downloads_dir '{}' is not a directory", canonical_dir.display()),
            );
        }
        match sandbox::resolve_upload_target_within_roots(&[canonical_dir], &rel) {
            Ok(p) => p,
            Err(e) => {
                return error_response(
                    actix_web::http::StatusCode::BAD_REQUEST,
                    400,
                    &e.to_string(),
                )
            }
        }
    } else {
        match sandbox::resolve_upload_target_within_roots(&entry.roots, &rel) {
            Ok(p) => p,
            Err(e) => {
                return error_response(
                    actix_web::http::StatusCode::BAD_REQUEST,
                    400,
                    &e.to_string(),
                )
            }
        }
    };

    // 策略钩子：同名即拒等策略由插件实现；超时/异常 fail-closed
    //
    // v2 批 gating（spec 14.2，顺序：沙箱解析 → gating → 建 session）：
    // 1. 带 batchId → 批已批准则免钩子（批准状态随批保留）；
    //    批 pending/rejected/不存在一律 403（防 ask 模式绕过批上下文）
    // 2. 无 batchId → 走 v1 per-file 钩子；钩子 ask → 403 batch-context-required（fail-closed）
    let meta = bedcode_plugin_api::UploadRequestMeta {
        relative_path: rel.clone(),
        size: body.size,
    };
    let batch_id = body.batch_id.clone();
    let batch_approved = if let Some(ref batch_id) = batch_id {
        match registry.check_batch(&plugin_id, &mount, batch_id).await {
            Ok(_) => true,
            Err(e) => {
                // GatingDenied 消息即 wire 值（batch-not-approved 等），发送方据此区分
                return error_response(actix_web::http::StatusCode::FORBIDDEN, 403, &e.to_string());
            }
        }
    } else {
        false
    };

    if !batch_approved {
        let decision = registry.call_upload_hook(&plugin_id, &mount, &meta).await;
        if !decision.allow {
            // v2：ask 模式下无批上下文的上传一律拒绝（防绕过 /upload）
            if decision.ask {
                return error_response(
                    actix_web::http::StatusCode::FORBIDDEN,
                    403,
                    "batch-context-required",
                );
            }
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
    }

    match registry
        .upload_sessions()
        .create(&plugin_id, &mount, target, body.size)
        .await
    {
        Ok(session) => {
            // v2：session 创建成功（钩子路径与批路径都发）→ 接收端「正在接收」任务
            let session_id = session.id.clone();
            registry
                .emit_filesrv_event(
                    "filesrv:receiving_started",
                    serde_json::json!({
                        "sessionId": session_id,
                        // 无批（accept/reject 策略路径）为 null，与移动端 wire 一致
                        "batchId": batch_id,
                        "relativePath": rel,
                        "size": body.size,
                    }),
                )
                .await;
            // 批内 session 活动刷新（approved 批 24h TTL 依据）
            if let Some(ref batch_id) = batch_id {
                registry.touch_batch(batch_id).await;
            }
            HttpResponse::Ok().json(ApiResponse::ok_with_data(UploadSessionResponse {
                session_id: session.id,
                received: 0,
            }))
        }
        Err(e) => error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            500,
            &e.to_string(),
        ),
    }
}

/// POST {mount}/transfer-request — 批量传输请求（v2，spec 2.1）
///
/// 批钩子三路分流：allow → 200 {batchId, decision:"approved"}；
/// ask → 批 pending + 本地事件 filesrv:transfer_request → 202 {decision:"pending"}；
/// deny → 403（message 为钩子 reason，如 policy-denied）。
/// 钩子 fail-closed：超时/插件异常/挂载不存在一律 deny（2s 超时，UPLOAD_HOOK_TIMEOUT）。
/// 沙箱不需要（仅元数据，不落盘）；不建任务不写记录（deny 零打扰）。
pub async fn transfer_request(
    params: web::Path<(String, String)>,
    body: web::Json<TransferRequestBody>,
) -> HttpResponse {
    let (plugin_id, mount) = params.into_inner();

    let entry = match get_mount(&plugin_id, &mount).await {
        Ok(e) => e,
        Err(resp) => return resp,
    };
    if let Err(resp) = require_op(&entry, FileOperation::Upload) {
        return resp;
    }

    let Some(registry) = registry() else {
        return error_response(
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            503,
            "file service not initialized",
        );
    };

    let dto = crate::plugin::file_service::transfer::TransferRequestDto {
        batch_id: body.batch_id.clone(),
        files: body.files.clone(),
        total_size: body.total_size,
    };
    match registry
        .create_transfer_request(&plugin_id, &mount, &dto)
        .await
    {
        Ok(crate::plugin::file_service::transfer::BatchDecision::Approved) => {
            HttpResponse::Ok().json(ApiResponse::ok_with_data(TransferRequestResponse {
                batch_id: dto.batch_id,
                decision: "approved".to_string(),
            }))
        }
        Ok(crate::plugin::file_service::transfer::BatchDecision::Pending) => {
            // 202：批已建 pending，等待接收端用户应答（异步批准协议）
            HttpResponse::Accepted().json(ApiResponse::ok_with_data(TransferRequestResponse {
                batch_id: dto.batch_id,
                decision: "pending".to_string(),
            }))
        }
        Err(e) => match e {
            crate::plugin::file_service::transfer::BatchError::PolicyDenied(reason) => {
                error_response(actix_web::http::StatusCode::FORBIDDEN, 403, &reason)
            }
            crate::plugin::file_service::transfer::BatchError::GatingDenied(msg) => {
                error_response(actix_web::http::StatusCode::FORBIDDEN, 403, &msg)
            }
            other => error_response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                500,
                &other.to_string(),
            ),
        },
    }
}

/// PUT {mount}/upload/{sid} — 从 web::Payload 流式 append
///
/// 不走 Json extractor（无大小上限）；按 512KB 缓冲累积后写入，
/// offset 与服务端已收不一致时返回 409（客户端应先 GET 查询续传点）
pub async fn append_upload(
    params: web::Path<(String, String, String)>,
    mut payload: web::Payload,
) -> HttpResponse {
    let (plugin_id, mount, sid) = params.into_inner();

    let Some(registry) = registry() else {
        return error_response(
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            503,
            "file service not initialized",
        );
    };
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
            return error_response(
                actix_web::http::StatusCode::NOT_FOUND,
                404,
                &e.to_string(),
            )
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
        Ok(()) => HttpResponse::Ok().json(ApiResponse::ok_with_data(UploadSessionResponse {
            session_id: sid,
            received: offset,
        })),
        Err(UploadSessionError::OffsetMismatch { expected, got }) => error_response(
            actix_web::http::StatusCode::CONFLICT,
            409,
            &format!("offset mismatch: server has {} bytes, client sent offset {}", expected, got),
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

/// GET {mount}/upload/{sid} — 查询 session 状态（已收字节，续传握手）
pub async fn query_upload(params: web::Path<(String, String, String)>) -> HttpResponse {
    let (plugin_id, mount, sid) = params.into_inner();
    let Some(registry) = registry() else {
        return error_response(
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            503,
            "file service not initialized",
        );
    };

    match registry.upload_sessions().get(&sid, &plugin_id, &mount).await {
        Some(session) => HttpResponse::Ok().json(ApiResponse::ok_with_data(UploadSessionResponse {
            session_id: session.id,
            received: session.received,
        })),
        None => error_response(
            actix_web::http::StatusCode::NOT_FOUND,
            404,
            "upload session not found",
        ),
    }
}

/// POST {mount}/upload/{sid}/complete — 原子 rename 落位
///
/// 目标已存在 → 409 duplicate-name（保留 .part，规格 7.4）
pub async fn complete_upload(params: web::Path<(String, String, String)>) -> HttpResponse {
    let (plugin_id, mount, sid) = params.into_inner();
    let Some(registry) = registry() else {
        return error_response(
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            503,
            "file service not initialized",
        );
    };

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
            // v2：接收端任务终态 + 归档（complete 成功）
            registry
                .emit_filesrv_event(
                    "filesrv:receiving_done",
                    serde_json::json!({
                        "sessionId": sid,
                        "state": "completed",
                        "reason": null,
                    }),
                )
                .await;
            HttpResponse::Ok().json(ApiResponse::ok())
        }
        Err(UploadSessionError::DuplicateName(_)) => {
            // v2：complete 409 竞态 → 该文件 receiving_done(failed, duplicate-name)，
            // 批内其他文件不受影响（spec 14.2 边界 5）
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
            error_response(actix_web::http::StatusCode::CONFLICT, 409, "duplicate-name")
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

/// DELETE {mount}/upload/{sid} — 取消（清理临时文件）
pub async fn cancel_upload(params: web::Path<(String, String, String)>) -> HttpResponse {
    let (plugin_id, mount, sid) = params.into_inner();
    let Some(registry) = registry() else {
        return error_response(
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            503,
            "file service not initialized",
        );
    };

    match registry
        .upload_sessions()
        .cancel(&sid, &plugin_id, &mount)
        .await
    {
        Ok(()) => {
            // v2：接收端任务终态（取消：用户 / 发送方 DELETE 均走此端点）
            registry
                .emit_filesrv_event(
                    "filesrv:receiving_done",
                    serde_json::json!({
                        "sessionId": sid,
                        "state": "cancelled",
                        "reason": null,
                    }),
                )
                .await;
            HttpResponse::Ok().json(ApiResponse::ok())
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
