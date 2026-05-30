//! WebSocket Server Implementation
//!
//! 不包含业务逻辑的 WebSocket 服务器基础设施
//! 提供连接管理、消息收发的基础框架

use crate::shared::websocket::server::{
    connection_manager::{ConnectionEvent, ConnectionManager},
    heartbeat::{HeartbeatConfig, HeartbeatEvent, HeartbeatManager},
    server_config::WsServerConfig,
    events::WsServerEvent,
    io::{ServerIo, ServerIoConfig},
};
use crate::shared::model::message::Message;
use crate::shared::websocket::{
    MessageHandler,
    server::connection_manager::Connection,
};
use crate::Result;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tracing::{debug, error, info, warn};

/// WebSocket 服务器
pub struct WsServer {
    /// 配置
    config: WsServerConfig,
    /// 消息处理器（使用 RwLock 支持动态设置）
    handler: Arc<RwLock<Option<Arc<dyn MessageHandler>>>>,
    /// 连接管理器（核心功能）
    connection_manager: Arc<ConnectionManager>,
    /// IO 模块（统一发送功能）
    server_io: Arc<ServerIo>,
    /// 心跳管理器
    heartbeat_manager: Arc<HeartbeatManager>,
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

        // 创建 ConnectionManager
        let connection_manager = Arc::new(ConnectionManager::new(&config));

        // 创建 ServerIo
        let server_io_config = ServerIoConfig::new(
            config.message_queue_size,
            5000,
            3,
            1000,
        );
        let server_io = ServerIo::new(server_io_config, connection_manager.clone());

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
        let connection_manager_for_events = connection_manager.clone();

