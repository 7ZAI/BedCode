//! WebSocket Client - Main Implementation
//!
//! 整合所有子模块的主客户端，提供统一的 API

use crate::shared::websocket::client::{
    connection::ConnectionManager, heartbeat::HeartbeatManager, io::IoManager,
    lifecycle::LifecycleManager, reconnect::ReconnectManager, router::MessageRouterManager,
    ConnectionStatus, IoEvent, WsClientConfig, WsClientEvent,
};
use crate::shared::websocket::message::WsMessage;
use crate::Result;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tracing::{debug, error, info};

/// WebSocket 客户端
pub struct WsClient {
    config: WsClientConfig,
    connection: Arc<ConnectionManager>,
    io: Arc<IoManager>,
    heartbeat: Arc<HeartbeatManager>,
    lifecycle: Arc<LifecycleManager>,
    router: Arc<MessageRouterManager>,
    reconnect: Arc<ReconnectManager>,
    ws_sender: RwLock<Option<mpsc::Sender<WsMsg>>>,
    running: Arc<std::sync::atomic::AtomicBool>,
    tasks: RwLock<ClientTasks>,
    event_tx: broadcast::Sender<WsClientEvent>,
}

#[derive(Debug, Default)]
struct ClientTasks {
    receiver: Option<tokio::task::JoinHandle<()>>,
    sender: Option<tokio::task::JoinHandle<()>>,
    heartbeat: Option<tokio::task::JoinHandle<()>>,
    event_forwarder: Option<tokio::task::JoinHandle<()>>,
}

impl WsClient {
    pub fn new(config: WsClientConfig) -> Arc<Self> {
        let lifecycle = LifecycleManager::new();
        let connection = ConnectionManager::new(config.clone(), lifecycle.clone());
        let io = IoManager::new();
        let heartbeat = HeartbeatManager::from_client_config(config.heartbeat_interval_secs);
        let router = MessageRouterManager::with_default_config();
        let reconnect = ReconnectManager::from_client_config(config.heartbeat_interval_secs);

        let (event_tx, _) = broadcast::channel(1024);

        Arc::new(Self {
            config: config.clone(),
            connection,
            io,
            heartbeat,
            lifecycle,
            router,
            reconnect,
            ws_sender: RwLock::new(None),
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            tasks: RwLock::new(ClientTasks::default()),
            event_tx,
        })
    }

