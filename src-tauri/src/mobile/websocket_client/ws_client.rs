//! WebSocket Client - Main Implementation
//!
//! 整合所有子模块的主客户端，提供统一的 API
//! 使用 RequestResponseManager 实现请求-响应模式

use crate::mobile::websocket_client::{
    connection::ConnectionManager, heartbeat::HeartbeatManager, io::IoManager,
    lifecycle::LifecycleManager, reconnect::ReconnectManager,
    ConnectionStatus, IoEvent, WsClientConfig, WsClientEvent, RequestResponseManager,
};
use crate::shared::model::message::Message;
use crate::mobile::websocket_client::MessageHandler;
use crate::Result;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tracing::{debug, error, info, warn};

/// WebSocket 客户端
pub struct WsClient {
    config: WsClientConfig,
    connection: Arc<ConnectionManager>,
    io: Arc<IoManager>,
    heartbeat: Arc<HeartbeatManager>,
    lifecycle: Arc<LifecycleManager>,
    reconnect: Arc<ReconnectManager>,
    /// 请求-响应管理器
    request_manager: Arc<RequestResponseManager>,
    /// 推送消息处理器
    handler: RwLock<Option<Arc<dyn MessageHandler>>>,
    /// WebSocket 发送通道
    ws_sender: RwLock<Option<mpsc::Sender<WsMsg>>>,
    /// 运行标记
    running: Arc<std::sync::atomic::AtomicBool>,
    /// 任务句柄
    tasks: RwLock<ClientTasks>,
    /// 事件广播器（推送消息、连接状态等）
    event_tx: broadcast::Sender<WsClientEvent>,
}

#[derive(Debug, Default)]
struct ClientTasks {
    receiver: Option<Arc<tokio::task::JoinHandle<()>>>,
    sender: Option<Arc<tokio::task::JoinHandle<()>>>,
    event_forwarder: Option<Arc<tokio::task::JoinHandle<()>>>,
    heartbeat: Option<Arc<tokio::task::JoinHandle<()>>>,
}

impl WsClient {
    pub fn new(config: WsClientConfig) -> Arc<Self> {
        let lifecycle = LifecycleManager::new();
        let connection = ConnectionManager::new(config.clone(), lifecycle.clone());
        let io = IoManager::new();
        let heartbeat = HeartbeatManager::from_client_config(config.heartbeat_interval_secs);
        let reconnect = ReconnectManager::from_client_config(config.heartbeat_interval_secs);
        let request_manager = RequestResponseManager::new();

        let (event_tx, _) = broadcast::channel(1024);

        Arc::new(Self {
            config: config.clone(),
            connection,
            io,
            heartbeat,
            lifecycle,
            reconnect,
            request_manager,
            handler: RwLock::new(None),
            ws_sender: RwLock::new(None),
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            tasks: RwLock::new(ClientTasks::default()),
            event_tx,
        })
    }

    pub fn config(&self) -> &WsClientConfig {
        &self.config
    }

    /// 订阅客户端事件（推送消息、连接状态等）
    pub fn subscribe(&self) -> broadcast::Receiver<WsClientEvent> {
        self.event_tx.subscribe()
    }

    /// 获取事件发送器
    pub fn event_tx(&self) -> broadcast::Sender<WsClientEvent> {
        self.event_tx.clone()
    }

    pub async fn get_status(&self) -> ConnectionStatus {
        self.lifecycle.get_status().await
    }

    pub async fn set_status(&self, status: ConnectionStatus) {
        self.lifecycle.set_status(status).await;
    }

    pub fn set_client_id(&self, client_id: impl Into<String>) {
        let client_id = client_id.into();
        let lifecycle = self.lifecycle.clone();
        tokio::spawn(async move {
            lifecycle.set_client_id(client_id).await;
        });
    }

    pub async fn get_client_id(&self) -> Option<String> {
        self.lifecycle.get_client_id().await
    }

    pub async fn is_connected(&self) -> bool {
        self.lifecycle.is_connected().await
    }

    /// 设置推送消息处理器
    pub async fn set_handler(&self, handler: Arc<dyn MessageHandler>) {
        *self.handler.write().await = Some(handler);
    }

    /// 获取请求-响应管理器
    pub fn request_manager(&self) -> Arc<RequestResponseManager> {
        self.request_manager.clone()
    }

