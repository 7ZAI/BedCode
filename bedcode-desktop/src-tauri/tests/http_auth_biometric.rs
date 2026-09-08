//! 生物认证 HTTP 端点集成测试（ticket 01：/api/auth/biometric-challenge + /api/auth/biometric-verify）
//!
//! 独立测试二进制（tests/ 下每个文件是独立编译产物），与 server_integration
//! 的全局单例（AppContext / WsSessionRegistry）互相隔离。
//! 真实启动 Actix 服务器 + reqwest 客户端走完整链路：中间件公开前缀放行 →
//! 路由 → controller → auth_service 抽取函数 → DB / 挑战管理器。
//!
//! 签名配套：测试内用 p256 临时密钥对，公钥按 SPKI DER base64 写入配对记录，
//! 私钥对 nonce 做 r||s 原始格式签名（与移动端 Android Keystore 行为一致，
//! 手法与 biometric.rs 既有单测相同）。
//!
//! 串行化：本文件只含一个 #[tokio::test]，子场景（T1-T6）严格串行。

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::dev::ServerHandle;
use base64::Engine;
use bedcode_lib::db::Database;
use bedcode_lib::events::DesktopSyncEvent;
use bedcode_lib::mdns::advertiser::MdnsAdvertiser;
use bedcode_lib::plugin::PluginHost;
use bedcode_lib::server::app::start_http_server;
use bedcode_lib::session::{SessionConfigManager, SessionManager};
use bedcode_lib::system::app_context::AppContext;
use bedcode_lib::system::app_context::AppContextBuilder;
use bedcode_lib::system::constants::network::SYNC_EVENT_BROADCAST_CAPACITY;
use bedcode_lib::system::info::SystemInfo;
use bedcode_lib::utils::auth::jwt::JwtService;
use bedcode_lib::utils::auth::QrTokenManager;
use bedcode_lib::AppConfig;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::SigningKey;
use p256::pkcs8::EncodePublicKey;

/// 构造带 JSON body 的 POST 请求
///
/// reqwest 未启用 "json" feature（与 server_integration.rs 同因：读响应手工
/// serde_json 解析），请求侧同样手工序列化 + 显式 Content-Type
fn post_json(client: &reqwest::Client, url: &str, value: &serde_json::Value) -> reqwest::RequestBuilder {
    client
        .post(url)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(value).expect("serialize json body failed"))
}

/// 探测空闲端口：绑定 127.0.0.1:0 由 OS 分配，立即释放后交给服务器绑定
fn pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("probe free port failed");
    listener.local_addr().expect("read probed port failed").port()
}

/// 启动测试服务器：真实 `start_http_server` + 默认网络配置（端口显式传入）
async fn spawn_test_server(port: u16) -> io::Result<(ServerHandle, tokio::task::JoinHandle<io::Result<()>>)> {
    let config = AppConfig::default().network;
    let (handle, server) = start_http_server(port, &config).await?;
    let server_task = tokio::spawn(server);
    Ok((handle, server_task))
}

