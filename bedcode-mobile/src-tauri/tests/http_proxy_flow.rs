//! http_proxy 集成测试（ticket 03）：JWT 注入 / auth 白名单 / 加密信封往返 /
//! 报文对齐 / 并发 request_id / 取消 / Egress 判定
//!
//! 起本地 actix mock 桌面端（dev-dependency，仅测试用），验证代理产出报文的
//! 语义与链路加密信封字节级互通（mock 侧用 bedcode-link-crypto 解密/加密回）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use serde_json::{json, Value};
use tokio::sync::OnceCell;

use bedcode_lib::commands::http_proxy::{execute_proxy, http_cancel, HttpProxyRequest};

/// 全局串行闸：代理的全局 state（desktop_targets / link crypto context /
/// pending requests）是进程级共享，测试并发执行会互相污染——所有用例串行跑
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 测试固定 Kd 身份密钥对（mock 桌面端持私钥；客户端 pin 公钥）
const KD_PRIV: [u8; 32] = [7u8; 32];
fn kd_pub() -> [u8; 32] {
    x25519_dalek::x25519(KD_PRIV, x25519_dalek::X25519_BASEPOINT_BYTES)
}

// ==================== Mock 桌面端 ====================

#[derive(Default)]
struct MockState {
    /// path → 捕获的请求（头子集 + body）
    captures: Mutex<HashMap<String, CapturedRequest>>,
}

#[derive(Clone, Debug, Default)]
struct CapturedRequest {
    authorization: Option<String>,
    content_length: Option<String>,
    accept_encoding: Option<String>,
    host: Option<String>,
    crypto_negotiation: Option<String>,
    body: String,
}

impl CapturedRequest {
    fn record_headers(&mut self, headers: &actix_web::http::header::HeaderMap) {
        let get = |name: &str| headers.get(name).map(|v| v.to_str().unwrap_or("").to_string());
        self.authorization = get("authorization");
        self.content_length = get("content-length");
        self.accept_encoding = get("accept-encoding");
        self.host = get("host");
        self.crypto_negotiation = get("x-bedcode-crypto");
    }
}

fn envelope(code: u16, msg: &str, data: Option<Value>) -> actix_web::HttpResponse {
    actix_web::HttpResponse::Ok().json(json!({ "code": code, "message": msg, "data": data }))
}

/// 加密响应：X-BedCode-Crypto: v1 标记头 + 信封 body（响应方向 key）
///
/// 响应复用请求协商的同一会话（桌面端不重新协商临时密钥；响应头固定 "v1"
/// 无公钥——spec §4，客户端按 `=== "v1"` 判定）
fn crypto_response(path: &str, keys: &bedcode_link_crypto::HttpTrafficKeys, body: &str) -> actix_web::HttpResponse {
    use bedcode_link_crypto::{encrypt_http_body, http_aad, Direction};
    let aad = http_aad(Direction::Outbound, path);
    let sealed = encrypt_http_body(&keys.response, body.as_bytes(), &aad).expect("seal");
    actix_web::HttpResponse::Ok()
        .insert_header((
            "X-BedCode-Crypto",
            format!("v{}", bedcode_link_crypto::PROTOCOL_VERSION),
        ))
        .body(sealed)
}

struct MockDesktop {
    addr: std::net::SocketAddr,
    state: Arc<MockState>,
    handle: actix_web::dev::ServerHandle,
}

impl MockDesktop {
    async fn start() -> Self {
        let state = Arc::new(MockState::default());
        let state_clone = state.clone();
        let server = actix_web::HttpServer::new(move || {
            actix_web::App::new()
                .app_data(actix_web::web::Data::from(state_clone.clone()))
                .route("/api/health", actix_web::web::get().to(mock_health))
                .route("/api/sessions", actix_web::web::get().to(mock_capture))
                .route("/api/auth/pairing", actix_web::web::post().to(mock_pairing))
                .route("/echo", actix_web::web::post().to(mock_echo))
                .route("/crypto", actix_web::web::post().to(mock_crypto))
                .route("/slow", actix_web::web::get().to(mock_slow))
        })
        .bind(("127.0.0.1", 0))
        .expect("bind mock proxy server");
        let addr = server.addrs()[0];
        let server = server.run();
        let handle = server.handle();
        // 常驻线程：每个 #[tokio::test] 是独立 current_thread runtime，测试结束会
        // abort 其 spawn 的 future——server 必须跑在独立 thread 的 multi_thread
        // runtime 上（进程退出时泄漏，无妨）
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("build server runtime");
            rt.block_on(async move {
                let _ = server.await;
            });
        });
        Self { addr, state, handle }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn capture(&self, path: &str) -> CapturedRequest {
        self.state
            .captures
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .unwrap_or_default()
    }

    async fn shutdown(&self) {
        let _ = self.handle.stop(false).await;
    }
}