    pub async fn connect(self: &Arc<Self>) -> Result<()> {
        info!("[WsClient] Starting connection to {}", self.config.url());

        let (stream, sender) = self.connection.connect().await?;

        *self.ws_sender.write().await = Some(sender.clone());
        self.running.store(true, std::sync::atomic::Ordering::SeqCst);

        self.spawn_io_tasks(stream, sender).await;
        self.start_event_forwarder().await;
        self.start_heartbeat_task().await;

        let _ = self.event_tx.send(WsClientEvent::Connected);

        info!("[WsClient] Connection established");
        Ok(())
    }

    async fn spawn_io_tasks(
        &self,
        stream: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        _sender: mpsc::Sender<WsMsg>,
    ) {
        let running = self.running.clone();

        let (tx, rx) = mpsc::channel::<WsMsg>(self.config.message_queue_size);
        *self.ws_sender.write().await = Some(tx);

        // 获取 handler 和 request_manager
        let handler = self.handler.read().await.clone();
        tracing::info!("[WsClient] Handler status: is_some={}", handler.is_some());
        let request_manager = self.request_manager.clone();
        let event_tx = self.event_tx.clone();
        let heartbeat = self.heartbeat.clone();

        let (write, read) = stream.split();
        let write = Arc::new(Mutex::new(write));

        let write_for_receiver = write.clone();
        let receiver_handle = {
            let running = running.clone();

            tokio::spawn(async move {
                use futures_util::StreamExt;
                let mut rx = read.fuse();

                info!("[WsClient] Receiver task started, waiting for messages...");

                loop {
                    if !running.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }

                    tokio::select! {
                        msg = rx.next() => {
                            match msg {
                                Some(Ok(WsMsg::Text(text))) => {
                                    info!("[WsClient] <<< RECV: {}...", &text[..text.len().min(1000)]);

                                    // 1. 尝试匹配 pending 请求
                                    match request_manager.try_match(WsMsg::Text(text.clone())).await {
                                        Some(_) => {
                                            // 未匹配，是推送消息，交给 handler 处理
                                            info!("[WsClient] Push message, handler is_some: {}", handler.is_some());
                                            let _ = event_tx.send(WsClientEvent::PushMessage {
                                                content: text.clone(),
                                            });

                                            if let Some(h) = &handler {
                                                h.handle(
                                                    WsMsg::Text(text),
                                                    "0.0.0.0:0".parse().unwrap(),
                                                    None,
                                                    None,
                                                );
                                            } else {
                                                warn!("[WsClient] No handler for push message!");
                                            }
                                        }
                                        None => {
                                            // 已匹配 pending 请求，无需处理
                                            debug!("[WsClient] Matched pending request");
                                        }
                                    }
                                }
                                Some(Ok(WsMsg::Binary(data))) => {
                                    debug!("[WsClient] <<< RECV Binary: {} bytes", data.len());
                                    // Binary 消息交给 handler 处理
                                    if let Some(h) = &handler {
                                        h.handle(
                                            WsMsg::Binary(data),
                                            "0.0.0.0:0".parse().unwrap(),
                                            None,
                                            None,
                                        );
                                    }
                                }
                                Some(Ok(WsMsg::Close(reason))) => {
                                    let reason_str = reason.map(|r| r.to_string()).unwrap_or_default();
                                    info!("[WsClient] Server closed: {}", reason_str);

                                    // 通知所有 pending 请求
                                    request_manager.on_error("Server closed").await;

                                    let _ = event_tx.send(WsClientEvent::ServerClosed { reason: reason_str });
                                    break;
                                }
                                Some(Ok(WsMsg::Ping(data))) => {
                                    let mut write = write_for_receiver.lock().await;
                                    if let Err(e) = write.send(WsMsg::Pong(data)).await {
                                        error!("[WsClient] Failed to send pong: {}", e);
                                        break;
                                    }
                                }
                                Some(Ok(WsMsg::Pong(_))) => {
                                    debug!("[WsClient] Received pong");
                                    heartbeat.on_pong_received();
                                    let _ = event_tx.send(WsClientEvent::HeartbeatResponse);
                                }
                                Some(Err(e)) => {
                                    error!("[WsClient] WebSocket error: {}", e);

                                    // 通知所有 pending 请求
                                    request_manager.on_error(&e.to_string()).await;

                                    let _ = event_tx.send(WsClientEvent::Error { message: e.to_string() });
                                    break;
                                }
                                None => break,
                                _ => {}
                            }
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
                    }
                }
            })
        };

        let write_for_sender = write.clone();
        let sender_handle = {
            let running = running.clone();

            tokio::spawn(async move {
                let mut rx = rx;

                loop {
                    if !running.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }

                    tokio::select! {
                        msg = rx.recv() => {
                            match msg {
                                Some(WsMsg::Text(text)) => {
                                    info!("[WsClient] >>> SEND: {}...", &text[..text.len().min(500)]);
                                    let mut write = write_for_sender.lock().await;
                                    if let Err(e) = write.send(WsMsg::Text(text)).await {
                                        error!("[WsClient] Send error: {}", e);
                                        break;
                                    }
                                }
                                Some(WsMsg::Binary(data)) => {
                                    let mut write = write_for_sender.lock().await;
                                    if let Err(e) = write.send(WsMsg::Binary(data)).await {
                                        error!("[WsClient] Send binary error: {}", e);
                                        break;
                                    }
                                }
                                Some(WsMsg::Close(_)) => {
                                    break;
                                }
                                None => break,
                                _ => {}
                            }
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
                    }
                }
            })
        };

        let mut tasks = self.tasks.write().await;
        tasks.receiver = Some(Arc::new(receiver_handle));
        tasks.sender = Some(Arc::new(sender_handle));
    }

