//! HTTP 代理命令面（ticket 03）：`http_request` / `http_cancel`
//!
//! 移动端前端只负责 UI 渲染，所有 HTTP 经本代理由 Rust 发出（spec §3）：
//! - 共享 `reqwest::Client`（连接池复用；`no_proxy()`——系统代理会劫持局域网目标，
//!   与 AuthHttpClient 同策略）
//! - Egress Policy 校验（ticket 02）：`kind="desktop"` 走 L1 桌面端目标校验（方案 a：
//!   调用方声明 + Rust 校验在途/已连接目标，阻止任意外网 URL 借 desktop 逃逸）；
//!   `kind="external"`（默认）走 L1/L2/L3 全层判定，未命中弹授权窗（fail-closed）
//! - JWT 注入（`/api/auth/*` 白名单除外，spec §5 第 4 条）
//! - 链路加密信封（bedcode-link-crypto；`enabled ∧ encrypt_http ∧ 已 pin` 且非 auth
//!   路径；GET/HEAD 无 body 仍带协商头；失败 fail-closed）
//! - 超时默认 30s（D5）+ `http_cancel(request_id)`（oneshot + tokio::select!，
//!   handoff §5.5 结论 2.2）
//! - 报文对齐（§5 第 5 条）：forbidden 头丢弃、POST/PUT 无 body 补 `Content-Length: 0`、
//!   Range 头补 `Accept-Encoding: identity`（桌面端实测 `allow_any_origin`，无需伪造
//!   Origin；reqwest 默认 UA 桌面端不校验）
//! - 结构化日志带 `request_id` 字段（§8 日志红线）

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::egress::{EgressDecision, ERROR_URL_NOT_DECLARED};
use crate::state::is_http_encryption_active;
use crate::AppError;
use crate::Result;

// ==================== 常量 ====================

/// 默认请求超时（D5：30s）
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// JWT 注入白名单前缀：`/api/auth/*` 不带 Bearer（与现状一致，spec §5 第 4 条）
const AUTH_PATH_PREFIX: &str = "/api/auth/";

/// 桌面端 HTTP 为局域网明文（单端口自定义），https 外网不得借 kind=desktop 逃逸
const DESKTOP_REQUIRED_SCHEME: &str = "http";

/// 请求头黑名单：浏览器禁止前端设置的 forbidden headers（对齐 Web fetch 语义，
/// §5.5 结论 2.3）——大小写不敏感
const FORBIDDEN_HEADERS_EXACT: &[&str] = &[
    "connection",
    "cookie",
    "host",
    "origin",
    "referer",
    "upgrade",
    "keep-alive",
];
const FORBIDDEN_HEADER_PREFIXES: &[&str] = &["proxy-", "sec-"];

/// HTTP 无 body 方法（加密信封不填充 body，仍带协商头）
fn has_request_body(method: &str) -> bool {
    let m = method.to_uppercase();
    m != "GET" && m != "HEAD"
}

// ==================== 请求/响应形状 ====================

/// http_request 命令参数（camelCase；kind 缺省 external）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpProxyRequest {
    /// 调用方生成的 UUID（D3）：日志/追踪/取消标识，多路复用响应路由
    pub request_id: String,
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// 原始请求体（字符串；无 body 为 None）
    #[serde(default)]
    pub body: Option<String>,
    /// 超时毫秒（缺省 30000）
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// "desktop" = 桌面端目标（L1 校验）；"external"（缺省）= 外网全层判定
    #[serde(default)]
    pub kind: Option<String>,
}

/// http_request 响应形状（前端 request() 归一化为 ApiResult）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpProxyResponse {
    pub status: u16,
    pub status_text: String,
    /// 响应头（已过滤 hop-by-hop；value 合并逗号）
    pub headers: HashMap<String, String>,
    /// 响应体文本（已解密；UTF-8 尽力解码）
    pub body_text: String,
}

// ==================== 全局代理状态 ====================

