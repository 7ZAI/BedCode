//! Auth Manager
//!
//! 认证管理器 - HTTP 认证（配对码 / QR / JWT reauth / 生物挑战应答）。
//! 设备身份持久化到文件，确保重启后 JWT 重连认证仍能使用相同身份
//!
//! 认证已从 WS 握手迁移到 HTTP（spec §4.5 六端点）：所有方法经
//! `AuthHttpClient` 直连桌面端 `/api/auth/*`，不再依赖 WS 通道。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth::http::{format_base_url, AuthHttpClient};
use crate::connection::manager::ConnectionManager;
use crate::router::MobileEvent;
use crate::system::constants::auth::DEFAULT_DEVICE_NAME;
use crate::AppError;
use crate::Result;

use super::{AuthCredentials, AuthStatus};

/// 持久化的设备身份
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceIdentity {
    device_id: String,
    fingerprint: String,
}

/// 设备身份文件名
const IDENTITY_FILE: &str = "device_identity.json";

/// 认证管理器
pub struct AuthManager {
    /// 关联的连接管理器
    connection: Arc<ConnectionManager>,
    /// HTTP 认证客户端
    http: Arc<AuthHttpClient>,
    /// 认证状态
    status: RwLock<AuthStatus>,
    /// 认证凭据
    credentials: RwLock<Option<AuthCredentials>>,
    /// 设备 ID（持久化，重启后保持一致）
    device_id: RwLock<String>,
    /// 设备名称
    device_name: RwLock<Option<String>>,
    /// 设备指纹（持久化，重启后保持一致）
    device_fingerprint: RwLock<String>,
    /// 身份文件路径（用于持久化 device_id 和 fingerprint）
    identity_path: RwLock<Option<PathBuf>>,
}

impl AuthManager {
    /// 创建新的认证管理器
    ///
    /// 优先从文件加载已持久化的 device_id 和 fingerprint，
    /// 不存在则生成新的并保存，确保重启后身份一致
    pub fn new(connection: Arc<ConnectionManager>) -> Arc<Self> {
        Arc::new(Self {
            connection,
            http: AuthHttpClient::new(),
            status: RwLock::new(AuthStatus::Unauthenticated),
            credentials: RwLock::new(None),
            // 临时值，init_identity() 会覆盖为持久化值
            device_id: RwLock::new(uuid::Uuid::new_v4().to_string()),
            device_name: RwLock::new(None),
            device_fingerprint: RwLock::new(uuid::Uuid::new_v4().to_string()),
            identity_path: RwLock::new(None),
        })
    }

