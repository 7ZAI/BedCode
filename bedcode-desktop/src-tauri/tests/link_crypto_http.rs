//! 链路加密 HTTP 集成测试（issue 02）
//!
//! 真 TrafficFilter 中间件全链路：客户端加密请求 → 过滤器解密 → handler 收明文
//! → 响应加密 → 客户端用 k_resp 解回。全局链为进程级单例，串行化触碰。

use std::sync::Mutex;

use actix_web::{test, web, App, HttpResponse};
use base64::Engine as _;

use bedcode_lib::server::filter::TrafficFilterChain;
use bedcode_lib::server::link_crypto::{
    self, derive_http_traffic_keys, encrypt_http_body, decrypt_http_body, LinkCryptoConfig,
    NEGOTIATION_HEADER,
};
use bedcode_lib::utils::crypto::x25519::{x25519_diffie_hellman, x25519_generate};

/// 全局链 + 配置快照均为进程级单例，测试串行化
static GLOBAL_LOCK: Mutex<()> = Mutex::new(());

/// 身份密钥懒初始化（OnceLock 保 tempdir 存活到进程结束）
fn init_identity_once() {
    static IDENTITY_DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let dir = IDENTITY_DIR.get_or_init(|| tempfile::tempdir().unwrap());
    link_crypto::init_identity(dir.path()).unwrap();
}

/// 客户端模拟：生成临时密钥并对 Kd_pub 派生两方向会话密钥
struct ClientCtx {
    negotiation: String,
    keys: link_crypto::HttpTrafficKeys,
    path: &'static str,
}

fn make_client(path: &'static str) -> ClientCtx {
    let eph = x25519_generate();
    let (_, kd_pub_b64) = link_crypto::identity_parts().unwrap();
    let kd_pub: [u8; 32] = base64::engine::general_purpose::STANDARD
        .decode(kd_pub_b64)
        .unwrap()
        .try_into()
        .unwrap();
    let shared = x25519_diffie_hellman(&eph, &kd_pub).unwrap();
    let keys = derive_http_traffic_keys(shared.as_bytes(), path).unwrap();
    ClientCtx {
        negotiation: format!("v1 {}", base64::engine::general_purpose::STANDARD.encode(eph.public())),
        keys,
        path,
    }
}

impl ClientCtx {
    fn seal_request(&self, plaintext: &[u8]) -> (Vec<u8>, String) {
        use crate_aad_shim::*;
        (
            encrypt_http_body(
                &self.keys.request,
                plaintext,
                &http_aad_shim(bedcode_lib::server::filter::Direction::Inbound, self.path),
            )
            .unwrap(),
            self.negotiation.clone(),
        )
    }

    fn open_response(&self, envelope: &[u8]) -> Vec<u8> {
        use crate_aad_shim::*;
        decrypt_http_body(
            &self.keys.response,
            envelope,
            &http_aad_shim(bedcode_lib::server::filter::Direction::Outbound, self.path),
        )
        .unwrap()
    }
}

/// AAD 构造是模块私有 fn，这里经 pub API 无法直接拿到——集成测试用等价字节
/// 序列复刻（spec §3 固定格式：b"v1" || dir || u32be(len) || path），并断言与
/// 单元测试一致；若协议变更此处会先红。
mod crate_aad_shim {
    pub fn http_aad_shim(
        direction: bedcode_lib::server::filter::Direction,
        path: &str,
    ) -> Vec<u8> {
        let mut aad = Vec::with_capacity(7 + path.len());
        aad.extend_from_slice(b"v1");
        aad.push(match direction {
            bedcode_lib::server::filter::Direction::Inbound => 0x01,
            bedcode_lib::server::filter::Direction::Outbound => 0x02,
        });
        aad.extend_from_slice(&(path.len() as u32).to_be_bytes());
        aad.extend_from_slice(path.as_bytes());
        aad
    }
}

async fn echo_handler(body: web::Bytes) -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "echo": String::from_utf8_lossy(&body) }))
}

fn test_app() -> actix_web::App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    App::new()
        .wrap(bedcode_lib::server::middleware::http_filter::TrafficFilter)
        .route("/echo", web::post().to(echo_handler))
}

