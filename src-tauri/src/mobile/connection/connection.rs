//! Connection Module - 连接管理模块
//!
//! 提供移动端到桌面端的连接管理、认证、配对功能

pub mod auth;
pub mod pairing;
pub mod types;

pub use auth::PairingModule;
pub use types::{
    ConnectionStatus, OutputEvent, PairingRequestResult, PendingRequest, RemoteDevice,
    RemoteSession,
};

use crate::desktop::websocket::message::{
    AuthPayload, AuthStage, ControlAction, ControlPayload, Message,
};
use crate::shared::error::{AppError, Result};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::interval;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use uuid::Uuid;

/// 移动端连接客户端
pub struct RemoteClient {
    /// 当前连接状态
    pub status: RwLock<ConnectionStatus>,
    /// 当前连接的设备
    pub current_device: RwLock<Option<RemoteDevice>>,
    /// WebSocket 发送器
    pub ws_sender: RwLock<Option<mpsc::Sender<WsMessage>>>,
    /// 待发送请求的映射
    pub pending_requests: Arc<RwLock<HashMap<String, PendingRequest>>>,
    /// 配对模块
    pub pairing: Arc<PairingModule>,
    /// 会话列表
    pub sessions: RwLock<Vec<RemoteSession>>,
    /// 输出事件发送器（用于推送到前端）
    pub output_tx: broadcast::Sender<OutputEvent>,
    /// 运行中标记
    pub running: Arc<std::sync::atomic::AtomicBool>,
    /// App Handle
    pub app_handle: RwLock<Option<AppHandle>>,
}

impl RemoteClient {
    /// 创建新的远程客户端
    pub fn new() -> Arc<Self> {
        let (output_tx, _) = broadcast::channel(1024);

        Arc::new(Self {
            status: RwLock::new(ConnectionStatus::Disconnected),
            current_device: RwLock::new(None),
            ws_sender: RwLock::new(None),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            pairing: PairingModule::new(),
            sessions: RwLock::new(Vec::new()),
            output_tx,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            app_handle: RwLock::new(None),
        })
    }

    /// 设置 AppHandle
    pub fn set_app_handle(&self, app_handle: AppHandle) {
        let mut handle = self.app_handle.write().blocking_write();
        *handle = Some(app_handle);
    }

    /// 获取当前连接状态
    pub async fn get_status(&self) -> ConnectionStatus {
        self.status.read().await.clone()
    }

    /// 获取配对码
    pub async fn get_pairing_code(&self) -> Option<PairingRequestResult> {
        self.pairing.get_pairing_code().await
    }

    /// 获取会话列表
    pub async fn get_sessions(&self) -> Vec<RemoteSession> {
        self.sessions.read().await.clone()
    }

    /// 订阅输出事件
    pub fn subscribe_output(&self) -> broadcast::Receiver<OutputEvent> {
        self.output_tx.subscribe()
    }

