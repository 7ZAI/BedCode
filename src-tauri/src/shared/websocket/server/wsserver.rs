//! WebSocket Server Implementation
//!
//! 不包含业务逻辑的 WebSocket 服务器基础设施
//! 提供连接管理、消息收发的基础框架

use crate::shared::websocket::server::{
    connection_manager::{ConnectionEvent, ConnectionManager},
    heartbeat::{HeartbeatConfig, HeartbeatEvent, HeartbeatManager},
    server_config::WsServerConfig,
};
use crate::shared::websocket::{
    message::WsMessage,
    message_handler::{handle_text_message, MessageHandlerDeps},
    traits::{DefaultClientInfo, ResponseHandler},
};
use crate::Result;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tracing::{debug, error, info};

/// WebSocket 消息处理结果
pub type HandlerResult = Result<Option<WsMessage>>;

/// 消息处理器 trait
pub trait MessageHandler: Send + Sync {
    /// 处理文本消息
    fn handle_text(
        &self,
        message: &WsMessage,
        addr: SocketAddr,
        client_info: &DefaultClientInfo,
    ) -> HandlerResult;

    /// 处理二进制消息  
    fn handle_binary(
        &self, 
        message: &WsMessage,
        addr: SocketAddr,
        client_info: &DefaultClientInfo,
    ) -> HandlerResult {
        let _ = (message, addr, client_info); 
        Ok(None)
    }

    /// 客户端认证成功回调
    fn on_authenticated(&self, addr: SocketAddr, client_id: &str);

    /// 客户端断开连接回调
    fn on_disconnected(&self, addr: SocketAddr, client_id: Option<&str>);

    /// 连接建立时的回调（可选实现）
    fn on_connected(&self, _addr: SocketAddr, _client_info: &DefaultClientInfo) {}
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
    /// 消息处理器
    handler: Option<Arc<dyn MessageHandler>>,
    /// 响应处理器
    response_handler: Option<Arc<dyn ResponseHandler>>,
    /// 连接管理器（核心功能）
    connection_manager: Arc<ConnectionManager>,
    /// 心跳管理器
    heartbeat_manager: Arc<HeartbeatManager>,
    /// 客户端信息映射（兼容 MessageHandlerDeps）
    clients: Arc<RwLock<HashMap<SocketAddr, DefaultClientInfo>>>,
    /// 关闭信号发送器
    shutdown_tx: broadcast::Sender<()>,
    /// 服务器事件发送器
    event_tx: broadcast::Sender<WsServerEvent>,
    /// 服务器是否运行中
    is_running: Arc<RwLock<bool>>,
}

impl WsServer {
    /// 创建新的 WebSocket 服务器（不带处理器）
    pub fn new(config: WsServerConfig) -> Self {
        Self::with_handler(config, None)
    }