#[actix_web::test]
async fn encrypted_request_plaintext_handler_encrypted_response() {
    let _guard = GLOBAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init_identity_once();

    link_crypto::update_config(LinkCryptoConfig {
        enabled: true,
        ..LinkCryptoConfig::default()
    });
    let chain = TrafficFilterChain::global();
    chain.clear();
    // 直接注册而非 sync_registration：后者受 REGISTERED 标志幂等保护，
    // 与测试的 chain.clear() 组合会跳过重注册（标志仍在但链已空）
    link_crypto::register_into(&chain);

    let app = test::init_service(test_app()).await;
    let client = make_client("/echo");
    let req_plain = br#"{"ping":"pong"}"#.to_vec();
    let (sealed, negotiation) = client.seal_request(&req_plain);

    let req = test::TestRequest::post()
        .uri("/echo")
        .insert_header((NEGOTIATION_HEADER, negotiation.as_str()))
        .set_payload(sealed)
        .to_request();
    let res = test::call_service(&app, req).await;
    assert!(res.status().is_success(), "status = {}", res.status());

    let body = test::read_body(res).await;
    let opened = client.open_response(&body);
    let value: serde_json::Value = serde_json::from_slice(&opened).unwrap();
    assert_eq!(
        value["echo"],
        serde_json::json!(r#"{"ping":"pong"}"#),
        "handler 应收到解密后的明文请求（整包回显）"
    );

    // 收尾：清空全局链与快照，不污染其他集成测试
    chain.clear();
    link_crypto::update_config(LinkCryptoConfig::default());
}

#[actix_web::test]
async fn tampered_envelope_rejected_with_400() {
    let _guard = GLOBAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init_identity_once();

    link_crypto::update_config(LinkCryptoConfig {
        enabled: true,
        ..LinkCryptoConfig::default()
    });
    let chain = TrafficFilterChain::global();
    chain.clear();
    // 同上：用 register_into 绕开 REGISTERED 标志与 clear() 的状态脱节
    link_crypto::register_into(&chain);

    let app = test::init_service(test_app()).await;
    let client = make_client("/echo");
    let (mut sealed, negotiation) = client.seal_request(b"hello");
    // 破坏信封 JSON 结构（非 GCM 层篡改，走 malformed 路径）
    sealed.truncate(sealed.len() / 2);

    let req = test::TestRequest::post()
        .uri("/echo")
        .insert_header((NEGOTIATION_HEADER, negotiation.as_str()))
        .set_payload(sealed)
        .to_request();
    let res = test::call_service(&app, req).await;
    assert_eq!(res.status(), actix_web::http::StatusCode::BAD_REQUEST);
    let text = test::read_body(res).await;
    assert!(
        text.windows(7).any(|w| w == b"decrypt") || text.windows(9).any(|w| w == b"malformed"),
        "400 响应应携带失败原因: {}",
        String::from_utf8_lossy(&text)
    );

    chain.clear();
    link_crypto::update_config(LinkCryptoConfig::default());
}

#[actix_web::test]
async fn strict_policy_rejects_unnegotiated_requests() {
    let _guard = GLOBAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init_identity_once();

    // allowPlaintextFallback=false：未携带协商头的非豁免请求一律拒绝
    link_crypto::update_config(LinkCryptoConfig {
        enabled: true,
        allow_plaintext_fallback: false,
        ..LinkCryptoConfig::default()
    });
    let chain = TrafficFilterChain::global();
    chain.clear();
    // 同上：用 register_into 绕开 REGISTERED 标志与 clear() 的状态脱节
    link_crypto::register_into(&chain);

    let app = test::init_service(test_app()).await;
    let req = test::TestRequest::post()
        .uri("/echo")
        .set_payload("plain legacy request")
        .to_request();
    let res = test::call_service(&app, req).await;
    assert_eq!(res.status(), actix_web::http::StatusCode::BAD_REQUEST);

    chain.clear();
    link_crypto::update_config(LinkCryptoConfig::default());
}
