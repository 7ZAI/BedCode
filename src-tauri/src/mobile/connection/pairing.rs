//! Pairing Module - 配对功能
//!
//! 提供设备配对相关功能

use super::types::{PairingRequestResult, PendingRequest};
use crate::desktop::websocket::message::{AuthPayload, AuthStage, Message};
use crate::shared::error::{AppError, Result};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use uuid::Uuid;

pub struct PairingModule {
    /// 配对码
    pub pairing_code: RwLock<Option<PairingRequestResult>>,
    /// 待发送请求的映射
    pending_requests: Arc<RwLock<std::collections::HashMap<String, PendingRequest>>>,
    /// 配对服务（用于生成设备 ID）
    pairing_service: Arc<crate::shared::auth::PairingService>,
    /// 设备 ID（持久化）
    device_id: RwLock<String>,
    /// 设备指纹
    device_fingerprint: RwLock<String>,
    /// 设备名称
    device_name: RwLock<String>,
    /// 会话令牌
    session_token: RwLock<Option<String>>,
}

impl PairingModule {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            pairing_code: RwLock::new(None),
            pending_requests: Arc::new(RwLock::new(std::collections::HashMap::new())),
            pairing_service: Arc::new(crate::shared::auth::PairingService::new()),
            device_id: RwLock::new(Uuid::new_v4().to_string()),
            device_fingerprint: RwLock::new(Uuid::new_v4().to_string()),
            device_name: RwLock::new("Mobile Device".to_string()),
            session_token: RwLock::new(None),
        })
    }

    /// 获取配对码
    pub async fn get_pairing_code(&self) -> Option<PairingRequestResult> {
        self.pairing_code.read().await.clone()
    }

    /// 获取设备 ID
    pub fn get_device_id(&self) -> String {
        self.device_id.read().blocking_read().clone()
    }

    /// 获取设备指纹
    pub fn get_device_fingerprint(&self) -> String {
        self.device_fingerprint.read().blocking_read().clone()
    }

    /// 获取设备名称
    pub fn get_device_name(&self) -> String {
        self.device_name.read().blocking_read().clone()
    }

    /// 设置设备名称
    pub fn set_device_name(&self, name: String) {
        *self.device_name.write().blocking_write() = name;
    }

    /// 获取会话令牌
    pub fn get_session_token(&self) -> Option<String> {
        self.session_token.read().blocking_read().clone()
    }

    /// 设置会话令牌
    pub async fn set_session_token(&self, token: String) {
        *self.session_token.write().await = Some(token);
    }

    /// 设置设备 ID
    pub async fn set_device_id(&self, id: String) {
        *self.device_id.write().await = id;
    }

    /// 请求配对
    pub async fn request_pairing(
        &self,
        ws_sender: &Option<tokio::sync::mpsc::Sender<WsMessage>>,
    ) -> Result<()> {
        let message_id = Uuid::new_v4().to_string();
        let message = Message::Auth {
            message_id: message_id.clone(),
            session_id: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
            payload: AuthPayload {
                stage: AuthStage::RequestPairing,
                device_id: Some(self.get_device_id()),
                device_name: Some(self.get_device_name()),
                device_fingerprint: Some(self.get_device_fingerprint()),
                pairing_code: None,
                session_token: None,
                error: None,
                qr_token: None,
            },
        };

        // 直接发送，不等待响应（响应通过 handle_message 处理）
        if let Some(sender) = ws_sender {
            let json = message.to_json()?;
            sender
                .send(WsMessage::Text(json))
                .await
                .map_err(|e| AppError::Network(format!("Failed to send pairing request: {}", e)))?;
        } else {
            return Err(AppError::Network("Not connected".to_string()));
        }

        Ok(())
    }

    /// 验证配对码
    pub async fn verify_pairing_code(
        &self,
        code: &str,
        ws_sender: &Option<tokio::sync::mpsc::Sender<WsMessage>>,
        pending_requests: &Arc<RwLock<std::collections::HashMap<String, PendingRequest>>>,
        timeout_ms: u64,
    ) -> Result<Message> {
        let message_id = Uuid::new_v4().to_string();
        let message = Message::Auth {
            message_id: message_id.clone(),
            session_id: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
            payload: AuthPayload {
                stage: AuthStage::VerifyCode,
                device_id: Some(self.get_device_id()),
                device_name: Some(self.get_device_name()),
                device_fingerprint: Some(self.get_device_fingerprint()),
                pairing_code: Some(code.to_string()),
                session_token: None,
                error: None,
                qr_token: None,
            },
        };

        // 创建 oneshot 通道用于接收响应
        let (tx, rx) = tokio::sync::oneshot::channel();

        // 创建超时任务
        let pending_clone = pending_requests.clone();
        let timeout_id = message_id.clone();
        let timeout = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)).await;
            if let Some(pending) = pending_clone.write().await.remove(&timeout_id) {
                let _ = pending.resolve.send(Message::error("TIMEOUT", "Request timeout"));
            }
        });

        // 保存待处理请求
        pending_requests.write().await.insert(
            message_id.clone(),
            PendingRequest {
                resolve: tx,
                timeout,
            },
        );

        // 发送消息
        if let Some(sender) = ws_sender {
            let json = message.to_json()?;
            sender
                .send(WsMessage::Text(json))
                .await
                .map_err(|e| AppError::Network(format!("Failed to send message: {}", e)))?;
        } else {
            return Err(AppError::Network("Not connected".to_string()));
        }

        // 等待响应
        let response = rx.await.map_err(|e| AppError::Network(format!("Channel error: {}", e)))?;

        Ok(response)
    }

    /// 使用 QR Token 认证
    pub async fn authenticate_with_qr(
        &self,
        qr_token: &str,
        ws_sender: &Option<tokio::sync::mpsc::Sender<WsMessage>>,
        pending_requests: &Arc<RwLock<std::collections::HashMap<String, PendingRequest>>>,
        timeout_ms: u64,
    ) -> Result<Message> {
        let message_id = Uuid::new_v4().to_string();
        let message = Message::Auth {
            message_id: message_id.clone(),
            session_id: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
            payload: AuthPayload {
                stage: AuthStage::QrConnect,
                device_id: Some(self.get_device_id()),
                device_name: Some(self.get_device_name()),
                device_fingerprint: Some(self.get_device_fingerprint()),
                pairing_code: None,
                session_token: None,
                error: None,
                qr_token: Some(qr_token.to_string()),
            },
        };

        // 创建 oneshot 通道用于接收响应
        let (tx, rx) = tokio::sync::oneshot::channel();

        // 创建超时任务
        let pending_clone = pending_requests.clone();
        let timeout_id = message_id.clone();
        let timeout = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)).await;
            if let Some(pending) = pending_clone.write().await.remove(&timeout_id) {
                let _ = pending.resolve.send(Message::error("TIMEOUT", "Request timeout"));
            }
        });

        // 保存待处理请求
        pending_requests.write().await.insert(
            message_id.clone(),
            PendingRequest {
                resolve: tx,
                timeout,
            },
        );

        // 发送消息
        if let Some(sender) = ws_sender {
            let json = message.to_json()?;
            sender
                .send(WsMessage::Text(json))
                .await
                .map_err(|e| AppError::Network(format!("Failed to send message: {}", e)))?;
        } else {
            return Err(AppError::Network("Not connected".to_string()));
        }

        // 等待响应
        let response = rx.await.map_err(|e| AppError::Network(format!("Channel error: {}", e)))?;

        Ok(response)
    }

    /// 使用已存储的凭据认证
    pub async fn authenticate(
        &self,
        ws_sender: &Option<tokio::sync::mpsc::Sender<WsMessage>>,
        pending_requests: &Arc<RwLock<std::collections::HashMap<String, PendingRequest>>>,
        timeout_ms: u64,
    ) -> Result<Message> {
        let token = match self.get_session_token() {
            Some(t) => t,
            None => return Err(AppError::Auth("No session token".to_string())),
        };

        let fingerprint = self.get_device_fingerprint();
        let device_id = self.get_device_id();

        let message_id = Uuid::new_v4().to_string();
        let message = Message::Auth {
            message_id: message_id.clone(),
            session_id: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
            payload: AuthPayload {
                stage: AuthStage::Authenticated,
                device_id: Some(device_id),
                device_name: Some(self.get_device_name()),
                device_fingerprint: Some(fingerprint),
                pairing_code: None,
                session_token: Some(token),
                error: None,
                qr_token: None,
            },
        };

        // 创建 oneshot 通道用于接收响应
        let (tx, rx) = tokio::sync::oneshot::channel();

        // 创建超时任务
        let pending_clone = pending_requests.clone();
        let timeout_id = message_id.clone();
        let timeout = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)).await;
            if let Some(pending) = pending_clone.write().await.remove(&timeout_id) {
                let _ = pending.resolve.send(Message::error("TIMEOUT", "Request timeout"));
            }
        });

        // 保存待处理请求
        pending_requests.write().await.insert(
            message_id.clone(),
            PendingRequest {
                resolve: tx,
                timeout,
            },
        );

        // 发送消息
        if let Some(sender) = ws_sender {
            let json = message.to_json()?;
            sender
                .send(WsMessage::Text(json))
                .await
                .map_err(|e| AppError::Network(format!("Failed to send message: {}", e)))?;
        } else {
            return Err(AppError::Network("Not connected".to_string()));
        }

        // 等待响应
        let response = rx.await.map_err(|e| AppError::Network(format!("Channel error: {}", e)))?;

        Ok(response)
    }

    /// 保存配对码
    pub async fn save_pairing_code(&self, code: String) {
        let result = PairingRequestResult {
            code: code.clone(),
            expires_in: 300, // 5分钟
        };
        *self.pairing_code.write().await = Some(result);
    }

    /// 清空配对码
    pub async fn clear_pairing_code(&self) {
        *self.pairing_code.write().await = None;
    }
}

impl Default for PairingModule {
    fn default() -> Self {
        Self::new().as_ref().clone()
    }
}

impl Clone for PairingModule {
    fn clone(&self) -> Self {
        Self {
            pairing_code: RwLock::new(None),
            pending_requests: self.pending_requests.clone(),
            pairing_service: self.pairing_service.clone(),
            device_id: RwLock::new(self.get_device_id()),
            device_fingerprint: RwLock::new(self.get_device_fingerprint()),
            device_name: RwLock::new(self.get_device_name()),
            session_token: RwLock::new(self.get_session_token()),
        }
    }
}