        let _handle = tokio::spawn(async move {
            let mut cm_rx = cm_events;
            let mut hb_rx = hb_events;

            loop {
                tokio::select! {
                    result = cm_rx.recv() => {
                        match result {
                            Ok(ConnectionEvent::Connected { id, addr }) => {
                                // 此事件已在 accept 阶段直接发布，这里不再重复发布
                                debug!("[EventForwarder] ConnectionEvent::Connected: {}", addr);
                            }
                            Ok(ConnectionEvent::Disconnected { id, addr, client_id }) => {
                                // 此事件已在 unregister 阶段直接发布，这里不再重复发布
                                debug!("[EventForwarder] ConnectionEvent::Disconnected: {} ({:?})", addr, client_id);
                            }
                            Ok(ConnectionEvent::Authenticated { id, client_id }) => {
                                // 发布认证成功事件
                                let addr = connection_manager_for_events.get(id).await.map(|c| c.addr);
                                if let Some(addr) = addr {
                                    let _ = event_tx_clone.send(WsServerEvent::AuthSuccess {
                                        addr,
                                        client_id,
                                    });
                                }
                            }
                            Ok(ConnectionEvent::Heartbeat { id }) => {
                                // 发布心跳事件
                                let addr = connection_manager_for_events.get(id).await.map(|c| c.addr);
                                if let Some(addr) = addr {
                                    let _ = event_tx_clone.send(WsServerEvent::Heartbeat {
                                        addr,
                                    });
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                        }
                    },
                    result = hb_rx.recv() => {
                        match result {
                            Ok(HeartbeatEvent::Timeout { id, addr }) => {
                                // 发布心跳超时事件
                                let client_id = connection_manager_for_events.get(id).await.and_then(|c| c.client_id);
                                let _ = event_tx_clone.send(WsServerEvent::HeartbeatTimeout {
                                    addr,
                                    client_id,
                                });
                            }
                            Ok(HeartbeatEvent::Authenticated { id, client_id }) => {
                                // 发布认证成功事件
                                let addr = connection_manager_for_events.get(id).await.map(|c| c.addr);
                                if let Some(addr) = addr {
                                    let _ = event_tx_clone.send(WsServerEvent::AuthSuccess {
                                        addr,
                                        client_id,
                                    });
                                }
                            }
                            Ok(HeartbeatEvent::Disconnected { id, addr }) => {
                                // 发布心跳超时导致的断开连接事件
                                let client_id = connection_manager_for_events.get(id).await.and_then(|c| c.client_id);
                                let _ = event_tx_clone.send(WsServerEvent::ClientDisconnected {
                                    addr,
                                    client_id,
                                    reason: Some("Heartbeat timeout".to_string()),
                                });
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                        }
                    }
                }
            }
        });

        Self {
            config,
            handler: Arc::new(RwLock::new(handler)),
            connection_manager,
            server_io,
            heartbeat_manager,
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

    /// 获取事件发送器（用于注册到全局事件匹配器）
    pub fn subscribe_sender(&self) -> broadcast::Sender<WsServerEvent> {
        self.event_tx.clone()
    }

    /// 设置消息处理器
    pub async fn set_handler(&self, handler: Arc<dyn MessageHandler>) {
        let mut h = self.handler.write().await;
        *h = Some(handler);
    }

    /// 向指定客户端发送消息
    pub async fn send_to(&self, addr: &SocketAddr, message: WsMsg) -> Result<()> {
        match message {
            WsMsg::Text(text) => {
                // 将文本解析为 Message，或者直接发送原始文本
                match Message::from_json(&text) {
                    Ok(msg) => self.server_io.send_to(addr, &msg).await,
                    Err(_) => {
                        // 解析失败，发送原始文本作为错误消息
                        let msg = Message::error("PARSE_ERROR", "Invalid message format");
                        self.server_io.send_to(addr, &msg).await
                    }
                }
            }
            WsMsg::Binary(data) => {
                // 二进制数据，发送错误响应
                let msg = Message::error("BINARY_NOT_SUPPORTED", "Binary messages not supported");
                self.server_io.send_to(addr, &msg).await
            }
            _ => Err(crate::AppError::WebSocket("Unsupported message type".to_string())),
        }
    }

    /// 向指定客户端发送文本消息
    pub async fn send_text_to(&self, addr: &SocketAddr, content: &str) -> Result<()> {
        // 将文本解析为 Message
        match Message::from_json(content) {
            Ok(msg) => self.server_io.send_to(addr, &msg).await,
            Err(_) => {
                // 解析失败，发送原始文本
                let msg = Message::error("PARSE_ERROR", "Invalid message format");
                self.server_io.send_to(addr, &msg).await
            }
        }
    }

    /// 向除指定客户端外的所有客户端广播消息
    pub async fn broadcast_to_others(&self, exclude_addr: &SocketAddr, message: &Message) -> Result<()> {
        self.server_io.broadcast_to_others(exclude_addr, message).await
    }

    /// 向所有客户端广播消息
    pub async fn broadcast(&self, message: &Message) -> Result<()> {
        self.server_io.broadcast(message).await
    }

    /// 发送并等待确认
    pub async fn send_with_ack(&self, addr: &SocketAddr, message: &Message, timeout: std::time::Duration) -> Result<Message> {
        self.server_io.send_with_ack(addr, message, timeout).await
    }

    /// 发送并自动重试
    pub async fn send_with_retry(&self, addr: &SocketAddr, message: &Message) -> Result<()> {
        self.server_io.send_with_retry(addr, message).await
    }

    /// 获取 ServerIo 引用
    pub fn server_io(&self) -> &Arc<ServerIo> {
        &self.server_io
    }

    /// 更新客户端认证状态
    pub async fn set_authenticated(&self, addr: &SocketAddr, client_id: Option<String>) {
        if let Some(id) = self.connection_manager.get_id_by_addr(addr).await {
            self.connection_manager.set_client_id(id, client_id.clone()).await;

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

    /// 获取客户端信息（返回 Connection）
    pub async fn get_client(&self, addr: &SocketAddr) -> Option<Connection> {
        let id = self.connection_manager.get_id_by_addr(addr).await?;
        self.connection_manager.get(id).await
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

    /// 检查服务器是否运行中
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// 启动服务器
    pub async fn start(&self) -> Result<()> {
        let addr: SocketAddr = format!("0.0.0.0:{}", self.config.port)
            .parse()
            .map_err(|e| crate::AppError::WebSocket(format!("Invalid address: {}", e)))?;

        // 发布服务器正在启动事件
        let _ = self.event_tx.send(WsServerEvent::ServerStarting {
            addr: "0.0.0.0:0".parse().unwrap_or(addr),
        });

        let listener = TcpListener::bind(&addr).await?;
        debug!("[WsServer] TCP listener bound to {}", addr);

        // 标记为运行中
        {
            let mut running = self.is_running.write().await;
            *running = true;
        }

        info!("[WsServer] WebSocket server listening on ws://{}", addr);

        // 发布服务器启动成功事件
        let _ = self.event_tx.send(WsServerEvent::ServerStarted {
            port: self.config.port,
        });

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    let (stream, addr) = accept_result?;
                    debug!("[WsServer] Received accept request from {}", addr);

                    let connection_manager = self.connection_manager.clone();
                    let mut shutdown_rx_inner = self.shutdown_tx.subscribe();
                    let event_tx_clone = self.event_tx.clone();
                    let config = self.config.clone();

                    // 为清理阶段创建 clone
                    let connection_manager_for_cleanup = self.connection_manager.clone();
                    let event_tx_for_cleanup = self.event_tx.clone();
                    let handler = self.handler.read().await.clone();

                    tokio::spawn(async move {
                        info!("[WsServer] New connection from {}", addr);

                        let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                            Ok(ws) => {
                                // 发布 WebSocket 握手成功事件
                                let _ = event_tx_clone.send(WsServerEvent::HandshakeSuccess {
                                    addr,
                                });
                                ws
                            }
                            Err(e) => {
                                // 发布 WebSocket 握手失败事件
                                let _ = event_tx_clone.send(WsServerEvent::HandshakeFailed {
                                    addr,
                                    error: e.to_string(),
                                });
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
                            Some(id) => {
                                // 发布连接注册成功事件
                                let _ = event_tx_clone.send(WsServerEvent::ConnectionRegistered {
                                    addr,
                                    connection_id: id.to_string(),
                                });
                                id
                            }
                            None => {
                                // IP 过滤或连接数限制拒绝
                                return;
                            }
                        };

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

                        // 创建接收任务（业务处理分离到线程池）
                        let tx_for_recv = tx_clone.clone();
                        let handler_for_recv = handler.clone();
                        let connection_manager_for_recv = connection_manager.clone();
                        let event_tx_for_recv = event_tx_clone.clone();
                        let mut recv_task = tokio::spawn(async move {
                            let tx = tx_for_recv;
                            while let Some(msg_result) = ws_receiver.next().await {
                                match msg_result {
                                    Ok(WsMsg::Text(text)) => {
                                        // 获取客户端 ID
                                        let client_id = {
                                            if let Some(id) = connection_manager_for_recv.get_id_by_addr(&addr).await {
                                                connection_manager_for_recv.get(id).await.and_then(|c| c.client_id)
                                            } else {
                                                None
                                            }
                                        };

                                        // 更新心跳
                                        if let Some(id) = connection_manager_for_recv.get_id_by_addr(&addr).await {
                                            connection_manager_for_recv.update_heartbeat(id).await;
                                        }

                                        debug!("[WsServer] <<< RECV from {}: {}", addr, &text[..text.len().min(1000)]);

                                        // 调用 handler 处理
                                        let handler = handler_for_recv.clone();
                                        if let Some(h) = handler {
                                            h.handle(WsMsg::Text(text), addr, client_id.as_deref(), Some(tx.clone()));
                                        }
                                    }
                                    Ok(WsMsg::Binary(data)) => {
                                        // 获取客户端 ID
                                        let client_id = {
                                            if let Some(id) = connection_manager_for_recv.get_id_by_addr(&addr).await {
                                                connection_manager_for_recv.get(id).await.and_then(|c| c.client_id)
                                            } else {
                                                None
                                            }
                                        };

                                        // 更新心跳
                                        if let Some(id) = connection_manager_for_recv.get_id_by_addr(&addr).await {
                                            connection_manager_for_recv.update_heartbeat(id).await;
                                        }

                                        debug!("[WsServer] <<< RECV Binary from {}: {} bytes", addr, data.len());

                                        // 调用 handler 处理
                                        let handler = handler_for_recv.clone();
                                        if let Some(h) = handler {
                                            h.handle(WsMsg::Binary(data), addr, client_id.as_deref(), Some(tx.clone()));
                                        }
                                    }
                                    Ok(WsMsg::Ping(data)) => {
                                        // 发布 Ping 收到事件
                                        let _ = event_tx_for_recv.send(WsServerEvent::PingReceived {
                                            addr,
                                        });
                                        let _ = tx.send(WsMsg::Pong(data)).await;
                                    }
                                    Ok(WsMsg::Pong(_)) => {
                                        // 发布 Pong 收到事件（心跳更新）
                                        let _ = event_tx_for_recv.send(WsServerEvent::PongReceived {
                                            addr,
                                        });
                                        // 更新心跳时间通过 ConnectionManager
                                        if let Some(id) = connection_manager_for_recv.get_id_by_addr(&addr).await {
                                            connection_manager_for_recv.update_heartbeat(id).await;
                                        }
                                    }
                                    Ok(WsMsg::Close(_)) => {
                                        // 发布收到关闭帧事件
                                        let _ = event_tx_for_recv.send(WsServerEvent::CloseFrameReceived {
                                            addr,
                                        });
                                        info!("Client {} closed connection", addr);
                                        break;
                                    }
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

                        // 发布连接注销事件
                        let _ = event_tx_for_cleanup.send(WsServerEvent::ConnectionUnregistered {
                            addr,
                            connection_id: connection_id.to_string(),
                        });

                        // 发布客户端断开连接事���
                        let _ = event_tx_for_cleanup.send(WsServerEvent::ClientDisconnected {
                            addr,
                            client_id,
                            reason: Some("Connection closed by client".to_string()),
                        });

                        info!("Client {} disconnected", addr);
                    });
                }

                _ = shutdown_rx.recv() => {
                    info!("WebSocket server shutting down");

                    // 发布服务器正在关闭事件
                    let _ = self.event_tx.send(WsServerEvent::ServerStopping {
                        reason: "Shutdown signal received".to_string(),
                    });

                    // 发布关闭信号接收事件
                    let _ = self.event_tx.send(WsServerEvent::ShutdownReceived {
                        reason: "Shutdown signal received".to_string(),
                    });

                    // 使用 ServerIo 广播关闭消息
                    let close_msg = Message::server_closed("Server shutting down", true);
                    let _ = self.server_io.broadcast(&close_msg).await;

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