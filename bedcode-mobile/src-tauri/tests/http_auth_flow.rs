//! 移动端 HTTP 认证客户端集成测试（L1）
//!
//! 认证已从 WS 握手迁移到 HTTP（spec §4.5 六端点）。本测试用 actix-web
//! 起 mock 桌面端 HTTP 服务器（移动端 crate 自带 actix-web 主依赖，零新增），
//! 逐路由返回与桌面端 `ApiResponse` 对称的 camelCase 信封，真实
//! `AuthHttpClient` + reqwest 打真请求，验证：
//! - 六端点 wire shape（请求体字段名 / 响应解析）
//! - 业务错误码映射（1005/1008/1009/1001 → AppError::Auth 携带 code）
//! - 传输层故障（连接拒绝）→ AppError::Internal
//! - resolve_base_url 有/无 target 两分支
//!
//! 本地无法跑 `biometric_sign` Kotlin bridge，生物端点到「验签 + 本地签名」
//! 为止，challenge/verify 的 HTTP 形状与解析由场景 ⑥ 验证。

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use actix_web::{web, App, HttpResponse, HttpServer};
use bedcode_lib::auth::http::{resolve_base_url, AuthHttpClient, DeviceAuthContext};
use bedcode_lib::connection::manager::ConnectionManager;
use bedcode_lib::state::clear_global_token;
use bedcode_lib::AppError;
use serde_json::{json, Value};

// 复用 WS 协议 mock（resolve_base_url happy path 需真实建连保存 target）
mod common;

/// mock 桌面端签发的配对码
const MOCK_PAIRING_CODE: &str = "654321";
/// verify/qr 签发的 token
const MOCK_TOKEN: &str = "mock-jwt-token";
/// reauth 刷新后签发的新 token（须与 verify 不同，验证 refresh 语义）
const MOCK_REAUTH_TOKEN: &str = "mock-jwt-token-refreshed";
/// 生物挑战值
const MOCK_NONCE: &str = "0123456789abcdef0123456789abcdef";

/// mock 响应模式：控制个别端点的业务拒绝行为
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MockMode {
    /// 全 happy path
    Happy,
    /// verify 回 1005（配对码无效/过期）
    VerifyRejected,
    /// biometric-challenge 回 1008（未绑定凭证）
    ChallengeRejected,
    /// biometric-verify 回 1009（验签失败）
    BiometricVerifyRejected,
    /// reauth 回 1001（token 签发失败）
    ReauthRejected,
}

impl MockMode {
    fn from_bits(bits: u8) -> Self {
        match bits {
            1 => Self::VerifyRejected,
            2 => Self::ChallengeRejected,
            3 => Self::BiometricVerifyRejected,
            4 => Self::ReauthRejected,
            _ => Self::Happy,
        }
    }

    fn bits(self) -> u8 {
        self as u8
    }
}

/// mock 服务器共享状态：模式 + 收到的全部请求（(path, body) 供断言 wire shape）
struct MockState {
    mode: AtomicU8,
    received: Mutex<Vec<(String, Value)>>,
}

impl MockState {
    fn new(mode: MockMode) -> Self {
        Self {
            mode: AtomicU8::new(mode.bits()),
            received: Mutex::new(Vec::new()),
        }
    }

    fn mode(&self) -> MockMode {
        MockMode::from_bits(self.mode.load(Ordering::SeqCst))
    }

    fn record(&self, path: &str, body: &Value) {
        self.received.lock().unwrap().push((path.to_string(), body.clone()));
    }

    /// 取指定 path 的最后一次请求体（测试断言请求字段）
    fn last_body(&self, path: &str) -> Value {
        self.received
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(p, _)| p == path)
            .map(|(_, b)| b.clone())
            .unwrap_or_else(|| panic!("no request recorded for {}", path))
    }
}

/// 统一信封构造（与桌面端 ApiResponse camelCase 对称）
fn envelope(code: u16, msg: &str, data: Option<Value>) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "code": code, "message": msg, "data": data }))
}

