//! WebSocket Server Implementation
//!
//! 不包含业务逻辑的 WebSocket 服务器基础设施
//! 提供连接管理、消息收发的基础框架

use crate::shared::websocket::message::{WsMessage, WsMessageType};
use crate::Result;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

/// WebSocket 消息处理结果
pub type HandlerResult = Result<Option<WsMessage>>;

/// 消息处理器 trait
pub trait MessageHandler: Send + Sync {
    /// 处理文本消息
    fn handle_text(
        &self,
        message: &WsMessage,
        addr: SocketAddr,
        client_info: &ClientInfo,
    ) -> HandlerResult;

    /// 处理二进制消息
    fn handle_binary(
        &self,
        message: &WsMessage,
        addr: SocketAddr,
        client_info: &ClientInfo,
    ) -> HandlerResult {
        let _ = (message, addr, client_info);
        Ok(None)
    }

    /// 客户端认证成功回调
    fn on_authenticated(&self, addr: SocketAddr, client_id: &str);

    /// 客户端断开连接回调
    fn on_disconnected(&self, addr: SocketAddr, client_id: Option<&str>);

    /// 连接建立时的回调（可选实现）
    fn on_connected(&self, _addr: SocketAddr, _client_info: &ClientInfo) {}
}

/// 心跳超时时间（秒）
const HEARTBEAT_TIMEOUT_SECS: u64 = 90;

/// 客户端连接信息
#[derive(Debug, Clone)]
pub struct ClientInfo {
    /// 客户端地址
    pub addr: SocketAddr,
    /// 客户端唯一标识
    pub client_id: Option<String>,
    /// 是否已认证
    pub authenticated: bool,
    /// 最后收到心跳的时间
    pub last_heartbeat: std::time::Instant,
}

/// WebSocket 服务器配置
#[derive(Debug, Clone)]
pub struct WsServerConfig {
    /// 监听端口
    pub port: u16,
    /// 心跳间隔（秒）
    pub heartbeat_interval_secs: u64,
    /// 心跳超时（秒）
    pub heartbeat_timeout_secs: u64,
    /// 消息队列大小
    pub message_queue_size: usize,
}

impl Default for WsServerConfig {
    fn default() -> Self {
        Self {
            port: 8765,
            heartbeat_interval_secs: 30,
            heartbeat_timeout_secs: 90,
            message_queue_size: 256,
        }
    }
}

/// WebSocket 服务器事件
#[derive(Debug, Clone)]
pub enum WsServerEvent {
    /// 新客户端连接
    ClientConnected {
        addr: SocketAddr,
        client_id: Option<String>,
    },
    /// 客户端断开
    ClientDisconnected {
        addr: SocketAddr,
        client_id: Option<String>,
    },
    /// 收到文本消息
    TextMessage {
        addr: SocketAddr,
        client_id: Option<String>,
        message_id: Option<String>,
        content: String,
    },
    /// 收到二进制消息
    BinaryMessage {
        addr: SocketAddr,
        client_id: Option<String>,
        message_id: Option<String>,
        data: Vec<u8>,
    },
    /// 收到心跳
    Heartbeat {
        addr: SocketAddr,
    },
    /// 服务器关闭
    ServerClosed {
        reason: String,
    },
}

/// WebSocket 服务器
pub struct WsServer {
    /// 配置
    config: WsServerConfig,
    /// 已连接客户端
    clients: Arc<RwLock<HashMap<SocketAddr, ClientInfo>>>,
    /// 客户端发送器映射
    client_senders: Arc<RwLock<HashMap<SocketAddr, mpsc::Sender<WsMsg>>>>,
    /// 关闭信号发送器
    shutdown_tx: broadcast::Sender<()>,
    /// 服务器事件发送器
    event_tx: broadcast::Sender<WsServerEvent>,
    /// 服务器是否运行中
    is_running: Arc<RwLock<bool>>,
}

impl WsServer {
    /// 创建新的 WebSocket 服务器
    pub fn new(config: WsServerConfig) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let (event_tx, _) = broadcast::channel(1024);