/// 组装真实服务 AppContext（app_handle=None 无头模式），每个测试进程只 init 一次
///
/// 与 ws_pairing_auth.rs 同一策略：全部服务真实实现 + 内存 SQLite，仅 Tauri
/// 前端事件能力降级。生物认证端点只用到 db / biometric_challenges / app_handle，
/// 其余字段（session/plugin/mdns 等）仍按生产组合方式填齐，避免 AppContext
/// 缺字段在其余路由意外触发时 panic
async fn init_test_app_context() {
    static INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if INIT.get().is_some() {
        return;
    }
    {
        let db = Arc::new(tokio::sync::Mutex::new(
            Database::new(Path::new(":memory:")).expect("create in-memory db failed"),
        ));
        db.lock().await.init_schema().expect("init db schema failed");

        let plugins_dir = std::env::temp_dir().join(format!("bedcode-bioitest-plugins-{}", std::process::id()));
        std::fs::create_dir_all(&plugins_dir).expect("create temp plugins dir failed");

        let session_db = Database::new(Path::new(":memory:")).expect("create session db failed");
        session_db.init_schema().expect("init session db schema failed");
        let session_manager = Arc::new(SessionManager::from_database(session_db, Arc::new(PathBuf::from("."))));
        let config_manager = Arc::new(SessionConfigManager::new(db.clone()));
        let plugin_host = Arc::new(
            PluginHost::new(
                db.clone(),
                &plugins_dir,
                session_manager.clone(),
                config_manager.clone(),
                None,
            )
            .await,
        );
        plugin_host.init_message_bus().await;

        let pairing_service = Arc::new(bedcode_lib::server::services::pairing_service::PairingService::new());
        let qr_manager = Arc::new(QrTokenManager::new());
        let mdns_advertiser = Arc::new(tokio::sync::RwLock::new(MdnsAdvertiser::new()));
        let (sync_tx, _) = tokio::sync::broadcast::channel::<DesktopSyncEvent>(SYNC_EVENT_BROADCAST_CAPACITY);
        let system_info = Arc::new(SystemInfo::collect());

        AppContextBuilder::new()
            .db(db.clone())
            .session_manager(session_manager.clone())
            .config_manager(config_manager.clone())
            .plugin_host(plugin_host.clone())
            .pairing_service(pairing_service.clone())
            .qr_manager(qr_manager.clone())
            .mdns_advertiser(mdns_advertiser.clone())
            .app_handle(None)
            .sync_tx(sync_tx)
            .resource_dir(Arc::new(PathBuf::from(".")))
            .system_info(system_info)
            .build_and_init();
        let _ = INIT.set(());
    }
}

