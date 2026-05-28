//! WebSocket Server IO Module
//!
//! 为 WsServer 提供统一的消息收发功能
//! 所有发送功能都通过此模块，不再分散在 ConnectionManager 中

use std::sync::Arc;
use tokio::sync::broadcast;
use std::time::Duration;
use std::net::SocketAddr;

use crate::Result;
use crate::shared::model::message::Message;
use crate::shared::websocket::server::connection_manager::ConnectionManager;
use tracing::{debug, info, warn};

/// Server IO 模块配置
#[derive(Debug, Clone)]
pub struct ServerIoConfig {
    /// 消息队列大小
    pub queue_size: usize,
    /// 默认超时时间（毫秒）
    pub default_timeout_ms: u64,
    /// 最大重试次数
    pub max_retries: u32,
    /// 重试间隔（毫秒）
    pub retry_interval_ms: u64,
}

impl Default for ServerIoConfig {
    fn default() -> Self {
        Self {
            queue_size: 256,
            default_timeout_ms: 5000,
            max_retries: 3,
            retry_interval_ms: 1000,
        }
    }
}

impl ServerIoConfig {
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
pub enum ServerIoEvent {
    /// 文本消息
    Text {
        /// 客户端地址
        addr: SocketAddr,
        /// 消息ID（如果有）
        message_id: Option<String>,
        /// 消息内容
        content: String,
    },
    /// 二进制消息
    Binary {
        /// 客户端地址
        addr: SocketAddr,
        /// 消息ID（如果有）
        message_id: Option<String>,
        /// 二进制数据
        data: Vec<u8>,
    },
    /// 消息发送成功
    SendSuccess {
        /// 客户端地址
        addr: SocketAddr,
    },
    /// 消息发送失败
    SendFailure {
        /// 客户端地址
        addr: SocketAddr,
        /// 错误信息
        error: String,
    },
    /// 连接关闭
    Close {
        /// 客户端地址
        addr: SocketAddr,
        /// 关闭原因
        reason: String,
    },
}

/// WebSocket Server IO 模块
///
/// 统一处理所有发送逻辑：
/// - 发送给单个客户端
/// - 广播给所有客户端
/// - 发送给除指定客户端外的其他客户端
/// - 按标签组广播
/// - 发送并等待确认
/// - 发送并自动重试
pub struct ServerIo {
    /// 配置
    config: ServerIoConfig,
    /// 连接管理器（用于获取发送通道）
    connection_manager: Arc<ConnectionManager>,
    /// 事件发送器
    event_tx: broadcast::Sender<ServerIoEvent>,
}

impl ServerIo {
    /// 创建新的 IO 模块
    pub fn new(config: ServerIoConfig, connection_manager: Arc<ConnectionManager>) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(config.queue_size);

        Arc::new(Self {
            config,
            connection_manager,
            event_tx,
        })
    }

    /// 订阅消息事件
    pub fn subscribe(&self) -> broadcast::Receiver<ServerIoEvent> {
        self.event_tx.subscribe()
    }

    /// 获取配置
    pub fn config(&self) -> &ServerIoConfig {
        &self.config
    }

    /// 发送消息到指定地址
    pub async fn send_to(&self, addr: &SocketAddr, msg: &Message) -> Result<()> {
        let json = msg.to_json()?;
        let ws_msg = tokio_tungstenite::tungstenite::Message::Text(json);

        if let Some(sender) = self.connection_manager.get_sender_by_addr(addr).await {
            match sender.send(ws_msg).await {
                Ok(()) => {
                    debug!("[ServerIo] Send success to {}", addr);
                    let _ = self.event_tx.send(ServerIoEvent::SendSuccess {
                        addr: *addr,
                    });
                    Ok(())
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    warn!("[ServerIo] Send failed to {}: {}", addr, error_msg);
                    let _ = self.event_tx.send(ServerIoEvent::SendFailure {
                        addr: *addr,
                        error: error_msg,
                    });
                    Err(crate::AppError::WebSocket(format!("Failed to send: {}", e)))
                }
            }
        } else {
            Err(crate::AppError::WebSocket(format!("Client {} not found", addr)))
        }
    }

