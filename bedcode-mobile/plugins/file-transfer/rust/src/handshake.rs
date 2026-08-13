//! 续传握手（经 http_fetch）
//!
//! 远端文件操作：目录列举、指纹获取、上传会话管理。
//! 所有请求经宿主 HTTP 代理，带 Authorization header。
//!
//! 对端文件服务端点（相对 base）：
//! - GET /list?path=… — 目录列举
//! - GET /file?path=… — 文件下载（Range）
//! - HEAD /file?path=… — 文件指纹（X-File-Size / X-File-Mtime）
//! - POST /upload — 创建 upload session
//! - GET /upload/{id} — 查询 session 已收字节
//! - POST /upload/{id}/complete — 完成上传
//! - DELETE /upload/{id} — 取消上传

use bedcode_plugin_api_mobile::host::{HostError, HostHttp};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 目录项（list 端点返回）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    /// 文件/目录名
    pub name: String,
    /// 文件大小（字节，目录为 0）
    #[serde(default)]
    pub size: u64,
    /// 修改时间（Unix 秒）
    #[serde(default)]
    pub mtime: u64,
    /// 是否为目录
    #[serde(default, rename = "isDir")]
    pub is_dir: bool,
}

/// 文件指纹（HEAD 响应或 list 降级）
#[derive(Debug, Clone)]
pub struct RemoteFingerprint {
    /// 文件大小（字节）
    pub size: u64,
    /// 修改时间（Unix 秒）
    pub mtime: u64,
}

/// 上传会话创建结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreated {
    /// 会话 ID
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// 已收字节（续传时为已传偏移，新上传为 0）
    #[serde(default)]
    pub received: u64,
}

/// HTTP 响应结构（host_http_fetch 非流式返回）
#[derive(Debug, Deserialize)]
struct HttpResponse {
    status: u64,
    #[serde(default)]
    body: String,
    #[serde(default)]
    headers: HashMap<String, String>,
}

// ==================== 目录列举 ====================

/// 目录列举结果（条目 + 可选的存储权限提示）
#[derive(Debug, Clone)]
pub struct RemoteListResult {
    /// 目录条目
    pub entries: Vec<DirEntry>,
    /// 非空时表示列表可能被对端存储权限过滤
    /// （移动端未授予「所有文件访问权限」时 read_dir 静默返回空），
    /// 透传给前端展示引导提示
    pub notice: Option<String>,
}

/// 列举远端目录
///
/// GET {base}/list?path={path}
pub fn list_remote(
    host: &impl HostHttp,
    base: &str,
    auth: &str,
    path: &str,
) -> Result<RemoteListResult, String> {
    let url = format!(
        "{}/list?path={}",
        base,
        urlencoded(path)
    );
    let resp = do_fetch(host, "GET", &url, auth, None)?;
    if resp.status != 200 {
        return Err(format!("list_remote: HTTP {}", resp.status));
    }
    let body: serde_json::Value = serde_json::from_str(&resp.body)
        .map_err(|e| format!("list_remote: parse body failed: {}", e))?;
    // 兼容两端 server 的响应形态差异：桌面端 ApiResponse 包装
    // （data.entries）、移动端 ListResponse（entries）、裸数组
    let entries = if let Some(entries) = unwrap_data(&body).get("entries") {
        entries
    } else {
        &body
    };
    let entries: Vec<DirEntry> = serde_json::from_value(entries.clone())
        .map_err(|e| format!("list_remote: parse entries failed: {}", e))?;
    let notice = unwrap_data(&body)
        .get("notice")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Ok(RemoteListResult { entries, notice })
}

// ==================== 文件指纹 ====================