    /// 创建带有消息处理器的 WebSocket 服务器
    pub fn with_handler(config: WsServerConfig, handler: Option<Arc<dyn MessageHandler>>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let (event_tx, _) = broadcast::channel(1024);

        // 从配置中获取 response_handler
        let response_handler = config.response_handler.clone();

        // 创建 ConnectionManager
        let connection_manager = Arc::new(ConnectionManager::new(&config));

        // 创建 HeartbeatManager
        let heartbeat_config = HeartbeatConfig::new(
            config.heartbeat_interval_secs,
            config.heartbeat_timeout_secs,
        );
        let heartbeat_manager = Arc::new(HeartbeatManager::new(
            heartbeat_config,
            connection_manager.clone(),
        ));

        // 启动心跳检测任务
        heartbeat_manager.spawn_checker();
        heartbeat_manager.spawn_event_forwarder();

        // 启动事件转发协程
        let cm_events = connection_manager.subscribe();
        let hb_events = heartbeat_manager.subscribe();
        let event_tx_clone = event_tx.clone();

        let _handle = tokio::spawn(async move {
            let mut cm_rx = cm_events;
            let mut hb_rx = hb_events;

            loop {
                tokio::select! {
                    result = cm_rx.recv() => {
                        match result {
                            Ok(ConnectionEvent::Connected { id: _, addr }) => {
                                let _ = event_tx_clone.send(WsServerEvent::ClientConnected {
                                    addr,
                                    client_id: None,
                                });
                            }
                            Ok(ConnectionEvent::Disconnected { id: _, addr, client_id }) => {
                                let _ = event_tx_clone.send(WsServerEvent::ClientDisconnected {
                                    addr,
                                    client_id,
                                });
                            }
                            Ok(ConnectionEvent::Authenticated { id: _, client_id: _ }) => {}
                            Ok(ConnectionEvent::Heartbeat { id: _ }) => {}
                            Err(broadcast::error::RecvError::Closed) => break,
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                        }
                    },
                    result = hb_rx.recv() => {
                        match result {
                            Ok(HeartbeatEvent::Timeout { id: _, addr: _ }) => {}
                            Ok(HeartbeatEvent::Authenticated { id: _, client_id: _ }) => {}
                            Ok(HeartbeatEvent::Disconnected { id: _, addr: _ }) => {}
                            Err(broadcast::error::RecvError::Closed) => break,
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                        }
                    }
                }
            }
        });

        Self {
            config,
            handler,
            response_handler,
            connection_manager,
            heartbeat_manager,
            clients: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx,
            event_tx,
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    /// 设置响应处理器
    pub fn set_response_handler(&mut self, handler: Option<Arc<dyn ResponseHandler>>) {
        self.response_handler = handler;
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
        if let Some(id) = self.connection_manager.get_id_by_addr(addr).await {
            self.connection_manager
                .send_to(id, message)
                .await
                .map_err(|e| crate::AppError::WebSocket(e))
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

        // 获取排除的连接 ID
        let exclude_id = self.connection_manager.get_id_by_addr(exclude_addr).await;

        self.connection_manager
            .broadcast_to_others(exclude_id.unwrap_or(0), &ws_message)
            .await;

        Ok(())
    }

    /// 向所有客户端广播消息
    pub async fn broadcast(&self, message: &WsMessage) -> Result<()> {
        let json = message.to_json()?;
        let ws_message = WsMsg::Text(json);
        self.connection_manager.broadcast(&ws_message).await;
        Ok(())
    }

    /// 更新客户端认证状态
    pub async fn set_authenticated(&self, addr: &SocketAddr, client_id: Option<String>) {
        if let Some(id) = self.connection_manager.get_id_by_addr(addr).await {
            self.connection_manager.set_client_id(id, client_id.clone()).await;

            // 同步更新 clients HashMap
            {
                let mut clients = self.clients.write().await;
                if let Some(client) = clients.get_mut(addr) {
                    client.authenticated = client_id.is_some();
                    client.client_id = client_id.clone();
                }
            }

            // 发送认证成功事件
            if client_id.is_some() {
                let _ = self.event_tx.send(WsServerEvent::ClientConnected {
                    addr: *addr,
                    client_id,
                });
            }
        }
    }

    /// 获取已连接客户端数
    pub async fn client_count(&self) -> usize {
        self.connection_manager.count().await
    }

    /// 获取已认证客户端数
    pub async fn authenticated_count(&self) -> usize {
        self.connection_manager.authenticated_count().await
    }

    /// 获取客户端信息（只读）
    pub async fn get_client(&self, addr: &SocketAddr) -> Option<DefaultClientInfo> {
        let id = self.connection_manager.get_id_by_addr(addr).await?;
        self.connection_manager.get(id).await.map(|conn| DefaultClientInfo {
            addr: conn.addr,
            client_id: conn.client_id,
            authenticated: conn.authenticated,
            last_heartbeat: conn.last_heartbeat,
        })
    }

    /// 获取所有已认证客户端地址
    pub async fn get_authenticated_clients(&self) -> Vec<SocketAddr> {
        let ids = self.connection_manager.authenticated_ids().await;
        let mut addrs = Vec::new();
        for id in ids {
            if let Some(conn) = self.connection_manager.get(id).await {
                addrs.push(conn.addr);
            }
        }
        addrs
    }

    /// 获取连接管理器引用（用于外部 handler）
    pub fn connection_manager(&self) -> &Arc<ConnectionManager> {
        &self.connection_manager
    }

    /// 获取客户端信息映射（兼容 MessageHandlerDeps）
    pub fn clients(&self) -> &Arc<RwLock<HashMap<SocketAddr, DefaultClientInfo>>> {
        &self.clients
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
        debug!("[WsServer] TCP listener bound to {}", addr);

        // 标记为运行中
        {
            let mut running = self.is_running.write().await;
            *running = true;
        }

        info!("[WsServer] WebSocket server listening on ws://{}", addr);

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    let (stream, addr) = accept_result?;
                    debug!("[WsServer] Received accept request from {}", addr);

                    let clients_map = self.clients.clone();
                    let connection_manager = self.connection_manager.clone();
                    let mut shutdown_rx_inner = self.shutdown_tx.subscribe();
                    let event_tx_clone = self.event_tx.clone();
                    let config = self.config.clone();

                    // 为清理阶段创建 clone
                    let connection_manager_for_cleanup = self.connection_manager.clone();
                    let clients_for_cleanup = self.clients.clone();
                    let event_tx_for_cleanup = self.event_tx.clone();
                    let handler = self.handler.clone();
                    let response_handler = self.response_handler.clone();

                    tokio::spawn(async move {
                        info!("[WsServer] New connection from {}", addr);

                        let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                            Ok(ws) => ws,
                            Err(e) => {
                                error!("WebSocket handshake error: {}", e);
                                return;
                            }
                        };

                        let (ws_sender, mut ws_receiver) = ws_stream.split();

                        // 创建消息通道
                        let (tx, mut rx) = mpsc::channel::<WsMsg>(config.message_queue_size);
                        let tx_clone = tx.clone();

                        // 使用 ConnectionManager 注册连接
                        let connection_id = match connection_manager.register(addr, tx_clone.clone()).await {
                            Some(id) => id,
                            None => {
                                // IP 过滤或连接数限制拒绝
                                return;
                            }
                        };

                        // 同步到 clients HashMap（供 MessageHandlerDeps 使用）
                        {
                            let mut clients = clients_map.write().await;
                            clients.insert(
                                addr,
                                DefaultClientInfo {
                                    addr,
                                    client_id: None,
                                    authenticated: false,
                                    last_heartbeat: std::time::Instant::now(),
                                },
                            );
                        }

                        // 发送客户端连接事件（由事件转发协程处理）

                        // 创建发送任务
                        let mut send_task = tokio::spawn(async move {
                            let mut ws_sender = ws_sender;
                            while let Some(msg) = rx.recv().await {
                                if ws_sender.send(msg).await.is_err() {
                                    debug!("[WsServer] Send task failed for {}, connection broken", addr);
                                    break;
                                }
                            }
                        });

                        // 创建接收任务
                        let tx_for_recv = tx_clone.clone();
                        let handler_for_recv = handler.clone();
                        let response_handler_for_recv = response_handler.clone();
                        let clients_for_recv = clients_map.clone();
                        let event_tx_for_recv = event_tx_clone.clone();
                        let connection_manager_for_recv = connection_manager.clone();
                        let mut recv_task = tokio::spawn(async move {
                            let tx = tx_for_recv;
                            let handler = handler_for_recv;
                            while let Some(msg_result) = ws_receiver.next().await {
                                match msg_result {
                                    Ok(WsMsg::Text(text)) => {
                                        let deps = MessageHandlerDeps {
                                            clients: clients_for_recv.clone(),
                                            tx: tx.clone(),
                                            event_tx: event_tx_for_recv.clone(),
                                            addr,
                                        };
                                        handle_text_message(&text, &deps, handler.as_ref(), response_handler_for_recv.as_ref()).await;
                                    }
                                    Ok(WsMsg::Ping(data)) => {
                                        let _ = tx.send(WsMsg::Pong(data)).await;
                                    }
                                    Ok(WsMsg::Pong(_)) => {
                                        // 更新心跳时间通过 ConnectionManager
                                        if let Some(id) = connection_manager_for_recv.get_id_by_addr(&addr).await {
                                            connection_manager_for_recv.update_heartbeat(id).await;
                                        }
                                    }
                                    Ok(WsMsg::Close(_)) => {
                                        info!("Client {} closed connection", addr);
                                        break;
                                    }
                                    Ok(WsMsg::Binary(_)) => {}
                                    Err(e) => {
                                        error!("WebSocket error: {}", e);
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        });

                        // 等待关闭或任务完成，当一个任务结束时取消另一个
                        tokio::select! {
                            _ = &mut send_task => {
                                recv_task.abort();
                            }
                            _ = &mut recv_task => {
                                send_task.abort();
                            }
                            _ = shutdown_rx_inner.recv() => {
                                send_task.abort();
                                recv_task.abort();
                                // 尝试发送关闭消息
                                let _ = tx_clone.send(WsMsg::Close(None)).await;
                            }
                        }

                        // 使用 ConnectionManager 注销连接
                        let client_id = {
                            if let Some(conn) = connection_manager_for_cleanup.get(connection_id).await {
                                conn.client_id.clone()
                            } else {
                                None
                            }
                        };
                        connection_manager_for_cleanup.unregister(connection_id).await;

                        // 同步从 clients HashMap 中移除
                        {
                            let mut clients_map = clients_for_cleanup.write().await;
                            clients_map.remove(&addr);
                        }

                        // 事件由事件转发协程处理

                        info!("Client {} disconnected", addr);
                    });
                }

                _ = shutdown_rx.recv() => {
                    info!("WebSocket server shutting down");

                    // 使用 ConnectionManager 广播关闭消息
                    let close_msg = WsMsg::Close(None);
                    self.connection_manager.broadcast(&close_msg).await;

                    // 等待客户端接收关闭消息
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

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
        info!("Sending shutdown signal to WebSocket server");
        let _ = self.shutdown_tx.send(());
        Ok(())
    }
}

impl Default for WsServer {
    fn default() -> Self {
        Self::new(WsServerConfig::default())
    }
}