    /// 连接到桌面端
    pub async fn connect(&self, address: String, port: u16) -> Result<()> {
        let status = self.status.read().await.clone();
        if status == ConnectionStatus::Connecting
            || status == ConnectionStatus::Connected
            || status == ConnectionStatus::Authenticated
        {
            tracing::warn!("Already connected or connecting");
            return Ok(());
        }

        *self.status.write().await = ConnectionStatus::Connecting;

        let url = format!("ws://{}:{}", address, port);
        tracing::info!("Connecting to {}", url);

        let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .map_err(|e| AppError::Network(format!("Failed to connect: {}", e)))?;

        tracing::info!("WebSocket connected");

        let (mut write, mut read) = ws_stream.split();
        let (tx, mut rx) = mpsc::channel::<WsMessage>(256);

        *self.ws_sender.write().await = Some(tx);
        *self.status.write().await = ConnectionStatus::Connected;

        *self.current_device.write().await = Some(RemoteDevice {
            id: format!("{}:{}", address, port),
            name: address.clone(),
            address,
            port,
            is_paired: false,
        });

        self.running.store(true, std::sync::atomic::Ordering::SeqCst);

        let self_clone = self.clone();
        let running = self.running.clone();

        let sender_task = tokio::spawn(async move {
            let mut heartbeat_interval = interval(Duration::from_secs(30));

            loop {
                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    tracing::info!("Sender task stopping");
                    break;
                }

                tokio::select! {
                    _ = heartbeat_interval.tick() => {
                        let heartbeat = Message::heartbeat();
                        if let Ok(json) = heartbeat.to_json() {
                            let _ = write.send(WsMessage::Text(json)).await;
                        }
                    }
                    msg = rx.recv() => {
                        match msg {
                            Some(WsMessage::Text(text)) => {
                                if let Err(e) = write.send(WsMessage::Text(text)).await {
                                    tracing::error!("Failed to send message: {}", e);
                                    break;
                                }
                            }
                            Some(WsMessage::Close(_)) => {
                                tracing::info!("Close message received, stopping");
                                break;
                            }
                            None => {
                                tracing::info!("Sender channel closed");
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        let self_clone2 = self.clone();

        let receiver_task = tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(WsMessage::Text(text)) => {
                        if let Err(e) = self_clone2.handle_message(&text).await {
                            tracing::error!("Failed to handle message: {}", e);
                        }
                    }
                    Ok(WsMessage::Close(_)) => {
                        tracing::info!("Server closed connection");
                        break;
                    }
                    Err(e) => {
                        tracing::error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }

            self_clone2.on_disconnected().await;
        });

        tokio::select! {
            _ = sender_task => {}
            _ = receiver_task => {}
        }

        Ok(())
    }

    /// 处理接收到的消息
    pub async fn handle_message(&self, text: &str) -> Result<()> {
        let message: Message = serde_json::from_str(text)
            .map_err(|e| AppError::Parse(format!("Failed to parse message: {}", e)))?;

        match message {
            Message::Output {
                message_id: _,
                session_id,
                payload,
                ..
            } => {
                let event = OutputEvent {
                    event_type: "output".to_string(),
                    session_id,
                    data: payload.data,
                    is_waiting: payload.is_waiting,
                };
                let _ = self.output_tx.send(event.clone());

                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit("remote-output", &event);
                }
            }
            Message::Auth {
                message_id, payload, ..
            } => {
                self.handle_auth_response(message_id, payload).await?;
            }
            Message::Control {
                message_id, payload, ..
            } => {
                self.handle_control_response(message_id, payload).await?;
            }
            Message::Heartbeat { .. } => {}
            Message::ServerClosed { reason, will_reconnect } => {
                tracing::warn!("Server closed: {}, will_reconnect: {}", reason, will_reconnect);

                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit(
                        "server-closed",
                        serde_json::json!({ "reason": reason, "will_reconnect": will_reconnect }),
                    );
                }

                *self.status.write().await = ConnectionStatus::Error(reason);
            }
            Message::Error { message_id, code, message } => {
                tracing::error!("Error from server: {} - {}", code, message);

                if let Some(msg_id) = message_id {
                    if let Some(pending) = self.pending_requests.write().await.remove(&msg_id) {
                        let _ = pending.resolve.send(Message::error(&code, &message));
                    }
                }
            }
            _ => {
                tracing::debug!("Unhandled message type");
            }
        }

        Ok(())
    }

    async fn handle_auth_response(
        &self,
        message_id: Option<String>,
        payload: AuthPayload,
    ) -> Result<()> {
        let msg_id = match message_id {
            Some(id) => id,
            None => return Ok(()),
        };

        let pending = match self.pending_requests.write().await.remove(&msg_id) {
            Some(p) => p,
            None => return Ok(()),
        };

        pending.timeout.abort();

        match payload.stage {
            AuthStage::VerifyCode => {
                let code = payload.pairing_code.unwrap_or_default();
                self.pairing.save_pairing_code(code.clone()).await;

                *self.status.write().await = ConnectionStatus::Pairing;

                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit(
                        "pairing-code-generated",
                        serde_json::json!({ "code": code }),
                    );
                }

                let _ = pending.resolve.send(Message::Auth {
                    message_id: msg_id,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    payload,
                });
            }
            AuthStage::Authenticated => {
                let device_id = payload.device_id.unwrap_or_default();
                let token = payload.session_token.unwrap_or_default();

                *self.pairing.session_token.write().await = Some(token.clone());
                *self.pairing.device_id.write().await = device_id.clone();

                if let Some(device) = self.current_device.write().await.as_mut() {
                    device.is_paired = true;
                }

                *self.status.write().await = ConnectionStatus::Authenticated;

                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit(
                        "device-connected",
                        serde_json::json!({
                            "device_id": device_id,
                            "device_name": self.pairing.get_device_name(),
                        }),
                    );
                }

                let _ = pending.resolve.send(Message::Auth {
                    message_id: msg_id,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    payload,
                });
            }
            AuthStage::Failed => {
                let error = payload
                    .error
                    .unwrap_or_else(|| "Authentication failed".to_string());
                *self.status.write().await = ConnectionStatus::Error(error.clone());

                let _ = pending.resolve.send(Message::error("AUTH_FAILED", &error));
            }
            _ => {
                tracing::debug!("Unhandled auth stage: {:?}", payload.stage);
            }
        }

        Ok(())
    }

