//! Mobile Authentication
//!
//! 认证和配对业务逻辑

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::shared::websocket::WsMessage;
use crate::Result;

use super::connection::{ConnectionManager, ConnectionStatus};

/// 认证凭据
#[derive(Debug, Clone)]
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
        Arc::new(Self {
            connection,
            status: RwLock::new(AuthStatus::Unauthenticated),
            credentials: RwLock::new(None),
            device_id: RwLock::new(None),
            device_name: RwLock::new(None),
            device_fingerprint: RwLock::new(None),
        })
    }

    /// 初始化设备信息
    pub async fn init_device_info(&self) {
        // 从 localStorage 或生成新的设备 ID
        // 这里使用固定值，实际应从 Tauri 获取
        let device_id = uuid::Uuid::new_v4().to_string();
        let fingerprint = uuid::Uuid::new_v4().to_string();

        *self.device_id.write().await = Some(device_id);
        *self.device_fingerprint.write().await = Some(fingerprint);
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

    /// 使用已存储凭据重新认证
    pub async fn authenticate(&self) -> Result<bool> {
        if !self.connection.is_connected().await {
            return Err(crate::AppError::WebSocket("Not connected".to_string()));
        }

        let creds = self.credentials.read().await.clone();
        if let Some(creds) = creds {
            *self.status.write().await = AuthStatus::Authenticating;

            let message = WsMessage::text(serde_json::to_string(&serde_json::json!({
                "type": "auth",
                "message_id": uuid::Uuid::new_v4().to_string(),
                "timestamp": chrono::Utc::now().timestamp_millis(),
                "payload": {
                    "stage": "authenticated",
                    "device_id": creds.pairing_id,
                    "device_fingerprint": creds.fingerprint,
                    "session_token": creds.session_token,
                }
            })).unwrap());

            match self.connection.send_and_wait(&message, std::time::Duration::from_secs(30)).await {
                Ok(response) => {
                    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&response.to_json()?) {
                        if payload.get("payload").and_then(|p| p.get("stage")) == Some(&serde_json::json!("authenticated")) {
                            *self.status.write().await = AuthStatus::Authenticated;
                            self.connection.set_paired().await;
                            return Ok(true);
                        }
                    }
                    *self.status.write().await = AuthStatus::Failed("Authentication failed".to_string());
                    Ok(false)
                }
                Err(e) => {
                    *self.status.write().await = AuthStatus::Failed(e.to_string());
                    Err(e)
                }
            }
        } else {
            Ok(false)
        }
    }

    /// 请求配对
    pub async fn request_pairing(&self) -> Result<()> {
        if !self.connection.is_connected().await {
            return Err(crate::AppError::WebSocket("Not connected".to_string()));
        }

        *self.status.write().await = AuthStatus::Authenticating;

        let device_id = self.device_id.read().await.clone().unwrap_or_default();
        let device_name = self.device_name.read().await.clone().unwrap_or_else(|| "Mobile Device".to_string());
        let fingerprint = self.device_fingerprint.read().await.clone().unwrap_or_default();

        let message = WsMessage::text(serde_json::to_string(&serde_json::json!({
            "type": "auth",
            "message_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "payload": {
                "stage": "request_pairing",
                "device_id": device_id,
                "device_name": device_name,
                "device_fingerprint": fingerprint,
            }
        })).unwrap());

        let response = self.connection.send_and_wait(&message, std::time::Duration::from_secs(30)).await?;

        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&response.to_json()?) {
            if payload.get("payload").and_then(|p| p.get("stage")) == Some(&serde_json::json!("verify_code")) {
                *self.status.write().await = AuthStatus::WaitingPairingCode;
                return Ok(());
            }
        }

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

        let message = WsMessage::text(serde_json::to_string(&serde_json::json!({
            "type": "auth",
            "message_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "payload": {
                "stage": "verify_code",
                "device_id": device_id,
                "device_name": device_name,
                "device_fingerprint": fingerprint,
                "pairing_code": code,
            }
        })).unwrap());

        let response = self.connection.send_and_wait(&message, std::time::Duration::from_secs(30)).await?;

        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&response.to_json()?) {
            let stage = payload.get("payload").and_then(|p| p.get("stage"));
            if stage == Some(&serde_json::json!("authenticated")) {
                // 提取凭据
                let pairing_id = payload.get("payload")
                    .and_then(|p| p.get("device_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let session_token = payload.get("payload")
                    .and_then(|p| p.get("session_token"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();

                let creds = AuthCredentials {
                    pairing_id: pairing_id.clone(),
                    fingerprint: fingerprint.clone(),
                    session_token: session_token.clone(),
                };

                self.set_credentials(creds).await;
                *self.status.write().await = AuthStatus::Authenticated;
                self.connection.set_paired().await;
                return Ok(true);
            }
        }

        *self.status.write().await = AuthStatus::Failed("Pairing verification failed".to_string());
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

        let message = WsMessage::text(serde_json::to_string(&serde_json::json!({
            "type": "auth",
            "message_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "payload": {
                "stage": "qr_connect",
                "device_id": device_id,
                "device_name": device_name,
                "device_fingerprint": fingerprint,
                "qr_token": token,
            }
        })).unwrap());

        let response = self.connection.send_and_wait(&message, std::time::Duration::from_secs(30)).await?;

        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&response.to_json()?) {
            if payload.get("payload").and_then(|p| p.get("stage")) == Some(&serde_json::json!("authenticated")) {
                let pairing_id = payload.get("payload")
                    .and_then(|p| p.get("device_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let session_token = payload.get("payload")
                    .and_then(|p| p.get("session_token"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();

                let creds = AuthCredentials {
                    pairing_id: pairing_id.clone(),
                    fingerprint: fingerprint.clone(),
                    session_token: session_token.clone(),
                };

                self.set_credentials(creds).await;
                *self.status.write().await = AuthStatus::Authenticated;
                self.connection.set_paired().await;
                return Ok(true);
            }
        }

        *self.status.write().await = AuthStatus::Failed("QR authentication failed".to_string());
        Ok(false)
    }
}