/// mock actix HTTP 服务器
struct MockDesktop {
    addr: SocketAddr,
    state: Arc<MockState>,
    handle: actix_web::dev::ServerHandle,
}

impl MockDesktop {
    async fn start(mode: MockMode) -> Self {
        let state = Arc::new(MockState::new(mode));
        let state_clone = state.clone();
        let http_server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::from(state_clone.clone()))
                .route("/api/auth/pairing", web::post().to(mock_pairing))
                .route("/api/auth/verify", web::post().to(mock_verify))
                .route("/api/auth/qr-connect", web::post().to(mock_qr))
                .route("/api/auth/reauth", web::post().to(mock_reauth))
                .route(
                    "/api/auth/biometric-challenge",
                    web::post().to(mock_biometric_challenge),
                )
                .route("/api/auth/biometric-verify", web::post().to(mock_biometric_verify))
                .route("/api/auth/biometric-bind", web::post().to(mock_biometric_bind))
        })
        .bind(("127.0.0.1", 0))
        .expect("bind mock auth server");
        let addr = http_server.addrs()[0];
        let server = http_server.run();
        let handle = server.handle();
        tokio::spawn(async move {
            let _ = server.await;
        });
        Self { addr, state, handle }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn last_body(&self, path: &str) -> Value {
        self.state.last_body(path)
    }

    /// 显式停掉 actix worker（防止跨测试泄漏线程；幂等，Drop 兜底）
    async fn shutdown(&self) {
        let _ = self.handle.stop(false).await;
    }
}

impl Drop for MockDesktop {
    fn drop(&mut self) {
        // 兜底停机（主路径是每测试末尾的 shutdown()）：万一断言 panic 提前退出，
        // 别让 actix worker 线程泄漏到后续测试
        let handle = self.handle.clone();
        tokio::spawn(async move {
            let _ = handle.stop(false).await;
        });
    }
}

// ==================== mock handlers ====================

async fn mock_pairing(body: web::Json<Value>, state: web::Data<MockState>) -> HttpResponse {
    state.record("/api/auth/pairing", &body);
    envelope(
        0,
        "ok",
        Some(json!({ "pairingCode": MOCK_PAIRING_CODE, "expiresIn": 300 })),
    )
}

async fn mock_verify(body: web::Json<Value>, state: web::Data<MockState>) -> HttpResponse {
    state.record("/api/auth/verify", &body);
    if state.mode() == MockMode::VerifyRejected {
        return envelope(1005, "Invalid or expired pairing code", None::<Value>);
    }
    envelope(0, "ok", Some(json!({ "token": MOCK_TOKEN, "expiresIn": 3600 })))
}

async fn mock_qr(body: web::Json<Value>, state: web::Data<MockState>) -> HttpResponse {
    state.record("/api/auth/qr-connect", &body);
    envelope(0, "ok", Some(json!({ "token": MOCK_TOKEN, "expiresIn": 3600 })))
}

async fn mock_reauth(body: web::Json<Value>, state: web::Data<MockState>) -> HttpResponse {
    state.record("/api/auth/reauth", &body);
    if state.mode() == MockMode::ReauthRejected {
        return envelope(1001, "Failed to generate token", None::<Value>);
    }
    envelope(0, "ok", Some(json!({ "token": MOCK_REAUTH_TOKEN, "expiresIn": 3600 })))
}

async fn mock_biometric_challenge(body: web::Json<Value>, state: web::Data<MockState>) -> HttpResponse {
    state.record("/api/auth/biometric-challenge", &body);
    if state.mode() == MockMode::ChallengeRejected {
        return envelope(1008, "Biometric credential not bound", None::<Value>);
    }
    envelope(0, "ok", Some(json!({ "challengeNonce": MOCK_NONCE, "expiresIn": 60 })))
}

async fn mock_biometric_verify(body: web::Json<Value>, state: web::Data<MockState>) -> HttpResponse {
    state.record("/api/auth/biometric-verify", &body);
    if state.mode() == MockMode::BiometricVerifyRejected {
        return envelope(1009, "Biometric signature verification failed", None::<Value>);
    }
    envelope(0, "ok", Some(json!({ "token": MOCK_TOKEN, "expiresIn": 3600 })))
}