/// 发起请求，连接失败（服务器 worker 尚未就绪）时按 25ms 间隔重试直至超时
async fn send_until(request: reqwest::RequestBuilder, timeout: Duration) -> reqwest::Result<reqwest::Response> {
    let deadline = Instant::now() + timeout;
    loop {
        match request.try_clone().expect("request must be cloneable").send().await {
            Ok(resp) => return Ok(resp),
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await;
                tokio::task::yield_now().await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// 解析响应体为 JSON
async fn body_json(resp: reqwest::Response) -> serde_json::Value {
    let bytes = resp.bytes().await.expect("read response body failed");
    serde_json::from_slice(&bytes).expect("response body must be valid JSON")
}

/// 生成 p256 临时密钥对：返回 (SPKI base64 公钥, SigningKey)
///
/// 公钥编码与移动端 Android Keystore PublicKey.getEncoded() 一致（SPKI X.509 DER）
fn make_keypair() -> (String, SigningKey) {
    let signing_key = SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();
    let spki_der = verifying_key.to_public_key_der().expect("encode public key");
    let spki_b64 = base64::engine::general_purpose::STANDARD.encode(spki_der.as_bytes());
    (spki_b64, signing_key)
}

/// 对消息做 r||s 原始格式签名并 base64（与移动端 Keystore 转换后的输出一致）
fn sign_message(signing_key: &SigningKey, message: &str) -> String {
    let signature: p256::ecdsa::Signature = signing_key.sign(message.as_bytes());
    let (r, s) = signature.split_scalars();
    let mut raw = r.to_bytes().to_vec();
    raw.extend_from_slice(&s.to_bytes());
    base64::engine::general_purpose::STANDARD.encode(&raw)
}

#[tokio::test]
async fn http_biometric_auth_contract() {
    // 测试日志输出到 harness；重复 init 静默跳过
    if tracing_subscriber::fmt().with_test_writer().try_init().is_err() {
        tracing::debug!("tracing subscriber already initialized");
    }

    init_test_app_context().await;

    let port = pick_free_port();
    let (handle, server_task) = spawn_test_server(port).await.expect("test server must start");
    let base = format!("http://127.0.0.1:{port}");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build reqwest client failed");

    let challenge_url = format!("{base}/api/auth/biometric-challenge");
    let verify_url = format!("{base}/api/auth/biometric-verify");
    let bind_url = format!("{base}/api/auth/biometric-bind");

    // 测试用配对：SPKI base64 公钥写入 DB（等价于已配对 + 已绑定生物凭证）
    let (spki_b64, signing_key) = make_keypair();
    let fingerprint = "fp-bio-http-001";
    let pairing_id = {
        let db_guard = AppContext::global().db().lock().await;
        db_guard
            .add_pairing("Bio Phone", fingerprint, &spki_b64, None)
            .expect("add pairing failed")
    };

    // ==================== T1：未配对指纹调 challenge → 1008 ====================

    let resp = send_until(
        post_json(
            &client,
            &challenge_url,
            &serde_json::json!({
                "deviceId": "unpaired-dev",
                "deviceFingerprint": "fp-never-paired",
            }),
        ),
        Duration::from_secs(5),
    )
    .await
    .expect("T1 challenge request must reach server");
    // handler 返回 HTTP 200 + 业务码（与既有 verify/qr 端点一致）
    let body = body_json(resp).await;
    assert_eq!(body["code"], 1008, "未配对设备挑战签发必须返回 1008");
    assert!(body["data"].is_null(), "失败响应的 data 字段必须缺省");
    // 未配对指纹不落连接历史（find_pairing_id_by_fingerprint 为 None → no-op）
    let db_guard = AppContext::global().db().lock().await;
    assert!(
        db_guard
            .get_connection_history("unpaired-dev")
            .expect("query history")
            .is_empty(),
        "未配对指纹不应产生连接历史"
    );
    drop(db_guard);

    // ==================== T2：已配对设备 challenge → 200 + nonce ====================

    let resp = send_until(
        post_json(
            &client,
            &challenge_url,
            &serde_json::json!({
                "deviceId": pairing_id,
                "deviceFingerprint": fingerprint,
            }),
        ),
        Duration::from_secs(5),
    )
    .await
    .expect("T2 challenge request must reach server");
    let body = body_json(resp).await;
    assert_eq!(body["code"], 0, "已配对设备挑战签发必须成功");
    let nonce = body["data"]["challengeNonce"]
        .as_str()
        .expect("challenge nonce must be present")
        .to_string();
    assert_eq!(nonce.len(), 32, "nonce 为 16 字节 hex");
    assert_eq!(
        body["data"]["expiresIn"].as_u64(),
        Some(60),
        "挑战有效期必须等于 BIO_CHALLENGE_TTL_SECS"
    );

    // ==================== T3：正确签名 verify → 200 + 可验 JWT + 公钥保留 ====================

    let signature = sign_message(&signing_key, &nonce);
    let resp = send_until(
        post_json(
            &client,
            &verify_url,
            &serde_json::json!({
                "deviceId": pairing_id,
                "deviceFingerprint": fingerprint,
                "challengeNonce": nonce,
                "signature": signature,
            }),
        ),
        Duration::from_secs(5),
    )
    .await
    .expect("T3 verify request must reach server");
    let body = body_json(resp).await;
    assert_eq!(body["code"], 0, "正确签名验证必须成功");
    let token = body["data"]["token"]
        .as_str()
        .expect("token must be present")
        .to_string();
    assert!(!token.is_empty(), "token 必须非空");

    // JWT 可被服务端验证，且 sub 指向配对记录 id
    let claims = JwtService::new()
        .verify_token_with_expiry(&token)
        .expect("returned token must be verifiable");
    assert_eq!(claims.sub, pairing_id, "JWT sub 必须等于配对记录 id");
    assert_eq!(claims.fingerprint.as_deref(), Some(fingerprint), "JWT 必须携带设备指纹");

    // add_pairing 必须保留 public_key（新的 verify 端点传 pairing.public_key，
    // 既有的 verify/qr 端点传空串会清空生物凭证——此处必须防覆盖）
    {
        let db_guard = AppContext::global().db().lock().await;
        let pairing = db_guard
            .get_pairing_by_fingerprint(fingerprint)
            .expect("query pairing")
            .expect("pairing must exist");
        assert_eq!(pairing.public_key, spki_b64, "验证后生物公钥必须原样保留");
        assert_eq!(pairing.device_name, "Bio Phone", "add_pairing 不应改设备展示名");
    }

    // 连接历史出现一次 biometric success
    {
        let db_guard = AppContext::global().db().lock().await;
        let history = db_guard.get_connection_history(&pairing_id).expect("query history");
        assert!(
            history
                .iter()
                .any(|h| h.auth_method == "biometric" && h.result == "success"),
            "验证成功必须记录 biometric/success 连接历史"
        );
    }

    // ==================== T4：同一 nonce 二次 verify → 1009（单次有效） ====================

    let resp = send_until(
        post_json(
            &client,
            &verify_url,
            &serde_json::json!({
                "deviceId": pairing_id,
                "deviceFingerprint": fingerprint,
                "challengeNonce": nonce,
                "signature": signature,
            }),
        ),
        Duration::from_secs(5),
    )
    .await
    .expect("T4 replay verify must reach server");
    let body = body_json(resp).await;
    assert_eq!(body["code"], 1009, "已消费的挑战重复验证必须返回 1009");
    assert_eq!(
        body["message"], "Biometric challenge invalid or expired",
        "重复消费的错因应指向挑战无效"
    );

    // ==================== T5：错误签名 verify → 1009 ====================

    // 新挑战（旧 nonce 已消费）
    let resp = send_until(
        post_json(
            &client,
            &challenge_url,
            &serde_json::json!({
                "deviceId": pairing_id,
                "deviceFingerprint": fingerprint,
            }),
        ),
        Duration::from_secs(5),
    )
    .await
    .expect("T5 challenge request must reach server");
    let body = body_json(resp).await;
    let nonce_t5 = body["data"]["challengeNonce"]
        .as_str()
        .expect("challenge nonce must be present")
        .to_string();

    // 用另一把密钥签名（签名不匹配）
    let (_, other_key) = make_keypair();
    let wrong_signature = sign_message(&other_key, &nonce_t5);
    let resp = send_until(
        post_json(
            &client,
            &verify_url,
            &serde_json::json!({
                "deviceId": pairing_id,
                "deviceFingerprint": fingerprint,
                "challengeNonce": nonce_t5,
                "signature": wrong_signature,
            }),
        ),
        Duration::from_secs(5),
    )
    .await
    .expect("T5 wrong-signature verify must reach server");
    let body = body_json(resp).await;
    assert_eq!(body["code"], 1009, "错误签名必须返回 1009");
    assert_eq!(
        body["message"], "Biometric signature verification failed",
        "签名不符的错因应指向验签失败"
    );

    // ==================== T6：篡改 nonce verify → 1009（challenge mismatch 分支） ====================

    let resp = send_until(
        post_json(
            &client,
            &challenge_url,
            &serde_json::json!({
                "deviceId": pairing_id,
                "deviceFingerprint": fingerprint,
            }),
        ),
        Duration::from_secs(5),
    )
    .await
    .expect("T6 challenge request must reach server");
    let body = body_json(resp).await;
    let nonce_t6 = body["data"]["challengeNonce"]
        .as_str()
        .expect("challenge nonce must be present")
        .to_string();
    // 翻转首字符得到必定不同的 nonce
    let tampered = if nonce_t6.starts_with('0') {
        format!("1{}", &nonce_t6[1..])
    } else {
        format!("0{}", &nonce_t6[1..])
    };
    let tampered_sig = sign_message(&signing_key, &tampered);
    let resp = send_until(
        post_json(
            &client,
            &verify_url,
            &serde_json::json!({
                "deviceId": pairing_id,
                "deviceFingerprint": fingerprint,
                "challengeNonce": tampered,
                "signature": tampered_sig,
            }),
        ),
        Duration::from_secs(5),
    )
    .await
    .expect("T6 tampered-nonce verify must reach server");
    let body = body_json(resp).await;
    assert_eq!(body["code"], 1009, "篡改 nonce 必须返回 1009");
    assert_eq!(
        body["message"], "Biometric challenge invalid or expired",
        "nonce 不匹配的错因应指向挑战无效"
    );

    // ==================== T7：有效 token 绑定新公钥 → code 0 + bound=true ====================

    let (new_spki_b64, _new_key) = make_keypair();
    let resp = send_until(
        post_json(
            &client,
            &bind_url,
            &serde_json::json!({
                "deviceId": pairing_id,
                "deviceFingerprint": fingerprint,
                "publicKey": new_spki_b64,
                "sessionToken": token,
            }),
        ),
        Duration::from_secs(5),
    )
    .await
    .expect("T7 bind request must reach server");
    let body = body_json(resp).await;
    assert_eq!(body["code"], 0, "有效 token 绑定公钥必须成功");
    assert_eq!(body["data"]["bound"], true, "非空公钥绑定 bound 必须为 true");
    {
        let db_guard = AppContext::global().db().lock().await;
        let pairing = db_guard
            .get_pairing_by_fingerprint(fingerprint)
            .expect("query pairing")
            .expect("pairing must exist");
        assert_eq!(pairing.public_key, new_spki_b64, "绑定后公钥必须更新为新值");
    }

    // ==================== T8：空公钥解绑 → code 0 + bound=false ====================

    let resp = send_until(
        post_json(
            &client,
            &bind_url,
            &serde_json::json!({
                "deviceId": pairing_id,
                "deviceFingerprint": fingerprint,
                "publicKey": "",
                "sessionToken": token,
            }),
        ),
        Duration::from_secs(5),
    )
    .await
    .expect("T8 unbind request must reach server");
    let body = body_json(resp).await;
    assert_eq!(body["code"], 0, "空公钥解绑必须成功");
    assert_eq!(body["data"]["bound"], false, "空公钥解绑 bound 必须为 false");
    {
        let db_guard = AppContext::global().db().lock().await;
        let pairing = db_guard
            .get_pairing_by_fingerprint(fingerprint)
            .expect("query pairing")
            .expect("pairing must exist");
        assert_eq!(pairing.public_key, "", "解绑后公钥必须清空");
    }

    // ==================== T9：无效 token 绑定 → code 1001 ====================

    let resp = send_until(
        post_json(
            &client,
            &bind_url,
            &serde_json::json!({
                "deviceId": pairing_id,
                "deviceFingerprint": fingerprint,
                "publicKey": spki_b64,
                "sessionToken": "not-a-valid-jwt",
            }),
        ),
        Duration::from_secs(5),
    )
    .await
    .expect("T9 invalid-token bind must reach server");
    let body = body_json(resp).await;
    assert_eq!(body["code"], 1001, "无效 token 绑定必须返回 1001");

    // ==================== T10：token 指纹不匹配 → code 1007 ====================

    // 为另一台设备签发 token，用它绑定 fingerprint 对应设备 → 指纹不一致被拒
    let other_token = JwtService::new()
        .generate_token(
            "other-device-id".to_string(),
            None,
            Some("other-device-fingerprint".to_string()),
        )
        .expect("generate other-device token");
    let resp = send_until(
        post_json(
            &client,
            &bind_url,
            &serde_json::json!({
                "deviceId": pairing_id,
                "deviceFingerprint": fingerprint,
                "publicKey": spki_b64,
                "sessionToken": other_token,
            }),
        ),
        Duration::from_secs(5),
    )
    .await
    .expect("T10 mismatched-token bind must reach server");
    let body = body_json(resp).await;
    assert_eq!(body["code"], 1007, "token 指纹不匹配必须返回 1007");

    // ==================== 收尾：优雅停机 + 清理 ====================

    drop(client);
    tokio::time::sleep(Duration::from_millis(100)).await;
    tokio::time::timeout(Duration::from_secs(10), handle.stop(true))
        .await
        .expect("graceful stop must complete within timeout");
    server_task
        .await
        .expect("server task must not panic")
        .expect("server must exit Ok after graceful stop");

    let plugins_dir = std::env::temp_dir().join(format!("bedcode-bioitest-plugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(plugins_dir);
}
