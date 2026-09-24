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
use bedcode_lib::mdns::advertiser::MdnsAdvertiser;
use bedcode_lib::wasm_core::PluginHost;
use bedcode_lib::server::core::app::start_http_server;
use bedcode_lib::system::app_context::AppContext;
use bedcode_lib::system::app_context::AppContextBuilder;
use bedcode_lib::system::info::SystemInfo;
use bedcode_lib::utils::auth::jwt::JwtService;
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

/// 会话中心插件 id（认证端点编排的权威实现方）
const SESSION_PLUGIN_ID: &str = "com.bedcode.terminal-session";

/// 随包插件产物目录（`cargo test` 前须重建产物，见 AGENTS §3）
///
/// 产物缺失时**显性失败**而非跳过：本 target 的生物认证链没有别的驱动方式，
/// 静默 `[skip]` 会把「未验证」伪装成「通过」。
fn bundled_plugins_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/plugins/desktop");
    assert!(
        dir.join("com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm")
            .exists(),
        "插件产物缺失：先跑 `node scripts/plugin-build.js --plugin com.bedcode.terminal-session`（workdir bedcode-desktop），目录 {}",
        dir.display()
    );
    dir
}

/// 无头集成测试的会话中心插件私有库根（经 [`WasmHostContext::set_plugin_db_root`]
/// 注入；认证记录下沉 v24 后配对 / 历史 / 计数真源在插件私有库，无私有库的
/// 无头上下文无法驱动认证链路）。进程级固定：init 一次，测试结束清理。
fn session_plugin_db_root() -> &'static PathBuf {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("bedcode-bioitest-pluginroot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    })
}

/// 会话中心插件私有库文件（插件 activate 后存在，schema 见插件
/// `auth_records::store::SCHEMA`：auth_pairings / auth_connection_history）
fn session_plugin_db_path() -> PathBuf {
    session_plugin_db_root().join("com.bedcode.terminal-session").join("plugin.db")
}

/// 白盒种子：直写认证中心私有库 `auth_pairings` 播种配对（id 固定，测试持有；
/// schema 由插件 activate 建出）
fn seed_pairing_in_plugin_db(id: &str, device_name: &str, fingerprint: &str) {
    let conn = rusqlite::Connection::open(session_plugin_db_path()).expect("open plugin db");
    conn.execute(
        "INSERT OR REPLACE INTO auth_pairings \
         (id, device_name, device_fingerprint, address, uid_hash, paired_at, last_seen, \
          connect_count, is_active) \
         VALUES (?1, ?2, ?3, NULL, NULL, '2026-09-22T00:00:00Z', NULL, 1, 1)",
        rusqlite::params![id, device_name, fingerprint],
    )
    .expect("seed pairing into plugin db");
}