async fn mock_biometric_bind(body: web::Json<Value>, state: web::Data<MockState>) -> HttpResponse {
    state.record("/api/auth/biometric-bind", &body);
    let public_key = body.get("publicKey").and_then(|v| v.as_str()).unwrap_or("");
    envelope(0, "ok", Some(json!({ "bound": !public_key.is_empty() })))
}

// ==================== 场景 ①：配对 → 验码 → token ====================

#[tokio::test]
async fn pairing_full_flow_via_http() {
    let mock = MockDesktop::start(MockMode::Happy).await;
    let client = AuthHttpClient::new();
    let base = mock.base_url();

    // 1. 发起配对：拿到配对码（桌面端 UI 展示，移动端仅需确认请求成功）
    let pairing = client
        .request_pairing(&base, "device-1", "test-phone", "fp-1")
        .await
        .expect("pairing should succeed");
    assert_eq!(pairing.pairing_code, MOCK_PAIRING_CODE);
    assert_eq!(pairing.expires_in, 300);

    // 请求体字段名（camelCase wire shape）
    let body = mock.last_body("/api/auth/pairing");
    assert_eq!(body["deviceId"], "device-1");
    assert_eq!(body["deviceName"], "test-phone");
    assert_eq!(body["fingerprint"], "fp-1");

    // 2. 验证配对码：签发 JWT
    let token = client
        .verify_pairing_code(
            &base,
            DeviceAuthContext {
                device_id: "device-1",
                device_name: "test-phone",
                fingerprint: "fp-1",
                uid_hash: Some("uid-hash-1"),
            },
            MOCK_PAIRING_CODE,
            "192.168.1.5",
        )
        .await
        .expect("verify should succeed");
    assert_eq!(token.token, MOCK_TOKEN);
    assert_eq!(token.expires_in, 3600);

    let body = mock.last_body("/api/auth/verify");
    assert_eq!(body["pairingCode"], MOCK_PAIRING_CODE);
    // address 必填：桌面端写入配对记录的客户端地址，取 TargetDevice.address
    assert_eq!(body["address"], "192.168.1.5");
    // uidHash（可选）：新客户端携设备唯一 ID 哈希，桌面端据其合并指纹再派生后的配对
    assert_eq!(body["uidHash"], "uid-hash-1");

    mock.shutdown().await;
}

// ==================== 场景 ②：QR 认证 ====================

#[tokio::test]
async fn qr_connect_success() {
    let mock = MockDesktop::start(MockMode::Happy).await;
    let client = AuthHttpClient::new();
    let base = mock.base_url();

    let token = client
        .qr_connect(
            &base,
            DeviceAuthContext {
                device_id: "device-2",
                device_name: "test-phone",
                fingerprint: "fp-2",
                uid_hash: Some("uid-hash-2"),
            },
            "qr-token-abc",
            "192.168.1.6",
        )
        .await
        .expect("qr connect should succeed");
    assert_eq!(token.token, MOCK_TOKEN);

    let body = mock.last_body("/api/auth/qr-connect");
    assert_eq!(body["qrToken"], "qr-token-abc");
    assert_eq!(body["deviceId"], "device-2");
    assert_eq!(body["address"], "192.168.1.6");
    assert_eq!(body["uidHash"], "uid-hash-2");

    mock.shutdown().await;
}

// ==================== 场景 ③：reauth 刷新 token ====================

#[tokio::test]
async fn reauth_refreshes_token() {
    let mock = MockDesktop::start(MockMode::Happy).await;
    let client = AuthHttpClient::new();
    let base = mock.base_url();

    // reauth 走 body（sessionToken）而非 Authorization 头——桌面端从 body 验 token
    let token = client
        .reauth(&base, "device-1", "fp-1", Some("uid-hash-1"), MOCK_TOKEN)
        .await
        .expect("reauth should succeed");
    assert_eq!(token.token, MOCK_REAUTH_TOKEN, "reauth 应签发刷新后的新 token");

    let body = mock.last_body("/api/auth/reauth");
    assert_eq!(body["sessionToken"], MOCK_TOKEN);
    assert_eq!(body["deviceId"], "device-1");
    assert_eq!(body["fingerprint"], "fp-1");
    assert_eq!(body["uidHash"], "uid-hash-1");

    mock.shutdown().await;
}

