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

/// 列举远端目录
///
/// GET {base}/list?path={path}
pub fn list_remote(
    host: &impl HostHttp,
    base: &str,
    auth: &str,
    path: &str,
) -> Result<Vec<DirEntry>, String> {
    let url = format!(
        "{}/list?path={}",
        base,
        urlencoded(path)
    );
    let resp = do_fetch(host, "GET", &url, auth, None)?;
    if resp.status != 200 {
        return Err(format!("list_remote: HTTP {}", resp.status));
    }
    serde_json::from_str(&resp.body)
        .map_err(|e| format!("list_remote: parse body failed: {}", e))
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
    let entries = list_remote(host, base, auth, &parent)?;
    let entry = entries
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
/// POST {base}/upload body={relativePath, size}
/// 成功返回 SessionCreated；409 = 同名被拒；其他 = 错误
pub fn create_session(
    host: &impl HostHttp,
    base: &str,
    auth: &str,
    relative_path: &str,
    size: u64,
) -> Result<SessionCreated, CreateSessionError> {
    let url = format!("{}/upload", base);
    let body = serde_json::json!({
        "relativePath": relative_path,
        "size": size,
    });
    let resp = do_fetch(host, "POST", &url, auth, Some(&body))
        .map_err(|e| CreateSessionError::Other(e))?;
    match resp.status {
        200 | 201 => serde_json::from_str(&resp.body)
            .map_err(|e| CreateSessionError::Other(format!("parse session response: {}", e))),
        409 => Err(CreateSessionError::DuplicateName),
        _ => Err(CreateSessionError::Other(format!(
            "create_session: HTTP {}",
            resp.status
        ))),
    }
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
            Ok(body
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
pub fn complete_session(
    host: &impl HostHttp,
    base: &str,
    auth: &str,
    session_id: &str,
) -> Result<(), String> {
    let url = format!("{}/upload/{}/complete", base, session_id);
    let resp = do_fetch(host, "POST", &url, auth, None)?;
    if resp.status >= 200 && resp.status < 300 {
        Ok(())
    } else {
        Err(format!("complete_session: HTTP {}", resp.status))
    }
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