        Self {
            config,
            clients: Arc::new(RwLock::new(HashMap::new())),
            client_senders: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx,
            event_tx,
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    /// 获取配置
    pub fn config(&self) -> &WsServerConfig {
        &self.config
    }

    /// 获取事件接收器
    pub fn subscribe(&self) -> broadcast::Receiver<WsServerEvent> {
        self.event_tx.subscribe()
    }

    /// 向指定客户端发送消息
    pub async fn send_to(&self, addr: &SocketAddr, message: WsMsg) -> Result<()> {
        let senders = self.client_senders.read().await;
        if let Some(tx) = senders.get(addr) {
            tx.send(message)
                .await
                .map_err(|e| crate::AppError::WebSocket(format!("Failed to send: {}", e)))?;
            Ok(())
        } else {
            Err(crate::AppError::WebSocket(format!(
                "Client {} not found",
                addr
            )))
        }
    }

    /// 向指定客户端发送文本消息
    pub async fn send_text_to(&self, addr: &SocketAddr, content: &str) -> Result<()> {
        let ws_msg = WsMsg::Text(content.to_string());
        self.send_to(addr, ws_msg).await
    }

    /// 向除指定客户端外的所有客户端广播消息
    pub async fn broadcast_to_others(&self, exclude_addr: &SocketAddr, message: &WsMessage) -> Result<()> {
        let json = message.to_json()?;
        let ws_message = WsMsg::Text(json);

        let clients = self.clients.read().await;
        let senders = self.client_senders.read().await;

        for (addr, _) in clients.iter() {
            if addr == exclude_addr {
                continue;
            }
            if let Some(tx) = senders.get(addr) {
                let _ = tx.try_send(ws_message.clone());
            }
        }
        Ok(())
    }

    /// 向所有客户端广播消息
    pub async fn broadcast(&self, message: &WsMessage) -> Result<()> {
        let json = message.to_json()?;
        let ws_message = WsMsg::Text(json);

        let senders = self.client_senders.read().await;
        for (_, tx) in senders.iter() {
            let _ = tx.try_send(ws_message.clone());
        }
        Ok(())
    }

    /// 更新客户端认证状态
    pub async fn set_authenticated(&self, addr: &SocketAddr, client_id: Option<String>) {
        let mut clients = self.clients.write().await;
        if let Some(client) = clients.get_mut(addr) {
            client.authenticated = true;
            client.client_id = client_id;
        }
    }

    /// 获取已连接客户端数
    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }

    /// 获取已认证客户端数
    pub async fn authenticated_count(&self) -> usize {
        let clients = self.clients.read().await;
        clients.values().filter(|c| c.authenticated).count()
    }

