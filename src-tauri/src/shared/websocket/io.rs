//! WebSocket 统一收发模块 (IoModule)
//!
//! 提供统一的消息发送、接收订阅、确认机制和重试功能

use std::sync::Arc;
use tokio::sync::broadcast;
use std::time::Duration;

use async_trait::async_trait;
use crate::Result;
use crate::shared::websocket::message::WsMessage;

/// IO 模块配置
#[derive(Debug, Clone)]
pub struct IoConfig {
    /// 消息队列大小
    pub queue_size: usize,
    /// 默认超时时间（毫秒）
    pub default_timeout_ms: u64,
    /// 最大重试次数
    pub max_retries: u32,
    /// 重试间隔（毫秒）
    pub retry_interval_ms: u64,
}

impl Default for IoConfig {
    fn default() -> Self {
        Self {
            queue_size: 256,
            default_timeout_ms: 5000,
            max_retries: 3,
            retry_interval_ms: 1000,
        }
    }
}

impl IoConfig {
    pub fn new(
        queue_size: usize,
        default_timeout_ms: u64,
        max_retries: u32,
        retry_interval_ms: u64,
    ) -> Self {
        Self {
            queue_size,
            default_timeout_ms,
            max_retries,
            retry_interval_ms,
        }
    }
}

/// IO 事件类型
#[derive(Debug, Clone)]
pub enum IoEvent {
    /// 文本消息
    Text {
        /// 消息ID（如果有）
        message_id: Option<String>,
        /// 消息内容
        content: String,
    },
    /// 二进制消息
    Binary {
        /// 消息ID（如果有）
        message_id: Option<String>,
        /// 二进制数据
        data: Vec<u8>,
    },
    /// 消息确认
    Ack {
        /// 对应的请求消息ID
        original_id: String,
    },
    /// 连接关闭
    Close {
        /// 关闭原因
        reason: String,
    },
    /// 错误
    Error {
        /// 错误信息
        message: String,
    },
}

/// 消息发送者 trait
#[async_trait]
pub trait MessageSender: Send + Sync {
    /// 发送消息
    async fn send(&self, msg: WsMessage) -> Result<()>;
}

/// 广播发送者 trait
#[async_trait]
pub trait BroadcastSender: Send + Sync {
    /// 广播消息
    async fn broadcast(&self, msg: WsMessage) -> Result<()>;
}

use tracing::{debug, info, warn, error};

/// WebSocket 统一收发模块
pub struct WebSocketIo {
    /// 配置
    config: IoConfig,
    /// 事件发送器
    event_tx: broadcast::Sender<IoEvent>,
}

impl WebSocketIo {
    /// 创建新的 IO 模块
    pub fn new(config: IoConfig) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(config.queue_size);

        Arc::new(Self {
            config,
            event_tx,
        })
    }

    /// 订阅消息事件
    pub fn subscribe(&self) -> broadcast::Receiver<IoEvent> {
        self.event_tx.subscribe()
    }

    /// 获取配置
    pub fn config(&self) -> &IoConfig {
        &self.config
    }

    /// 发送消息
    pub async fn send(
        &self,
        sender: &dyn MessageSender,
        msg: &WsMessage,
    ) -> Result<()> {
        let json = msg.to_json()?;
        debug!("[WebSocketIo] >>> SEND: {}...", &json[..json.len().min(200)]);

        sender.send(msg.clone()).await
    }

    /// 广播消息
    pub async fn broadcast(
        &self,
        sender: &dyn BroadcastSender,
        msg: &WsMessage,
    ) -> Result<()> {
        let json = msg.to_json()?;
        info!("[WebSocketIo] >>> BROADCAST: {}...", &json[..json.len().min(200)]);

        sender.broadcast(msg.clone()).await
    }

    /// 发送并等待确认
    pub async fn send_with_ack(
        &self,
        sender: &dyn MessageSender,
        msg: &WsMessage,
        timeout: Duration,
    ) -> Result<WsMessage> {
        let message_id = msg.message_id().map(|s| s.to_string());

        let sent_id = match message_id {
            Some(id) => id,
            None => {
                return Err(crate::AppError::WebSocket(
                    "Message has no message_id, cannot wait for response".to_string(),
                ))
            }
        };

        let mut receiver = self.event_tx.subscribe();

        // 发送消息
        self.send(sender, msg).await?;

        // 等待确认响应
        let timeout_duration = if timeout.as_millis() == 0 {
            Duration::from_millis(self.config.default_timeout_ms)
        } else {
            timeout
        };

        let result = tokio::time::timeout(timeout_duration, async {
            loop {
                match receiver.recv().await {
                    Ok(IoEvent::Text { message_id: resp_id, content }) => {
                        if let Some(ref resp_id) = resp_id {
                            if *resp_id == sent_id {
                                return Ok(WsMessage::text(content));
                            }
                        }
                    }
                    Ok(IoEvent::Binary { message_id: resp_id, data }) => {
                        if let Some(ref resp_id) = resp_id {
                            if *resp_id == sent_id {
                                return Ok(WsMessage::binary(data));
                            }
                        }
                    }
                    Ok(IoEvent::Ack { original_id }) => {
                        if original_id == sent_id {
                            return Ok(WsMessage::ack(original_id));
                        }
                    }
                    Ok(IoEvent::Error { message }) => {
                        return Err(crate::AppError::WebSocket(message));
                    }
                    Ok(IoEvent::Close { reason }) => {
                        return Err(crate::AppError::WebSocket(format!("Connection closed: {}", reason)));
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(crate::AppError::WebSocket("Event channel closed".to_string()));
                    }
                }
            }
        });

        match result.await {
            Ok(Ok(msg)) => Ok(msg),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(crate::AppError::WebSocket("Response timeout".to_string())),
        }
    }

    /// 发送并自动重试
    pub async fn send_with_retry(
        &self,
        sender: &dyn MessageSender,
        msg: &WsMessage,
    ) -> Result<()> {
        let max_retries = self.config.max_retries;
        let retry_interval = Duration::from_millis(self.config.retry_interval_ms);

        let mut last_error = None;

        for attempt in 0..max_retries {
            match self.send(sender, msg).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < max_retries - 1 {
                        warn!(
                            "[WebSocketIo] Send failed (attempt {}/{}), retrying in {:?}: {}",
                            attempt + 1,
                            max_retries,
                            retry_interval,
                            last_error.as_ref().unwrap()
                        );
                        tokio::time::sleep(retry_interval).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            crate::AppError::WebSocket("Send failed with unknown error".to_string())
        }))
    }
}