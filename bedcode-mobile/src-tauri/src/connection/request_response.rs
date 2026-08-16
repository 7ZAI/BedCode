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

        tracing::debug!(
            "[RequestResponseManager] Trying to match message, type={}, id={:?}, pending_count={}",
            message.message_type().unwrap_or("unknown"),
            id,
            self.pending.lock().await.len()
        );

        if let Some(id) = id {
            if let Some(pending) = self.pending.lock().await.remove(&id) {
                tracing::debug!("[RequestResponseManager] ✓ Matched pending request for id={}", id);
                let _ = pending.tx.send(Ok(message));
                return None;  // 已匹配，不返回消息
            }
            // 调试终端组件订阅偏移量时使用：推送消息（含终端输出广播）每帧都会命中
            // 此分支（带 message_id 但无 pending 请求），逐帧 WARN 刷屏，已注释；
            // 排查订阅/匹配问题时恢复即可
            // let pending_count_before = self.pending.lock().await.len();
            // tracing::warn!(
            //     "[RequestResponseManager] ✗ No pending request for id={}, pending_count={}",
            //     id, pending_count_before
            // );
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

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造指定 message_id 的终端输入消息（测试辅助）
    /// with_request_id 会把 message_id 回填为给定值，模拟服务端响应携带请求 ID
    fn terminal_with_id(id: &str) -> Message {
        Message::input_with_response("sess-1", "ls -la", None).with_request_id(id)
    }

    /// 将消息编码为 Text 帧（测试辅助）
    fn text_frame(msg: &Message) -> WsMsg {
        WsMsg::Text(msg.to_json().unwrap())
    }

    #[tokio::test]
    async fn test_register_increments_pending_count() {
        // 注册一个请求后 pending 应随之增长
        let mgr = RequestResponseManager::new();
        assert_eq!(mgr.pending_count().await, 0);
        let _rx = mgr.register("m-1".to_string()).await;
        assert_eq!(mgr.pending_count().await, 1);
        let _rx2 = mgr.register("m-2".to_string()).await;
        assert_eq!(mgr.pending_count().await, 2);
    }

    #[tokio::test]
    async fn test_try_match_delivers_response_by_message_id() {
        // 普通响应：message_id 命中 pending，等待者收到消息且 pending 被移除
        let mgr = RequestResponseManager::new();
        let rx = mgr.register("m-1".to_string()).await;
        let matched = mgr.try_match(text_frame(&terminal_with_id("m-1"))).await;
        assert!(matched.is_none(), "匹配成功时不应作为推送消息返回");
        assert_eq!(mgr.pending_count().await, 0);
        let msg = rx.await.unwrap().unwrap();
        assert_eq!(msg.message_id(), Some("m-1"));
        assert_eq!(msg.message_type(), Some("terminal"));
    }

    #[tokio::test]
    async fn test_try_match_delivers_ack_by_request_id() {
        // ACK 消息没有 message_id，应通过 request_id 关联 pending 请求
        let mgr = RequestResponseManager::new();
        let rx = mgr.register("req-9".to_string()).await;
        let ack = Message::ack("req-9");
        let matched = mgr.try_match(text_frame(&ack)).await;
        assert!(matched.is_none());
        let msg = rx.await.unwrap().unwrap();
        assert_eq!(msg.message_type(), Some("ack"));
        match msg {
            Message::Ack { request_id, code, .. } => {
                assert_eq!(request_id, "req-9");
                assert_eq!(code, crate::model::message::ACK_CODE_SUCCESS);
            }
            _ => panic!("expected ack message"),
        }
    }

    #[tokio::test]
    async fn test_try_match_unknown_id_returns_as_push() {
        // 未注册的 message_id：无法匹配，消息应原样返回给调用方（推送消息语义）
        let mgr = RequestResponseManager::new();
        let result = mgr.try_match(text_frame(&terminal_with_id("ghost"))).await;
        assert_eq!(result.map(|m| m.message_id().map(|s| s.to_string())), Some(Some("ghost".to_string())));
        assert_eq!(mgr.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_try_match_ack_unknown_request_id_returns_as_push() {
        // 服务端发来无人等待的 ACK（如超时后才到达的响应）：不吞掉，返回给调用方
        let mgr = RequestResponseManager::new();
        let result = mgr.try_match(text_frame(&Message::ack("stale-1"))).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().message_type(), Some("ack"));
    }

    #[tokio::test]
    async fn test_try_match_consumes_pending_only_once() {
        // 重复响应：第一次命中并移除，第二次因 pending 已空转为推送返回
        let mgr = RequestResponseManager::new();
        let _rx = mgr.register("m-1".to_string()).await;
        let frame = text_frame(&terminal_with_id("m-1"));
        assert!(mgr.try_match(frame.clone()).await.is_none());
        let second = mgr.try_match(frame).await;
        assert_eq!(second.map(|m| m.message_id().map(|s| s.to_string())), Some(Some("m-1".to_string())));
    }

    #[tokio::test]
    async fn test_remove_cleans_timed_out_request() {
        // 超时清理路径：remove 后 pending 为空，迟到的响应只能作为推送返回
        let mgr = RequestResponseManager::new();
        let _rx = mgr.register("m-1".to_string()).await;
        mgr.remove("m-1").await;
        assert_eq!(mgr.pending_count().await, 0);
        let result = mgr.try_match(text_frame(&terminal_with_id("m-1"))).await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_on_error_notifies_all_pending() {
        // 连接断开：所有等待者收到 WebSocket 错误，pending 清空
        let mgr = RequestResponseManager::new();
        let rx1 = mgr.register("m-1".to_string()).await;
        let rx2 = mgr.register("m-2".to_string()).await;
        mgr.on_error("connection lost").await;
        assert_eq!(mgr.pending_count().await, 0);
        for rx in [rx1, rx2] {
            let err = rx.await.unwrap().unwrap_err();
            match err {
                crate::AppError::WebSocket(msg) => assert_eq!(msg, "connection lost"),
                other => panic!("expected WebSocket error, got {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn test_on_error_without_pending_is_noop() {
        // 无 pending 时 on_error 不应 panic 也不应产生任何效果
        let mgr = RequestResponseManager::new();
        mgr.on_error("boom").await;
        assert_eq!(mgr.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_try_match_ping_keeps_pending_intact() {
        // 协议控制帧（Ping/Pong）不参与匹配，pending 必须保持原样
        let mgr = RequestResponseManager::new();
        let _rx = mgr.register("m-1".to_string()).await;
        assert!(mgr.try_match(WsMsg::Ping(vec![].into())).await.is_none());
        assert!(mgr.try_match(WsMsg::Pong(vec![].into())).await.is_none());
        assert_eq!(mgr.pending_count().await, 1);
    }

    #[tokio::test]
    async fn test_try_match_invalid_json_keeps_pending() {
        // 非法 JSON：解码失败应记日志并跳过，不消耗 pending（等待者可继续等后续响应）
        let mgr = RequestResponseManager::new();
        let _rx = mgr.register("m-1".to_string()).await;
        let result = mgr.try_match(WsMsg::Text("{not valid json".into())).await;
        assert!(result.is_none());
        assert_eq!(mgr.pending_count().await, 1);
    }

    #[tokio::test]
    async fn test_try_match_binary_frame_is_skipped() {
        // try_match 入口只接受 Text 帧（文档明确“只处理 Text 消息”），
        // Binary 帧直接被跳过、不进入解码也不消耗 pending；
        // Binary 解码能力由 codec::decode 独立提供（见 codec 测试）
        let mgr = RequestResponseManager::new();
        let _rx = mgr.register("m-1".to_string()).await;
        let json = terminal_with_id("m-1").to_json().unwrap();
        let matched = mgr.try_match(WsMsg::Binary(json.into_bytes().into())).await;
        assert!(matched.is_none());
        assert_eq!(mgr.pending_count().await, 1);
    }
}