    /// 从文件加载持久化的设备身份
    ///
    /// 首次调用时传入 app 数据目录，后续调用无效果（OnceLock 保证）
    /// 文件存在则直接加载；文件不存在（首次安装或卸载重装后）优先用
    /// 设备唯一 ID（Android ANDROID_ID，卸载重装保持一致）派生身份，
    /// 保证重装后仍是同一设备号，桌面端配对/生物凭证/连接历史可复用；
    /// 拿不到设备 ID（非 Android 或插件异常）才回退随机 UUID。
    pub async fn init_identity(&self, app: &tauri::AppHandle, app_data_dir: PathBuf) {
        let path = app_data_dir.join(IDENTITY_FILE);
        *self.identity_path.write().await = Some(path.clone());

        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    if let Ok(identity) = serde_json::from_str::<DeviceIdentity>(&content) {
                        tracing::info!("Loaded persisted device identity: device_id={}", identity.device_id);
                        *self.device_id.write().await = identity.device_id;
                        *self.device_fingerprint.write().await = identity.fingerprint;
                        return;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read device identity file: {}", e);
                }
            }
        }

        // 文件不存在或读取失败：优先用设备唯一 ID 派生稳定身份
        if let Some(uid) = self.stable_device_uid(app) {
            let (device_id, fingerprint) = derive_identity_from_uid(&uid);
            tracing::info!(
                "Derived device identity from stable device UID: device_id={}",
                device_id
            );
            *self.device_id.write().await = device_id;
            *self.device_fingerprint.write().await = fingerprint;
            self.save_identity().await;
            return;
        }

        // 设备 UID 不可用：回退随机 UUID（非 Android 平台/插件异常）
        tracing::warn!("Stable device UID unavailable, falling back to random UUID identity");
        self.save_identity().await;
    }

    /// 获取设备唯一 ID（卸载重装保持一致）
    ///
    /// 复用 tauri-plugin-machine-uid：Android 实现为 Settings.Secure.ANDROID_ID，
    /// 同一设备同一签名下卸载重装不变化（恢复出厂设置才变）；desktop 为机器硬件标识。
    fn stable_device_uid(&self, app: &tauri::AppHandle) -> Option<String> {
        use tauri_plugin_machine_uid::MachineUidExt;
        app.machine_uid().get_machine_uid().ok()?.id
    }

    /// 保存设备身份到文件
    async fn save_identity(&self) {
        let path = self.identity_path.read().await.clone();
        let Some(path) = path else { return };

        let identity = DeviceIdentity {
            device_id: self.device_id.read().await.clone(),
            fingerprint: self.device_fingerprint.read().await.clone(),
        };

        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!("Failed to create identity dir: {}", e);
                return;
            }
        }

        match serde_json::to_string_pretty(&identity) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&path, content) {
                    tracing::warn!("Failed to save device identity: {}", e);
                } else {
                    tracing::info!("Saved device identity to {:?}", path);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to serialize device identity: {}", e);
            }
        }
    }

    /// 获取设备 ID
    pub async fn get_device_id(&self) -> String {
        self.device_id.read().await.clone()
    }

    /// 获取设备名称
    pub async fn get_device_name(&self) -> Option<String> {
        self.device_name.read().await.clone()
    }

    /// 设置设备名称
    pub async fn set_device_name(&self, name: String) {
        *self.device_name.write().await = Some(name);
    }

    /// 获取认证凭据
    pub async fn get_credentials(&self) -> Option<AuthCredentials> {
        self.credentials.read().await.clone()
    }

    /// 存储认证凭据
    pub async fn set_credentials(&self, credentials: AuthCredentials) {
        *self.credentials.write().await = Some(credentials);
    }

    /// 获取认证状态
    pub async fn get_status(&self) -> AuthStatus {
        self.status.read().await.clone()
    }

    /// 解析目标设备 base URL 与客户端地址（供桌面端配对记录，一次读锁）
    async fn target_context(&self) -> Result<(String, String)> {
        let target = self
            .connection
            .get_target()
            .await
            .ok_or_else(|| AppError::Auth("No target device".to_string()))?;
        Ok((format_base_url(&target.address, target.port), target.address))
    }

    /// 认证成功统一收尾：持久化凭据 + 全局 token + 设备级 Authed + 04 契口
    ///
    /// HTTP 认证路径不经过 `handler/auth.rs`（WS 应答处理器）——经此契口
    /// 广播 `MobileEvent::AuthSuccess` 是唯一等价补齐点：04 的事件 WS 订阅
    /// 后恢复建连；事件转发层（event.rs）同时转发为前端 ws_auth_success。
    ///
    /// 链路加密 pin 随认证响应落地（修复：pin 此前仅在无人调用的 HTTP auth
    /// 函数里写入，配对/重认证全走本处导致前端 localStorage 恒空、设置页误报
    /// 未配对）：桌面端下发身份公钥/指纹时，1) 写入 Rust 侧运行期 context
    /// （事件 WS 建连协商直接用）；2) 广播 `LinkCryptoPin` 事件 → 前端写
    /// localStorage（HTTP/终端 WS 通道 + 设置页指纹展示）。
    async fn apply_auth_success(
        &self,
        session_token: String,
        fingerprint: String,
        kd_public_b64: Option<String>,
        kd_fingerprint: Option<String>,
    ) {
        let device_id = self.device_id.read().await.clone();
        let creds = AuthCredentials {
            pairing_id: device_id,
            fingerprint,
            session_token: session_token.clone(),
        };
        self.set_credentials(creds).await;
        crate::state::set_global_token(&session_token);
        *self.status.write().await = AuthStatus::Authenticated;
        self.connection.set_authed().await;

        // pin 落地：Rust 侧 context（事件 WS 建连时读取）+ 前端事件广播
        if let Some(kd) = &kd_public_b64 {
            tracing::info!(
                "[apply_auth_success] link crypto pin refreshed (kd fingerprint={})",
                kd_fingerprint.as_deref().unwrap_or("n/a")
            );
            crate::state::update_link_crypto_pin(Some(kd.clone()));
        }
        let _ = self
            .connection
            .event_tx()
            .send(MobileEvent::LinkCryptoPin { kd_public_b64, kd_fingerprint });

        let _ = self
            .connection
            .event_tx()
            .send(MobileEvent::AuthSuccess { session_token });
    }

    /// 使用已持有的 JWT token 重新认证（断线重连）——HTTP reauth
    ///
    /// 桌面端校验 token 后签发刷新后的新 JWT，写回凭据与全局 token。
    pub async fn authenticate_with_token(&self, token: &str) -> Result<bool> {
        let (base_url, _address) = self.target_context().await?;
        let device_id = self.device_id.read().await.clone();
        let fingerprint = self.device_fingerprint.read().await.clone();

        *self.status.write().await = AuthStatus::Authenticating;
        tracing::info!("[authenticate] HTTP reauth (token length={})", token.len());

        match self.http.reauth(&base_url, &device_id, &fingerprint, token).await {
            Ok(data) => {
                // reauth 返回刷新后的新 token：refresh 语义，写回凭据与全局
                self.apply_auth_success(
                    data.token,
                    fingerprint,
                    data.kd_public_b64,
                    data.kd_fingerprint,
                )
                .await;
                tracing::info!("[authenticate] HTTP re-authentication successful");
                Ok(true)
            }
            Err(e) => {
                tracing::warn!("[authenticate] HTTP reauth failed: {}", e);
                *self.status.write().await = AuthStatus::Failed(e.to_string());
                Err(e)
            }
        }
    }

    /// 请求配对（HTTP）：桌面端生成一次性配对码
    ///
    /// 配对码展示在桌面端，移动端进入等待用户输入状态即可。
    pub async fn request_pairing(&self) -> Result<()> {
        let (base_url, _address) = self.target_context().await?;
        *self.status.write().await = AuthStatus::Authenticating;

        let device_id = self.device_id.read().await.clone();
        let device_name = self
            .device_name
            .read()
            .await
            .clone()
            .unwrap_or_else(|| DEFAULT_DEVICE_NAME.to_string());
        let fingerprint = self.device_fingerprint.read().await.clone();

        match self
            .http
            .request_pairing(&base_url, &device_id, &device_name, &fingerprint)
            .await
        {
            Ok(data) => {
                tracing::info!(
                    "[request_pairing] HTTP pairing accepted (code len={})",
                    data.pairing_code.len()
                );
                *self.status.write().await = AuthStatus::WaitingPairingCode;
                Ok(())
            }
            Err(e) => {
                tracing::error!("[request_pairing] HTTP pairing failed: {}", e);
                *self.status.write().await = AuthStatus::Failed(e.to_string());
                Err(e)
            }
        }
    }

    /// 验证配对码（HTTP）：通过后签发 JWT
    ///
    /// 业务拒绝（1005 配对码无效/过期）→ Ok(false)，由 command 层发
    /// auth_failed 事件；网络故障 → Err。
    pub async fn verify_pairing_code(&self, code: &str) -> Result<bool> {
        let (base_url, address) = self.target_context().await?;
        let device_id = self.device_id.read().await.clone();
        let device_name = self
            .device_name
            .read()
            .await
            .clone()
            .unwrap_or_else(|| DEFAULT_DEVICE_NAME.to_string());
        let fingerprint = self.device_fingerprint.read().await.clone();

        match self
            .http
            .verify_pairing_code(&base_url, &device_id, &device_name, &fingerprint, code, &address)
            .await
        {
            Ok(data) => {
                self.apply_auth_success(
                    data.token,
                    fingerprint,
                    data.kd_public_b64,
                    data.kd_fingerprint,
                )
                .await;
                Ok(true)
            }
            Err(AppError::Auth(msg)) => {
                tracing::warn!("[verify_pairing_code] Rejected: {}", msg);
                *self.status.write().await = AuthStatus::Failed(msg.clone());
                Ok(false)
            }
            Err(e) => {
                tracing::warn!("[verify_pairing_code] Failed: {}", e);
                *self.status.write().await = AuthStatus::Failed(e.to_string());
                Err(e)
            }
        }
    }

    /// 使用 QR token 认证（HTTP）
    pub async fn authenticate_with_qr(&self, token: &str) -> Result<bool> {
        let (base_url, address) = self.target_context().await?;
        let device_id = self.device_id.read().await.clone();
        let device_name = self
            .device_name
            .read()
            .await
            .clone()
            .unwrap_or_else(|| DEFAULT_DEVICE_NAME.to_string());
        let fingerprint = self.device_fingerprint.read().await.clone();

        match self
            .http
            .qr_connect(&base_url, &device_id, &device_name, &fingerprint, token, &address)
            .await
        {
            Ok(data) => {
                self.apply_auth_success(
                    data.token,
                    fingerprint,
                    data.kd_public_b64,
                    data.kd_fingerprint,
                )
                .await;
                Ok(true)
            }
            Err(AppError::Auth(msg)) => {
                tracing::warn!("[authenticate_with_qr] Rejected: {}", msg);
                *self.status.write().await = AuthStatus::Failed(msg.clone());
                Ok(false)
            }
            Err(e) => {
                tracing::warn!("[authenticate_with_qr] Failed: {}", e);
                *self.status.write().await = AuthStatus::Failed(e.to_string());
                Err(e)
            }
        }
    }

    /// 生物认证登录（HTTP 挑战-应答握手）
    ///
    /// 1. `POST /api/auth/biometric-challenge` 获取一次性挑战值（60s 有效）
    /// 2. 弹系统生物识别，认证通过后解锁 Keystore 私钥签名
    /// 3. `POST /api/auth/biometric-verify` 回传签名，桌面端验签后签发 JWT
    pub async fn authenticate_with_biometric(&self) -> Result<bool> {
        let (base_url, _address) = self.target_context().await?;
        let device_id = self.device_id.read().await.clone();
        let fingerprint = self.device_fingerprint.read().await.clone();

        *self.status.write().await = AuthStatus::Authenticating;

        // 1. 请求挑战值
        // 桌面端拒绝（未绑定凭证/未配对等）：透传真实原因，前端可提示用户改用配对码
        let challenge = match self.http.biometric_challenge(&base_url, &device_id, &fingerprint).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("[authenticate_with_biometric] Challenge rejected: {}", e);
                *self.status.write().await = AuthStatus::Failed(e.to_string());
                return Err(e);
            }
        };

        // 2. 生物认证解锁私钥并签名挑战值
        let signature =
            match crate::plugin::android_plugins::biometric_sign(&fingerprint, &challenge.challenge_nonce).await {
                Ok(sig) => sig,
                Err(e) => {
                    tracing::warn!("[authenticate_with_biometric] Biometric sign failed: {}", e);
                    *self.status.write().await = AuthStatus::Failed(format!("Biometric authentication failed: {}", e));
                    return Ok(false);
                }
            };

        // 3. 回传签名验证（挑战过期/验签失败 → 1009）
        match self
            .http
            .biometric_verify(
                &base_url,
                &device_id,
                &fingerprint,
                &challenge.challenge_nonce,
                &signature,
            )
            .await
        {
            Ok(data) => {
                self.apply_auth_success(
                    data.token,
                    fingerprint,
                    data.kd_public_b64,
                    data.kd_fingerprint,
                )
                .await;
                tracing::info!("[authenticate_with_biometric] Biometric authentication successful");
                Ok(true)
            }
            Err(e) => {
                tracing::warn!("[authenticate_with_biometric] Verification rejected: {}", e);
                *self.status.write().await = AuthStatus::Failed(e.to_string());
                Err(e)
            }
        }
    }

    /// 绑定生物凭证：生成密钥对 + 生物门卫自检，通过后注册公钥到桌面端
    ///
    /// 自检：生成密钥后立即弹一次指纹/人脸，对本地随机挑战签名并验签——
    /// 确认"合法主人"确实能解锁这把钥匙，通过后才把公钥注册到桌面端。
    /// 避免"绑定时从未验证主人"：否则任何人拿到已配对手机都能注册钥匙。
    /// 返回桌面端是否接受绑定。
    ///
    /// 说明：ExchangeCertificate 无 HTTP 端点（spec §4.5 仅六端点），绑定
    /// 需已认证持久 WS → 04 事件 WS 落地前后端绑定功能暂时不可用，
    /// 调用 send_and_wait 报 Not connected，属预期过渡回归（04 内接线）。
    pub async fn bind_biometric_credential(&self) -> Result<bool> {
        if !self.connection.is_connected().await {
            return Err(AppError::WebSocket("Not connected".to_string()));
        }

        let fingerprint = self.device_fingerprint.read().await.clone();

        // 1. 本地生成密钥对（私钥存 Keystore，需生物认证解锁）
        let public_key = match crate::plugin::android_plugins::biometric_generate_keypair(&fingerprint).await {
            Ok(pk) => pk,
            Err(e) => {
                tracing::error!("[bind_biometric_credential] Key generation failed: {}", e);
                return Err(e);
            }
        };

        // 2. 生物门卫自检：弹指纹/人脸签名本地随机挑战并验签。
        //    取消/失败/验签不通过 → 删除本地密钥，绑定中止（不向桌面端注册）
        {
            use rand::RngCore;
            let mut bytes = [0u8; 16];
            rand::thread_rng().fill_bytes(&mut bytes);
            let nonce = hex::encode(bytes);

            let signature = match crate::plugin::android_plugins::biometric_sign(&fingerprint, &nonce).await {
                Ok(sig) => sig,
                Err(e) => {
                    tracing::warn!(
                        "[bind_biometric_credential] Biometric self-check cancelled/failed: {}",
                        e
                    );
                    let _ = crate::plugin::android_plugins::biometric_delete_key(&fingerprint).await;
                    return Err(e);
                }
            };

            if let Err(e) = verify_biometric_signature(&public_key, &nonce, &signature) {
                tracing::error!(
                    "[bind_biometric_credential] Biometric self-check signature invalid: {}",
                    e
                );
                let _ = crate::plugin::android_plugins::biometric_delete_key(&fingerprint).await;
                return Err(AppError::Auth("Biometric self-check failed".to_string()));
            }
            tracing::info!("[bind_biometric_credential] Biometric self-check passed (owner verified)");
        }

        // 3. 通过已认证连接把公钥注册到桌面端
        let message = crate::connection::request::AuthRequest::exchange_biometric_credential(&fingerprint, &public_key);
        let response = match self
            .connection
            .send_and_wait(&message, crate::connection::request::timeouts::AUTH)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                // 注册失败时清理本地密钥，避免留下孤儿公钥/私钥
                let _ = crate::plugin::android_plugins::biometric_delete_key(&fingerprint).await;
                return Err(e);
            }
        };

        match crate::connection::request::ResponseParser::parse_auth_response(&response) {
            Some(crate::enums::auth::AuthStage::Authenticated) => {
                tracing::info!("[bind_biometric_credential] Credential bound to desktop");
                Ok(true)
            }
            Some(crate::enums::auth::AuthStage::Failed) => {
                let _ = crate::plugin::android_plugins::biometric_delete_key(&fingerprint).await;
                tracing::warn!("[bind_biometric_credential] Desktop rejected binding");
                Ok(false)
            }
            _ => {
                let _ = crate::plugin::android_plugins::biometric_delete_key(&fingerprint).await;
                Ok(false)
            }
        }
    }

    /// 解绑生物凭证：删除本地密钥 + 通知桌面端清空公钥
    ///
    /// 与绑定的通道限制相同：需已认证持久 WS（04 后可用）。
    pub async fn unbind_biometric_credential(&self) -> Result<bool> {
        if !self.connection.is_connected().await {
            return Err(AppError::WebSocket("Not connected".to_string()));
        }

        let fingerprint = self.device_fingerprint.read().await.clone();

        // 1. 通知桌面端清空公钥
        let message = crate::connection::request::AuthRequest::exchange_biometric_credential(&fingerprint, "");
        let response = match self
            .connection
            .send_and_wait(&message, crate::connection::request::timeouts::AUTH)
            .await
        {
            Ok(r) => Some(r),
            Err(e) => {
                tracing::warn!("[unbind_biometric_credential] Desktop notification failed: {}", e);
                // 桌面端通知失败仍继续删除本地密钥，避免本地密钥悬空
                None
            }
        };

        // 2. 删除本地密钥
        if let Err(e) = crate::plugin::android_plugins::biometric_delete_key(&fingerprint).await {
            tracing::warn!("[unbind_biometric_credential] Failed to delete local key: {}", e);
        }

        match response {
            Some(crate::model::message::Message::Auth { payload, .. })
                if payload.stage == crate::enums::auth::AuthStage::Authenticated =>
            {
                tracing::info!("[unbind_biometric_credential] Credential unbound");
                Ok(true)
            }
            _ => {
                tracing::warn!("[unbind_biometric_credential] Desktop did not confirm unbind");
                Ok(false)
            }
        }
    }

    /// 检查本地生物认证密钥是否存在
    pub async fn has_biometric_key(&self) -> Result<bool> {
        let fingerprint = self.device_fingerprint.read().await.clone();
        crate::plugin::android_plugins::biometric_has_key(&fingerprint).await
    }

    /// 设备是否支持生物认证密钥（硬件可用 + 已录入生物特征）
    ///
    /// 返回 (是否支持, BiometricManager 结果码)
    pub async fn is_biometric_supported(&self) -> Result<(bool, i32)> {
        crate::plugin::android_plugins::biometric_device_supported().await
    }
}

