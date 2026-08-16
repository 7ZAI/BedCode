//! IO Module - WebSocket Read/Write Operations
//!
//! 职责：处理 WebSocket 消息的读取循环和写入通道
//! 与连接管理分离：只负责 IO，不处理连接建立

use std::sync::Arc;
use tokio::sync::broadcast;

use crate::system::constants::connection::BROADCAST_CHANNEL_CAPACITY;

/// 客户端事件（IO 模块输出）
#[derive(Debug, Clone)]
pub enum IoEvent {
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
    /// 连接关闭
    ConnectionClosed {
        reason: String,
    },
    /// IO 错误
    Error {
        message: String,
    },
}

/// IO 管理器 - 负责消息的收发
pub struct IoManager {
    /// 事件广播器（供外部订阅）
    event_tx: broadcast::Sender<IoEvent>,
    /// 运行标记
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl IoManager {
    /// 创建新的 IO 管理器
    pub fn new() -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        Arc::new(Self {
            event_tx,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// 获取事件接收器
    pub fn subscribe(&self) -> broadcast::Receiver<IoEvent> {
        self.event_tx.subscribe()
    }

    /// 发送事件
    fn emit(&self, event: IoEvent) {
        let _ = self.event_tx.send(event);
    }
}

impl Default for IoManager {
    fn default() -> Self {
        Self {
            event_tx: broadcast::channel(BROADCAST_CHANNEL_CAPACITY).0,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}