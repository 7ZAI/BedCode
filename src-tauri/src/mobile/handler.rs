//! Mobile Message Handler
//!
//! 实现 ClientMessageHandler trait，处理来自服务器的各类消息

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::broadcast;
use tracing;
use crate::shared::websocket::{
    ClientMessageHandler, HandlerResult,
};
use crate::shared::model::message::Message;
use crate::shared::enums::{TerminalAction, TerminalPayload};
use crate::shared::enums::auth::AuthStage;

/// Mobile 业务事件（发送给前端）
#[derive(Debug, Clone)]
pub enum MobileEvent {
    /// 连接成功
    Connected,
    /// 断开连接
    Disconnected,
    /// 收到输出
    Output {
        session_id: String,
        data: String,
        is_waiting: bool,
        /// 全局递增索引，用于去重
        index: usize,
    },
    /// 订阅响应
    SubscribeResponse {
        session_id: String,
        current_max_seq: u64,
        history_count: usize,
    },
    /// 取消订阅响应
    UnsubscribeResponse {
        session_id: String,
    },
    /// 认证成功
    AuthSuccess {
        device_id: String,
        session_token: String,
    },
    /// 认证失败
    AuthFailed {
        reason: String,
    },
    /// 配对请求
    PairingRequest {
        device_name: String,
    },
    /// 配对码验证
    PairingVerified,
    /// 错误
    Error {
        message: String,
    },
    /// 服务器关闭
    ServerClosed {
        reason: String,
    },
    /// 确认响应（服务端默认响应）
    Ack {
        request_id: String,
    },
}

/// Mobile 消息处理器
pub struct MobileHandler {
    event_tx: broadcast::Sender<MobileEvent>,
}

impl MobileHandler {
    pub fn new() -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(1024);
        Arc::new(Self { event_tx })
    }

    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<MobileEvent> {
        self.event_tx.subscribe()
    }

    /// 发送事件
    fn send_event(&self, event: MobileEvent) {
        let _ = self.event_tx.send(event);
    }
}

impl ClientMessageHandler for MobileHandler {
    fn handle(
        &self,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = HandlerResult> + Send + '_>> {
        let self_clone = self.clone();
        Box::pin(async move {
            // 直接使用 Message 类型处理消息，避免不必要的序列化
            match message {
                Message::Terminal { session_id, payload, .. } => {
                    // 处理终端消息的各种动作
                    match payload.action {
                        TerminalAction::Output { data, is_waiting, index } => {
                            tracing::debug!("[MobileHandler] Output received: session_id={}, data_len={}, is_waiting={}, index={}", session_id, data.len(), is_waiting, index);
                            self_clone.send_event(MobileEvent::Output {
                                session_id: session_id.clone(),
                                data,
                                is_waiting,
                                index,
                            });
                        }
                        TerminalAction::SubscribeResponse { current_max_seq, history_count } => {
                            tracing::debug!("[MobileHandler] SubscribeResponse: session_id={}, current_max_seq={}, history_count={}",
                                session_id, current_max_seq, history_count);
                            self_clone.send_event(MobileEvent::SubscribeResponse {
                                session_id,
                                current_max_seq,
                                history_count,
                            });
                        }
                        TerminalAction::UnsubscribeResponse => {
                            tracing::debug!("[MobileHandler] UnsubscribeResponse: session_id={}", session_id);
                            self_clone.send_event(MobileEvent::UnsubscribeResponse {
                                session_id,
                            });
                        }
                        // 其他动作类型（Input, Subscribe, Unsubscribe）在移动端不处理
                        _ => {}
                    }
                }
                Message::Auth { payload, .. } => {
                    match payload.stage {
                        AuthStage::Authenticated => {
                            if let (Some(device_id), Some(session_token)) = (payload.device_id, payload.session_token) {
                                self_clone.send_event(MobileEvent::AuthSuccess {
                                    device_id,
                                    session_token,
                                });
                            }
                        }
                        AuthStage::VerifyCode => {
                            self_clone.send_event(MobileEvent::PairingVerified);
                        }
                        AuthStage::Failed => {
                            let reason = payload.error.unwrap_or_else(|| "Authentication failed".to_string());
                            self_clone.send_event(MobileEvent::AuthFailed { reason });
                        }
                        _ => {}
                    }
                }
                Message::ServerClosed { reason, .. } => {
                    self_clone.send_event(MobileEvent::ServerClosed { reason });
                }
                Message::Error { message, code, .. } => {
                    let msg = if !message.is_empty() { message } else { code };
                    self_clone.send_event(MobileEvent::Error { message: msg });
                }
                Message::Ack { request_id, .. } => {
                    tracing::debug!("[MobileHandler] Ack received for request_id={}", request_id);
                    self_clone.send_event(MobileEvent::Ack { request_id });
                }
                // 其他消息类型不处理
                _ => {}
            }

            Ok(None)
        })
    }

    fn name(&self) -> &str {
        "MobileHandler"
    }
}

impl Default for MobileHandler {
    fn default() -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Self { event_tx }
    }
}

impl Clone for MobileHandler {
    fn clone(&self) -> Self {
        Self {
            event_tx: self.event_tx.clone(),
        }
    }
}
