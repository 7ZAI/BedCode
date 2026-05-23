//! WebSocket 统一收发模块 (IoModule)
//!
//! 提供统一的消息发送、接收订阅、确认机制和重试功能

use std::sync::Arc;
use tokio::sync::broadcast;
use std::time::Duration;

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
pub trait MessageSender: Send + Sync {
    /// 发送消息
    async fn send(&self, msg: WsMessage) -> Result<()>;
}

/// 广播发送者 trait
pub trait BroadcastSender: Send + Sync {
    /// 广播消息
    async fn broadcast(&self, msg: WsMessage) -> Result<()>;
}