impl Drop for MockDesktop {
    fn drop(&mut self) {
        let handle = self.handle.clone();
        tokio::spawn(async move {
            let _ = handle.stop(false).await;
        });
    }
}

// ==================== Mock handlers ====================

fn record(
    state: &actix_web::web::Data<MockState>,
    path: &str,
    headers: &actix_web::http::header::HeaderMap,
    body: &str,
) {
    let mut req = CapturedRequest::default();
    req.record_headers(headers);
    req.body = body.to_string();
    state.captures.lock().unwrap().insert(path.to_string(), req);
}

async fn mock_health() -> actix_web::HttpResponse {
    envelope(
        0,
        "ok",
        Some(json!({ "status": "ok", "port": 4455, "uptime_secs": 42 })),
    )
}

async fn mock_capture(req: actix_web::HttpRequest, state: actix_web::web::Data<MockState>) -> actix_web::HttpResponse {
    record(&state, "/api/sessions", req.headers(), "");
    envelope(0, "ok", Some(json!({ "sessions": [] })))
}

async fn mock_pairing(
    body: actix_web::web::Bytes,
    req: actix_web::HttpRequest,
    state: actix_web::web::Data<MockState>,
) -> actix_web::HttpResponse {
    let body_text = String::from_utf8_lossy(&body).to_string();
    record(&state, "/api/auth/pairing", req.headers(), &body_text);
    // 携带 kdPublicB64/kdFingerprint → 测 pin 刷新收束
    envelope(
        0,
        "ok",
        Some(json!({
            "pairingCode": "ABCDEF",
            "expiresIn": 300,
            "kdPublicB64": base64::engine::general_purpose::STANDARD.encode(kd_pub()),
            "kdFingerprint": "aabbccdd",
        })),
    )
}

async fn mock_echo(
    body: actix_web::web::Bytes,
    req: actix_web::HttpRequest,
    state: actix_web::web::Data<MockState>,
) -> actix_web::HttpResponse {
    record(&state, "/echo", req.headers(), &String::from_utf8_lossy(&body));
    envelope(0, "echoed", None)
}

async fn mock_crypto(
    body: actix_web::web::Bytes,
    req: actix_web::HttpRequest,
    state: actix_web::web::Data<MockState>,
) -> actix_web::HttpResponse {
    use bedcode_link_crypto::{decrypt_http_body, derive_http_keys, http_aad, Direction};
    record(&state, "/crypto", req.headers(), &String::from_utf8_lossy(&body));
    // 解密请求信封（mock 持 kd 私钥；X25519 对称 → 相同 shared）
    let negotiation = req
        .headers()
        .get("x-bedcode-crypto")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or_default();
    let ek_b64 = negotiation.strip_prefix("v1 ").unwrap_or("");
    let ek: [u8; 32] = base64::engine::general_purpose::STANDARD
        .decode(ek_b64)
        .unwrap()
        .try_into()
        .unwrap();
    let keys = derive_http_keys(&KD_PRIV, &ek, "/crypto").expect("derive");
    let aad = http_aad(Direction::Inbound, "/crypto");
    let plain = decrypt_http_body(&keys.request, &body, &aad).expect("mock decrypt request");
    assert_eq!(plain, b"hello sealed", "请求信封解密应得原文");
    crypto_response("/crypto", &keys, "{\"code\":0,\"message\":\"decrypted ok\"}")
}

async fn mock_slow() -> actix_web::HttpResponse {
    tokio::time::sleep(Duration::from_secs(8)).await;
    envelope(0, "slow done", None)
}

// ==================== 测试 helpers ====================

fn proxy_req(request_id: &str, method: &str, url: &str, kind: &str) -> HttpProxyRequest {
    HttpProxyRequest {
        request_id: request_id.to_string(),
        method: method.to_string(),
        url: url.to_string(),
        headers: HashMap::new(),
        body: None,
        timeout_ms: Some(5_000),
        kind: Some(kind.to_string()),
    }
}