    /// 检查服务器是否运行中
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// 启动服务器
    pub async fn start(&self) -> Result<()> {
        let addr: SocketAddr = format!("0.0.0.0:{}", self.config.port)
            .parse()
            .map_err(|e| crate::AppError::WebSocket(format!("Invalid address: {}", e)))?;

        let listener = TcpListener::bind(&addr).await?;

        // 标记为运行中
        {
            let mut running = self.is_running.write().await;
            *running = true;
        }

        tracing::info!("WebSocket server listening on ws://{}", addr);

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        // 心跳检查任务
        let heartbeat_clients = self.clients.clone();
        let heartbeat_senders = self.client_senders.clone();
        let mut heartbeat_shutdown = self.shutdown_tx.subscribe();
        let timeout_secs = self.config.heartbeat_timeout_secs;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let mut clients = heartbeat_clients.write().await;
                        let senders = heartbeat_senders.read().await;
                        let now = std::time::Instant::now();

                        let timeout_clients: Vec<SocketAddr> = clients
                            .iter()
                            .filter(|(_, client)| {
                                now.duration_since(client.last_heartbeat).as_secs() > timeout_secs
                            })
                            .map(|(addr, _)| *addr)
                            .collect();

                        for addr in timeout_clients {
                            tracing::warn!("Client {} heartbeat timeout, disconnecting", addr);
                            clients.remove(&addr);
                            if let Some(tx) = senders.get(&addr) {
                                let _ = tx.send(WsMsg::Close(None));
                            }
                        }
                    }
                    _ = heartbeat_shutdown.recv() => {
                        break;
                    }
                }
            }
        });

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    let (stream, addr) = accept_result?;

                    let clients = self.clients.clone();
                    let client_senders = self.client_senders.clone();
                    let mut shutdown_rx_inner = self.shutdown_tx.subscribe();
                    let event_tx_clone = self.event_tx.clone();
                    let config = self.config.clone();

                    // 为清理阶段创建 clone
                    let clients_for_cleanup = clients.clone();
                    let senders_for_cleanup = client_senders.clone();
                    let event_tx_for_cleanup = self.event_tx.clone();

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

                        // 创建消息通道
                        let (tx, mut rx) = mpsc::channel::<WsMsg>(config.message_queue_size);
                        let tx_clone = tx.clone();

                        // 注册客户端
                        {
                            let mut clients = clients.write().await;
                            clients.insert(
                                addr,
                                ClientInfo {
                                    addr,
                                    client_id: None,
                                    authenticated: false,
                                    last_heartbeat: std::time::Instant::now(),
                                },
                            );
                        }

                        // 注册发送器
                        {
                            let mut senders = client_senders.write().await;
                            senders.insert(addr, tx);
                        }

                        // 创建发送任务
                        let send_task = tokio::spawn(async move {
                            let mut ws_sender = ws_sender;
                            while let Some(msg) = rx.recv().await {
                                if ws_sender.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        });

                        // 创建接收任务
                        let tx_for_recv = tx_clone.clone();
                        let recv_task = tokio::spawn(async move {
                            let tx = tx_for_recv;
                            while let Some(msg_result) = ws_receiver.next().await {
                                match msg_result {
                                    Ok(WsMsg::Text(text)) => {
                                        // 更新心跳时间
                                        {
                                            let mut clients = clients.write().await;
                                            if let Some(client) = clients.get_mut(&addr) {
                                                client.last_heartbeat = std::time::Instant::now();
                                            }
                                        }

                                        // 解析消息
                                        match WsMessage::from_json(&text) {
                                            Ok(ws_msg) => {
                                                match ws_msg.message_type() {
                                                    WsMessageType::Ping => {
                                                        let pong = WsMessage::pong();
                                                        let _ = tx.send(WsMsg::Text(pong.to_json().unwrap_or_default())).await;
                                                    }
                                                    WsMessageType::Pong => {
                                                        let mut clients = clients.write().await;
                                                        if let Some(client) = clients.get_mut(&addr) {
                                                            client.last_heartbeat = std::time::Instant::now();
                                                        }
                                                    }
                                                    _ => {
                                                        let client_id = {
                                                            let clients = clients.read().await;
                                                            clients.get(&addr).and_then(|c| c.client_id.clone())
                                                        };

                                                        match &ws_msg {
                                                            WsMessage::Text { message_id, payload, .. } => {
                                                                let _ = event_tx_clone.send(WsServerEvent::TextMessage {
                                                                    addr,
                                                                    client_id,
                                                                    message_id: Some(message_id.clone()),
                                                                    content: payload.content.clone(),
                                                                });
                                                            }
                                                            WsMessage::Binary { message_id, payload, .. } => {
                                                                let data = base64::Engine::decode(
                                                                    &base64::engine::general_purpose::STANDARD,
                                                                    &payload.data,
                                                                ).unwrap_or_default();
                                                                let _ = event_tx_clone.send(WsServerEvent::BinaryMessage {
                                                                    addr,
                                                                    client_id,
                                                                    message_id: Some(message_id.clone()),
                                                                    data,
                                                                });
                                                            }
                                                            _ => {}
                                                        }
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!("Parse message error: {}", e);
                                            }
                                        }
                                    }
                                    Ok(WsMsg::Ping(data)) => {
                                        let _ = tx.send(WsMsg::Pong(data)).await;
                                    }
                                    Ok(WsMsg::Pong(_)) => {
                                        let mut clients = clients.write().await;
                                        if let Some(client) = clients.get_mut(&addr) {
                                            client.last_heartbeat = std::time::Instant::now();
                                        }
                                    }
                                    Ok(WsMsg::Close(_)) => {
                                        tracing::info!("Client {} closed connection", addr);
                                        break;
                                    }
                                    Ok(WsMsg::Binary(_)) => {}
                                    Err(e) => {
                                        tracing::error!("WebSocket error: {}", e);
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        });

                        // 等待关闭或任务完成
                        tokio::select! {
                            _ = send_task => {}
                            _ = recv_task => {}
                            _ = shutdown_rx_inner.recv() => {
                                // 从 client_senders 获取发送器来发送关闭消息
                                let senders = client_senders.read().await;
                                if let Some(tx) = senders.get(&addr) {
                                    let _ = tx.send(WsMsg::Close(None)).await;
                                }
                            }
                        }

                        // 获取断开事件需要的 client_id - 使用新的 clone
                        let client_id = {
                            let clients = clients_for_cleanup.read().await;
                            clients.get(&addr).and_then(|c| c.client_id.clone())
                        };
                        let _ = event_tx_for_cleanup.send(WsServerEvent::ClientDisconnected {
                            addr,
                            client_id,
                        });

                        // 移除客户端 - 使用新的 clone
                        {
                            let mut c = clients_for_cleanup.write().await;
                            c.remove(&addr);
                        }
                        {
                            let mut s = senders_for_cleanup.write().await;
                            s.remove(&addr);
                        }

                        tracing::info!("Client {} disconnected", addr);
                    });
                }

                _ = shutdown_rx.recv() => {
                    tracing::info!("WebSocket server shutting down");

                    // 发送关闭消息给所有客户端
                    let close_msg = WsMsg::Close(None);
                    let senders = self.client_senders.read().await;
                    for (_, tx) in senders.iter() {
                        let _ = tx.send(close_msg.clone()).await;
                    }

                    {
                        let mut running = self.is_running.write().await;
                        *running = false;
                    }

                    let _ = self.event_tx.send(WsServerEvent::ServerClosed {
                        reason: "Server shutting down".to_string(),
                    });

                    break;
                }
            }
        }

        Ok(())
    }

    /// 停止服务器
    pub async fn stop(&self) -> Result<()> {
        tracing::info!("Sending shutdown signal to WebSocket server");
        let _ = self.shutdown_tx.send(());
        Ok(())
    }
}

impl Default for WsServer {
    fn default() -> Self {
        Self::new(WsServerConfig::default())
    }
}