// ==================== 场景 ④：业务错误码映射 ====================

#[tokio::test]
async fn business_error_codes_map_to_auth_error() {
    let mock = MockDesktop::start(MockMode::VerifyRejected).await;
    let client = AuthHttpClient::new();
    let base = mock.base_url();

    // verify 回 1005（配对码无效）：AppError::Auth 携带业务码与原因
    let err = client
        .verify_pairing_code(
            &base,
            DeviceAuthContext {
                device_id: "device-1",
                device_name: "test-phone",
                fingerprint: "fp-1",
                uid_hash: None, // 老客户端：不携带 uidHash，桌面端跳过合并回退常规配对
            },
            "000000",
            "192.168.1.5",
        )
        .await
        .expect_err("1005 should be an error");
    match &err {
        AppError::Auth(msg) => {
            assert!(msg.contains("1005"), "err 应携带业务码: {}", msg);
            assert!(
                msg.contains("Invalid or expired pairing code"),
                "err 应透传桌面消息: {}",
                msg
            );
        }
        other => panic!("expected AppError::Auth, got {:?}", other),
    }

    mock.shutdown().await;
}

#[tokio::test]
async fn biometric_challenge_rejection_1008() {
    let mock = MockDesktop::start(MockMode::ChallengeRejected).await;
    let client = AuthHttpClient::new();
    let base = mock.base_url();

    let err = client
        .biometric_challenge(&base, "device-1", "fp-1")
        .await
        .expect_err("1008 should be an error");
    match &err {
        AppError::Auth(msg) => {
            assert!(msg.contains("1008"), "err 应携带业务码: {}", msg);
            assert!(msg.contains("credential not bound"), "err: {}", msg);
        }
        other => panic!("expected AppError::Auth, got {:?}", other),
    }

    mock.shutdown().await;
}

#[tokio::test]
async fn biometric_verify_rejection_1009() {
    let mock = MockDesktop::start(MockMode::BiometricVerifyRejected).await;
    let client = AuthHttpClient::new();
    let base = mock.base_url();

    let err = client
        .biometric_verify(&base, "device-1", "fp-1", MOCK_NONCE, "bad-sig")
        .await
        .expect_err("1009 should be an error");
    match &err {
        AppError::Auth(msg) => {
            assert!(msg.contains("1009"), "err 应携带业务码: {}", msg);
            assert!(msg.contains("signature verification failed"), "err: {}", msg);
        }
        other => panic!("expected AppError::Auth, got {:?}", other),
    }

    mock.shutdown().await;
}

#[tokio::test]
async fn reauth_rejection_1001() {
    let mock = MockDesktop::start(MockMode::ReauthRejected).await;
    let client = AuthHttpClient::new();
    let base = mock.base_url();

    let err = client
        .reauth(&base, "device-1", "fp-1", None, MOCK_TOKEN)
        .await
        .expect_err("1001 should be an error");
    match &err {
        AppError::Auth(msg) => {
            assert!(msg.contains("1001"), "err 应携带业务码: {}", msg);
        }
        other => panic!("expected AppError::Auth, got {:?}", other),
    }

    mock.shutdown().await;
}

// ==================== 场景 ⑤：传输层故障 ====================