/// 测试环境初始化：重置全局 Egress 目标与授权记忆
fn reset_egress() {
    let p = bedcode_lib::egress::policy();
    p.clear_desktop_targets();
    p.revoke_all_grants();
    // 重置加密上下文为默认（防泄漏到其它用例）
    bedcode_lib::state::set_link_crypto_context(Default::default());
}

async fn mock_once() -> &'static MockDesktop {
    static MOCK: OnceCell<MockDesktop> = OnceCell::const_new();
    MOCK.get_or_init(MockDesktop::start).await
}

// ==================== 用例 ====================

/// JWT 注入 + auth 白名单：/api/sessions 带 Bearer；/api/auth/pairing 不带
#[tokio::test]
async fn jwt_injection_and_auth_whitelist() {
    let _serial = SERIAL.lock().unwrap();
    reset_egress();
    let mock = mock_once().await;

    // desktop 类先声明目标（前端 setApiBaseUrl 语义）
    bedcode_lib::egress::policy().add_desktop_target(&mock.addr.ip().to_string(), mock.addr.port());
    bedcode_lib::state::set_global_token("test-jwt-token");

    let resp = execute_proxy(
        proxy_req("jwt-1", "GET", &format!("{}/api/sessions", mock.base_url()), "desktop"),
        None,
    )
    .await
    .expect("desktop session request should pass");
    assert_eq!(resp.status, 200);
    let cap = mock.capture("/api/sessions");
    assert_eq!(cap.authorization.as_deref(), Some("Bearer test-jwt-token"));

    let mut auth_req = proxy_req(
        "auth-1",
        "POST",
        &format!("{}/api/auth/pairing", mock.base_url()),
        "desktop",
    );
    auth_req.body = Some(json!({ "deviceId": "dev-1", "deviceName": "mi-pad", "fingerprint": "fp" }).to_string());
    let resp = execute_proxy(auth_req, None).await.expect("auth request should pass");
    assert_eq!(resp.status, 200);
    let cap = mock.capture("/api/auth/pairing");
    assert!(cap.authorization.is_none(), "auth 路径不得注入 JWT");
    // pin 刷新收束：auth 响应带 kdPublicB64 → Rust 侧落地
    let ctx = bedcode_lib::state::get_link_crypto_context();
    assert!(ctx.kd_public_b64.is_some(), "auth 响应应触发 pin 刷新落地");

    bedcode_lib::state::clear_global_token();
}

/// Egress L1：desktop 类未声明目标 → fail-closed 拒绝；声明后放行
#[tokio::test]
async fn desktop_requires_declared_target() {
    let _serial = SERIAL.lock().unwrap();
    reset_egress();
    let mock = mock_once().await;

    let err = execute_proxy(
        proxy_req("d-1", "GET", &format!("{}/api/health", mock.base_url()), "desktop"),
        None,
    )
    .await
    .expect_err("未声明目标应拒绝");
    let msg = err.to_string();
    assert!(msg.contains("EXTERNAL_URL_NOT_DECLARED"), "got: {msg}");

    // https 外网不得借 desktop 逃逸
    let err = execute_proxy(
        proxy_req("d-2", "GET", "https://api.github.com/repos/x", "desktop"),
        None,
    )
    .await
    .expect_err("desktop 类 https 应拒绝");
    assert!(err.to_string().contains("EXTERNAL_URL_NOT_DECLARED"));

    // 声明后放行（probe 时序模拟：probe 前先 declare）
    bedcode_lib::egress::policy().add_desktop_target(&mock.addr.ip().to_string(), mock.addr.port());
    let resp = execute_proxy(
        proxy_req("d-3", "GET", &format!("{}/api/health", mock.base_url()), "desktop"),
        None,
    )
    .await
    .expect("声明后应放行");
    assert_eq!(resp.status, 200);
}

