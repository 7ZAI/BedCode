//! Mobile Authentication
//!
//! 认证和配对业务逻辑
//!
//! 设备身份 (device_id, fingerprint) 持久化到文件，
//! 确保重启后 JWT 重连认证仍能使用相同的身份

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use crate::shared::model::message::Message;
use crate::shared::enums::auth::AuthStage;
use crate::mobile::remote::request::{AuthRequest, ResponseParser, timeouts};
use crate::Result;

use crate::mobile::remote::ConnectionManager;

/// 持久化的设备身份
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceIdentity {
    device_id: String,
    fingerprint: String,
}

/// 设备身份文件名
const IDENTITY_FILE: &str = "device_identity.json";

/// 认证凭据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthCredentials {
    /// 设备配对 ID
    pub pairing_id: String,
    /// 设备指纹
    pub fingerprint: String,
    /// 会话令牌
    pub session_token: String,
}

/// 认证状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStatus {
    /// 未认证
    Unauthenticated,
    /// 正在认证
    Authenticating,
    /// 等待配对码输入
    WaitingPairingCode,
    /// 已认证
    Authenticated,
    /// 认证失败
    Failed(String),
}

/// 认证管理器
pub struct AuthManager {
    /// 关联的连接管理器
    connection: Arc<ConnectionManager>,
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
    /// 如果文件不存在则生成新身份并保存
    pub async fn init_identity(&self, app_data_dir: PathBuf) {
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

        // 文件不存在或读取失败，保存当前身份
        self.save_identity().await;
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

    /// 获取设备指纹
    pub async fn get_device_fingerprint(&self) -> String {
        self.device_fingerprint.read().await.clone()
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

    /// 使用前端传入的 JWT token 重新认证（重连时使用）
    pub async fn authenticate_with_token(&self, token: &str) -> Result<bool> {
        if !self.connection.is_connected().await {
            tracing::error!("[authenticate] Not connected");
            return Err(crate::AppError::WebSocket("Not connected".to_string()));
        }

        *self.status.write().await = AuthStatus::Authenticating;

        let device_id = self.device_id.read().await.clone();
        let fingerprint = self.device_fingerprint.read().await.clone();

        tracing::info!("[authenticate] Sending JWT re-auth (token length={})", token.len());
        let message = AuthRequest::reauthenticate(&device_id, &fingerprint, token);

        match self.connection.send_and_wait(&message, timeouts::AUTH).await {
            Ok(response) => {
                if let Some(AuthStage::Authenticated) = ResponseParser::parse_auth_response(&response) {
                    *self.status.write().await = AuthStatus::Authenticated;
                    self.connection.set_paired().await;
                    tracing::info!("[authenticate] JWT re-authentication successful");
                    return Ok(true);
                }
                *self.status.write().await = AuthStatus::Failed("Re-authentication failed".to_string());
                Ok(false)
            }
            Err(e) => {
                *self.status.write().await = AuthStatus::Failed(e.to_string());
                Err(e)
            }
        }
    }

    /// 请求配对
    pub async fn request_pairing(&self) -> Result<()> {
        tracing::info!("[request_pairing] ENTERED");

        if !self.connection.is_connected().await {
            tracing::error!("[request_pairing] Not connected");
            return Err(crate::AppError::WebSocket("Not connected".to_string()));
        }
        tracing::info!("[request_pairing] is_connected OK");

        *self.status.write().await = AuthStatus::Authenticating;

        let device_id = self.device_id.read().await.clone();
        let device_name = self.device_name.read().await.clone().unwrap_or_else(|| "Mobile Device".to_string());
        let fingerprint = self.device_fingerprint.read().await.clone();

        let message = AuthRequest::request_pairing(&device_id, &device_name, &fingerprint);

        tracing::info!("[request_pairing] Calling send_and_wait (30s timeout)...");
        let response = match self.connection.send_and_wait(&message, timeouts::AUTH).await {
            Ok(r) => {
                tracing::info!("[request_pairing] send_and_wait returned Ok");
                r
            }
            Err(e) => {
                tracing::error!("[request_pairing] send_and_wait failed: {}", e);
                return Err(e);
            }
        };

        // 检查响应
        if let Some(AuthStage::VerifyCode) = ResponseParser::parse_auth_response(&response) {
            tracing::info!("[request_pairing] Response stage: VerifyCode");
            *self.status.write().await = AuthStatus::WaitingPairingCode;
            return Ok(());
        }

        tracing::error!("[request_pairing] Failed - unexpected response format");
        Err(crate::AppError::WebSocket("Pairing request failed".to_string()))
    }

    /// 验证配对码
    pub async fn verify_pairing_code(&self, code: &str) -> Result<bool> {
        if !self.connection.is_connected().await {
            return Err(crate::AppError::WebSocket("Not connected".to_string()));
        }

        let device_id = self.device_id.read().await.clone();
        let device_name = self.device_name.read().await.clone().unwrap_or_else(|| "Mobile Device".to_string());
        let fingerprint = self.device_fingerprint.read().await.clone();

        let message = AuthRequest::verify_pairing_code(&device_id, &device_name, &fingerprint, code);

        let response = self.connection.send_and_wait(&message, timeouts::AUTH).await?;

        // 检查响应
        match ResponseParser::parse_auth_response(&response) {
            Some(AuthStage::Authenticated) => {
                // 提取凭据
                let pairing_id = if let Message::Auth { payload, .. } = &response {
                    payload.device_id.clone().unwrap_or_default()
                } else {
                    String::new()
                };
                let session_token = if let Message::Auth { payload, .. } = &response {
                    payload.session_token.clone().unwrap_or_default()
                } else {
                    String::new()
                };

                let creds = AuthCredentials {
                    pairing_id: pairing_id.clone(),
                    fingerprint,
                    session_token: session_token.clone(),
                };

                self.set_credentials(creds).await;
                *self.status.write().await = AuthStatus::Authenticated;
                self.connection.set_paired().await;
                return Ok(true);
            }
            Some(AuthStage::Failed) => {
                // 提取错误信息
                let error_msg = if let Message::Auth { payload, .. } = &response {
                    payload.error.clone().unwrap_or_else(|| "Pairing verification failed".to_string())
                } else {
                    "Pairing verification failed".to_string()
                };
                tracing::warn!("[verify_pairing_code] Failed: {}", error_msg);
                *self.status.write().await = AuthStatus::Failed(error_msg);
                return Ok(false);
            }
            _ => {
                tracing::warn!("[verify_pairing_code] Unexpected response stage");
                *self.status.write().await = AuthStatus::Failed("Unexpected response".to_string());
                return Ok(false);
            }
        }
        Ok(false)
    }

    /// 使用 QR token 认证
    pub async fn authenticate_with_qr(&self, token: &str) -> Result<bool> {
        if !self.connection.is_connected().await {
            return Err(crate::AppError::WebSocket("Not connected".to_string()));
        }

        let device_id = self.device_id.read().await.clone();
        let device_name = self.device_name.read().await.clone().unwrap_or_else(|| "Mobile Device".to_string());
        let fingerprint = self.device_fingerprint.read().await.clone();

        let message = AuthRequest::authenticate_with_qr(&device_id, &device_name, &fingerprint, token);

        let response = self.connection.send_and_wait(&message, timeouts::AUTH).await?;

        // 检查响应
        if let Some(AuthStage::Authenticated) = ResponseParser::parse_auth_response(&response) {
            let pairing_id = if let Message::Auth { payload, .. } = &response {
                payload.device_id.clone().unwrap_or_default()
            } else {
                String::new()
            };
            let session_token = if let Message::Auth { payload, .. } = &response {
                payload.session_token.clone().unwrap_or_default()
            } else {
                String::new()
            };

            let creds = AuthCredentials {
                pairing_id: pairing_id.clone(),
                fingerprint,
                session_token: session_token.clone(),
            };

            self.set_credentials(creds).await;
            *self.status.write().await = AuthStatus::Authenticated;
            self.connection.set_paired().await;
            return Ok(true);
        }

        *self.status.write().await = AuthStatus::Failed("QR authentication failed".to_string());
        Ok(false)
    }
}
