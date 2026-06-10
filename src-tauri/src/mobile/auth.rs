//! Mobile Authentication
//!
//! 认证和配对业务逻辑

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use crate::shared::model::message::Message;
use crate::shared::enums::auth::AuthStage;
use crate::mobile::remote::request::{AuthRequest, ResponseParser, timeouts};
use crate::Result;

use crate::mobile::remote::ConnectionManager;

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
    /// 设备 ID
    device_id: RwLock<Option<String>>,
    /// 设备名称
    device_name: RwLock<Option<String>>,
    /// 设备指纹
    device_fingerprint: RwLock<Option<String>>,
}

impl AuthManager {
    /// 创建新的认证管理器
    pub fn new(connection: Arc<ConnectionManager>) -> Arc<Self> {
        let device_id = uuid::Uuid::new_v4().to_string();
        let fingerprint = uuid::Uuid::new_v4().to_string();

        Arc::new(Self {
            connection,
            status: RwLock::new(AuthStatus::Unauthenticated),
            credentials: RwLock::new(None),
            device_id: RwLock::new(Some(device_id)),
            device_name: RwLock::new(None),
            device_fingerprint: RwLock::new(Some(fingerprint)),
        })
    }

    /// 获取设备 ID
    pub async fn get_device_id(&self) -> Option<String> {
        self.device_id.read().await.clone()
    }

    /// 获取设备指纹
    pub async fn get_device_fingerprint(&self) -> Option<String> {
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

        let device_id = self.device_id.read().await.clone().unwrap_or_default();
        let fingerprint = self.device_fingerprint.read().await.clone().unwrap_or_default();

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

        let device_id = self.device_id.read().await.clone().unwrap_or_default();
        let device_name = self.device_name.read().await.clone().unwrap_or_else(|| "Mobile Device".to_string());
        let fingerprint = self.device_fingerprint.read().await.clone().unwrap_or_default();

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

        let device_id = self.device_id.read().await.clone().unwrap_or_default();
        let device_name = self.device_name.read().await.clone().unwrap_or_else(|| "Mobile Device".to_string());
        let fingerprint = self.device_fingerprint.read().await.clone().unwrap_or_default();

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

        let device_id = self.device_id.read().await.clone().unwrap_or_default();
        let device_name = self.device_name.read().await.clone().unwrap_or_else(|| "Mobile Device".to_string());
        let fingerprint = self.device_fingerprint.read().await.clone().unwrap_or_default();

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