/// 共享 reqwest Client（连接池复用；无全局超时——每请求显式设置）
static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// 在途请求取消通道：request_id → oneshot（完成/取消/超时后 remove 防泄漏）
static PENDING_REQUESTS: OnceLock<tokio::sync::Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>> =
    OnceLock::new();

fn client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            // 移动端只直连局域网桌面端/指定外网；系统代理会把局域网目标劫持走
            // （实测回环被代理返回 502，AuthHttpClient 同策略）
            .no_proxy()
            .build()
            .expect("reqwest client build failed")
    })
}

fn pending_requests() -> &'static tokio::sync::Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>> {
    PENDING_REQUESTS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

// ==================== 报文对齐（纯函数，可单测） ====================

/// 过滤 forbidden headers（浏览器语义）：exact 黑名单 + 前缀黑名单，大小写不敏感
pub fn sanitize_headers(headers: &HashMap<String, String>) -> HashMap<String, String> {
    headers
        .iter()
        .filter(|(k, _)| {
            let lower = k.to_lowercase();
            !FORBIDDEN_HEADERS_EXACT.contains(&lower.as_str())
                && !FORBIDDEN_HEADER_PREFIXES.iter().any(|p| lower.starts_with(p))
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// 报文对齐补充头（§5.5 结论 2.3 / §5 第 5 条）：
/// - POST/PUT 无 body → `Content-Length: 0`（桌面端按报文语义解析）
/// - Range 头存在 → `Accept-Encoding: identity`（部分服务器对 Range 分段响应拒绝
///   压缩；reqwest 无 gzip feature 本就发 identity，这里显式对齐）
/// - 不伪造 Origin（桌面端 `allow_any_origin`，实测不校验）
pub fn align_wire_headers(headers: &mut HashMap<String, String>, method: &str, body: Option<&str>) {
    let m = method.to_uppercase();
    if (m == "POST" || m == "PUT") && body.map_or(true, |b| b.is_empty()) {
        headers.insert("Content-Length".to_string(), "0".to_string());
    }
    if headers.keys().any(|k| k.to_lowercase() == "range") {
        headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    }
}

/// JWT 注入判定：非 `/api/auth/*` 且 token 非空
pub fn should_inject_jwt(path: &str, token: &str) -> bool {
    !path.starts_with(AUTH_PATH_PREFIX) && !token.is_empty()
}

/// 链路加密判定：主开关 ∧ HTTP 子开关 ∧ 已 pin（state.rs），且非 auth 路径
pub fn should_encrypt(path: &str) -> bool {
    is_http_encryption_active() && !path.starts_with(AUTH_PATH_PREFIX)
}

// ==================== 核心命令 ====================

/// 统一 HTTP 代理命令（D1：invoke promise 路由回调用方）
#[tauri::command]
pub async fn http_request(request: HttpProxyRequest, app: tauri::AppHandle) -> Result<HttpProxyResponse> {
    execute_proxy(request, Some(&app)).await
}

/// 代理核心执行（命令与集成测试共用；`app` 为 None 时 L3 弹窗路径 fail-closed 拒绝）
pub async fn execute_proxy(request: HttpProxyRequest, app: Option<&tauri::AppHandle>) -> Result<HttpProxyResponse> {
    use tauri::Emitter;

    let request_id = request.request_id.clone();
    let timeout = Duration::from_millis(request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT.as_millis() as u64));
    let kind = request.kind.as_deref().unwrap_or("external");

    // ---------- URL 解析 + Egress 校验 ----------
    let parsed = crate::egress::parse_url_lite(&request.url)
        .ok_or_else(|| AppError::Egress(format!("{ERROR_URL_NOT_DECLARED}: malformed url {}", request.url)))?;

    if kind == "desktop" {
        // 方案 a（L1 时序落地）：desktop 类请求必须命中桌面端目标集合
        // （前端 setApiBaseUrl 时经 egress_declare_desktop_target 声明，
        // 覆盖 httpProbe 先于 ws_connect 的时序缺口），且仅允许 http 明文
        if parsed.scheme != DESKTOP_REQUIRED_SCHEME {
            return Err(AppError::Egress(format!(
                "{ERROR_URL_NOT_DECLARED}: desktop kind requires http://, got {}",
                request.url
            )));
        }
        if !crate::egress::policy().is_desktop_target(&parsed.host, parsed.port) {
            tracing::warn!(
                request_id = %request_id,
                host = %parsed.host,
                port = ?parsed.port,
                "egress: desktop target not declared, denying"
            );
            return Err(AppError::Egress(format!(
                "{ERROR_URL_NOT_DECLARED}: host:port not a declared desktop target: {}",
                request.url
            )));
        }
    } else {
        // external：Egress 全层判定（L1/L2/L3）
        let source = "host"; // 宿主前端调用；插件路径走 wasm_host（ticket 06）
        match crate::egress::policy().decide(&request.url, source) {
            EgressDecision::Allow(_) => {}
            EgressDecision::Deny(e) => {
                tracing::warn!(
                    request_id = %request_id,
                    url = %request.url,
                    code = %e.code,
                    "egress: url denied"
                );
                return Err(AppError::Egress(format!("{}: {}", e.code, e.url)));
            }
            EgressDecision::NeedConsent(mut req) => {
                req.id = request_id.clone();
                // app 为 None（测试）或 emit 失败 → fail-closed 拒绝
                let allowed = match app {
                    Some(handle) => crate::egress::policy()
                        .request_consent(handle, req)
                        .await
                        .map_err(|e| AppError::Egress(format!("consent flow failed: {e}")))?,
                    None => {
                        tracing::warn!(
                            request_id = %request_id,
                            url = %request.url,
                            "egress: consent required but no app handle (test), denying"
                        );
                        false
                    }
                };
                if !allowed {
                    return Err(AppError::Egress(format!(
                        "{}: {}",
                        crate::egress::ERROR_URL_DENIED,
                        request.url
                    )));
                }
            }
        }
    }

    // ---------- 构建请求（报文对齐 + JWT + 加密信封） ----------
    let method = reqwest::Method::from_bytes(request.method.to_uppercase().as_bytes())
        .map_err(|e| AppError::Egress(format!("unsupported method {}: {e}", request.method)))?;

    let mut headers = sanitize_headers(&request.headers);
    let path = parsed.path.clone();

    if should_inject_jwt(&path, &crate::state::get_global_token()) {
        headers.insert(
            "Authorization".to_string(),
            format!("Bearer {}", crate::state::get_global_token()),
        );
    }

    // 链路加密信封（非 auth 且开关开且已 pin）
    let encryption_active = should_encrypt(&path);
    let mut request_keys: Option<bedcode_link_crypto::HttpTrafficKeys> = None;
    let mut body_bytes: Option<Vec<u8>> = request.body.clone().map(String::into_bytes);

    if encryption_active {
        let ctx = crate::state::get_link_crypto_context();
        let kd_raw = ctx.kd_public_b64.as_deref().ok_or_else(|| {
            AppError::Egress("LINK_ENCRYPTION_NO_PIN: encryption active but no pinned key".to_string())
        })?;
        let kd_pub: [u8; 32] = base64::engine::general_purpose::STANDARD
            .decode(kd_raw)
            .map_err(|e| AppError::Egress(format!("pin b64 decode failed: {e}")))?
            .try_into()
            .map_err(|v: Vec<u8>| AppError::Egress(format!("pin length mismatch: expected 32, got {}", v.len())))?;
        let (eph_priv, eph_pub) = bedcode_link_crypto::generate_ephemeral();
        let keys = bedcode_link_crypto::derive_http_keys(&eph_priv, &kd_pub, &path)
            .map_err(|e| AppError::Egress(format!("derive http keys failed: {e}")))?;
        let aad = bedcode_link_crypto::http_aad(bedcode_link_crypto::Direction::Inbound, &path);
        if has_request_body(method.as_str()) {
            let plain = body_bytes.clone().unwrap_or_default();
            let sealed = bedcode_link_crypto::encrypt_http_body(&keys.request, &plain, &aad)
                .map_err(|e| AppError::Egress(format!("encrypt http body failed: {e}")))?;
            body_bytes = Some(sealed);
        }
        headers.insert(
            "X-BedCode-Crypto".to_string(),
            format!("v1 {}", base64::engine::general_purpose::STANDARD.encode(eph_pub)),
        );
        request_keys = Some(keys);
    }

    align_wire_headers(
        &mut headers,
        method.as_str(),
        body_bytes.as_deref().map(|b| std::str::from_utf8(b).unwrap_or("")),
    );

    // ---------- 发送（select! 取消） ----------
    let mut builder = client().request(method, &request.url).timeout(timeout);
    if !headers.is_empty() {
        let mut header_map = reqwest::header::HeaderMap::new();
        for (k, v) in &headers {
            if let (Ok(k), Ok(v)) = (
                reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                reqwest::header::HeaderValue::from_str(v),
            ) {
                header_map.insert(k, v);
            }
        }
        builder = builder.headers(header_map);
    }
    if let Some(b) = &body_bytes {
        builder = builder.body(b.clone());
    }
    let req = builder
        .build()
        .map_err(|e| AppError::Egress(format!("build request failed: {e}")))?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    pending_requests().lock().await.insert(request_id.clone(), cancel_tx);

    let fut = client().execute(req);
    tokio::pin!(fut);
    let result = tokio::select! {
        res = &mut fut => res,
        _ = cancel_rx => {
            pending_requests().lock().await.remove(&request_id);
            tracing::info!(request_id = %request_id, url = %request.url, "http proxy: request cancelled");
            return Err(AppError::Egress("REQUEST_CANCELED: http request cancelled".to_string()));
        }
    };
    pending_requests().lock().await.remove(&request_id);

    let response = result.map_err(|e| {
        tracing::error!(request_id = %request_id, url = %request.url, error = %e, "http proxy: request failed");
        AppError::Egress(format!("HTTP request failed: {e}"))
    })?;

    // ---------- 响应（解密 + pin 刷新） ----------
    let status = response.status();
    let status_text = status.canonical_reason().unwrap_or("").to_string();
    let mut resp_headers: HashMap<String, String> = HashMap::new();
    for (k, v) in response.headers() {
        resp_headers
            .entry(k.as_str().to_string())
            .and_modify(|existing| {
                existing.push_str(", ");
                existing.push_str(v.to_str().unwrap_or(""));
            })
            .or_insert_with(|| v.to_str().unwrap_or("").to_string());
    }
    let has_crypto_resp_header = resp_headers.get("x-bedcode-crypto").map(|v| v == "v1").unwrap_or(false);

    let raw_body = response
        .bytes()
        .await
        .map_err(|e| AppError::Egress(format!("read response body failed: {e}")))?;

    let body_text = if encryption_active {
        match request_keys {
            Some(keys) if has_crypto_resp_header => {
                let aad = bedcode_link_crypto::http_aad(bedcode_link_crypto::Direction::Outbound, &path);
                let plain = bedcode_link_crypto::decrypt_http_body(&keys.response, &raw_body, &aad)
                    .map_err(|e| AppError::Egress(format!("decrypt http body failed: {e}")))?;
                String::from_utf8(plain).unwrap_or_else(|_| {
                    tracing::warn!(request_id = %request_id, "http proxy: decrypted body not utf-8");
                    String::new()
                })
            }
            _ => {
                // 预期加密而响应明文（downgrade）：strict 断连报错；非 strict 明文续跑
                let strict = crate::state::get_link_crypto_context().strict_mode;
                if strict {
                    tracing::error!(request_id = %request_id, "http proxy: strict mode downgrade detected");
                    return Err(AppError::Egress("LINK_ENCRYPTION_DOWNGRADE".to_string()));
                }
                tracing::warn!(request_id = %request_id, "http proxy: response unencrypted (downgrade tolerated)");
                String::from_utf8_lossy(&raw_body).to_string()
            }
        }
    } else {
        String::from_utf8_lossy(&raw_body).to_string()
    };

    // pin 刷新收束（handoff §3.5）：auth 响应携带 kdPublicB64 → Rust 落地 +
    // 广播 ws_link_crypto_pin（前端 localStorage 保留作设置页展示，裁决在 Rust）
    if path.starts_with(AUTH_PATH_PREFIX) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body_text) {
            if let Some(kd) = v.pointer("/data/kdPublicB64").and_then(|k| k.as_str()) {
                if !kd.is_empty() {
                    crate::state::update_link_crypto_pin(Some(kd.to_string()));
                    let kd_fingerprint = v.pointer("/data/kdFingerprint").and_then(|f| f.as_str());
                    tracing::info!(
                        request_id = %request_id,
                        "http proxy: link crypto pin refreshed from auth response"
                    );
                    if let Some(handle) = app {
                        let _ = handle.emit(
                            "ws_link_crypto_pin",
                            json!({ "kdPublicB64": kd, "kdFingerprint": kd_fingerprint }),
                        );
                    }
                }
            }
        }
    }

    tracing::debug!(
        request_id = %request_id,
        method = %request.method.to_uppercase(),
        host = %parsed.host,
        status = %status.as_u16(),
        encrypted = %encryption_active,
        "http proxy: completed"
    );

    Ok(HttpProxyResponse {
        status: status.as_u16(),
        status_text,
        headers: resp_headers,
        body_text,
    })
}

