//! WebSocket Client Implementation
//!
//! 不包含业务逻辑的 WebSocket 客户端基础设施
//! 提供连接管理、消息收发的基础框架

use crate::shared::websocket::message::{WsMessage, WsMessageType};
use crate::Result;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use base64::Engine;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::interval;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tracing::{debug, error, info, warn};

/// 连接状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// 未连接
    Disconnected,
    /// 正在连接
    Connecting,
    /// 已连接（WebSocket 连接已建立，等待认证）
    Connected,
    /// 已认证（配对成功，仅移动端使用）
    Paired,
    /// 连接错误
    Error(String),
}

/// WebSocket 客户端配置
#[derive(Debug, Clone)]
pub struct WsClientConfig {
    /// 服务器地址
    pub address: String,
    /// 服务器端口
    pub port: u16,
    /// 心跳间隔（秒）
    pub heartbeat_interval_secs: u64,
    /// 消息队列大小
    pub message_queue_size: usize,
    /// 连接超时（毫秒）
    pub connect_timeout_ms: u64,
}

impl WsClientConfig {
    /// 创建新配置
    pub fn new(address: impl Into<String>, port: u16) -> Self {
        Self {
            address: address.into(),
            port,
            heartbeat_interval_secs: 30,
            message_queue_size: 256,
            connect_timeout_ms: 10000,
        }
    }

    /// 获取 WebSocket URL
    pub fn url(&self) -> String {
        format!("ws://{}:{}", self.address, self.port)
    }
}

impl Default for WsClientConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1".to_string(),
            port: 8765,
            heartbeat_interval_secs: 30,
            message_queue_size: 256,
            connect_timeout_ms: 10000,
        }
    }
}

/// WebSocket 客户端事件
#[derive(Debug, Clone)]
pub enum WsClientEvent {
    /// 连接成功
    Connected,
    /// 连接断开
    Disconnected,
    /// 收到文本消息
    TextMessage {
        message_id: Option<String>,
        content: String,
    },
    /// 收到二进制消息
    BinaryMessage {
        message_id: Option<String>,
        data: Vec<u8>,
    },
    /// 收到心跳响应
    HeartbeatResponse,
    /// 收到 Ack 确认
    Ack {
        message_id: String,
    },
    /// 连接错误
    Error {
        message: String,
    },
    /// 服务器关闭
    ServerClosed {
        reason: String,
    },
}

/// WebSocket 客户端
pub struct WsClient {
    /// 配置
    config: WsClientConfig,
    /// 连接状态
    status: RwLock<ConnectionStatus>,
    /// WebSocket 发送器
    ws_sender: RwLock<Option<mpsc::Sender<WsMsg>>>,
    /// 运行中标记
    running: Arc<std::sync::atomic::AtomicBool>,
    /// 事件发送器
    event_tx: broadcast::Sender<WsClientEvent>,
    /// 客户端 ID
    client_id: RwLock<Option<String>>,
    /// 消息处理器
    handler: RwLock<Option<Arc<dyn crate::shared::websocket::traits::ClientMessageHandler>>>,
    /// 发送策略
    strategy: RwLock<Arc<dyn crate::shared::websocket::traits::SendStrategy>>,
    /// 发送拦截器链
    interceptors: RwLock<Vec<Arc<dyn crate::shared::websocket::traits::SendInterceptor>>>,
    /// 发送任务句柄
    sender_task: RwLock<Option<tokio::task::JoinHandle<()>>>,
    /// 接收任务句柄
    receiver_task: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl WsClient {
    /// 创建新的 WebSocket 客户端
    pub fn new(config: WsClientConfig) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(1024);

        Arc::new(Self {
            config,
            status: RwLock::new(ConnectionStatus::Disconnected),
            ws_sender: RwLock::new(None),
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            event_tx,
            client_id: RwLock::new(None),
            handler: RwLock::new(None),
            strategy: RwLock::new(Arc::new(crate::shared::websocket::traits::DefaultSendStrategy)),
            interceptors: RwLock::new(Vec::new()),
            sender_task: RwLock::new(None),
            receiver_task: RwLock::new(None),
        })
    }

    /// 获取配置
    pub fn config(&self) -> &WsClientConfig {
        &self.config
    }