/// 获取远端文件指纹（双路径实现，spec §7.4）
///
/// 优先 HEAD /file 读 X-File-Size/X-File-Mtime headers；
/// 不可用（headers 缺失或 HEAD 不支持）则降级 GET /list 父目录查找该条目。
pub fn fingerprint(
    host: &impl HostHttp,
    base: &str,
    auth: &str,
    path: &str,
) -> Result<RemoteFingerprint, String> {
    // 路径 1：HEAD 请求读 headers
    if let Ok(fp) = fingerprint_via_head(host, base, auth, path) {
        return Ok(fp);
    }
    // 路径 2：降级 list 父目录
    fingerprint_via_list(host, base, auth, path)
}

/// HEAD 方式获取指纹
fn fingerprint_via_head(
    host: &impl HostHttp,
    base: &str,
    auth: &str,
    path: &str,
) -> Result<RemoteFingerprint, String> {
    let url = format!("{}/file?path={}", base, urlencoded(path));
    let resp = do_fetch(host, "HEAD", &url, auth, None)?;
    if resp.status != 200 {
        return Err(format!("HEAD HTTP {}", resp.status));
    }
    let size = header_val(&resp.headers, "x-file-size")
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or_else(|| "missing X-File-Size header".to_string())?;
    let mtime = header_val(&resp.headers, "x-file-mtime")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    Ok(RemoteFingerprint { size, mtime })
}

/// list 降级方式获取指纹
fn fingerprint_via_list(
    host: &impl HostHttp,
    base: &str,
    auth: &str,
    path: &str,
) -> Result<RemoteFingerprint, String> {
    let (parent, file_name) = split_parent_name(path);
    let result = list_remote(host, base, auth, &parent)?;
    let entry = result
        .entries
        .iter()
        .find(|e| e.name == file_name)
        .ok_or_else(|| format!("file '{}' not found in list", path))?;
    Ok(RemoteFingerprint {
        size: entry.size,
        mtime: entry.mtime,
    })
}

// ==================== 上传会话 ====================

/// 创建上传会话
///
/// POST {base}/upload body={relativePath, size, batchId?}
/// 成功返回 SessionCreated；409 = 同名被拒；403 = 批 gating 拒绝/策略拒绝；其他 = 错误
/// v2：带 batch_id 时宿主走批 gating（已批准批免钩子）；不带时走 v1 per-file 钩子
pub fn create_session(
    host: &impl HostHttp,
    base: &str,
    auth: &str,
    relative_path: &str,
    size: u64,
    batch_id: Option<&str>,
) -> Result<SessionCreated, CreateSessionError> {
    let url = format!("{}/upload", base);
    let mut body = serde_json::json!({
        "relativePath": relative_path,
        "size": size,
    });
    if let Some(bid) = batch_id {
        body["batchId"] = serde_json::json!(bid);
    }
    let resp = do_fetch(host, "POST", &url, auth, Some(&body))
        .map_err(|e| CreateSessionError::Other(e))?;
    match resp.status {
        200 | 201 => {
            let body: serde_json::Value = serde_json::from_str(&resp.body)
                .map_err(|e| CreateSessionError::Other(format!("parse session response: {}", e)))?;
            serde_json::from_value(unwrap_data(&body).clone())
                .map_err(|e| CreateSessionError::Other(format!("parse session response: {}", e)))
        }
        409 => Err(CreateSessionError::DuplicateName),
        403 => Err(CreateSessionError::Other(format!(
            "create_session: HTTP 403 {}",
            resp.body.trim()
        ))),
        _ => Err(CreateSessionError::Other(format!(
            "create_session: HTTP {}",
            resp.status
        ))),
    }
}

/// 批量传输请求结果（v2，POST /transfer-request）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferRequestOutcome {
    /// 200：接收端钩子 allow，批直接批准，批内任务可调度
    Approved,
    /// 202：接收端钩子 ask，批进入 pending，批内任务等待对方同意
    Pending,
}

/// 批量传输请求错误（v2）
#[derive(Debug)]
pub enum TransferRequestError {
    /// 403：接收端策略拒绝（reason 如 policy-denied）——任务转 rejected(policy-denied)
    Denied(String),
    /// 网络错误/超时/非预期状态码——任务转 failed
    Network(String),
}