    async fn start_event_forwarder(&self) {
        let io_subscription = self.io.subscribe();
        let lifecycle_subscription = self.lifecycle.subscribe();
        let event_tx = self.event_tx.clone();

        let handle = tokio::spawn(async move {
            let mut io_rx = io_subscription;
            let mut lifecycle_rx = lifecycle_subscription;

            loop {
                tokio::select! {
                    event = io_rx.recv() => {
                        match event {
                            Ok(IoEvent::HeartbeatResponse) => {
                                let _ = event_tx.send(WsClientEvent::HeartbeatResponse);
                            }
                            Ok(IoEvent::ConnectionClosed { reason }) => {
                                let _ = event_tx.send(WsClientEvent::ServerClosed { reason });
                            }
                            Ok(IoEvent::Error { message }) => {
                                let _ = event_tx.send(WsClientEvent::Error { message });
                            }
                            _ => {}
                        }
                    }
                    event = lifecycle_rx.recv() => {
                        match event {
                            Ok(crate::mobile::websocket_client::lifecycle::LifecycleEvent::Disconnected) => {
                                let _ = event_tx.send(WsClientEvent::Disconnected);
                            }
                            _ => {}
                        }
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
                }
            }
        });

        let mut tasks = self.tasks.write().await;
        tasks.event_forwarder = Some(Arc::new(handle));
    }

    /// 启动心跳保活任务
    ///
    /// 定期发送 WebSocket Ping 帧，检测连接是否仍然活跃。
    /// 连续超时 max_timeouts 次后发送 Error 事件，触发断连通知。
    async fn start_heartbeat_task(&self) {
        let running = self.running.clone();
        let ws_sender = self.ws_sender.read().await.clone();
        let heartbeat = self.heartbeat.clone();
        let event_tx = self.event_tx.clone();

        let interval = heartbeat.config().interval;
        let max_timeouts = heartbeat.config().max_timeouts;

        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            interval_timer.tick().await;

            loop {
                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                interval_timer.tick().await;

                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                // 发送 Ping
                if let Some(sender) = ws_sender.as_ref() {
                    match sender.send(WsMsg::Ping(vec![])).await {
                        Ok(_) => {
                            debug!("[Heartbeat] Ping sent");
                        }
                        Err(e) => {
                            warn!("[Heartbeat] Failed to send ping: {}", e);
                            let consecutive = heartbeat.increment_timeout().await;
                            if consecutive >= max_timeouts {
                                warn!("[Heartbeat] Max timeouts reached ({}), connection lost", consecutive);
                                let _ = event_tx.send(WsClientEvent::Error {
                                    message: format!("Heartbeat timeout after {} consecutive misses", consecutive),
                                });
                                break;
                            }
                            continue;
                        }
                    }
                } else {
                    warn!("[Heartbeat] No ws_sender, stopping heartbeat");
                    break;
                }

                // 检查心跳超时
                if heartbeat.is_connection_lost().await {
                    let consecutive = heartbeat.increment_timeout().await;
                    warn!("[Heartbeat] Heartbeat timeout (consecutive: {})", consecutive);
                    if consecutive >= max_timeouts {
                        warn!("[Heartbeat] Max timeouts reached, connection lost");
                        let _ = event_tx.send(WsClientEvent::Error {
                            message: format!("Heartbeat timeout after {} consecutive misses", consecutive),
                        });
                        break;
                    }
                }
            }

            heartbeat.stop().await;
            debug!("[Heartbeat] Task stopped");
        });