/// 从设备唯一 ID 派生稳定身份（device_id + fingerprint）
///
/// 同一设备同一签名下卸载重装后 UID 不变，因此派生出的身份不变；
/// 用哈希而非原始 UID，避免设备标识直接入库/上链。
fn derive_identity_from_uid(uid: &str) -> (String, String) {
    use sha2::{Digest, Sha256};
    let device_hash = hex::encode(Sha256::digest(format!("bedcode-device:{}", uid).as_bytes()));
    let fingerprint_hash = hex::encode(Sha256::digest(format!("bedcode-fingerprint:{}", uid).as_bytes()));
    // 取前 32 字符保证与旧 UUID 长度风格一致（36 字符左右），便于日志阅读
    (device_hash[..32].to_string(), fingerprint_hash[..32].to_string())
}

/// 验证生物认证签名（绑定自检用，与桌面端 verify_biometric_signature 算法一致）
///
/// - `public_key_spki_b64`: 绑定公钥（SPKI X.509 DER，base64）
/// - `message`: 被签名的消息（挑战值 hex 字符串的 UTF-8 字节）
/// - `signature_b64`: 签名（原始 r||s 格式，base64）
fn verify_biometric_signature(public_key_spki_b64: &str, message: &str, signature_b64: &str) -> Result<()> {
    use base64::Engine;
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::{Signature, VerifyingKey};
    use p256::pkcs8::DecodePublicKey;

    let spki_der = base64::engine::general_purpose::STANDARD
        .decode(public_key_spki_b64)
        .map_err(|e| AppError::Auth(format!("Invalid public key encoding: {}", e)))?;

    let verifying_key = VerifyingKey::from_public_key_der(&spki_der)
        .map_err(|e| AppError::Auth(format!("Invalid public key: {}", e)))?;

    let raw_sig = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|e| AppError::Auth(format!("Invalid signature encoding: {}", e)))?;

    let signature = Signature::from_slice(&raw_sig).map_err(|e| AppError::Auth(format!("Invalid signature: {}", e)))?;

    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|_| AppError::Auth("Biometric signature verification failed".to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::SigningKey;
    use p256::pkcs8::EncodePublicKey;

    /// 与桌面端 test_verify_signature_roundtrip 对称：验证 SPKI 公钥 + hex 消息 + r||s 签名
    #[test]
    fn test_verify_biometric_signature_roundtrip() {
        let signing_key = SigningKey::random(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();

        let spki_der = verifying_key.to_public_key_der().expect("encode public key");
        let spki_b64 = base64::engine::general_purpose::STANDARD.encode(spki_der.as_bytes());

        let message = "0123456789abcdef0123456789abcdef";
        let signature: p256::ecdsa::Signature = signing_key.sign(message.as_bytes());
        let (r, s) = signature.split_scalars();
        let mut raw = r.to_bytes().to_vec();
        raw.extend_from_slice(&s.to_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(&raw);

        assert!(verify_biometric_signature(&spki_b64, message, &sig_b64).is_ok());
        // 篡改消息应失败
        assert!(verify_biometric_signature(&spki_b64, "tampered", &sig_b64).is_err());
        // 篡改签名应失败
        let bad_sig = base64::engine::general_purpose::STANDARD.encode(&raw[..63]);
        assert!(verify_biometric_signature(&spki_b64, message, &bad_sig).is_err());
    }
}