/// 取消在途请求（request_id 多路复用；不存在时静默成功）
#[tauri::command]
pub async fn http_cancel(request_id: String) -> Result<()> {
    let removed = pending_requests().lock().await.remove(&request_id);
    if removed.is_none() {
        tracing::debug!(request_id = %request_id, "http proxy: cancel for unknown request_id");
    }
    Ok(())
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_drops_forbidden_headers() {
        let mut h = HashMap::new();
        h.insert("Content-Type".to_string(), "application/json".to_string());
        h.insert("Host".to_string(), "evil.com".to_string());
        h.insert("Proxy-Authorization".to_string(), "Basic xxx".to_string());
        h.insert("Sec-WebSocket-Key".to_string(), "abc".to_string());
        h.insert("X-Custom".to_string(), "ok".to_string());
        let cleaned = sanitize_headers(&h);
        assert!(cleaned.contains_key("Content-Type"));
        assert!(cleaned.contains_key("X-Custom"));
        assert!(!cleaned.contains_key("Host"));
        assert!(!cleaned.contains_key("Proxy-Authorization"));
        assert!(!cleaned.contains_key("Sec-WebSocket-Key"));
    }

    #[test]
    fn align_wire_headers_content_length_and_encoding() {
        // POST 无 body → Content-Length: 0
        let mut h = HashMap::new();
        align_wire_headers(&mut h, "POST", None);
        assert_eq!(h.get("Content-Length").map(String::as_str), Some("0"));
        // GET 不补
        let mut h = HashMap::new();
        align_wire_headers(&mut h, "GET", None);
        assert!(!h.contains_key("Content-Length"));
        // Range → Accept-Encoding: identity
        let mut h = HashMap::new();
        h.insert("Range".to_string(), "bytes=0-1023".to_string());
        align_wire_headers(&mut h, "GET", None);
        assert_eq!(h.get("Accept-Encoding").map(String::as_str), Some("identity"));
    }

    #[test]
    fn jwt_injection_whitelist() {
        assert!(!should_inject_jwt("/api/auth/pairing", "tok"));
        assert!(!should_inject_jwt("/api/auth/reauth", ""));
        assert!(should_inject_jwt("/api/sessions", "tok"));
        assert!(!should_inject_jwt("/api/sessions", ""));
    }

    #[test]
    fn request_body_semantics() {
        assert!(!has_request_body("GET"));
        assert!(!has_request_body("head"));
        assert!(has_request_body("POST"));
        assert!(has_request_body("delete"));
    }
}