    async fn handle_control_response(
        &self,
        message_id: Option<String>,
        payload: ControlPayload,
    ) -> Result<()> {
        let msg_id = match message_id {
            Some(id) => id,
            None => return Ok(()),
        };

        match payload.action {
            ControlAction::SessionList { sessions } => {
                let remote_sessions: Vec<RemoteSession> = sessions
                    .into_iter()
                    .map(|s| RemoteSession {
                        id: s.id,
                        name: s.name,
                        status: s.status,
                        created_at: s.created_at,
                        started_at: s.started_at,
                    })
                    .collect();
                *self.sessions.write().await = remote_sessions.clone();

                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit("session-list-updated", &remote_sessions);
                }

                if let Some(pending) = self.pending_requests.write().await.remove(&msg_id) {
                    let _ = pending.resolve.send(Message::Control {
                        message_id: msg_id,
                        session_id: None,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        payload,
                    });
                }
            }
            ControlAction::SessionConfigList { configs } => {
                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit("session-config-list", &configs);
                }

                if let Some(pending) = self.pending_requests.write().await.remove(&msg_id) {
                    let _ = pending.resolve.send(Message::Control {
                        message_id: msg_id,
                        session_id: None,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        payload,
                    });
                }
            }
            ControlAction::SessionChanged {
                change_type, session, ..
            } => {
                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit(
                        "session-changed",
                        serde_json::json!({
                            "change_type": change_type,
                            "session": session,
                        }),
                    );
                }
            }
            _ => {
                if let Some(pending) = self.pending_requests.write().await.remove(&msg_id) {
                    let _ = pending.resolve.send(Message::Control {
                        message_id: msg_id,
                        session_id: None,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        payload,
                    });
                }
            }
        }

        Ok(())
    }

    /// 断开连接
    pub async fn disconnect(&self) {
        tracing::info!("Disconnecting...");

        self.running.store(false, std::sync::atomic::Ordering::SeqCst);
        *self.ws_sender.write().await = None;
        self.pending_requests.write().await.clear();

        *self.status.write().await = ConnectionStatus::Disconnected;
        *self.current_device.write().await = None;

        if let Some(handle) = self.app_handle.read().await.as_ref() {
            let _ = handle.emit("device-disconnected", ());
        }

        tracing::info!("Disconnected");
    }

    /// 连接断开时的回调
    pub async fn on_disconnected(&self) {
        tracing::warn!("Connection lost");

        self.running.store(false, std::sync::atomic::Ordering::SeqCst);
        *self.ws_sender.write().await = None;
        self.pending_requests.write().await.clear();

        *self.status.write().await = ConnectionStatus::Disconnected;

        if let Some(handle) = self.app_handle.read().await.as_ref() {
            let _ = handle.emit("connection-lost", ());
        }
    }

    /// 发送消息并等待响应
    pub async fn send_and_wait(&self, message: Message, timeout_ms: u64) -> Result<Message> {
        let message_id = message.message_id().unwrap_or("").to_string();

        let (tx, rx) = tokio::sync::oneshot::channel();

        let pending_requests = self.pending_requests.clone();
        let timeout_id = message_id.clone();
        let timeout = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(timeout_ms)).await;
            if let Some(pending) = pending_requests.write().await.remove(&timeout_id) {
                let _ = pending.resolve.send(Message::error("TIMEOUT", "Request timeout"));
            }
        });

        self.pending_requests.write().await.insert(
            message_id.clone(),
            PendingRequest {
                resolve: tx,
                timeout,
            },
        );

        if let Some(sender) = self.ws_sender.read().await.as_ref() {
            let json = message.to_json()?;
            sender
                .send(WsMessage::Text(json))
                .await
                .map_err(|e| AppError::Network(format!("Failed to send message: {}", e)))?;
        } else {
            return Err(AppError::Network("Not connected".to_string()));
        }

        let response = rx.await.map_err(|e| AppError::Network(format!("Channel error: {}", e)))?;

        Ok(response)
    }

    /// 请求配对
    pub async fn request_pairing(&self) -> Result<()> {
        let status = self.status.read().await.clone();
        if status != ConnectionStatus::Connected && status != ConnectionStatus::Pairing {
            return Err(AppError::Network("Not connected".to_string()));
        }

        let ws_sender = self.ws_sender.read().await.clone();
        self.pairing.request_pairing(&ws_sender).await?;

        *self.status.write().await = ConnectionStatus::Pairing;

        Ok(())
    }

    /// 验证配���码
    pub async fn verify_pairing_code(&self, code: &str) -> Result<bool> {
        let ws_sender = self.ws_sender.read().await.clone();
        let response = self
            .pairing
            .verify_pairing_code(code, &ws_sender, &self.pending_requests, 30000)
            .await?;

        if let Message::Auth { payload, .. } = response {
            match payload.stage {
                AuthStage::Authenticated => {
                    if let Some(token) = payload.session_token {
                        *self.pairing.session_token.write().await = Some(token);
                    }
                    *self.status.write().await = ConnectionStatus::Authenticated;
                    return Ok(true);
                }
                _ => {
                    *self.status.write().await = ConnectionStatus::Error(
                        payload.error.unwrap_or_else(|| "Pairing failed".to_string()),
                    );
                    return Ok(false);
                }
            }
        }

        Ok(false)
    }

    /// 使用 QR Token 认证
    pub async fn authenticate_with_qr(&self, qr_token: &str) -> Result<bool> {
        let ws_sender = self.ws_sender.read().await.clone();
        let response = self
            .pairing
            .authenticate_with_qr(qr_token, &ws_sender, &self.pending_requests, 30000)
            .await?;

        if let Message::Auth { payload, .. } = response {
            match payload.stage {
                AuthStage::Authenticated => {
                    if let Some(token) = payload.session_token {
                        *self.pairing.session_token.write().await = Some(token);
                    }
                    if let Some(id) = payload.device_id {
                        *self.pairing.device_id.write().await = id;
                    }
                    *self.status.write().await = ConnectionStatus::Authenticated;

                    if let Some(device) = self.current_device.write().await.as_mut() {
                        device.is_paired = true;
                    }

                    return Ok(true);
                }
                _ => {
                    *self.status.write().await = ConnectionStatus::Error(
                        payload.error.unwrap_or_else(|| "QR authentication failed".to_string()),
                    );
                    return Ok(false);
                }
            }
        }

        Ok(false)
    }

    /// 使用已存储的凭据认证
    pub async fn authenticate(&self) -> Result<bool> {
        let ws_sender = self.ws_sender.read().await.clone();
        let response = self
            .pairing
            .authenticate(&ws_sender, &self.pending_requests, 30000)
            .await?;

        if let Message::Auth { payload, .. } = response {
            if payload.stage == AuthStage::Authenticated {
                *self.status.write().await = ConnectionStatus::Authenticated;
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// 发送终端输入
    pub async fn send_input(&self, session_id: &str, data: &str) -> Result<()> {
        if self.status.read().await.clone() != ConnectionStatus::Authenticated {
            return Err(AppError::Network("Not authenticated".to_string()));
        }

        let message = Message::input(session_id, data, None);
        let json = message.to_json()?;

        if let Some(sender) = self.ws_sender.read().await.as_ref() {
            sender
                .send(WsMessage::Text(json))
                .await
                .map_err(|e| AppError::Network(format!("Failed to send input: {}", e)))?;
        } else {
            return Err(AppError::Network("Not connected".to_string()));
        }

        Ok(())
    }

    /// 发送控制命令
    pub async fn send_control(&self, action: ControlAction, session_id: Option<&str>) -> Result<Message> {
        if self.status.read().await.clone() != ConnectionStatus::Authenticated {
            return Err(AppError::Network("Not authenticated".to_string()));
        }

        let message = Message::control(action, session_id);
        let response = self.send_and_wait(message, 30000).await?;

        Ok(response)
    }

    /// 加载会话列表
    pub async fn load_sessions(&self) -> Result<Vec<RemoteSession>> {
        let response = self
            .send_control(ControlAction::ListSessions, None)
            .await?;

        if let Message::Control { payload, .. } = response {
            if let ControlAction::SessionList { sessions } = payload.action {
                let remote_sessions: Vec<RemoteSession> = sessions
                    .into_iter()
                    .map(|s| RemoteSession {
                        id: s.id,
                        name: s.name,
                        status: s.status,
                        created_at: s.created_at,
                        started_at: s.started_at,
                    })
                    .collect();
                *self.sessions.write().await = remote_sessions.clone();
                return Ok(remote_sessions);
            }
        }

        Ok(Vec::new())
    }
}

impl Default for RemoteClient {
    fn default() -> Self {
        Self::new().as_ref().clone()
    }
}

impl Clone for RemoteClient {
    fn clone(&self) -> Self {
        Self {
            status: RwLock::new(ConnectionStatus::Disconnected),
            current_device: RwLock::new(None),
            ws_sender: RwLock::new(None),
            pending_requests: self.pending_requests.clone(),
            pairing: self.pairing.clone(),
            sessions: RwLock::new(Vec::new()),
            output_tx: self.output_tx.clone(),
            running: self.running.clone(),
            app_handle: RwLock::new(None),
        }
    }
}