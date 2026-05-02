//! WebSocket Server Implementation
//!
//! 提供 WebSocket 服务端功能，处理移动端连接和消息路由

use super::message::{AuthPayload, AuthStage, ControlAction, Message};
use crate::auth::PairingService;
use crate::db::Database;
use crate::pty::PtyOutputEvent;
use crate::session::SessionManager;
use crate::Result;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;

/// 心跳超时时间（秒）
const HEARTBEAT_TIMEOUT_SECS: u64 = 90;

/// 客户端连接信息
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub addr: SocketAddr,
    pub device_id: Option<String>,
    pub authenticated: bool,
    pub session_ids: Vec<String>,
    /// 订阅的会话列表（用于输出转发）
    pub subscribed_sessions: Vec<String>,
    /// 最后收到心跳的时间
    pub last_heartbeat: Instant,
}

/// WebSocket 服务器
pub struct WebSocketServer {
    port: u16,
    session_manager: Arc<SessionManager>,
    db: Arc<Mutex<Database>>,
    pairing_service: Arc<PairingService>,
    clients: Arc<RwLock<HashMap<SocketAddr, ClientInfo>>>,
    /// 客户端发送器映射（用于向特定客户端发送消息）
    client_senders: Arc<RwLock<HashMap<SocketAddr, mpsc::UnboundedSender<WsMessage>>>>,
    /// Shutdown signal sender
    shutdown_tx: broadcast::Sender<()>,
    /// Whether the server is running
    is_running: Arc<RwLock<bool>>,
}

