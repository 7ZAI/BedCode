//! WebSocket Server Implementation
//!
//! 提供 WebSocket 服务端功能，处理移动端连接和消息路由

use crate::desktop::websocket::handlers::handle_message;
use crate::desktop::websocket::output_forwarder::OutputForwarder;
use crate::desktop::websocket::message::{Message, DeviceConnectionEvent};
use crate::shared::auth::PairingService;
use crate::shared::auth::QrTokenManager;
use crate::shared::db::Database;
use crate::desktop::plugin::PluginManager;
use crate::desktop::session::SessionManager;
use crate::Result;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use tauri::Emitter;
use tauri::AppHandle;

/// 心跳超时时间（秒）
const HEARTBEAT_TIMEOUT_SECS: u64 = 90;

/// 客户端连接信息
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub addr: SocketAddr,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub authenticated: bool,
    pub session_ids: Vec<String>,
    /// 订阅的会话列表（用于输出转发）
    pub subscribed_sessions: Vec<String>,
    /// 最后收到心跳的时间
    pub last_heartbeat: Instant,
    /// 客户端的终端列数（每个客户端独立）
    pub cols: u16,
    /// 客户端的终端行数（每个客户端独立）
    pub rows: u16,
}

/// WebSocket 服务器
pub struct WebSocketServer {
    port: u16,
    session_manager: Arc<SessionManager>,
    plugin_manager: Arc<PluginManager>,
    db: Arc<Mutex<Database>>,
    pairing_service: Arc<PairingService>,
    qr_manager: Arc<QrTokenManager>,
    clients: Arc<RwLock<HashMap<SocketAddr, ClientInfo>>>,
    /// 客户端发送器映射（用于向特定客户端发送消息）
    client_senders: Arc<RwLock<HashMap<SocketAddr, mpsc::UnboundedSender<WsMessage>>>>,
    /// Shutdown signal sender
    shutdown_tx: broadcast::Sender<()>,
    /// Whether the server is running
    is_running: Arc<RwLock<bool>>,
    /// Tauri AppHandle for emitting events to frontend
    app_handle: Option<Arc<AppHandle>>,
}