    /// 广播消息给所有客户端
    pub async fn broadcast(&self, msg: &Message) -> Result<()> {
        let json = msg.to_json()?;
        let json_for_log = json.clone();

        info!("[ServerIo] >>> BROADCAST: {}...", &json_for_log[..json_for_log.len().min(200)]);

        let ws_msg = tokio_tungstenite::tungstenite::Message::Text(json);
        let senders = self.connection_manager.get_all_senders().await;
        for sender in senders {
            let _ = sender.try_send(ws_msg.clone());
        }
        Ok(())
    }

    /// 发送给除指定客户端外的所有客户端
    pub async fn broadcast_to_others(&self, exclude_addr: &SocketAddr, msg: &Message) -> Result<()> {
        let json = msg.to_json()?;

        // 获取排除的连接 ID
        let exclude_id = self.connection_manager.get_id_by_addr(exclude_addr).await;

        let senders = if let Some(id) = exclude_id {
            self.connection_manager.get_other_senders(id).await
        } else {
            self.connection_manager.get_all_senders().await
        };

        let ws_msg = tokio_tungstenite::tungstenite::Message::Text(json);
        for sender in senders {
            let _ = sender.try_send(ws_msg.clone());
        }
        Ok(())
    }

    /// 按标签组广播
    pub async fn broadcast_to_tag(&self, tag: &str, msg: &Message) -> Result<()> {
        let json = msg.to_json()?;
        let json_for_log = json.clone();

        info!("[ServerIo] >>> BROADCAST to tag '{}': {}...", tag, &json_for_log[..json_for_log.len().min(200)]);

        let ws_msg = tokio_tungstenite::tungstenite::Message::Text(json);
        let senders = self.connection_manager.get_senders_by_tag(tag).await;
        for sender in senders {
            let _ = sender.try_send(ws_msg.clone());
        }
        Ok(())
    }

    /// 发送给多个指定客户端
    pub async fn broadcast_to_ids(&self, ids: &[crate::shared::websocket::server::connection_manager::ConnectionId], msg: &Message) -> Result<()> {
        let json = msg.to_json()?;
        let ws_msg = tokio_tungstenite::tungstenite::Message::Text(json);

        let senders = self.connection_manager.get_senders(ids).await;
        for sender in senders {
            let _ = sender.try_send(ws_msg.clone());
        }
        Ok(())
    }

