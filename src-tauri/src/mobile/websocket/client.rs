//! Mobile WebSocket Client Implementation
//!
//! 移动端 WebSocket 客户端，用于连接到桌面端并进行远程控制

use crate::desktop::websocket::message::{
    AuthPayload, AuthStage, ControlAction, ControlPayload, InputPayload, Message,
    OutputPayload, SessionSummary,
};
use crate::shared::auth::qr_token::QrTokenManager;
use crate::shared::error::{AppError, Result};
use crate::shared::auth::PairingService;
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::interval;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use uuid::Uuid;

/// 连接状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// 未连接
    Disconnected,
    /// 正在连接
    Connecting,
    /// 已连接（WebSocket 连接已建立，等待认证）
    Connected,
    /// 配对中（等待用户输入配对码）
    Pairing,
    /// 已认证（配对成功）
    Authenticated,
    /// 连接错误
    Error(String),
}

/// 远程设备信息
#[derive(Debug, Clone)]
pub struct RemoteDevice {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub is_paired: bool,
}

/// 配对请求结果
#[derive(Debug, Clone)]
pub struct PairingRequestResult {
    pub code: String,
    pub expires_in: u64,
}

/// 会话信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct RemoteSession {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
}

/// 待发送的请求
struct PendingRequest {
    resolve: tokio::sync::oneshot::Sender<Message>,
    timeout: tokio::task::JoinHandle<()>,
}

