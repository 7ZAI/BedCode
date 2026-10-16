//! Request-Response Manager
//!
//! 管理 WebSocket 请求-响应模式，使用 Map<message_id, oneshot::Sender> 实现
//! 自己解码原始 WebSocket 消息，判断是否匹配 pending 请求

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

use crate::connection::codec::{JsonCodec, MessageCodec};
use crate::model::message::Message;
use crate::Result;

/// 等待中的请求
struct PendingRequest {
    /// oneshot 发送器，用于通知等待者
    tx: oneshot::Sender<Result<Message>>,
}

/// 一次入站文本帧经请求-响应匹配后的裁决
#[derive(Debug)]
pub enum MatchOutcome {
    /// 已匹配 pending 请求（响应已按 message_id/request_id 投递）：调用方勿再处理
    Matched,
    /// 未匹配的推送帧（可解析为移动端业务 `Message`）：调用方交给 handler 路由
    Push(Message),
    /// 无法解析为业务 `Message` 的帧（插件端点帧 / 畸形帧 / 非 JSON）：调用方仍应
    /// 交给 handler——handler 内部有 `PluginEventRouter` 侦查路由，或自行丢弃留痕
    Unroutable,
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
    /// 解码消息，根据 message_id 查找并通知等待者，返回三态裁决：
    /// - `Matched`：已匹配 pending 请求（响应已投递），调用方**勿**再处理
    /// - `Push`：可解析为业务 `Message` 的推送帧，调用方交给 handler
    /// - `Unroutable`：解析失败的帧（如插件端点事件帧 `{"type":"event",...}`——
    ///   这些帧不是移动端 `Message` 信封）。**必须交还调用方给 handler 侦察路由**（handler
    ///   内有 `PluginEventRouter` 或自行丢弃）：在匹配层吞掉 = 事件帧永远到不了
    ///   handler（票 03 集成必红），畸形帧也无从留痕。
    pub async fn try_match(&self, raw_message: WsMsg) -> MatchOutcome {
        // 只处理 Text 消息
        let text = match raw_message {
            WsMsg::Text(t) => t,
            _ => return MatchOutcome::Unroutable,
        };

        // 解码消息
        let message = match self.codec.decode(WsMsg::Text(text)) {
            Ok(Some(msg)) => msg,
            // 解析失败（含插件事件帧）：交还调用方让 handler 侦察（debug 留痕，不刷屏）
            Ok(None) => return MatchOutcome::Unroutable,
            Err(e) => {
                tracing::debug!("[RequestResponseManager] Undecodable frame handed to handler: {}", e);
                return MatchOutcome::Unroutable;
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
                return MatchOutcome::Matched; // 已匹配，响应已投递
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
        MatchOutcome::Push(message)
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
    use crate::enums::SessionControlAction;

    /// 构造指定 message_id 的会话控制消息（测试辅助）
    /// with_request_id 会把 message_id 回填为给定值，模拟服务端响应携带请求 ID
    fn control_with_id(id: &str) -> Message {
        Message::session_control_with_response(SessionControlAction::ListSessions, None).with_request_id(id)
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
        let matched = mgr.try_match(text_frame(&control_with_id("m-1"))).await;
        assert!(matches!(matched, MatchOutcome::Matched), "匹配成功时应裁决为 Matched");
        assert_eq!(mgr.pending_count().await, 0);
        let msg = rx.await.unwrap().unwrap();
        assert_eq!(msg.message_id(), Some("m-1"));
        assert_eq!(msg.message_type(), Some("session_control"));
    }

    #[tokio::test]
    async fn test_try_match_delivers_ack_by_request_id() {
        // ACK 消息没有 message_id，应通过 request_id 关联 pending 请求
        let mgr = RequestResponseManager::new();
        let rx = mgr.register("req-9".to_string()).await;
        let ack = Message::ack("req-9");
        let matched = mgr.try_match(text_frame(&ack)).await;
        assert!(matches!(matched, MatchOutcome::Matched));
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
        let result = mgr.try_match(text_frame(&control_with_id("ghost"))).await;
        match result {
            MatchOutcome::Push(m) => assert_eq!(m.message_id().map(|s| s.to_string()), Some("ghost".to_string())),
            other => panic!("expected Push(ghost), got {:?}", other),
        }
        assert_eq!(mgr.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_try_match_ack_unknown_request_id_returns_as_push() {
        // 服务端发来无人等待的 ACK（如超时后才到达的响应）：不吞掉，返回给调用方
        let mgr = RequestResponseManager::new();
        let result = mgr.try_match(text_frame(&Message::ack("stale-1"))).await;
        match result {
            MatchOutcome::Push(m) => assert_eq!(m.message_type(), Some("ack")),
            other => panic!("expected Push(ack), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_try_match_consumes_pending_only_once() {
        // 重复响应：第一次命中并移除，第二次因 pending 已空转为推送返回
        let mgr = RequestResponseManager::new();
        let _rx = mgr.register("m-1".to_string()).await;
        let frame = text_frame(&control_with_id("m-1"));
        assert!(matches!(mgr.try_match(frame.clone()).await, MatchOutcome::Matched));
        let second = mgr.try_match(frame).await;
        match second {
            MatchOutcome::Push(m) => assert_eq!(m.message_id().map(|s| s.to_string()), Some("m-1".to_string())),
            other => panic!("expected Push(m-1), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_remove_cleans_timed_out_request() {
        // 超时清理路径：remove 后 pending 为空，迟到的响应只能作为推送返回
        let mgr = RequestResponseManager::new();
        let _rx = mgr.register("m-1".to_string()).await;
        mgr.remove("m-1").await;
        assert_eq!(mgr.pending_count().await, 0);
        let result = mgr.try_match(text_frame(&control_with_id("m-1"))).await;
        assert!(matches!(result, MatchOutcome::Push(_)));
    }

    /// 票 03 关键：插件端点事件帧（`{"type":"event",...}`）不是移动端 `Message`
    /// 信封，**必须裁决为 `Unroutable` 交还调用方给 handler 侦察路由**——若在此层
    /// 吞掉，事件帧永远到不了 PluginEventRouter（集成必红）。
    #[tokio::test]
    async fn test_try_match_event_frame_is_unroutable_not_swallowed() {
        let mgr = RequestResponseManager::new();
        let frame =
            WsMsg::Text(r#"{"type":"event","event":"session:created","payload":{"session":{"id":"s1"}}}"#.to_string());
        match mgr.try_match(frame).await {
            MatchOutcome::Unroutable => {}
            other => panic!("事件帧应裁决为 Unroutable（交 handler），实际 {:?}", other),
        }
        // 未注册该帧对应的 pending：pending 不应被污染
        assert_eq!(mgr.pending_count().await, 0);
    }

    /// 畸形帧（非 JSON）同样 Unroutable：不 panic、不留 pending、交调用方留痕。
    #[tokio::test]
    async fn test_try_match_garbage_frame_is_unroutable() {
        let mgr = RequestResponseManager::new();
        let frame = WsMsg::Text("not json at all".to_string());
        assert!(matches!(mgr.try_match(frame).await, MatchOutcome::Unroutable));
        assert_eq!(mgr.pending_count().await, 0);
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
        // 协议控制帧（Ping/Pong）不参与匹配，pending 必须保持原样（非 Text 帧裁决
        // 为 Unroutable；实际生产路径 receiver 不把 Ping/Pong 送进 try_match）
        let mgr = RequestResponseManager::new();
        let _rx = mgr.register("m-1".to_string()).await;
        assert!(matches!(
            mgr.try_match(WsMsg::Ping(vec![].into())).await,
            MatchOutcome::Unroutable
        ));
        assert!(matches!(
            mgr.try_match(WsMsg::Pong(vec![].into())).await,
            MatchOutcome::Unroutable
        ));
        assert_eq!(mgr.pending_count().await, 1);
    }

    #[tokio::test]
    async fn test_try_match_invalid_json_keeps_pending() {
        // 非法 JSON：解码失败裁决为 Unroutable（交还调用方留痕丢弃——票 03 关键：
        // 插件事件帧也走此路径，吞掉即事件到不了 handler），pending 保持原样
        let mgr = RequestResponseManager::new();
        let _rx = mgr.register("m-1".to_string()).await;
        let result = mgr.try_match(WsMsg::Text("{not valid json".into())).await;
        assert!(matches!(result, MatchOutcome::Unroutable));
        assert_eq!(mgr.pending_count().await, 1);
    }

    #[tokio::test]
    async fn test_try_match_binary_frame_is_skipped() {
        // try_match 入口只接受 Text 帧（文档明确“只处理 Text 消息”），
        // Binary 帧直接被跳过、不进入解码也不消耗 pending；
        // Binary 解码能力由 codec::decode 独立提供（见 codec 测试）
        let mgr = RequestResponseManager::new();
        let _rx = mgr.register("m-1".to_string()).await;
        let json = control_with_id("m-1").to_json().unwrap();
        let matched = mgr.try_match(WsMsg::Binary(json.into_bytes().into())).await;
        assert!(matches!(matched, MatchOutcome::Unroutable));
        assert_eq!(mgr.pending_count().await, 1);
    }
}