/// 发起批量传输请求（v2，批内首个任务启动时调用一次）
///
/// POST {base}/transfer-request body={batchId, files:[{relativePath,size}], totalSize}
/// 与桌面端 handshake.rs 同构：200 → Approved；202 → Pending；403 → Denied；其他 → Network
pub fn request_transfer(
    host: &impl HostHttp,
    base: &str,
    auth: &str,
    batch_id: &str,
    files: &[bedcode_plugin_api_mobile::UploadRequestMeta],
    total_size: u64,
) -> Result<TransferRequestOutcome, TransferRequestError> {
    let url = format!("{}/transfer-request", base);
    let body = serde_json::json!({
        "batchId": batch_id,
        "files": files,
        "totalSize": total_size,
    });
    let resp = do_fetch(host, "POST", &url, auth, Some(&body))
        .map_err(|e| TransferRequestError::Network(format!("request_transfer: {}", e)))?;
    match resp.status {
        200 => Ok(TransferRequestOutcome::Approved),
        202 => Ok(TransferRequestOutcome::Pending),
        403 => Err(TransferRequestError::Denied(
            // 宿主 403 为 { code, message } JSON（两端统一 error_response 形态）；
            // 提取 message 字段作为拒绝原因（policy-denied 等），兜底退回原文
            extract_error_message(&resp.body),
        )),
        other => Err(TransferRequestError::Network(format!(
            "request_transfer: HTTP {}",
            other
        ))),
    }
}

/// 从宿主错误响应体提取 message（{ code, message } 或裸字符串；可能被 JSON 转义）
fn extract_error_message(body: &str) -> String {
    let trimmed = body.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
            return msg.to_string();
        }
        if let Some(s) = v.as_str() {
            return s.to_string();
        }
    }
    trimmed.to_string()
}

/// 查询上传会话已收字节
///
/// GET {base}/upload/{session_id}
/// 404 = session 丢失（需重建）
pub fn query_session(
    host: &impl HostHttp,
    base: &str,
    auth: &str,
    session_id: &str,
) -> Result<u64, QuerySessionError> {
    let url = format!("{}/upload/{}", base, session_id);
    let resp = do_fetch(host, "GET", &url, auth, None)
        .map_err(|e| QuerySessionError::Other(e))?;
    match resp.status {
        200 => {
            let body: serde_json::Value = serde_json::from_str(&resp.body)
                .map_err(|e| QuerySessionError::Other(format!("parse: {}", e)))?;
            Ok(unwrap_data(&body)
                .get("received")
                .and_then(|v| v.as_u64())
                .unwrap_or(0))
        }
        404 => Err(QuerySessionError::SessionLost),
        _ => Err(QuerySessionError::Other(format!(
            "query_session: HTTP {}",
            resp.status
        ))),
    }
}

/// 完成上传会话
///
/// POST {base}/upload/{session_id}/complete
/// 409 = 目标同名（该文件 rejected(duplicate-name)，批内其他不受影响）
///
/// v2：批内文件同名沿用 v1 per-file 同名即拒（complete 409 → 该文件 rejected），
/// 错误类型化以便发送方把 409 与其他失败区分（§2.3 响应码语义）
pub fn complete_session(
    host: &impl HostHttp,
    base: &str,
    auth: &str,
    session_id: &str,
) -> Result<(), CompleteSessionError> {
    let url = format!("{}/upload/{}/complete", base, session_id);
    let resp = do_fetch(host, "POST", &url, auth, None)
        .map_err(|e| CompleteSessionError::Other(e))?;
    match resp.status {
        200 | 201 => Ok(()),
        409 => Err(CompleteSessionError::DuplicateName),
        other => Err(CompleteSessionError::Other(format!(
            "complete_session: HTTP {}",
            other
        ))),
    }
}

