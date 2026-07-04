//! Request-Response Manager
//!
//! 管理 WebSocket 请求-响应模式，使用 Map<message_id, oneshot::Sender> 实现
//! 自己解码原始 WebSocket 消息，判断是否匹配 pending 请求

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

use crate::model::message::Message;
use crate::connection::codec::{JsonCodec, MessageCodec};
use crate::Result;

/// 等待中的请求
struct PendingRequest {
    /// oneshot 发送器，用于通知等待者
    tx: oneshot::Sender<Result<Message>>,
}

/// 请求-响应管理器
///
/// 使用 `Map<message_id, oneshot::Sender>` 实现精准投递：
/// - 发送请求时注册 pending 请求
/// - 收到响应时解码消息，根据 message_id 查找并通知等待者
pub struct RequestResponseManager {
    /// 等待中的请求，key = message_id
    pending: Mutex<HashMap<String, PendingRequest>>,
    /// JSON 编解码器
    codec: Arc<JsonCodec>,
}

impl RequestResponseManager {
    /// 创建新的请求-响应管理器
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::new(HashMap::new()),
            codec: Arc::new(JsonCodec::new()),
        })
    }

    /// 注册一个 pending 请求
    ///
    /// 返回 oneshot 接收器，用于等待响应
    pub async fn register(&self, message_id: String) -> oneshot::Receiver<Result<Message>> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(message_id, PendingRequest { tx });
        rx
    }

    /// 尝试匹配原始 WebSocket 消息
    ///
    /// 解码消息，根据 message_id 查找并通知等待者
    /// 返回 Some(Message) 表示未匹配（是推送消息），返回 None 表示已匹配处理
    pub async fn try_match(&self, raw_message: WsMsg) -> Option<Message> {
        // 只处理 Text 消息
        let text = match raw_message {
            WsMsg::Text(t) => t,
            _ => return None,
        };

        // 解码消息
        let message = match self.codec.decode(WsMsg::Text(text)) {
            Ok(Some(msg)) => msg,
            Ok(None) => return None,  // 协议消息
            Err(e) => {
                tracing::warn!("[RequestResponseManager] Failed to decode: {}", e);
                return None;
            }
        };

        // 尝试匹配 message_id 或 request_id（ACK 消息使用 request_id）
        let id = match &message {
            // ACK 消息使用 request_id 关联请求
            Message::Ack { request_id, .. } => Some(request_id.clone()),
            // 其他消息使用 message_id
            _ => message.message_id().map(|s| s.to_string()),
        };

        tracing::info!(
            "[RequestResponseManager] Trying to match message, type={}, id={:?}, pending_count={}",
            message.message_type().unwrap_or("unknown"),
            id,
            self.pending.lock().await.len()
        );

        if let Some(id) = id {
            let pending_count_before = self.pending.lock().await.len();
            if let Some(pending) = self.pending.lock().await.remove(&id) {
                tracing::info!("[RequestResponseManager] ✓ Matched pending request for id={}", id);
                let _ = pending.tx.send(Ok(message));
                return None;  // 已匹配，不返回消息
            } else {
                tracing::warn!(
                    "[RequestResponseManager] ✗ No pending request for id={}, pending_count={}",
                    id, pending_count_before
                );
            }
        } else {
            tracing::warn!("[RequestResponseManager] Message has no id, cannot match");
        }

        // 未匹配，返回消息给调用方处理
        Some(message)
    }

    /// 发送错误响应（连接断开、超时等场景）
    ///
    /// 通知所有等待中的请求失败
    pub async fn on_error(&self, error_msg: &str) {
        let mut pending = self.pending.lock().await;
        let count = pending.len();
        if count > 0 {
            tracing::warn!("[RequestResponseManager] Notifying {} pending requests of error", count);
            for (id, req) in pending.drain() {
                tracing::debug!("[RequestResponseManager] Sending error to pending request id={}", id);
                let _ = req.tx.send(Err(crate::AppError::WebSocket(error_msg.to_string())));
            }
        }
    }

    /// 清理超时的请求
    ///
    /// 由调用方在超时后调用
    pub async fn remove(&self, message_id: &str) {
        self.pending.lock().await.remove(message_id);
    }

    /// 获取当前等待中的请求数量
    pub async fn pending_count(&self) -> usize {
        self.pending.lock().await.len()
    }
}

impl Default for RequestResponseManager {
    fn default() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            codec: Arc::new(JsonCodec::new()),
        }
    }
}