    /// 获取事件接收器
    pub fn subscribe(&self) -> broadcast::Receiver<WsClientEvent> {
        self.event_tx.subscribe()
    }

    /// 获取当前连接状态
    pub async fn get_status(&self) -> ConnectionStatus {
        self.status.read().await.clone()
    }

    /// 设置连接状态（用于配对后更新状态）
    pub async fn set_status(&self, status: ConnectionStatus) {
        *self.status.write().await = status;
    }

    /// 设置客户端 ID
    pub fn set_client_id(&self, client_id: impl Into<String>) {
        let mut guard = self.client_id.blocking_write();
        *guard = Some(client_id.into());
    }

    /// 获取客户端 ID
    pub fn get_client_id(&self) -> Option<String> {
        let guard = self.client_id.blocking_read();
        guard.clone()
    }

    /// 检查是否已连接
    pub async fn is_connected(&self) -> bool {
        let status = self.status.read().await;
        *status == ConnectionStatus::Connected || *status == ConnectionStatus::Connecting
    }

    /// 连接到服务器
    pub async fn connect(self: &Arc<Self>) -> Result<()> {
        debug!("[WsClient] connect() method entered, url: {}", self.config.url());
        // 使用原子操作确保只有一个连接任务在运行
        if self.running.compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        ).is_err() {
            tracing::warn!("Already connected or connecting");
            return Ok(());
        }

        // 检查当前状态
        {
            let status = self.status.read().await.clone();
            if status == ConnectionStatus::Connected {
                self.running.store(false, std::sync::atomic::Ordering::SeqCst);
                tracing::warn!("Already connected");
                return Ok(());
            }
        }

        let url = self.config.url();
        info!("[WsClient] Connecting to {}", url);
        debug!("[WsClient] Address: {}, Port: {}", self.config.address, self.config.port);

        // 建立 WebSocket 连接（带超时）
        let connect_start = std::time::Instant::now();
        info!("[WsClient] Starting WebSocket handshake with {}ms timeout...", self.config.connect_timeout_ms);

        let ws_stream = tokio::time::timeout(
            std::time::Duration::from_millis(self.config.connect_timeout_ms),
            tokio_tungstenite::connect_async(&url),
        )
        .await
        .map_err(|_| {
            error!("[WsClient] Connection timeout after {}ms", self.config.connect_timeout_ms);
            crate::AppError::WebSocket("Connection timeout".to_string())
        })?
        .map_err(|e| {
            error!("[WsClient] Failed to connect to {}: {:#}", url, e);
            crate::AppError::WebSocket(format!("Failed to connect: {}", e))
        })?;

        let connect_duration = connect_start.elapsed();
        tracing::info!("WebSocket handshake completed in {}ms", connect_duration.as_millis());

        // 获取读写流 - 解包 tuple 并使用 split
        let (ws_stream, _) = ws_stream;
        let (mut write, mut read) = ws_stream.split();

        // 创建消息通道
        let (tx, mut rx) = mpsc::channel::<WsMsg>(self.config.message_queue_size);
        let tx_for_recv = tx.clone();

        // 保存发送器
        *self.ws_sender.write().await = Some(tx);

        // 设置状态为已连接
        *self.status.write().await = ConnectionStatus::Connected;

        let self_clone = self.clone();
        let running = self.running.clone();