/// 白盒断言：认证中心私有库中某设备的连接历史（auth_method, result）
fn plugin_db_history(device_id: &str) -> Vec<(String, String)> {
    let conn = rusqlite::Connection::open(session_plugin_db_path()).expect("open plugin db");
    let mut stmt = conn
        .prepare("SELECT auth_method, result FROM auth_connection_history WHERE device_id = ?1")
        .expect("prepare history query");
    let rows = stmt
        .query_map(rusqlite::params![device_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("query history");
    rows.filter_map(std::result::Result::ok).collect()
}

/// 白盒断言：认证中心私有库中某指纹的配对显示名（无行 → None）
fn plugin_db_pairing_name(fingerprint: &str) -> Option<String> {
    let conn = rusqlite::Connection::open(session_plugin_db_path()).expect("open plugin db");
    conn.query_row(
        "SELECT device_name FROM auth_pairings WHERE device_fingerprint = ?1",
        rusqlite::params![fingerprint],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

/// 白盒断言/种子（异步版本）：宿主主库 `plugin_secrets` 公钥读
async fn biometric_public_key_in_db(fingerprint: &str) -> Option<String> {
    let db_guard = AppContext::global().db().lock().await;
    db_guard
        .conn()
        .query_row(
            "SELECT value FROM plugin_secrets WHERE plugin_id = ?1 AND key = ?2",
            rusqlite::params![SESSION_PLUGIN_ID, format!("biometric:{fingerprint}")],
            |row| row.get::<_, String>(0),
        )
        .ok()
}

/// 白盒种子：写宿主主库 `plugin_secrets` 生物凭证公钥（幂等覆盖）
async fn seed_biometric_public_key(fingerprint: &str, public_key: &str) {
    let db_guard = AppContext::global().db().lock().await;
    db_guard
        .conn()
        .execute(
            "INSERT INTO plugin_secrets (plugin_id, key, value, updated_at) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(plugin_id, key) DO UPDATE SET value = ?3, updated_at = ?4",
            rusqlite::params![
                SESSION_PLUGIN_ID,
                format!("biometric:{fingerprint}"),
                public_key,
                "2026-09-22T00:00:00Z"
            ],
        )
        .expect("seed biometric public key");
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
/// 前端事件能力降级。
///
/// 票 13：三条生物认证端点（challenge / verify / bind）的编排已整体下沉会话中心
/// 插件——宿主 /api/auth/* 无实现，网关按 PluginRequired 转发，故测试必须加载并
/// 激活**真实插件产物**（插件的公钥验签 / 挑战签发经 host-auth 原语落到主库，
/// 不依赖无头下不可达的插件私有库）。其余字段（session/mdns 等）仍按生产组合
/// 方式填齐，避免 AppContext 缺字段在其余路由意外触发时 panic
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

        let plugins_dir = bundled_plugins_dir();
        // 用户插件目录：独立空目录（复用 plugins_dir 会让随包插件被标成
        // UserInstalled 来源，激活时撞审批门禁）
        let user_plugins_dir =
            std::env::temp_dir().join(format!("bedcode-bioitest-userplugins-{}", std::process::id()));
        std::fs::create_dir_all(&user_plugins_dir).expect("create temp user plugins dir failed");

        let plugin_host = Arc::new(
            PluginHost::new(
                db.clone(),
                &plugins_dir,
                &user_plugins_dir, // 用户插件目录：独立空目录（见上方来源标注说明）
                None,
            )
            .await,
        );
        plugin_host.init_message_bus().await;
        // v24 认证记录下沉：配对/历史真源 = 认证中心私有库。无头上下文无 AppHandle，
        // 必须在 activation 前注入私有库根（activate 建表走 host-plugin-database）
        plugin_host
            .wasm_host_ctx()
            .set_plugin_db_root(Some(session_plugin_db_root().clone()));
        plugin_host
            .activate_plugin(SESSION_PLUGIN_ID, false)
            .await
            .expect("activate com.bedcode.terminal-session (bundled artifact)");

        let mdns_advertiser = Arc::new(tokio::sync::RwLock::new(MdnsAdvertiser::new()));
        let system_info = Arc::new(SystemInfo::collect());

        AppContextBuilder::new()
            .db(db.clone())
            .plugin_host(plugin_host.clone())
            .mdns_advertiser(mdns_advertiser.clone())
            .app_handle(None)
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
    if tracing_subscriber::fmt().with_test_writer().with_max_level(tracing::Level::DEBUG).try_init().is_err() {
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

    // 测试用配对：v24 认证记录下沉后双落点——配对记录在认证中心私有库
    // `auth_pairings`（白盒种子：直写私有库，schema 由 activate 建出）；生物
    // 公钥在宿主 `plugin_secrets`（key = `biometric:<fp>`，§8 指定存储位）
    let (spki_b64, signing_key) = make_keypair();
    let fingerprint = "fp-bio-http-001";
    let pairing_id = "p-bio-http-001";
    seed_biometric_public_key(fingerprint, &spki_b64).await;
    seed_pairing_in_plugin_db(pairing_id, "Bio Phone", fingerprint);

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
    // 未配对指纹不落连接历史（配对记录查询为 None → record_event no-op）
    assert!(
        plugin_db_history("unpaired-dev").is_empty(),
        "未配对指纹不应产生连接历史"
    );

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

    // 配对播种必须保留生物公钥（verify 端点只读公钥验签，不得覆盖/清空——
    // §8 凭据红线：公钥在宿主 plugin_secrets，@plugin_secrets）
    {
        let stored = biometric_public_key_in_db(fingerprint).await;
        assert_eq!(stored.as_deref(), Some(spki_b64.as_str()), "验证后生物公钥必须原样保留");
        assert_eq!(
            plugin_db_pairing_name(fingerprint).as_deref(),
            Some("Bio Phone"),
            "配对播种不应改设备展示名"
        );
    }

    // 连接历史出现一次 biometric success（真源 = 认证中心私有库）
    {
        let history = plugin_db_history(pairing_id);
        assert!(
            history.iter().any(|(m, r)| m == "biometric" && r == "success"),
            "验证成功必须记录 biometric/success 连接历史, got: {history:?}"
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
        let stored = biometric_public_key_in_db(fingerprint).await;
        assert_eq!(
            stored.as_deref(),
            Some(new_spki_b64.as_str()),
            "绑定后公钥必须更新为新值（宿主 plugin_secrets）"
        );
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
        let stored = biometric_public_key_in_db(fingerprint).await;
        assert_eq!(stored, None, "解绑后公钥键必须删除（宿主 plugin_secrets 无行）");
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
    // 私有库根目录清理（含 auth_pairings / auth_connection_history 测试数据）
    let _ = std::fs::remove_dir_all(session_plugin_db_root());
}