impl WebSocketServer {
    /// 创建新的 WebSocket 服务器
    pub fn new(
        port: u16,
        session_manager: Arc<SessionManager>,
        db: Arc<Mutex<Database>>,
        pairing_service: Arc<PairingService>,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            port,
            session_manager,
            db,
            pairing_service,
            clients: Arc::new(RwLock::new(HashMap::new())),
            client_senders: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx,
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    /// 启动服务器
    pub async fn start(&self) -> Result<()> {
        let addr: SocketAddr = format!("0.0.0.0:{}", self.port)
            .parse()
            .map_err(|e| crate::AppError::WebSocket(format!("Invalid address: {}", e)))?;
        let listener = TcpListener::bind(&addr).await?;

        // Mark as running
        {
            let mut running = self.is_running.write().await;
            *running = true;
        }

        tracing::info!("WebSocket server listening on ws://{}", addr);

        // 启动输出转发任务
        let output_forwarder = OutputForwarder {
            session_manager: self.session_manager.clone(),
            clients: self.clients.clone(),
            client_senders: self.client_senders.clone(),
        };

        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let mut forwarder_shutdown = self.shutdown_tx.subscribe();
        let mut heartbeat_shutdown = self.shutdown_tx.subscribe();

        // 启动输出转发任务
        tokio::spawn(async move {
            output_forwarder.run(&mut forwarder_shutdown).await;
        });

        // 启动心跳超时检测任务
        let heartbeat_clients = self.clients.clone();
        let heartbeat_senders = self.client_senders.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // 检查所有客户端的心跳超时
                        let mut clients = heartbeat_clients.write().await;
                        let senders = heartbeat_senders.read().await;
                        let now = Instant::now();

                        let timeout_clients: Vec<SocketAddr> = clients
                            .iter()
                            .filter(|(_, client)| {
                                now.duration_since(client.last_heartbeat).as_secs() > HEARTBEAT_TIMEOUT_SECS
                            })
                            .map(|(addr, _)| *addr)
                            .collect();

                        for addr in timeout_clients {
                            tracing::warn!("Client {} heartbeat timeout, disconnecting", addr);
                            clients.remove(&addr);
                            if let Some(tx) = senders.get(&addr) {
                                let _ = tx.send(WsMessage::Close(None));
                            }
                        }
                    }
                    _ = heartbeat_shutdown.recv() => {
                        tracing::info!("Heartbeat checker shutting down");
                        break;
                    }
                }
            }
        });

        loop {
            tokio::select! {
                // Accept new connections
                accept_result = listener.accept() => {
                    let (stream, addr) = accept_result?;

                    let session_manager = self.session_manager.clone();
                    let db = self.db.clone();
                    let pairing_service = self.pairing_service.clone();
                    let clients = self.clients.clone();
                    let client_senders = self.client_senders.clone();
                    let mut shutdown_rx_inner = self.shutdown_tx.subscribe();

                    tokio::spawn(async move {
                        tracing::info!("New connection from {}", addr);

                        let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                            Ok(ws) => ws,
                            Err(e) => {
                                tracing::error!("WebSocket handshake error: {}", e);
                                return;
                            }
                        };

                        let (ws_sender, mut ws_receiver) = ws_stream.split();

                        // 创建无界通道用于向客户端发送消息
                        let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();

                        // 注册客户端
                        {
                            let mut clients = clients.write().await;
                            clients.insert(
                                addr,
                                ClientInfo {
                                    addr,
                                    device_id: None,
                                    authenticated: false,
                                    session_ids: vec![],
                                    subscribed_sessions: vec![],
                                    last_heartbeat: Instant::now(),
                                },
                            );
                        }

                        // 注册发送器
                        {
                            let mut senders = client_senders.write().await;
                            senders.insert(addr, tx.clone());
                        }

                        // 用于接收任务的 tx 克隆
                        let tx_recv = tx.clone();

                        // 发送任务：从通道接收消息并发送到WebSocket
                        let send_task = async move {
                            let mut ws_sender = ws_sender;
                            while let Some(msg) = rx.recv().await {
                                if ws_sender.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        };

                        // 接收任务：处理来自WebSocket的消息
                        let clients_for_recv = clients.clone();
                        let recv_task = async move {
                            while let Some(msg_result) = ws_receiver.next().await {
                                match msg_result {
                                    Ok(WsMessage::Text(text)) => {
                                        match Message::from_json(&text) {
                                            Ok(message) => {
                                                let response = handle_message(
                                                    message,
                                                    addr,
                                                    &session_manager,
                                                    &db,
                                                    &pairing_service,
                                                    &clients_for_recv,
                                                )
                                                .await;

                                                match response {
                                                    Ok(Some(resp)) => {
                                                        if let Ok(json) = resp.to_json() {
                                                            // 通过通道发送响应
                                                            let _ = tx_recv.send(WsMessage::Text(json));
                                                        }
                                                    }
                                                    Ok(None) => {}
                                                    Err(e) => {
                                                        let error_msg =
                                                            Message::error("HANDLER_ERROR", &e.to_string());
                                                        if let Ok(json) = error_msg.to_json() {
                                                            let _ = tx_recv.send(WsMessage::Text(json));
                                                        }
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!("Parse message error: {}", e);
                                                let error_msg = Message::error("PARSE_ERROR", &e.to_string());
                                                if let Ok(json) = error_msg.to_json() {
                                                    let _ = tx_recv.send(WsMessage::Text(json));
                                                }
                                            }
                                        }
                                    }
                                    Ok(WsMessage::Ping(data)) => {
                                        let _ = tx_recv.send(WsMessage::Pong(data));
                                    }
                                    Ok(WsMessage::Pong(_)) => {}
                                    Ok(WsMessage::Close(_)) => {
                                        tracing::info!("Client {} closed connection", addr);
                                        break;
                                    }
                                    Err(e) => {
                                        tracing::error!("WebSocket error: {}", e);
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        };

                        // 等待关闭信号或任一任务完成
                        tokio::select! {
                            _ = send_task => {},
                            _ = recv_task => {},
                            _ = shutdown_rx_inner.recv() => {
                                tracing::info!("Closing connection to {} due to shutdown", addr);
                                let _ = tx.send(WsMessage::Close(None));
                            }
                        }

                        // 移除客户端
                        {
                            let mut clients = clients.write().await;
                            clients.remove(&addr);
                        }
                        {
                            let mut senders = client_senders.write().await;
                            senders.remove(&addr);
                        }
                        tracing::info!("Client {} disconnected", addr);
                    });
                }

                // Handle shutdown signal
                _ = shutdown_rx.recv() => {
                    tracing::info!("WebSocket server shutting down");

                    // Mark as not running
                    {
                        let mut running = self.is_running.write().await;
                        *running = false;
                    }

                    // Close all client connections
                    let senders = self.client_senders.read().await;
                    tracing::info!("Closing {} client connections", senders.len());

                    for (_, tx) in senders.iter() {
                        let _ = tx.send(WsMessage::Close(None));
                    }

                    break;
                }
            }
        }

        Ok(())
    }

    /// Stop the server gracefully
    pub async fn stop(&self) -> Result<()> {
        tracing::info!("Sending shutdown signal to WebSocket server");
        let _ = self.shutdown_tx.send(());
        Ok(())
    }

    /// Check if the server is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// 获取已连接客户端数
    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }
}

/// 输出转发器
///
/// 负责将 PTY 输出转发给订阅了相应会话的客户端
struct OutputForwarder {
    session_manager: Arc<SessionManager>,
    clients: Arc<RwLock<HashMap<SocketAddr, ClientInfo>>>,
    client_senders: Arc<RwLock<HashMap<SocketAddr, mpsc::UnboundedSender<WsMessage>>>>,
}

impl OutputForwarder {
    async fn run(&self, shutdown_rx: &mut broadcast::Receiver<()>) {
        let mut output_rx = self.session_manager.subscribe_output();

        loop {
            tokio::select! {
                result = output_rx.recv() => {
                    match result {
                        Ok(event) => {
                            // 转发输出给订阅了该会话的客户端
                            if let Err(e) = self.forward_output(&event).await {
                                tracing::error!("Failed to forward output: {}", e);
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            tracing::debug!("Output channel closed");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("Output channel lagged {} messages", n);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("Output forwarder shutting down");
                    break;
                }
            }
        }
    }

    /// 转发PTY输出到订阅的客户端
    async fn forward_output(&self, event: &PtyOutputEvent) -> Result<()> {
        // 解码输出数据用于检测等待输入状态
        let decoded_data = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &event.data,
        ).unwrap_or_default();

        // 检测是否等待输入
        let is_waiting = crate::parser::detect_waiting_input(
            &String::from_utf8_lossy(&decoded_data)
        );

        // 创建输出消息
        let message = Message::output(&event.session_id, &decoded_data, is_waiting);
        let json = message.to_json()?;
        let ws_message = WsMessage::Text(json);

        // 获取所有订阅了该会话的客户端
        let clients = self.clients.read().await;
        let senders = self.client_senders.read().await;

        for (addr, client) in clients.iter() {
            if client.authenticated && client.subscribed_sessions.contains(&event.session_id) {
                if let Some(tx) = senders.get(addr) {
                    if tx.send(ws_message.clone()).is_err() {
                        tracing::debug!("Failed to send output to client {}", addr);
                    }
                }
            }
        }

        Ok(())
    }
}

/// 处理消息
async fn handle_message(
    message: Message,
    addr: SocketAddr,
    session_manager: &Arc<SessionManager>,
    db: &Arc<Mutex<Database>>,
    pairing_service: &Arc<PairingService>,
    clients: &Arc<RwLock<HashMap<SocketAddr, ClientInfo>>>,
) -> Result<Option<Message>> {
    // 更新客户端心跳时间（任何消息都算作活跃）
    {
        let mut clients = clients.write().await;
        if let Some(client) = clients.get_mut(&addr) {
            client.last_heartbeat = Instant::now();
        }
    }

    match message {
        Message::Auth { message_id, payload, .. } => handle_auth(payload, message_id, addr, db, pairing_service, clients).await,

        Message::Input { message_id, session_id, payload, .. } => {
            // 检查认证
            {
                let clients = clients.read().await;
                let client = clients.get(&addr);
                if client.map(|c| !c.authenticated).unwrap_or(true) {
                    return Ok(Some(Message::error_with_id(&message_id, "UNAUTHORIZED", "Not authenticated")));
                }
            }

            // 处理输入
            if let Some(key) = &payload.special_key {
                session_manager.send_special_key(&session_id, key.as_str()).await?;
            } else {
                session_manager.write_input(&session_id, &payload.data).await?;
            }

            Ok(None)
        }

        Message::Control { message_id, payload, .. } => {
            // 检查认证
            {
                let clients = clients.read().await;
                let client = clients.get(&addr);
                if client.map(|c| !c.authenticated).unwrap_or(true) {
                    return Ok(Some(Message::error_with_id(&message_id, "UNAUTHORIZED", "Not authenticated")));
                }
            }

            handle_control(payload.action, message_id, session_manager, db, clients, addr).await
        }

        Message::Heartbeat { .. } => {
            // 心跳时间已在 handle_message 开头更新
            Ok(Some(Message::heartbeat()))
        }

        _ => Ok(Some(Message::error("UNKNOWN_MESSAGE", "Unknown message type"))),
    }
}

/// 处理认证消息
async fn handle_auth(
    payload: AuthPayload,
    request_message_id: String,
    addr: SocketAddr,
    db: &Arc<Mutex<Database>>,
    pairing_service: &Arc<PairingService>,
    clients: &Arc<RwLock<HashMap<SocketAddr, ClientInfo>>>,
) -> Result<Option<Message>> {
    match payload.stage {
        AuthStage::RequestPairing => {
            // 客户端请求配对
            Ok(Some(Message::Auth {
                message_id: request_message_id,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: AuthPayload {
                    stage: AuthStage::VerifyCode,
                    device_id: payload.device_id,
                    device_name: payload.device_name,
                    error: None,
                    ..Default::default()
                },
            }))
        }

        AuthStage::VerifyCode => {
            // 验证配对码 - 使用 PairingService 进行验证
            let code = payload.pairing_code.unwrap_or_default();

            // 使用配对服务验证配对码
            let is_valid = pairing_service.verify_code(&code).await;

            if is_valid {
                // 配对成功，存储设备信息
                let device_id = payload.device_id.unwrap_or_else(|| {
                    uuid::Uuid::new_v4().to_string()
                });
                let device_name = payload.device_name.unwrap_or_else(|| "Unknown Device".to_string());
                let fingerprint = payload.device_fingerprint.unwrap_or_default();

                let db = db.lock().await;
                db.add_pairing(&device_name, &fingerprint, "")?;
                drop(db);

                // 更新客户端状态
                {
                    let mut clients = clients.write().await;
                    if let Some(client) = clients.get_mut(&addr) {
                        client.device_id = Some(device_id.clone());
                        client.authenticated = true;
                    }
                }

                // 清除已使用的配对码
                pairing_service.clear_code().await;

                Ok(Some(Message::Auth {
                    message_id: request_message_id,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    payload: AuthPayload {
                        stage: AuthStage::Authenticated,
                        device_id: Some(device_id),
                        session_token: Some(uuid::Uuid::new_v4().to_string()),
                        error: None,
                        ..Default::default()
                    },
                }))
            } else {
                // 检查是否有当前配对码来判断是过期还是无效
                let current_code = pairing_service.get_current_code().await;
                let error_message = if current_code.is_none() {
                    "No pairing code available. Please generate a new code."
                } else {
                    "Invalid or expired pairing code"
                };

                Ok(Some(Message::Auth {
                    message_id: request_message_id,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    payload: AuthPayload {
                        stage: AuthStage::Failed,
                        error: Some(error_message.to_string()),
                        ..Default::default()
                    },
                }))
            }
        }

        AuthStage::Authenticated => {
            // 已认证的设备
            let device_id = payload.device_id.unwrap_or_default();

            // 检查设备是否已配对
            let db = db.lock().await;
            let pairings = db.get_pairings()?;
            drop(db);

            let is_paired = pairings.iter().any(|p| p.id == device_id && p.is_active);

            if is_paired {
                {
                    let mut clients = clients.write().await;
                    if let Some(client) = clients.get_mut(&addr) {
                        client.device_id = Some(device_id.clone());
                        client.authenticated = true;
                    }
                }

                Ok(Some(Message::Auth {
                    message_id: request_message_id,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    payload: AuthPayload {
                        stage: AuthStage::Authenticated,
                        device_id: Some(device_id),
                        session_token: Some(uuid::Uuid::new_v4().to_string()),
                        error: None,
                        ..Default::default()
                    },
                }))
            } else {
                Ok(Some(Message::Auth {
                    message_id: request_message_id,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    payload: AuthPayload {
                        stage: AuthStage::Failed,
                        error: Some("Device not paired".to_string()),
                        ..Default::default()
                    },
                }))
            }
        }

        _ => Ok(Some(Message::error_with_id(&request_message_id, "INVALID_AUTH_STAGE", "Invalid auth stage"))),
    }
}

/// 处理控制消息
async fn handle_control(
    action: ControlAction,
    request_message_id: String,
    session_manager: &Arc<SessionManager>,
    db: &Arc<Mutex<Database>>,
    clients: &Arc<RwLock<HashMap<SocketAddr, ClientInfo>>>,
    addr: SocketAddr,
) -> Result<Option<Message>> {
    match action {
        ControlAction::ListSessions => {
            let sessions = session_manager.list_sessions().await;
            let summaries = sessions
                .into_iter()
                .map(|s| super::message::SessionSummary {
                    id: s.id,
                    name: s.name,
                    status: format!("{:?}", s.status),
                })
                .collect();

            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: super::message::ControlPayload {
                    action: ControlAction::SessionList { sessions: summaries },
                },
            }))
        }

        ControlAction::ListSessionConfigs => {
            let db = db.lock().await;
            let configs = db.get_session_configs()?;
            drop(db);

            let summaries = configs
                .into_iter()
                .map(|c| super::message::SessionConfigSummary {
                    id: c.id,
                    name: c.name,
                    environment: c.environment,
                    wsl_distro: c.wsl_distro,
                    working_dir: c.working_dir,
                    command: c.command,
                })
                .collect();

            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: super::message::ControlPayload {
                    action: ControlAction::SessionConfigList { configs: summaries },
                },
            }))
        }

        ControlAction::StartSession { config_id } => {
            let session_id = session_manager.create_session(&config_id).await?;
            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: super::message::ControlPayload {
                    action: ControlAction::StartSession { config_id },
                },
            }))
        }

        ControlAction::StopSession { session_id } => {
            session_manager.kill_session(&session_id).await?;

            // 从客户端订阅列表中移除该会话
            {
                let mut clients = clients.write().await;
                if let Some(client) = clients.get_mut(&addr) {
                    client.subscribed_sessions.retain(|s| s != &session_id);
                }
            }

            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: super::message::ControlPayload {
                    action: ControlAction::StopSession { session_id },
                },
            }))
        }

        ControlAction::ResizeSession { session_id, cols, rows } => {
            session_manager.resize_session(&session_id, cols, rows).await?;
            Ok(None)
        }

        ControlAction::ListQuickActions => {
            let db = db.lock().await;
            let actions = db.get_quick_actions()?;
            drop(db);

            let summaries = actions
                .into_iter()
                .map(|a| super::message::QuickActionSummary {
                    id: a.id,
                    name: a.name,
                    content: a.content,
                    icon: a.icon,
                    color: a.color,
                })
                .collect();

            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: super::message::ControlPayload {
                    action: ControlAction::QuickActionList { actions: summaries },
                },
            }))
        }

        ControlAction::JoinSession { session_id } => {
            // 检查会话是否存在
            let sessions = session_manager.list_sessions().await;
            if !sessions.iter().any(|s| s.id == session_id) {
                return Ok(Some(Message::error_with_id(&request_message_id, "SESSION_NOT_FOUND", &format!("Session not found: {}", session_id))));
            }

            // 更新客户端订阅列表
            {
                let mut clients = clients.write().await;
                if let Some(client) = clients.get_mut(&addr) {
                    if !client.subscribed_sessions.contains(&session_id) {
                        client.subscribed_sessions.push(session_id.clone());
                        tracing::info!("Client {} joined session {}", addr, session_id);
                    }
                }
            }

            // 返回成功响应
            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: super::message::ControlPayload {
                    action: ControlAction::JoinSession { session_id },
                },
            }))
        }

        ControlAction::LeaveSession { session_id } => {
            // 从客户端订阅列表中移除
            {
                let mut clients = clients.write().await;
                if let Some(client) = clients.get_mut(&addr) {
                    client.subscribed_sessions.retain(|s| s != &session_id);
                    tracing::info!("Client {} left session {}", addr, session_id);
                }
            }

            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: super::message::ControlPayload {
                    action: ControlAction::LeaveSession { session_id },
                },
            }))
        }

        _ => Ok(None),
    }
}

impl Default for AuthPayload {
    fn default() -> Self {
        Self {
            stage: AuthStage::RequestPairing,
            device_id: None,
            device_name: None,
            device_fingerprint: None,
            pairing_code: None,
            session_token: None,
            error: None,
        }
    }
}