        // 发送任务：处理待发送消息和心跳
        let mut sender_task = tokio::spawn(async move {
            let mut heartbeat_interval = interval(std::time::Duration::from_secs(
                self_clone.config.heartbeat_interval_secs,
            ));

            loop {
                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    tracing::info!("Sender task stopping");
                    break;
                }

                tokio::select! {
                    _ = heartbeat_interval.tick() => {
                        // 发送心跳 - 使用我们自己的 WsMessage，经过拦截器链
                        let heartbeat = WsMessage::ping();
                        let interceptors = self_clone.interceptors.read().await;

                        // 发送前拦截器
                        for interceptor in interceptors.iter() {
                            if let Err(e) = interceptor.on_before_send(&heartbeat) {
                                tracing::warn!("Interceptor {} failed: {}", interceptor.name(), e);
                            }
                        }

                        // 发送心跳
                        let send_result = if let Ok(json) = heartbeat.to_json() {
                            write.send(WsMsg::Text(json)).await.map_err(|e| e.to_string())
                        } else {
                            Err("Failed to serialize heartbeat".to_string())
                        };

                        // 发送后拦截器
                        let send_result: Result<()> = send_result.map(|_| ()).map_err(|e| crate::AppError::WebSocket(e.to_string()));
                        for interceptor in interceptors.iter() {
                            interceptor.on_after_send(&heartbeat, &send_result);
                        }

                        if send_result.is_err() {
                            tracing::error!("Failed to send heartbeat, triggering disconnect");
                            // 发送失败时触发断开流程，确保状态一致
                            let _ = self_clone.event_tx.send(WsClientEvent::Error {
                                message: "Heartbeat send failed".to_string(),
                            });
                            break;
                        }
                    }
                    msg = rx.recv() => {
                        match msg {
                            Some(WsMsg::Text(text)) => {
                                if write.send(WsMsg::Text(text)).await.is_err() {
                                    tracing::error!("Failed to send message");
                                    break;
                                }
                            }
                            Some(WsMsg::Binary(data)) => {
                                if write.send(WsMsg::Binary(data)).await.is_err() {
                                    tracing::error!("Failed to send binary message");
                                    break;
                                }
                            }
                            Some(WsMsg::Close(_)) => {
                                tracing::info!("Close message received, stopping");
                                break;
                            }
                            Some(WsMsg::Ping(data)) => {
                                if write.send(WsMsg::Pong(data)).await.is_err() {
                                    break;
                                }
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

        // 接收任务：处理接收到的消息
        let self_clone2 = self.clone();
        let receiver_task = tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(WsMsg::Text(text)) => {
                        if let Err(e) = self_clone2.handle_text_message(&text).await {
                            tracing::error!("Failed to handle message: {}", e);
                        }
                    }
                    Ok(WsMsg::Binary(data)) => {
                        // 二进制消息尝试作为 UTF-8 文本处理
                        match String::from_utf8(data.clone()) {
                            Ok(text) => {
                                if let Err(e) = self_clone2.handle_text_message(&text).await {
                                    tracing::error!("Failed to handle binary message: {}", e);
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Binary message is not valid UTF-8: {} bytes, error: {}", data.len(), e);
                                // 发送错误事件，而不是静默丢弃
                                let _ = self_clone2.event_tx.send(WsClientEvent::Error {
                                    message: format!("Invalid UTF-8 binary message: {}", e),
                                });
                            }
                        }
                    }
                    Ok(WsMsg::Close(reason)) => {
                        tracing::info!("Server closed connection: {:?}", reason);
                        let _ = self_clone2.event_tx.send(WsClientEvent::ServerClosed {
                            reason: reason.map(|r| r.to_string()).unwrap_or_default(),
                        });
                        break;
                    }
                    Ok(WsMsg::Ping(data)) => {
                        tracing::debug!("Received ping, sending pong via channel");
                        if tx_for_recv.send(WsMsg::Pong(data)).await.is_err() {
                            tracing::error!("Failed to send pong");
                            break;
                        }
                    }
                    Ok(WsMsg::Pong(_)) => {
                        tracing::debug!("Received pong");
                        let _ = self_clone2.event_tx.send(WsClientEvent::HeartbeatResponse);
                    }
                    Err(e) => {
                        tracing::error!("WebSocket error: {}", e);
                        let _ = self_clone2.event_tx.send(WsClientEvent::Error {
                            message: e.to_string(),
                        });
                        break;
                    }
                    _ => {}
                }
            }

            // 连接断开
            self_clone2.on_disconnected().await;
        });

        // 存储任务句柄以便后续管理
        *self.sender_task.write().await = Some(sender_task);
        *self.receiver_task.write().await = Some(receiver_task);

        // 发送连接成功事件
        let _ = self.event_tx.send(WsClientEvent::Connected);

        Ok(())
    }

    /// 处理接收到的文本消息
    async fn handle_text_message(&self, text: &str) -> Result<()> {
        match WsMessage::from_json(text) {
            Ok(ws_msg) => {
                // 如果有注册处理器，调用它
                let handler_response = if let Some(handler) = self.handler.read().await.as_ref() {
                    match handler.handle(ws_msg.clone()).await {
                        Ok(Some(response)) => {
                            // 发送响应
                            let _ = self.send(&response).await;
                            Some(response)
                        }
                        Ok(None) => None,
                        Err(e) => {
                            tracing::error!("Handler error: {}", e);
                            None
                        }
                    }
                } else {
                    None
                };

                // 如果处理器已经处理了消息，不再进行默认处理
                if handler_response.is_some() {
                    return Ok(());
                }

                match ws_msg.message_type() {
                    WsMessageType::Ping => {
                        // 收到 ping，发送 pong
                        let pong = WsMessage::pong();
                        if let Some(sender) = self.ws_sender.read().await.as_ref() {
                            let _ = sender.send(WsMsg::Text(pong.to_json()?)).await;
                        }
                    }
                    WsMessageType::Pong => {
                        let _ = self.event_tx.send(WsClientEvent::HeartbeatResponse);
                    }
                    WsMessageType::Text { .. } => {
                        if let WsMessage::Text { message_id, payload, .. } = ws_msg {
                            let _ = self.event_tx.send(WsClientEvent::TextMessage {
                                message_id: Some(message_id),
                                content: payload.content,
                            });
                        }
                    }
                    WsMessageType::Binary { .. } => {
                        if let WsMessage::Binary { message_id, payload, .. } = ws_msg {
                            let data = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &payload.data)
                                .map_err(|e| crate::AppError::Parse(format!("Failed to decode base64: {}", e)))?;
                            let _ = self.event_tx.send(WsClientEvent::BinaryMessage {
                                message_id: Some(message_id),
                                data,
                            });
                        }
                    }
                    WsMessageType::Close => {
                        if let WsMessage::Close { reason } = ws_msg {
                            let _ = self.event_tx.send(WsClientEvent::ServerClosed {
                                reason,
                            });
                        }
                    }
                    WsMessageType::Error { .. } => {
                        if let WsMessage::Error { message_id: _, code, message } = ws_msg {
                            let _ = self.event_tx.send(WsClientEvent::Error {
                                message: format!("{}: {}", code, message),
                            });
                        }
                    }
                    WsMessageType::Ack => {
                        // 发送 Ack 事件，以便 send_and_wait 可以等待响应
                        if let WsMessage::Ack { original_id, .. } = ws_msg {
                            let _ = self.event_tx.send(WsClientEvent::Ack {
                                message_id: original_id,
                            });
                        }
                    }
                }
                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to parse message: {}", e);
                Err(crate::AppError::Parse(format!("Failed to parse message: {}", e)))
            }
        }
    }

    /// 断开连接
    pub async fn disconnect(&self) {
        tracing::info!("Disconnecting...");

        // 停止运行标记
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);

        // 关闭 WebSocket 发送器，触发发送任务退出
        *self.ws_sender.write().await = None;

        // 等待任务结束，带超时避免永久阻塞
        self.await_tasks(5).await;

        // 清理状态（仅在非主动断开时由 on_disconnected 处理，这里确保一致性）
        // 检查当前状态，避免重复清理
        let status = self.status.read().await.clone();
        if status != ConnectionStatus::Disconnected {
            self.cleanup_internal().await;
        }

        tracing::info!("Disconnected");
    }