    /// 发送并等待确认（用于请求-响应模式）
    pub async fn send_with_ack(
        &self,
        addr: &SocketAddr,
        msg: &Message,
        timeout: Duration,
    ) -> Result<Message> {
        let message_id = msg.message_id().map(|s| s.to_string());

        let sent_id = match message_id {
            Some(id) => id,
            None => {
                return Err(crate::AppError::WebSocket(
                    "Message has no message_id, cannot wait for response".to_string(),
                ))
            }
        };

        // 先发送消息
        self.send_to(addr, msg).await?;

        let mut receiver = self.event_tx.subscribe();

        // 等待确认响应
        let timeout_duration = if timeout.as_millis() == 0 {
            Duration::from_millis(self.config.default_timeout_ms)
        } else {
            timeout
        };

        let result = tokio::time::timeout(timeout_duration, async {
            loop {
                match receiver.recv().await {
                    Ok(ServerIoEvent::Text { addr: resp_addr, message_id: resp_id, content }) => {
                        if resp_addr == *addr {
                            if let Some(ref resp_id) = resp_id {
                                if *resp_id == sent_id {
                                    // 尝试解析为业务消息
                                    match Message::from_json(&content) {
                                        Ok(msg) => return Ok(msg),
                                        Err(_) => {
                                            // 解析失败，返回错误消息
                                            return Ok(Message::error("PARSE_ERROR", "Failed to parse response"));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(ServerIoEvent::Binary { addr: resp_addr, message_id: resp_id, data: _ }) => {
                        if resp_addr == *addr {
                            if let Some(ref resp_id) = resp_id {
                                if *resp_id == sent_id {
                                    return Ok(Message::error("BINARY_ERROR", "Binary response not supported"));
                                }
                            }
                        }
                    }
                    Ok(ServerIoEvent::SendSuccess { addr: success_addr }) => {
                        if success_addr == *addr {
                            // 发送成功但没有匹配的业务响应，继续等待
                            continue;
                        }
                    }
                    Ok(ServerIoEvent::SendFailure { addr: fail_addr, error }) => {
                        if fail_addr == *addr {
                            return Err(crate::AppError::WebSocket(error));
                        }
                    }
                    Ok(ServerIoEvent::Close { addr: close_addr, reason }) => {
                        if close_addr == *addr {
                            return Err(crate::AppError::WebSocket(format!("Connection closed: {}", reason)));
                        }
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
    pub async fn send_with_retry(&self, addr: &SocketAddr, msg: &Message) -> Result<()> {
        let max_retries = self.config.max_retries;
        let retry_interval = Duration::from_millis(self.config.retry_interval_ms);

        let mut last_error = None;

        for attempt in 0..max_retries {
            match self.send_to(addr, msg).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < max_retries - 1 {
                        warn!(
                            "[ServerIo] Send failed to {} (attempt {}/{}), retrying in {:?}",
                            addr,
                            attempt + 1,
                            max_retries,
                            retry_interval
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

    /// 发出接收到的文本消息事件
    pub fn emit_text(&self, addr: SocketAddr, message_id: Option<String>, content: String) {
        let _ = self.event_tx.send(ServerIoEvent::Text {
            addr,
            message_id,
            content,
        });
    }

    /// 发出接收到的二进制消息事件
    pub fn emit_binary(&self, addr: SocketAddr, message_id: Option<String>, data: Vec<u8>) {
        let _ = self.event_tx.send(ServerIoEvent::Binary {
            addr,
            message_id,
            data,
        });
    }

    /// 发出连接关闭事件
    pub fn emit_close(&self, addr: SocketAddr, reason: String) {
        let _ = self.event_tx.send(ServerIoEvent::Close {
            addr,
            reason,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn test_server_io_config_default() {
        let config = ServerIoConfig::default();
        assert_eq!(config.queue_size, 256);
        assert_eq!(config.default_timeout_ms, 5000);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_interval_ms, 1000);
    }

    #[tokio::test]
    async fn test_server_io_config_custom() {
        let config = ServerIoConfig::new(512, 10000, 5, 2000);
        assert_eq!(config.queue_size, 512);
        assert_eq!(config.default_timeout_ms, 10000);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.retry_interval_ms, 2000);
    }

    #[tokio::test]
    async fn test_server_io_new() {
        let io = ServerIo::new(ServerIoConfig::default());
        let _receiver = io.subscribe();
        let _config = io.config();
    }

    #[tokio::test]
    async fn test_emit_text_event() {
        let io = ServerIo::new(ServerIoConfig::default());
        let mut receiver = io.subscribe();

        io.emit_text("127.0.0.1:8080".parse().unwrap(), Some("msg1".to_string()), "hello".to_string());

        if let Ok(event) = receiver.recv().await {
            match event {
                ServerIoEvent::Text { addr, message_id, content } => {
                    assert_eq!(addr, "127.0.0.1:8080".parse().unwrap());
                    assert_eq!(message_id, Some("msg1".to_string()));
                    assert_eq!(content, "hello");
                }
                _ => panic!("Expected Text event"),
            }
        } else {
            panic!("Failed to receive event");
        }
    }
}