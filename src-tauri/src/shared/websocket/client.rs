//! WebSocket Client Implementation
//!
//! 不包含业务逻辑的 WebSocket 客户端基础设施
//! 提供连接管理、消息收发的基础框架

use crate::shared::websocket::message::{WsMessage, WsMessageType};
use crate::Result;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::interval;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

/// 连接状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// 未连接
    Disconnected,
    /// 正在连接
    Connecting,
    /// 已连接（WebSocket 连接已建立）
    Connected,
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
        // 检查当前状态
        {
            let status = self.status.read().await.clone();
            if status == ConnectionStatus::Connecting || status == ConnectionStatus::Connected {
                tracing::warn!("Already connected or connecting");
                return Ok(());
            }
        }

        // 设置状态为连接中
        *self.status.write().await = ConnectionStatus::Connecting;

        let url = self.config.url();
        tracing::info!("Connecting to {}", url);

        // 建立 WebSocket 连接（带超时）
        let ws_stream = tokio::time::timeout(
            std::time::Duration::from_millis(self.config.connect_timeout_ms),
            tokio_tungstenite::connect_async(&url),
        )
        .await
        .map_err(|_| crate::AppError::WebSocket("Connection timeout".to_string()))?
        .map_err(|e| crate::AppError::WebSocket(format!("Failed to connect: {}", e)))?;

        tracing::info!("WebSocket connected");

        // 获取读写流 - 解包 tuple 并使用 split
        let (ws_stream, _) = ws_stream;
        let (mut write, mut read) = ws_stream.split();

        // 创建消息通道
        let (tx, mut rx) = mpsc::channel::<WsMsg>(self.config.message_queue_size);

        // 保存发送器
        *self.ws_sender.write().await = Some(tx);

        // 设置状态为已连接
        *self.status.write().await = ConnectionStatus::Connected;

        // 启动运行标记
        self.running.store(true, std::sync::atomic::Ordering::SeqCst);

        let self_clone = self.clone();
        let running = self.running.clone();

        // 发送任务：处理待发送消息和心跳
        let sender_task = tokio::spawn(async move {
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
                        // 发送心跳 - 使用我们自己的 WsMessage
                        let heartbeat = WsMessage::ping();
                        if let Ok(json) = heartbeat.to_json() {
                            if write.send(WsMsg::Text(json)).await.is_err() {
                                tracing::error!("Failed to send heartbeat");
                                break;
                            }
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

        let self_clone2 = self.clone();

        // 接收任务：处理接收到的消息
        let receiver_task = tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(WsMsg::Text(text)) => {
                        if let Err(e) = self_clone2.handle_text_message(&text).await {
                            tracing::error!("Failed to handle message: {}", e);
                        }
                    }
                    Ok(WsMsg::Binary(data)) => {
                        // 二进制消息作为文本处理
                        if let Ok(text) = String::from_utf8(data.clone()) {
                            let _ = self_clone2.handle_text_message(&text).await;
                        }
                    }
                    Ok(WsMsg::Close(reason)) => {
                        tracing::info!("Server closed connection: {:?}", reason);
                        let _ = self_clone2.event_tx.send(WsClientEvent::ServerClosed {
                            reason: reason.map(|r| r.to_string()).unwrap_or_default(),
                        });
                        break;
                    }
                    Ok(WsMsg::Ping(_)) => {
                        // 自动响应 Pong
                        tracing::debug!("Received ping");
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

        // 发送连接成功事件
        let _ = self.event_tx.send(WsClientEvent::Connected);

        // 等待任一任务结束
        tokio::select! {
            _ = sender_task => {}
            _ = receiver_task => {}
        }

        Ok(())
    }

    /// 处理接收到的文本消息
    async fn handle_text_message(&self, text: &str) -> Result<()> {
        match WsMessage::from_json(text) {
            Ok(ws_msg) => {
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
                            let data = base64::Engine::decode(
                                &base64::engine::general_purpose::STANDARD,
                                &payload.data,
                            ).unwrap_or_default();
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
                        // 确认消息，暂不处理
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

        // 停止运行
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);

        // 关闭 WebSocket 发送器
        *self.ws_sender.write().await = None;

        // 更新状态
        *self.status.write().await = ConnectionStatus::Disconnected;

        // 发送断开事件
        let _ = self.event_tx.send(WsClientEvent::Disconnected);

        tracing::info!("Disconnected");
    }

    /// 连接断开时的回调
    async fn on_disconnected(&self) {
        tracing::warn!("Connection lost");

        // 停止运行
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);

        // 关闭 WebSocket 发送器
        *self.ws_sender.write().await = None;

        // 更新状态
        *self.status.write().await = ConnectionStatus::Disconnected;

        // 发送断开事件
        let _ = self.event_tx.send(WsClientEvent::Disconnected);
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
}