    /// 等待后台任务完成（带超时）
    async fn await_tasks(&self, timeout_secs: u64) {
        if let Some(sender_handle) = self.sender_task.write().await.take() {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                sender_handle
            ).await;
        }
        if let Some(receiver_handle) = self.receiver_task.write().await.take() {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                receiver_handle
            ).await;
        }
    }

    /// 内部清理方法，避免重复代码
    async fn cleanup_internal(&self) {
        // 更新状态
        *self.status.write().await = ConnectionStatus::Disconnected;

        // 发送断开事件
        let _ = self.event_tx.send(WsClientEvent::Disconnected);
    }

    /// 连接断开时的回调
    async fn on_disconnected(&self) {
        tracing::warn!("Connection lost");

        // 停止运行标记
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);

        // 关闭 WebSocket 发送器
        *self.ws_sender.write().await = None;

        // 等待任务结束，带超时避免永久阻塞
        self.await_tasks(5).await;

        // 清理状态
        self.cleanup_internal().await;
    }

    /// 发送文本消息
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

    /// 发送 WsMessage
    pub async fn send(&self, message: &WsMessage) -> Result<()> {
        if let Some(sender) = self.ws_sender.read().await.as_ref() {
            let json = message.to_json()?;
            sender
                .send(WsMsg::Text(json))
                .await
                .map_err(|e| crate::AppError::WebSocket(format!("Failed to send: {}", e)))?;
            Ok(())
        } else {
            Err(crate::AppError::WebSocket("Not connected".to_string()))
        }
    }

    // ==================== 泛型扩展方法 ====================

    /// 设置消息处理器（异步版本，避免阻塞）
    pub async fn set_handler(&self, handler: Arc<dyn crate::shared::websocket::traits::ClientMessageHandler>) {
        let mut guard = self.handler.write().await;
        *guard = Some(handler);
    }

    /// 设置消息处理器（同步版本，仅用于兼容）
    #[allow(dead_code)]
    pub fn set_handler_blocking(&self, handler: Arc<dyn crate::shared::websocket::traits::ClientMessageHandler>) {
        let mut guard = self.handler.blocking_write();
        *guard = Some(handler);
    }

    /// 获取当前消息处理器
    pub fn get_handler(&self) -> Option<Arc<dyn crate::shared::websocket::traits::ClientMessageHandler>> {
        let guard = self.handler.blocking_read();
        guard.clone()
    }

    /// 移除消息处理器
    pub fn remove_handler(&self) {
        let mut guard = self.handler.blocking_write();
        *guard = None;
    }

    /// 设置发送策略
    pub fn set_strategy(&self, strategy: Arc<dyn crate::shared::websocket::traits::SendStrategy>) {
        let mut guard = self.strategy.blocking_write();
        *guard = strategy;
    }

    /// 获取当前发送策略
    pub fn get_strategy(&self) -> Arc<dyn crate::shared::websocket::traits::SendStrategy> {
        let guard = self.strategy.blocking_read();
        guard.clone()
    }

    /// 添加发送拦截器
    pub fn add_interceptor(&self, interceptor: Arc<dyn crate::shared::websocket::traits::SendInterceptor>) {
        let mut guard = self.interceptors.blocking_write();
        guard.push(interceptor);
    }

    /// 移除指定拦截器（按名称）
    pub fn remove_interceptor(&self, name: &str) {
        let mut guard = self.interceptors.blocking_write();
        guard.retain(|i| i.name() != name);
    }

    /// 移除所有拦截器
    pub fn clear_interceptors(&self) {
        let mut guard = self.interceptors.blocking_write();
        guard.clear();
    }

    /// 获取所有拦截器
    pub fn get_interceptors(&self) -> Vec<Arc<dyn crate::shared::websocket::traits::SendInterceptor>> {
        let guard = self.interceptors.blocking_read();
        guard.clone()
    }

    /// 发送消息并等待响应
    pub async fn send_and_wait(&self, message: &WsMessage, timeout: std::time::Duration) -> Result<WsMessage> {
        let message_id = message.message_id().map(|s| s.to_string());

        // 如果没有 message_id，无法匹配响应
        let sent_id = match message_id {
            Some(id) => id,
            None => return Err(crate::AppError::WebSocket("Message has no message_id, cannot wait for response".to_string())),
        };

        // 订阅事件以接收响应
        let mut receiver = self.event_tx.subscribe();

        // 发送消息
        self.send(message).await?;

        // 等待响应或超时
        let timeout = tokio::time::timeout(timeout, async {
            loop {
                match receiver.recv().await {
                    Ok(WsClientEvent::TextMessage { message_id: resp_id, content }) => {
                        // 检查是否是我们发送的消息的响应
                        if let Some(resp_id) = resp_id {
                            if resp_id == sent_id {
                                return Ok(WsMessage::text(content));
                            }
                        }
                        // 消息ID不匹配，继续等待
                    }
                    Ok(WsClientEvent::Ack { message_id }) => {
                        // 检查是否是与我们发送的消息匹配的 Ack
                        if message_id == sent_id {
                            // Ack 确认消息已收到，构造一���简单的响应
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
                    // 忽略其他事件类型，继续等待
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
}