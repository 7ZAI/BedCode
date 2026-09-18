//! Authentication Service
//!
//! 处理设备配对和认证逻辑

use crate::db::Pairing;
use crate::system::app_context::AppContext;
use crate::utils::auth::biometric::verify_biometric_signature;

// ==================== 生物认证（WS 与 HTTP 共用） ====================

/// 生物认证业务错误（WS / HTTP 调用方各自映射为协议错误格式）
#[derive(Debug)]
pub enum BiometricAuthError {
    /// 设备未配对
    NotPaired,
    /// 已配对但未绑定生物凭证公钥
    CredentialNotBound,
    /// 挑战值无效（不存在/过期/已消费/不匹配），携带底层原因
    ChallengeInvalid(String),
    /// 生物签名校验失败
    SignatureInvalid(String),
    /// 数据库访问失败（internal，调用方按服务端异常处理）
    Database(String),
}

/// 签发生物认证挑战值（WS BiometricRequest 与 HTTP biometric-challenge 共用）
///
/// 设备必须已配对且绑定生物凭证公钥，否则拒绝下发（与 WS 旧路径语义一致：
/// 未配对与无凭证统一归为 CredentialNotBound）。挑战以设备指纹为键：
/// 同一设备的多条通道共享一次挑战，单次有效、60s 过期
pub async fn issue_biometric_challenge(fingerprint: &str) -> std::result::Result<String, BiometricAuthError> {
    if fingerprint.is_empty() {
        return Err(BiometricAuthError::CredentialNotBound);
    }

    let pairing = {
        let db_guard = AppContext::global().db().lock().await;
        db_guard.get_pairing_by_fingerprint(fingerprint)
    }
    .map_err(|e| BiometricAuthError::Database(e.to_string()))?;

    let binding_ready = pairing.as_ref().map(|p| !p.public_key.is_empty()).unwrap_or(false);
    if !binding_ready {
        return Err(BiometricAuthError::CredentialNotBound);
    }

    let nonce = AppContext::global().biometric_challenges().generate(fingerprint).await;
    Ok(nonce)
}

/// 验证生物认证签名并返回配对记录（WS BiometricVerify 与 HTTP biometric-verify 共用）
///
/// 按指纹消费挑战值（单次有效）；随后取配对记录，用绑定公钥验签。
/// 成功返回 pairing：调用方凭其 id 签发 JWT，并须保留 public_key（防旧端点覆盖清空）
pub async fn verify_biometric_challenge(
    fingerprint: &str,
    nonce: &str,
    signature: &str,
) -> std::result::Result<Pairing, BiometricAuthError> {
    if fingerprint.is_empty() {
        return Err(BiometricAuthError::CredentialNotBound);
    }

    // 1. 校验并消费挑战值（单次、未过期、匹配）
    if let Err(e) = AppContext::global()
        .biometric_challenges()
        .verify_and_consume(fingerprint, nonce)
        .await
    {
        return Err(BiometricAuthError::ChallengeInvalid(e.to_string()));
    }

    // 2. 取配对记录与绑定的公钥
    let pairing = {
        let db_guard = AppContext::global().db().lock().await;
        db_guard.get_pairing_by_fingerprint(fingerprint)
    }
    .map_err(|e| BiometricAuthError::Database(e.to_string()))?;
    let Some(pairing) = pairing else {
        return Err(BiometricAuthError::NotPaired);
    };
    if pairing.public_key.is_empty() {
        return Err(BiometricAuthError::CredentialNotBound);
    }

    // 3. 验签（生物认证通过后由安全硬件签名）
    if let Err(e) = verify_biometric_signature(&pairing.public_key, nonce, signature) {
        return Err(BiometricAuthError::SignatureInvalid(e.to_string()));
    }

    Ok(pairing)
}