        let mut tasks = self.tasks.write().await;
        tasks.heartbeat = Some(Arc::new(handle));
    }

    pub async fn disconnect(&self) {
        info!("[WsClient] Disconnecting...");

        self.running.store(false, std::sync::atomic::Ordering::SeqCst);
        self.heartbeat.stop().await;
        *self.ws_sender.write().await = None;

        // 通知所有 pending 请求
        self.request_manager.on_error("Disconnected").await;

        self.await_tasks(3).await;

        self.lifecycle.set_status(ConnectionStatus::Disconnected).await;

        let _ = self.event_tx.send(WsClientEvent::Disconnected);

        info!("[WsClient] Disconnected");
    }

    async fn await_tasks(&self, _timeout_secs: u64) {
        // Note: We cannot directly await JoinHandle wrapped in Arc.
        // The tasks will be aborted when running flag is set to false above.
    }

    /// 发送消息（不等待响应）
    pub async fn send(&self, message: &Message) -> Result<()> {
        tracing::info!("[WsClient] send() called, checking ws_sender...");
        if let Some(sender) = self.ws_sender.read().await.as_ref() {
            let json = message.to_json()?;
            tracing::info!("[WsClient] >>> SEND to mpsc queue: {}...", &json[..json.len().min(500)]);
            sender
                .send(WsMsg::Text(json))
                .await
                .map_err(|e| crate::AppError::WebSocket(format!("Failed to send: {}", e)))?;
            tracing::info!("[WsClient] send() completed - message queued");
            Ok(())
        } else {
            tracing::error!("[WsClient] send() failed - ws_sender is None!");
            Err(crate::AppError::WebSocket("Not connected".to_string()))
        }
    }

    /// 发送原始文本
    pub async fn send_text(&self, content: &str) -> Result<()> {
        if let Some(sender) = self.ws_sender.read().await.as_ref() {
            let ws_msg = WsMsg::Text(content.to_string());
            sender
                .send(ws_msg)
                .await
                .map_err(|e| crate::AppError::WebSocket(format!("Failed to send: {}", e)))?;
            Ok(())
        } else {
            Err(crate::AppError::WebSocket("Not connected".to_string()))
        }
    }

    /// 发送消息并等待响应
    ///
    /// 使用 RequestResponseManager 实现精准投递：
    /// 1. 发送消息前注册 pending 请求
    /// 2. 收到响应时根据 message_id 匹配
    /// 3. 通过 oneshot 通道通知等待者
    pub async fn send_and_wait(
        &self,
        message: &Message,
        timeout: std::time::Duration,
    ) -> Result<Message> {
        let message_id = message.message_id()
            .ok_or_else(|| crate::AppError::WebSocket("Message has no message_id".to_string()))?
            .to_string();

        // 1. 注册 pending 请求
        let rx = self.request_manager.register(message_id.clone()).await;

        // 2. 发送消息
        if let Err(e) = self.send(message).await {
            // 发送失败，清理 pending
            self.request_manager.remove(&message_id).await;
            return Err(e);
        }

        // 3. 等待响应
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                // oneshot 通道关闭
                self.request_manager.remove(&message_id).await;
                Err(crate::AppError::WebSocket("Response channel closed".to_string()))
            }
            Err(_) => {
                // 超时，清理 pending
                self.request_manager.remove(&message_id).await;
                Err(crate::AppError::WebSocket("Response timeout".to_string()))
            }
        }
    }

    pub async fn reconnect(self: &Arc<Self>) -> Result<()> {
        if !self.reconnect.should_retry().await {
            return Err(crate::AppError::WebSocket("Max retries exceeded".to_string()));
        }

        if let Some(delay) = self.reconnect.start().await {
            info!("[WsClient] Reconnecting in {:?}...", delay);
            tokio::time::sleep(delay).await;

            match self.connect().await {
                Ok(_) => {
                    self.reconnect.on_success().await;
                    Ok(())
                }
                Err(e) => {
                    self.reconnect.on_failure(e.to_string()).await;
                    Err(crate::AppError::WebSocket(format!("Reconnect failed: {}", e)))
                }
            }
        } else {
            Err(crate::AppError::WebSocket("Reconnect abandoned".to_string()))
        }
    }
}