#[tokio::test]
async fn connection_refused_is_internal_error() {
    let client = AuthHttpClient::with_client(reqwest::Client::builder().no_proxy().build().expect("build client"));

    // 连回环保留端口 1：无监听者，连接立即拒绝。
    // （「bind :0 再释放」在并行测试下有端口抢占竞态；固定不可用端口无竞态）
    // no_proxy：本机系统代理会把不可达目标伪装成 502，掩盖真实的连接拒绝
    let err = client
        .request_pairing("http://127.0.0.1:1", "device-1", "test-phone", "fp-1")
        .await
        .expect_err("refused connection should be an error");
    match &err {
        AppError::Internal(msg) => {
            assert!(msg.contains("HTTP request"), "err 应带请求上下文: {}", msg);
            assert!(msg.contains("failed"), "err: {}", msg);
        }
        other => panic!("expected AppError::Internal, got {:?}", other),
    }
}

// ==================== 场景 ⑥：生物 challenge → verify（HTTP 形状） ====================

#[tokio::test]
async fn biometric_challenge_verify_http_shape() {
    let mock = MockDesktop::start(MockMode::Happy).await;
    let client = AuthHttpClient::new();
    let base = mock.base_url();

    // 1. challenge：拿到一次性挑战值（本地不弹生物识别，只验证 HTTP 请求/解析）
    let challenge = client
        .biometric_challenge(&base, "device-1", "fp-1")
        .await
        .expect("challenge should succeed");
    assert_eq!(challenge.challenge_nonce, MOCK_NONCE);
    assert_eq!(challenge.expires_in, 60);

    let body = mock.last_body("/api/auth/biometric-challenge");
    assert_eq!(body["deviceId"], "device-1");
    // 桌面端 BiometricChallengeRequest 字段名是 deviceFingerprint
    assert_eq!(body["deviceFingerprint"], "fp-1");

    // 2. verify：回传签名（真实签名由 Kotlin bridge 产生，此处形状验证）
    let token = client
        .biometric_verify(&base, "device-1", "fp-1", MOCK_NONCE, "base64-signature")
        .await
        .expect("verify should succeed");
    assert_eq!(token.token, MOCK_TOKEN);

    let body = mock.last_body("/api/auth/biometric-verify");
    assert_eq!(body["challengeNonce"], MOCK_NONCE);
    assert_eq!(body["signature"], "base64-signature");
    assert_eq!(body["deviceFingerprint"], "fp-1");

    mock.shutdown().await;
}

// ==================== 场景 ⑦：生物凭证绑定（HTTP 形状） ====================

#[tokio::test]
async fn biometric_bind_http_shape() {
    let mock = MockDesktop::start(MockMode::Happy).await;
    let client = AuthHttpClient::new();
    let base = mock.base_url();

    // 绑定：公钥注册，返回 bound=true
    let data = client
        .biometric_bind(&base, "device-1", "fp-1", "spki-base64", "jwt-token")
        .await
        .expect("bind should succeed");
    assert!(data.bound, "非空公钥绑定 bound 必须为 true");

    let body = mock.last_body("/api/auth/biometric-bind");
    assert_eq!(body["deviceId"], "device-1");
    assert_eq!(body["deviceFingerprint"], "fp-1");
    assert_eq!(body["publicKey"], "spki-base64");
    assert_eq!(body["sessionToken"], "jwt-token");

    // 解绑：空公钥，返回 bound=false
    let data = client
        .biometric_bind(&base, "device-1", "fp-1", "", "jwt-token")
        .await
        .expect("unbind should succeed");
    assert!(!data.bound, "空公钥解绑 bound 必须为 false");

    mock.shutdown().await;
}

// ==================== resolve_base_url：有/无 target ====================

#[tokio::test]
async fn resolve_base_url_happy_path_with_target() {
    // 复用 WS mock：connect_without_emit 保存 target 成功建连，在 disconnect
    // （清 target）之前解析 base URL
    let ws_server = common::MockDesktopServer::start().await;
    let manager = ConnectionManager::new();

    manager
        .connect_without_emit("127.0.0.1".to_string(), ws_server.addr.port(), None)
        .await
        .expect("ws connect should succeed");

    let base = resolve_base_url(&manager).await.expect("target present");
    assert_eq!(base, format!("http://127.0.0.1:{}", ws_server.addr.port()));

    manager.disconnect().await;
    clear_global_token();
}