impl WebSocketServer {
    /// 创建新的 WebSocket 服务器
    pub fn new(
        port: u16,
        session_manager: Arc<SessionManager>,
        plugin_manager: Arc<PluginManager>,
        db: Arc<Mutex<Database>>,
        pairing_service: Arc<PairingService>,
        qr_manager: Arc<QrTokenManager>,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            port,
            session_manager,
            plugin_manager,
            db,
            pairing_service,
            qr_manager,
            clients: Arc::new(RwLock::new(HashMap::new())),
            client_senders: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx,
            is_running: Arc::new(RwLock::new(false)),
            app_handle: None,
        }
    }

    /// 设置 AppHandle（用于发送事件到前端）
    pub fn set_app_handle(&mut self, app_handle: Arc<AppHandle>) {
        self.app_handle = Some(app_handle);
    }

    /// 向除指定客户端外的所有客户端广播消息
    pub async fn broadcast_to_others(&self, exclude_addr: SocketAddr, message: Message) {
        let json = match message.to_json() {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("Failed to serialize message: {}", e);
                return;
            }
        };
        let ws_message = WsMessage::Text(json);

        let clients = self.clients.read().await;
        let senders = self.client_senders.read().await;

        for (addr, client) in clients.iter() {
            // 跳过排除的客户端和未认证的客户端
            if addr == &exclude_addr || !client.authenticated {
                continue;
            }
            if let Some(tx) = senders.get(addr) {
                if let Err(e) = tx.send(ws_message.clone()) {
                    tracing::debug!("Failed to broadcast to {}: {}", addr, e);
                }
            }
        }
    }

    /// 向所有已认证客户端广播消息
    pub async fn broadcast_to_all(&self, message: Message) {
        let json = match message.to_json() {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("Failed to serialize message: {}", e);
                return;
            }
        };
        let ws_message = WsMessage::Text(json);

        let clients = self.clients.read().await;
        let senders = self.client_senders.read().await;

        for (addr, client) in clients.iter() {
            if !client.authenticated {
                continue;
            }
            if let Some(tx) = senders.get(addr) {
                if let Err(e) = tx.send(ws_message.clone()) {
                    tracing::debug!("Failed to broadcast to {}: {}", addr, e);
                }
            }
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

        // 创建输出转发器
        let output_forwarder = OutputForwarder::new(
            self.session_manager.clone(),
            self.clients.clone(),
            self.client_senders.clone(),
        );

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
        let heartbeat_app_handle = self.app_handle.clone();
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
                            // Emit disconnect event before removing
                            if let Some(client) = clients.get(&addr) {
                                if client.authenticated {
                                    if let Some(ref handle) = heartbeat_app_handle {
                                        let _ = handle.emit("device-disconnected", &DeviceConnectionEvent {
                                            addr: addr.to_string(),
                                            device_id: client.device_id.clone().unwrap_or_default(),
                                            device_name: client.device_name.clone(),
                                            event: "disconnected".to_string(),
                                        });
                                    }
                                }
                            }
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
                    let plugin_manager = self.plugin_manager.clone();
                    let db = self.db.clone();
                    let pairing_service = self.pairing_service.clone();
                    let qr_manager = self.qr_manager.clone();
                    let clients = self.clients.clone();
                    let client_senders = self.client_senders.clone();
                    let mut shutdown_rx_inner = self.shutdown_tx.subscribe();
                    let app_handle = self.app_handle.clone();

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
                                    device_name: None,
                                    authenticated: false,
                                    session_ids: vec![],
                                    subscribed_sessions: vec![],
                                    last_heartbeat: Instant::now(),
                                    cols: 120,
                                    rows: 40,
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
                        let senders_for_recv = client_senders.clone();
                        // Clone app_handle for disconnect event (recv_task moves the original)
                        let app_handle_for_disconnect = app_handle.clone();
                        let recv_task = async move {
                            while let Some(msg_result) = ws_receiver.next().await {
                                match msg_result {
                                    Ok(WsMessage::Text(text)) => {
                                        tracing::debug!("Received message from {}: {}", addr, &text[..text.len().min(500)]);
                                        match Message::from_json(&text) {
                                            Ok(message) => {
                                                // 提取 request ID 以便错误响应也能带上
                                                let request_id = message.message_id().map(|s| s.to_string());
                                                let response = handle_message(
                                                    message,
                                                    addr,
                                                    &session_manager,
                                                    &plugin_manager,
                                                    &db,
                                                    &pairing_service,
                                                    &qr_manager,
                                                    &clients_for_recv,
                                                    &senders_for_recv,
                                                    &app_handle,
                                                )
                                                .await;

                                                match response {
                                                    Ok(Some(resp)) => {
                                                        if let Ok(json) = resp.to_json() {
                                                            tracing::info!(
                                                                "Sending response to {}: {}",
                                                                addr,
                                                                &json[..json.len().min(200)]
                                                            );
                                                            let _ = tx_recv.send(WsMessage::Text(json));
                                                        }
                                                    }
                                                    Ok(None) => {}
                                                    Err(e) => {
                                                        tracing::error!(
                                                            "Handler error for client {} (request {:?}): {}",
                                                            addr, request_id, e
                                                        );
                                                        let error_msg = match &request_id {
                                                            Some(id) => Message::error_with_id(id, "HANDLER_ERROR", &e.to_string()),
                                                            None => Message::error("HANDLER_ERROR", &e.to_string()),
                                                        };
                                                        if let Ok(json) = error_msg.to_json() {
                                                            let _ = tx_recv.send(WsMessage::Text(json));
                                                        }
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!("Parse message error from {}: {}", addr, e);
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

                        // 通知前端设备断开
                        {
                            let clients_read = clients.read().await;
                            if let Some(client) = clients_read.get(&addr) {
                                if client.authenticated {
                                    // Emit Tauri event for desktop frontend
                                    if let Some(ref handle) = app_handle_for_disconnect {
                                        let _ = handle.emit("device-disconnected", &DeviceConnectionEvent {
                                            addr: addr.to_string(),
                                            device_id: client.device_id.clone().unwrap_or_default(),
                                            device_name: client.device_name.clone(),
                                            event: "disconnected".to_string(),
                                        });
                                    }

                                    // Broadcast client_disconnected to other WebSocket clients
                                    let device_name = client.device_name.clone().unwrap_or_else(|| "Unknown".to_string());
                                    let disconnect_msg = Message::client_disconnected(&device_name, "Connection closed");
                                    if let Ok(json) = disconnect_msg.to_json() {
                                        let ws_msg = tokio_tungstenite::tungstenite::protocol::Message::Text(json);
                                        let senders = client_senders.read().await;
                                        for (a, client) in clients_read.iter() {
                                            if a != &addr && client.authenticated {
                                                if let Some(tx) = senders.get(a) {
                                                    let _ = tx.send(ws_msg.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // 移除客户端（不重复发送事件，前面已发送）
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

                    // 广播服务端关闭消息给所有客户端
                    let server_closed_msg = Message::server_closed("Server shutting down", false);
                    if let Ok(json) = server_closed_msg.to_json() {
                        let ws_msg = WsMessage::Text(json);
                        let senders = self.client_senders.read().await;
                        for (_, tx) in senders.iter() {
                            let _ = tx.send(ws_msg.clone());
                        }
                    }

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

    /// 获取已认证的客户端列表
    pub async fn get_connected_devices(&self) -> Vec<DeviceConnectionInfo> {
        let clients = self.clients.read().await;
        clients
            .iter()
            .filter(|(_, c)| c.authenticated)
            .map(|(addr, c)| DeviceConnectionInfo {
                addr: addr.to_string(),
                device_id: c.device_id.clone().unwrap_or_default(),
                session_count: c.subscribed_sessions.len(),
            })
            .collect()
    }

    /// 更新指定客户端的终端尺寸
    pub async fn update_client_size(&self, addr: &SocketAddr, cols: u16, rows: u16) {
        let mut clients = self.clients.write().await;
        if let Some(client) = clients.get_mut(addr) {
            client.cols = cols;
            client.rows = rows;
            tracing::debug!("Updated client {} terminal size to {}x{}", addr, cols, rows);
        }
    }

    /// 获取指定客户端的终端尺寸
    pub async fn get_client_size(&self, addr: &SocketAddr) -> Option<(u16, u16)> {
        let clients = self.clients.read().await;
        clients.get(addr).map(|c| (c.cols, c.rows))
    }
}

/// 设备连接信息（前端展示用）
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceConnectionInfo {
    pub addr: String,
    pub device_id: String,
    pub session_count: usize,
}