/// 移动端 WebSocket 客户端
pub struct RemoteClient {
    /// 当前连接状态
    status: RwLock<ConnectionStatus>,
    /// 当前连接的设备
    current_device: RwLock<Option<RemoteDevice>>,
    /// WebSocket 发送器
    ws_sender: RwLock<Option<mpsc::Sender<WsMessage>>>,
    /// 待发送请求的映射
    pending_requests: Arc<RwLock<HashMap<String, PendingRequest>>>,
    /// 配对码
    pairing_code: RwLock<Option<PairingRequestResult>>,
    /// 配对服务（用于生成设备 ID）
    pairing_service: Arc<PairingService>,
    /// 设备 ID（持久化）
    device_id: RwLock<String>,
    /// 设备指纹
    device_fingerprint: RwLock<String>,
    /// 设备名称
    device_name: RwLock<String>,
    /// 会话令牌
    session_token: RwLock<Option<String>>,
    /// 运行中标记
    running: Arc<std::sync::atomic::AtomicBool>,
    /// 输出事件发送器（用于推送到前端）
    output_tx: broadcast::Sender<OutputEvent>,
    /// 会话列表
    sessions: RwLock<Vec<RemoteSession>>,
    /// App Handle
    app_handle: RwLock<Option<AppHandle>>,
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
            pairing_code: RwLock::new(None),
            pairing_service: Arc::new(PairingService::new()),
            device_id: RwLock::new(Uuid::new_v4().to_string()),
            device_fingerprint: RwLock::new(Uuid::new_v4().to_string()),
            device_name: RwLock::new("Mobile Device".to_string()),
            session_token: RwLock::new(None),
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            output_tx,
            sessions: RwLock::new(Vec::new()),
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
        self.pairing_code.read().await.clone()
    }

    /// 获取会话列表
    pub async fn get_sessions(&self) -> Vec<RemoteSession> {
        self.sessions.read().await.clone()
    }

    /// 订阅输出事件
    pub fn subscribe_output(&self) -> broadcast::Receiver<OutputEvent> {
        self.output_tx.subscribe()
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

    /// 连接到桌面端
    pub async fn connect(&self, address: String, port: u16) -> Result<()> {
        // 检查当前状态
        let status = self.status.read().await.clone();
        if status == ConnectionStatus::Connecting || status == ConnectionStatus::Connected || status == ConnectionStatus::Authenticated {
            tracing::warn!("Already connected or connecting");
            return Ok(());
        }

        // 设置状态为连接中
        *self.status.write().await = ConnectionStatus::Connecting;

        let url = format!("ws://{}:{}", address, port);
        tracing::info!("Connecting to {}", url);

        // 建立 WebSocket 连接
        let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .map_err(|e| AppError::Network(format!("Failed to connect: {}", e)))?;

        tracing::info!("WebSocket connected");

        // 获取读写流
        let (mut write, mut read) = ws_stream.split();

        // 创建 channel 用于发送消息
        let (tx, mut rx) = mpsc::channel::<WsMessage>(256);

        // 保存发送器
        *self.ws_sender.write().await = Some(tx);

        // 设置状态为已连接
        *self.status.write().await = ConnectionStatus::Connected;

        // 记录连接的设备
        *self.current_device.write().await = Some(RemoteDevice {
            id: format!("{}:{}", address, port),
            name: address.clone(),
            address,
            port,
            is_paired: false,
        });

        // 启动运行标记
        self.running.store(true, std::sync::atomic::Ordering::SeqCst);

        let self_clone = self.clone();
        let running = self.running.clone();

        // 发送任务：处理待发送消息和心跳
        let sender_task = tokio::spawn(async move {
            let mut heartbeat_interval = interval(Duration::from_secs(30));

            loop {
                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    tracing::info!("Sender task stopping");
                    break;
                }

                tokio::select! {
                    _ = heartbeat_interval.tick() => {
                        // 发送心跳
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

        // 接收任务：处理接收到的消息
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

            // 连接断开
            self_clone2.on_disconnected().await;
        });

        // 等待任一任务结束
        tokio::select! {
            _ = sender_task => {}
            _ = receiver_task => {}
        }

        Ok(())
    }

    /// 处理接收到的消息
    async fn handle_message(&self, text: &str) -> Result<()> {
        let message: Message = serde_json::from_str(text)
            .map_err(|e| AppError::Parse(format!("Failed to parse message: {}", e)))?;

        tracing::debug!("Received message type: {:?}", std::mem::discriminant(&message));

        match message {
            Message::Output { message_id, session_id, payload, .. } => {
                // 处理输出消息
                let event = OutputEvent {
                    event_type: "output".to_string(),
                    session_id,
                    data: payload.data,
                    is_waiting: payload.is_waiting,
                };
                let _ = self.output_tx.send(event.clone());

                // 发送到前端
                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit("remote-output", &event);
                }
            }
            Message::Auth { message_id, payload, .. } => {
                self.handle_auth_response(message_id, payload).await?;
            }
            Message::Control { message_id, payload, .. } => {
                self.handle_control_response(message_id, payload).await?;
            }
            Message::Heartbeat { .. } => {
                // 心跳响应，无需特殊处理
            }
            Message::ServerClosed { reason, will_reconnect } => {
                tracing::warn!("Server closed: {}, will_reconnect: {}", reason, will_reconnect);

                // 通知前端
                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit("server-closed", serde_json::json!({ "reason": reason, "will_reconnect": will_reconnect }));
                }

                // 更新状态
                *self.status.write().await = ConnectionStatus::Error(reason);
            }
            Message::Error { message_id, code, message } => {
                tracing::error!("Error from server: {} - {}", code, message);

                // 如果有待处理的请求，拒绝它
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

    /// 处理认证响应
    async fn handle_auth_response(&self, message_id: Option<String>, payload: AuthPayload) -> Result<()> {
        let msg_id = match message_id {
            Some(id) => id,
            None => return Ok(()),
        };

        // 查找待处理的请求
        let pending = match self.pending_requests.write().await.remove(&msg_id) {
            Some(p) => p,
            None => return Ok(()),
        };

        // 取消超时任务
        pending.timeout.abort();

        match payload.stage {
            AuthStage::VerifyCode => {
                // 配对码已生成，保存并通知前端
                let code = payload.pairing_code.unwrap_or_default();
                let result = PairingRequestResult {
                    code: code.clone(),
                    expires_in: 300, // 5分钟
                };
                *self.pairing_code.write().await = Some(result);

                // 设置状态为配对中
                *self.status.write().await = ConnectionStatus::Pairing;

                // 通知前端显示配对码输入框
                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit("pairing-code-generated", serde_json::json!({ "code": code }));
                }

                // 返回响应（让调用者知道需要用户输入配对码）
                let _ = pending.resolve.send(Message::Auth {
                    message_id: msg_id,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    payload,
                });
            }
            AuthStage::Authenticated => {
                // 认证成功
                let device_id = payload.device_id.unwrap_or_default();
                let fingerprint = payload.device_fingerprint.unwrap_or_default();
                let token = payload.session_token.unwrap_or_default();

                // 保存凭据
                *self.session_token.write().await = Some(token.clone());
                *self.device_id.write().await = device_id.clone();

                // 更新设备为已配对
                if let Some(device) = self.current_device.write().await.as_mut() {
                    device.is_paired = true;
                }

                // 设置状态为已认证
                *self.status.write().await = ConnectionStatus::Authenticated;

                // 通知前端
                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit("device-connected", serde_json::json!({
                        "device_id": device_id,
                        "device_name": self.get_device_name(),
                    }));
                }

                let _ = pending.resolve.send(Message::Auth {
                    message_id: msg_id,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    payload,
                });
            }
            AuthStage::Failed => {
                // 认证失败
                let error = payload.error.unwrap_or_else(|| "Authentication failed".to_string());
                *self.status.write().await = ConnectionStatus::Error(error.clone());

                let _ = pending.resolve.send(Message::error("AUTH_FAILED", &error));
            }
            _ => {
                tracing::debug!("Unhandled auth stage: {:?}", payload.stage);
            }
        }

        Ok(())
    }

    /// 处理控��响应
    async fn handle_control_response(&self, message_id: Option<String>, payload: ControlPayload) -> Result<()> {
        let msg_id = match message_id {
            Some(id) => id,
            None => return Ok(()),
        };

        match payload.action {
            ControlAction::SessionList { sessions } => {
                // 更新会话列表
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

                // 通知前端
                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit("session-list-updated", &remote_sessions);
                }

                // 完成待处理的请求
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
                // 通知前端会话配置列表
                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit("session-config-list", &configs);
                }

                // 完成待处理的请求
                if let Some(pending) = self.pending_requests.write().await.remove(&msg_id) {
                    let _ = pending.resolve.send(Message::Control {
                        message_id: msg_id,
                        session_id: None,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        payload,
                    });
                }
            }
            ControlAction::SessionChanged { change_type, session } => {
                // 通知前端会话变更
                if let Some(handle) = self.app_handle.read().await.as_ref() {
                    let _ = handle.emit("session-changed", serde_json::json!({
                        "change_type": change_type,
                        "session": session,
                    }));
                }
            }
            _ => {
                // 其他控制消息，完成待处理的请求
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

        // 停止运行
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);

        // 关闭 WebSocket 发送器
        *self.ws_sender.write().await = None;

        // 清空待处理请求
        self.pending_requests.write().await.clear();

        // 更新状态
        *self.status.write().await = ConnectionStatus::Disconnected;
        *self.current_device.write().await = None;

        // 通知前端
        if let Some(handle) = self.app_handle.read().await.as_ref() {
            let _ = handle.emit("device-disconnected", ());
        }

        tracing::info!("Disconnected");
    }

    /// 连接断开时的回调
    async fn on_disconnected(&self) {
        tracing::warn!("Connection lost");

        // 停止运行
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);

        // 关闭 WebSocket 发送器
        *self.ws_sender.write().await = None;

        // 清空待处理请求
        self.pending_requests.write().await.clear();

        // 更新状态
        *self.status.write().await = ConnectionStatus::Disconnected;

        // 通知前端
        if let Some(handle) = self.app_handle.read().await.as_ref() {
            let _ = handle.emit("connection-lost", ());
        }
    }

    /// 发送消息并等待响应
    pub async fn send_and_wait(&self, message: Message, timeout_ms: u64) -> Result<Message> {
        let message_id = message.message_id().unwrap_or("").to_string();

        // 创建 oneshot 通道用于接收响应
        let (tx, rx) = tokio::sync::oneshot::channel();

        // 创建超时任务
        let pending_requests = self.pending_requests.clone();
        let timeout_id = message_id.clone();
        let timeout = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(timeout_ms)).await;
            // 超时后移除待处理请求
            if let Some(pending) = pending_requests.write().await.remove(&timeout_id) {
                let _ = pending.resolve.send(Message::error("TIMEOUT", "Request timeout"));
            }
        });

        // 保存待处理请求
        self.pending_requests.write().await.insert(
            message_id.clone(),
            PendingRequest {
                resolve: tx,
                timeout,
            },
        );

        // 发送消息
        if let Some(sender) = self.ws_sender.read().await.as_ref() {
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

    /// 请求配对
    pub async fn request_pairing(&self) -> Result<()> {
        let status = self.status.read().await.clone();
        if status != ConnectionStatus::Connected && status != ConnectionStatus::Pairing {
            return Err(AppError::Network("Not connected".to_string()));
        }

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

        // 发送请求并等待响应
        let _response = self.send_and_wait(message, 30000).await?;

        // 设置状态为配对中（如果后端返回 verify_code）
        *self.status.write().await = ConnectionStatus::Pairing;

        Ok(())
    }

    /// 验证配对码
    pub async fn verify_pairing_code(&self, code: &str) -> Result<bool> {
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

        let response = self.send_and_wait(message, 30000).await?;

        // 检查响应
        if let Message::Auth { payload, .. } = response {
            match payload.stage {
                AuthStage::Authenticated => {
                    // 认证成功
                    if let Some(token) = payload.session_token {
                        *self.session_token.write().await = Some(token);
                    }
                    *self.status.write().await = ConnectionStatus::Authenticated;
                    return Ok(true);
                }
                _ => {
                    *self.status.write().await = ConnectionStatus::Error(
                        payload.error.unwrap_or_else(|| "Pairing failed".to_string())
                    );
                    return Ok(false);
                }
            }
        }

        Ok(false)
    }

    /// 使用 QR Token 认证
    pub async fn authenticate_with_qr(&self, qr_token: &str) -> Result<bool> {
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

        let response = self.send_and_wait(message, 30000).await?;

        // 检查响应
        if let Message::Auth { payload, .. } = response {
            match payload.stage {
                AuthStage::Authenticated => {
                    // 认证成功
                    if let Some(token) = payload.session_token {
                        *self.session_token.write().await = Some(token);
                    }
                    if let Some(id) = payload.device_id {
                        *self.device_id.write().await = id;
                    }
                    *self.status.write().await = ConnectionStatus::Authenticated;

                    // 更新设备
                    if let Some(device) = self.current_device.write().await.as_mut() {
                        device.is_paired = true;
                    }

                    return Ok(true);
                }
                _ => {
                    *self.status.write().await = ConnectionStatus::Error(
                        payload.error.unwrap_or_else(|| "QR authentication failed".to_string())
                    );
                    return Ok(false);
                }
            }
        }

        Ok(false)
    }

    /// 使用已存储的凭据认证
    pub async fn authenticate(&self) -> Result<bool> {
        let token = match self.get_session_token() {
            Some(t) => t,
            None => return Ok(false),
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

        let response = self.send_and_wait(message, 30000).await?;

        // 检查响应
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
        let response = self.send_control(ControlAction::ListSessions, None).await?;

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

/// 输出事件
#[derive(Debug, Clone, serde::Serialize)]
pub struct OutputEvent {
    pub event_type: String,
    pub session_id: String,
    pub data: String,
    pub is_waiting: bool,
}

impl Default for RemoteClient {
    fn default() -> Self {
        Self::new()
    }
}