/// 绑定/解绑生物凭证公钥（HTTP biometric-bind 端点专用）
///
/// 按指纹取配对记录（未配对 → NotPaired），更新 public_key；
/// `public_key` 空串 = 解绑，非空 = 绑定。成功返回是否绑定。
pub async fn bind_biometric_credential(
    fingerprint: &str,
    public_key: &str,
) -> std::result::Result<bool, BiometricAuthError> {
    if fingerprint.is_empty() {
        return Err(BiometricAuthError::NotPaired);
    }

    let pairing = {
        let db_guard = AppContext::global().db().lock().await;
        db_guard.get_pairing_by_fingerprint(fingerprint)
    }
    .map_err(|e| BiometricAuthError::Database(e.to_string()))?;
    let Some(pairing) = pairing else {
        return Err(BiometricAuthError::NotPaired);
    };

    {
        let db_guard = AppContext::global().db().lock().await;
        db_guard
            .update_pairing_public_key(&pairing.id, public_key)
            .map_err(|e| BiometricAuthError::Database(e.to_string()))?;
    }

    let is_binding = !public_key.is_empty();
    tracing::info!(pairing_id = %pairing.id, binding = is_binding, "Biometric credential updated via HTTP");
    Ok(is_binding)
}

/// 格式化设备显示名称：名称 + 首次连接 IP
pub fn format_device_display_name(device_name: &str, address: &str) -> String {
    // address 格式为 "IP:PORT"，提取 IP 部分
    let ip = address.rsplit_once(':').map(|(ip, _)| ip).unwrap_or(address);
    format!("{} ({})", device_name, ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== format_device_display_name ====================

    #[test]
    fn test_display_name_extracts_ip_with_port() {
        assert_eq!(
            format_device_display_name("My Phone", "192.168.1.5:8080"),
            "My Phone (192.168.1.5)"
        );
    }

    #[test]
    fn test_display_name_ipv6_with_port() {
        // IPv6 地址带端口时，rsplit_once 只切最后一个冒号，括号保留
        assert_eq!(
            format_device_display_name("Phone", "[fe80::1]:8080"),
            "Phone ([fe80::1])"
        );
    }

    #[test]
    fn test_display_name_without_port_keeps_address() {
        assert_eq!(format_device_display_name("Phone", "myhost"), "Phone (myhost)");
    }

    // ==================== 生物认证核心逻辑（票据 09） ====================

    /// 本组测试构造独立 AppContext（内存 DB + 真实 challenge manager），
    /// 用全局锁串行化——AppContext 是进程级 OnceLock，首个 init 者胜出，
    /// 且 lib 单测内无其他模块构造 AppContext（grep 确认），故可独占。
    /// 依赖 wasmtime 的 PluginHost 初始化较慢，仅构造一次复用。
    use crate::db::{Database, Pairing};
    use crate::events::DesktopSyncEvent;
    use crate::mdns::advertiser::MdnsAdvertiser;
    use crate::plugin::PluginHost;
    use crate::server::services::pairing_service::PairingService;
    use crate::session::{SessionConfigManager, SessionManager};
    use crate::system::app_context::AppContextBuilder;
    use crate::system::constants::network::SYNC_EVENT_BROADCAST_CAPACITY;
    use crate::system::info::SystemInfo;
    use crate::utils::auth::QrTokenManager;
    use std::path::PathBuf;
    use std::sync::Arc;

    static APP_CTX_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 构造全局 AppContext（幂等：OnceLock 已初始化则复用）
    fn ensure_app_ctx() -> &'static AppContext {
        static INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        INIT.get_or_init(|| {
            let db = Arc::new(tokio::sync::Mutex::new(
                Database::new(std::path::Path::new(":memory:")).expect("in-memory db"),
            ));
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                db.lock().await.init_schema().expect("init schema");
            });
            let plugins_dir =
                std::env::temp_dir().join(format!("bedcode-authsvc-plugins-{}", std::process::id()));
            std::fs::create_dir_all(&plugins_dir).expect("temp plugins dir");

            let session_db = Database::new(std::path::Path::new(":memory:")).expect("session db");
            session_db.init_schema().expect("session schema");
            let session_manager = Arc::new(SessionManager::from_database(
                session_db,
                Arc::new(PathBuf::from(".")),
            ));
            let config_manager = Arc::new(SessionConfigManager::new(db.clone()));
            let plugin_host = Arc::new(rt.block_on(PluginHost::new(
                db.clone(),
                &plugins_dir,
                &plugins_dir,
                session_manager.clone(),
                config_manager.clone(),
                None,
            )));
            rt.block_on(async { plugin_host.init_message_bus().await });

            let pairing_service = Arc::new(PairingService::new());
            let qr_manager = Arc::new(QrTokenManager::new());
            let mdns_advertiser = Arc::new(tokio::sync::RwLock::new(MdnsAdvertiser::new()));
            let (sync_tx, _) =
                tokio::sync::broadcast::channel::<DesktopSyncEvent>(SYNC_EVENT_BROADCAST_CAPACITY);
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
        });
        AppContext::global()
    }

    /// 预置已绑定公钥的配对记录
    fn seed_pairing(fingerprint: &str, public_key: &str) -> Pairing {
        let ctx = ensure_app_ctx();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let db = ctx.db();
            {
                let guard = db.lock().await;
                guard
                    .add_pairing("itest-device", fingerprint, public_key, Some("127.0.0.1:9000"), None)
                    .expect("add pairing");
            }
            let guard = db.lock().await;
            guard.get_pairing_by_fingerprint(fingerprint).expect("fetch pairing").unwrap()
        })
    }

    #[test]
    fn issue_challenge_empty_fingerprint_returns_not_bound() {
        let _guard = APP_CTX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(async { issue_biometric_challenge("").await.unwrap_err() });
        assert!(matches!(err, BiometricAuthError::CredentialNotBound));
    }

    #[test]
    fn issue_challenge_unpaired_device_returns_not_bound() {
        let _guard = APP_CTX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        ensure_app_ctx();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(async { issue_biometric_challenge("fp-unknown-zzz").await.unwrap_err() });
        assert!(matches!(err, BiometricAuthError::CredentialNotBound));
    }

    #[test]
    fn issue_challenge_paired_without_key_returns_not_bound() {
        let _guard = APP_CTX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // 已配对但未绑定公钥
        seed_pairing("fp-plain", "");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(async { issue_biometric_challenge("fp-plain").await.unwrap_err() });
        assert!(matches!(err, BiometricAuthError::CredentialNotBound));
    }

    #[test]
    fn issue_challenge_paired_with_key_returns_nonce() {
        let _guard = APP_CTX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        seed_pairing("fp-bound", "fake-spki-base64");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let nonce = rt.block_on(async { issue_biometric_challenge("fp-bound").await.unwrap() });
        assert_eq!(nonce.len(), 32, "挑战值应为 32 字节 hex");
    }

    #[test]
    fn verify_consumes_nonce_single_use() {
        let _guard = APP_CTX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        seed_pairing("fp-consume", "fake-key");
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let nonce = issue_biometric_challenge("fp-consume").await.unwrap();
            // 无有效签名：首次调用应因签名失败而报 SignatureInvalid（但 nonce 已消费）
            let first = verify_biometric_challenge("fp-consume", &nonce, "bad-sig").await;
            assert!(matches!(first, Err(BiometricAuthError::SignatureInvalid(_))));
            // 同一 nonce 二次使用：挑战已消费 → ChallengeInvalid
            let second = verify_biometric_challenge("fp-consume", &nonce, "bad-sig").await;
            assert!(matches!(second, Err(BiometricAuthError::ChallengeInvalid(_))));
        });
    }

    #[test]
    fn verify_unpaired_fingerprint_returns_not_paired() {
        let _guard = APP_CTX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        ensure_app_ctx();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(async {
            verify_biometric_challenge("fp-ghost", "nonce", "sig").await.unwrap_err()
        });
        assert!(matches!(err, BiometricAuthError::ChallengeInvalid(_)), "未配对设备无挑战，消费即失败");
    }

    #[test]
    fn bind_empty_key_unbinds() {
        let _guard = APP_CTX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let pairing = seed_pairing("fp-bind", "original-key");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let binding = rt.block_on(async { bind_biometric_credential("fp-bind", "").await.unwrap() });
        assert!(!binding, "空串 = 解绑，应返回 false");
        // 解绑后 challenge 下发被拒
        let err = rt.block_on(async { issue_biometric_challenge("fp-bind").await.unwrap_err() });
        assert!(matches!(err, BiometricAuthError::CredentialNotBound));
        let _ = pairing;
    }

    #[test]
    fn bind_valid_key_binds() {
        let _guard = APP_CTX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        seed_pairing("fp-bind2", "old-key");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let binding = rt.block_on(async {
            bind_biometric_credential("fp-bind2", "new-spki-key").await.unwrap()
        });
        assert!(binding, "非空串 = 绑定，应返回 true");
    }

    #[test]
    fn bind_unpaired_returns_not_paired() {
        let _guard = APP_CTX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        ensure_app_ctx();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(async { bind_biometric_credential("fp-nopair", "key").await.unwrap_err() });
        assert!(matches!(err, BiometricAuthError::NotPaired));
    }
}