    pub fn config(&self) -> &WsClientConfig {
        &self.config
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WsClientEvent> {
        self.event_tx.subscribe()
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

    pub async fn connect(self: &Arc<Self>) -> Result<()> {
        info!("[WsClient] Starting connection to {}", self.config.url());

        let (stream, sender) = self.connection.connect().await?;

        *self.ws_sender.write().await = Some(sender.clone());
        self.running.store(true, std::sync::atomic::Ordering::SeqCst);

        self.spawn_io_tasks(stream, sender).await;
        self.start_event_forwarder().await;

        let _ = self.event_tx.send(WsClientEvent::Connected);

        info!("[WsClient] Connection established");
        Ok(())
    }

    async fn spawn_io_tasks(
        &self,
        stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        sender: mpsc::Sender<WsMsg>,
    ) {
        let running = self.running.clone();

        let (tx, rx) = mpsc::channel::<WsMsg>(self.config.message_queue_size);
        *self.ws_sender.write().await = Some(tx);

        let (mut write, read) = stream.split();

        let receiver_handle = {
            let running = running.clone();
            let event_tx = self.event_tx.clone();

            tokio::spawn(async move {
                use futures_util::StreamExt;
                let mut rx = read.fuse();

                loop {
                    if !running.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }

                    tokio::select! {
                        msg = rx.next() => {
                            match msg {
                                Some(Ok(WsMsg::Text(text))) => {
                                    debug!("[WsClient] <<< RECV: {}...", &text[..text.len().min(200)]);
                                    let _ = event_tx.send(WsClientEvent::TextMessage {
                                        message_id: None,
                                        content: text,
                                    });
                                }
                                Some(Ok(WsMsg::Binary(data))) => {
                                    if let Ok(text) = String::from_utf8(data.clone()) {
                                        let _ = event_tx.send(WsClientEvent::TextMessage {
                                            message_id: None,
                                            content: text,
                                        });
                                    } else {
                                        let _ = event_tx.send(WsClientEvent::BinaryMessage {
                                            message_id: None,
                                            data,
                                        });
                                    }
                                }
                                Some(Ok(WsMsg::Close(reason))) => {
                                    let reason_str = reason.map(|r| r.to_string()).unwrap_or_default();
                                    info!("[WsClient] Server closed: {}", reason_str);
                                    let _ = event_tx.send(WsClientEvent::ServerClosed { reason: reason_str });
                                    break;
                                }
                                Some(Ok(WsMsg::Ping(data))) => {
                                    if let Err(e) = write.send(WsMsg::Pong(data)).await {
                                        error!("[WsClient] Failed to send pong: {}", e);
                                        break;
                                    }
                                }
                                Some(Ok(WsMsg::Pong(_))) => {
                                    debug!("[WsClient] Received pong");
                                    let _ = event_tx.send(WsClientEvent::HeartbeatResponse);
                                }
                                Some(Err(e)) => {
                                    error!("[WsClient] WebSocket error: {}", e);
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

        let sender_handle = {
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
                                    info!("[WsClient] >>> SEND: {}...", &text[..text.len().min(200)]);
                                    if let Err(e) = write.send(WsMsg::Text(text)).await {
                                        error!("[WsClient] Send error: {}", e);
                                        break;
                                    }
                                }
                                Some(WsMsg::Binary(data)) => {
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
        tasks.receiver = Some(receiver_handle);
        tasks.sender = Some(sender_handle);
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
                            Ok(crate::shared::websocket::client::lifecycle::LifecycleEvent::Disconnected) => {
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
        tasks.event_forwarder = Some(handle);
    }

    pub async fn disconnect(&self) {
        info!("[WsClient] Disconnecting...");

        self.running.store(false, std::sync::atomic::Ordering::SeqCst);
        *self.ws_sender.write().await = None;

        self.await_tasks(3).await;

        self.lifecycle.set_status(ConnectionStatus::Disconnected).await;

        let _ = self.event_tx.send(WsClientEvent::Disconnected);

        info!("[WsClient] Disconnected");
    }

    async fn await_tasks(&self, timeout_secs: u64) {
        let tasks = self.tasks.write().await;

        if let Some(handle) = &tasks.receiver {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), handle).await;
        }

        if let Some(handle) = &tasks.sender {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), handle).await;
        }

        if let Some(handle) = &tasks.heartbeat {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), handle).await;
        }
    }

    pub async fn send(&self, message: &WsMessage) -> Result<()> {
        if let Some(sender) = self.ws_sender.read().await.as_ref() {
            let json = message.to_json()?;
            tracing::info!("[WsClient] >>> SEND: {}...", &json[..json.len().min(200)]);
            sender
                .send(WsMsg::Text(json))
                .await
                .map_err(|e| crate::AppError::WebSocket(format!("Failed to send: {}", e)))?;
            Ok(())
        } else {
            Err(crate::AppError::WebSocket("Not connected".to_string()))
        }
    }

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

    pub async fn send_and_wait(
        &self,
        message: &WsMessage,
        timeout: std::time::Duration,
    ) -> Result<WsMessage> {
        let message_id = message.message_id().map(|s| s.to_string());

        let sent_id = match message_id {
            Some(id) => id,
            None => {
                return Err(crate::AppError::WebSocket(
                    "Message has no message_id, cannot wait for response".to_string(),
                ))
            }
        };

        let mut receiver = self.event_tx.subscribe();

        self.send(message).await?;

        let timeout = tokio::time::timeout(timeout, async {
            loop {
                match receiver.recv().await {
                    Ok(WsClientEvent::TextMessage {
                        message_id: resp_id,
                        content,
                    }) => {
                        if let Some(ref resp_id) = resp_id {
                            if *resp_id == sent_id {
                                return Ok(WsMessage::text(content));
                            }
                        }
                    }
                    Ok(WsClientEvent::Ack { message_id }) => {
                        if message_id == sent_id {
                            return Ok(WsMessage::ack(sent_id));
                        }
                    }
                    Ok(WsClientEvent::Error { message }) => {
                        return Err(crate::AppError::WebSocket(message));
                    }
                    Ok(WsClientEvent::Disconnected) => {
                        return Err(crate::AppError::WebSocket("Connection lost".to_string()));
                    }
                    Ok(WsClientEvent::ServerClosed { reason }) => {
                        return Err(crate::AppError::WebSocket(format!("Server closed: {}", reason)));
                    }
                    Ok(_) => {}
                    Err(_) => {
                        return Err(crate::AppError::WebSocket("Receiver error".to_string()));
                    }
                }
            }
        });

        match timeout.await {
            Ok(result) => result,
            Err(_) => Err(crate::AppError::WebSocket("Response timeout".to_string())),
        }
    }

    pub async fn reconnect(&self) -> Result<()> {
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