/// 报文对齐：POST 无 body → Content-Length: 0；伪造 Host 丢弃；Range → identity
#[tokio::test]
async fn wire_alignment() {
    let _serial = SERIAL.lock().unwrap();
    reset_egress();
    let mock = mock_once().await;
    bedcode_lib::egress::policy().add_desktop_target(&mock.addr.ip().to_string(), mock.addr.port());

    let mut req = proxy_req("w-1", "POST", &format!("{}/echo", mock.base_url()), "desktop");
    req.headers.insert("Host".to_string(), "evil.example.com".to_string());
    req.headers.insert("Range".to_string(), "bytes=0-1023".to_string());
    let _ = execute_proxy(req, None).await.expect("echo should pass");

    let cap = mock.capture("/echo");
    assert_eq!(
        cap.content_length.as_deref(),
        Some("0"),
        "POST 无 body 应补 Content-Length: 0"
    );
    assert_eq!(
        cap.accept_encoding.as_deref(),
        Some("identity"),
        "Range 头应补 Accept-Encoding: identity"
    );
    assert_ne!(cap.host.as_deref(), Some("evil.example.com"), "伪造 Host 应被丢弃");
}

/// 链路加密：信封字节级互通（mock 解密请求 → 加密响应 → 代理解密）
#[tokio::test]
async fn crypto_envelope_roundtrip() {
    let _serial = SERIAL.lock().unwrap();
    reset_egress();
    let mock = mock_once().await;
    bedcode_lib::egress::policy().add_desktop_target(&mock.addr.ip().to_string(), mock.addr.port());

    // 开加密：主开关 + HTTP 子开关 + pin（对应前端推送 set_link_crypto_context）
    bedcode_lib::state::set_link_crypto_context(bedcode_lib::state::LinkCryptoContext {
        enabled: true,
        strict_mode: true,
        encrypt_ws_event: true,
        encrypt_http: true,
        kd_public_b64: Some(base64::engine::general_purpose::STANDARD.encode(kd_pub())),
    });

    let mut req = proxy_req("c-1", "POST", &format!("{}/crypto", mock.base_url()), "desktop");
    req.body = Some("hello sealed".to_string());
    let resp = execute_proxy(req, None).await.expect("crypto request should pass");
    assert_eq!(resp.status, 200);
    assert!(
        resp.body_text.contains("decrypted ok"),
        "响应应被解密为明文: {}",
        resp.body_text
    );
    // mock 侧已断言请求信封解密得 "hello sealed"（panic 即失败）

    reset_egress();
}

/// 并发 request_id 多路复用：10 并发请求互不串扰
#[tokio::test]
async fn concurrent_request_ids() {
    let _serial = SERIAL.lock().unwrap();
    reset_egress();
    let mock = mock_once().await;
    bedcode_lib::egress::policy().add_desktop_target(&mock.addr.ip().to_string(), mock.addr.port());

    let mut handles = Vec::new();
    for i in 0..10 {
        let url = format!("{}/api/sessions", mock.base_url());
        handles.push(tokio::spawn(async move {
            let req = proxy_req(&format!("conc-{i}"), "GET", &url, "desktop");
            execute_proxy(req, None).await
        }));
    }
    for (i, h) in handles.into_iter().enumerate() {
        let resp = h.await.expect("join").expect("request should pass");
        assert_eq!(resp.status, 200, "并发请求 {i} 失败");
    }
}

/// 取消：http_cancel(request_id) → 在途请求返回 REQUEST_CANCELED
#[tokio::test]
async fn cancel_inflight_request() {
    let _serial = SERIAL.lock().unwrap();
    reset_egress();
    let mock = mock_once().await;
    bedcode_lib::egress::policy().add_desktop_target(&mock.addr.ip().to_string(), mock.addr.port());

    let url = format!("{}/slow", mock.base_url());
    let req = proxy_req("cancel-1", "GET", &url, "desktop");
    let handle = tokio::spawn(async move { execute_proxy(req, None).await });

    tokio::time::sleep(Duration::from_millis(200)).await;
    http_cancel("cancel-1".to_string()).await.expect("cancel");

    let result = handle.await.expect("join");
    let err = result.expect_err("取消后应报错");
    assert!(err.to_string().contains("REQUEST_CANCELED"), "got: {}", err);
}

/// Egress fail-closed：external 未声明且无弹窗渠道（测试 app=None）→ DENIED
#[tokio::test]
async fn external_undeclared_denied() {
    let _serial = SERIAL.lock().unwrap();
    reset_egress();
    let mock = mock_once().await;

    let err = execute_proxy(
        proxy_req("e-1", "GET", &format!("{}/api/health", mock.base_url()), "external"),
        None,
    )
    .await
    .expect_err("未声明外网应拒绝");
    assert!(err.to_string().contains("EXTERNAL_URL_DENIED"), "got: {}", err);
}