/// 完成上传会话错误（v2 类型化：409 与其他失败区分）
#[derive(Debug)]
pub enum CompleteSessionError {
    /// 目标同名（409）→ 该文件 rejected(duplicate-name)
    DuplicateName,
    /// 其他失败（网络/非预期状态码）
    Other(String),
}

/// 取消上传会话
///
/// DELETE {base}/upload/{session_id}
pub fn cancel_session(
    host: &impl HostHttp,
    base: &str,
    auth: &str,
    session_id: &str,
) -> Result<(), String> {
    let url = format!("{}/upload/{}", base, session_id);
    let resp = do_fetch(host, "DELETE", &url, auth, None)?;
    if resp.status >= 200 && resp.status < 300 || resp.status == 404 {
        Ok(())
    } else {
        Err(format!("cancel_session: HTTP {}", resp.status))
    }
}

// ==================== 错误类型 ====================

/// 创建上传会话错误
#[derive(Debug)]
pub enum CreateSessionError {
    /// 同名被拒（409）
    DuplicateName,
    /// 其他错误
    Other(String),
}

/// 查询上传会话错误
#[derive(Debug)]
pub enum QuerySessionError {
    /// session 丢失（404，需重建）
    SessionLost,
    /// 其他错误
    Other(String),
}

// ==================== 内部辅助 ====================

/// 提取响应体中的业务数据（兼容两端 server 的响应包装差异）：
/// - 桌面端统一 ApiResponse 包装：`{ code, message, data: {...} }`
/// - 移动端直接返回 DTO：`{ path, entries }` / `{ sessionId, received }`
///
/// 对象含非 null `data` 字段时返回 `data` 引用，否则返回原对象
fn unwrap_data<'a>(body: &'a serde_json::Value) -> &'a serde_json::Value {
    match body.get("data") {
        Some(v) if !v.is_null() => v,
        _ => body,
    }
}

/// 执行 HTTP 请求
fn do_fetch(
    host: &impl HostHttp,
    method: &str,
    url: &str,
    auth: &str,
    body: Option<&serde_json::Value>,
) -> Result<HttpResponse, String> {
    let mut headers = serde_json::Map::new();
    if !auth.is_empty() {
        headers.insert(
            "Authorization".to_string(),
            serde_json::Value::String(format!("Bearer {}", auth)),
        );
    }
    headers.insert(
        "Content-Type".to_string(),
        serde_json::Value::String("application/json".to_string()),
    );

    let mut req = serde_json::Map::new();
    req.insert("method".to_string(), serde_json::Value::String(method.to_string()));
    req.insert("url".to_string(), serde_json::Value::String(url.to_string()));
    req.insert("headers".to_string(), serde_json::Value::Object(headers));
    if let Some(b) = body {
        req.insert("body".to_string(), serde_json::Value::String(b.to_string()));
    }

    let result = host
        .http_fetch(&serde_json::Value::Object(req))
        .map_err(|e: HostError| format!("http_fetch failed: {}", e))?;

    let result = result.ok_or_else(|| "http_fetch returned None".to_string())?;
    serde_json::from_value(result)
        .map_err(|e| format!("parse http response: {}", e))
}

/// URL 编码（最小实现，仅编码空格和特殊字符）
fn urlencoded(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('?', "%3F")
        .replace('&', "%26")
        .replace('=', "%3D")
}

/// 从响应 headers 中取值（不区分大小写）
fn header_val<'a>(headers: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    let lower = key.to_lowercase();
    headers
        .iter()
        .find(|(k, _)| k.to_lowercase() == lower)
        .map(|(_, v)| v.as_str())
}

/// 拆分路径为 (父目录, 文件名)
fn split_parent_name(path: &str) -> (String, String) {
    let path = path.trim_matches('/');
    match path.rfind('/') {
        Some(pos) => (path[..pos].to_string(), path[pos + 1..].to_string()),
        None => (String::new(), path.to_string()),
    }
}
