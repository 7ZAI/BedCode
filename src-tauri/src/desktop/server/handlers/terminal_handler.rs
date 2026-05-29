//! Terminal Handler - 终端消息处理器
//!
//! 处理 `Message::Terminal` 消息，包括输入、订阅、取消订阅等操作
//! 委托给具体的服务层处理

use crate::desktop::server::message::{Message as BusinessMessage, TerminalAction, TerminalPayload};
use crate::desktop::server::router::handler::RouteHandler;
use crate::desktop::server::services::terminal_service::handle_input;
use crate::desktop::session::{GlobalOutputManager, OutputEvent, SessionManager};
use crate::shared::enums::{TerminalAction as SharedTerminalAction, TerminalPayload as SharedTerminalPayload};
use crate::shared::model::message::Message;
use crate::shared::websocket::server::context::RouteContext;
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

/// 终端消息处理器
///
/// 处理所有终端相关操作：
/// - Input: 输入数据到 PTY
/// - Subscribe: 订阅会话输出
/// - Unsubscribe: 取消订阅
pub struct TerminalHandler {
    session_manager: Option<Arc<SessionManager>>,
}

impl TerminalHandler {
    pub fn new(session_manager: Option<Arc<SessionManager>>) -> Self {
        Self { session_manager }
    }
}

#[async_trait]
impl RouteHandler for TerminalHandler {
    async fn handle(
        &self,
        message: BusinessMessage,
        ctx: &RouteContext,
    ) -> Result<Option<BusinessMessage>> {
        // 解析 Terminal 消息
        let (session_id, action, _message_id) = match message {
            BusinessMessage::Terminal {
                message_id,
                session_id,
                payload,
                ..
            } => (session_id, payload.action, message_id),
            _ => return Ok(None),
        };

        // 根据动作类型分发处理
        match action {
            TerminalAction::Input { data, special_key } => {
                // 委托给 input_service 处理
                let payload = TerminalPayload {
                    action: TerminalAction::Input { data, special_key },
                };
                handle_input(&session_id, payload, &self.session_manager).await
            }
            TerminalAction::Subscribe { start_seq: _ } => {
                let global_manager = GlobalOutputManager::global();

                // 创建 OutputEvent 到 WebSocket 的转发通道
                let (output_tx, mut output_rx) = mpsc::channel::<OutputEvent>(256);

                // 获取 WebSocket 发送通道
                let ws_sender = ctx
                    .connection_manager
                    .get_sender(ctx.connection_id)
                    .await;

                match ws_sender {
                    Some(sender) => {
                        let session_id_clone = session_id.clone();
                        let client_id = ctx.client_id.clone();

                        // 启动转发任务：将 OutputEvent 转换为 Message 并发送到 WebSocket
                        tokio::spawn(async move {
                            while let Some(event) = output_rx.recv().await {
                                let message = Message::output(
                                    &session_id_clone,
                                    event.data.as_bytes(),
                                    event.is_waiting,
                                    event.index as usize,
                                );
                                if let Ok(json) = message.to_json() {
                                    if sender.send(tokio_tungstenite::tungstenite::Message::Text(json))
                                        .await
                                        .is_err()
                                    {
                                        tracing::warn!(
                                            "[TerminalHandler] Failed to send output to client {}",
                                            client_id
                                        );
                                        break;
                                    }
                                }
                            }
                            tracing::debug!(
                                "[TerminalHandler] Output forwarder stopped for client {}",
                                client_id
                            );
                        });

                        // 订阅
                        match global_manager
                            .subscribe(&session_id, &ctx.client_id, output_tx)
                            .await
                        {
                            Some(response) => {
                                info!(
                                    "Client {} subscribed to session {}, min_seq={}, max_seq={}, history_count={}",
                                    ctx.client_id, session_id, response.min_seq, response.max_seq, response.history_count
                                );
                                Ok(Some(BusinessMessage::subscribe_response(
                                    &session_id,
                                    response.min_seq,
                                    response.max_seq,
                                    response.history_count,
                                )))
                            }
                            None => {
                                tracing::warn!(
                                    "[TerminalHandler] Session {} not found for subscribe",
                                    session_id
                                );
                                Ok(Some(BusinessMessage::error(
                                    "SESSION_NOT_FOUND",
                                    &format!("Session {} not found", session_id),
                                )))
                            }
                        }
                    }
                    None => {
                        tracing::warn!(
                            "[TerminalHandler] No WebSocket sender for client {}",
                            ctx.client_id
                        );
                        Ok(Some(BusinessMessage::error(
                            "NO_CONNECTION",
                            "WebSocket connection not found",
                        )))
                    }
                }
            }
            TerminalAction::Unsubscribe => {
                let global_manager = GlobalOutputManager::global();

                if global_manager
                    .unsubscribe(&session_id, &ctx.client_id)
                    .await
                {
                    info!(
                        "Client {} unsubscribed from session {}",
                        ctx.client_id, session_id
                    );
                    Ok(Some(BusinessMessage::unsubscribe_response(&session_id)))
                } else {
                    Ok(Some(BusinessMessage::error(
                        "SESSION_NOT_FOUND",
                        &format!("Session {} not found", session_id),
                    )))
                }
            }
            // 其他动作类型不需要处理（如 Output, SubscribeResponse, UnsubscribeResponse）
            _ => Ok(None),
        }
    }
}
