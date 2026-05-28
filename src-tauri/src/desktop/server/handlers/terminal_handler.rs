//! Terminal Handler - 终端消息处理器
//!
//! 处理 `Message::Terminal` 消息，包括输入、订阅、取消订阅等操作
//! 委托给具体的服务层处理

use crate::desktop::server::message::{Message as BusinessMessage, TerminalAction, TerminalPayload};
use crate::desktop::server::router::handler::RouteHandler;
use crate::desktop::server::services::terminal_service::handle_input;
use crate::desktop::session::SessionManager;
use crate::desktop::websocket_manager::WebSocketManager;
use crate::shared::websocket::server::context::RouteContext;
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;
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
        let (session_id, action, message_id) = match message {
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
            TerminalAction::Subscribe { start_seq } => {
                let ws_manager = WebSocketManager::global();
                let subscription_manager = ws_manager.subscription_manager();

                let client_id = ctx.client_id.clone();
                match subscription_manager.subscribe(client_id.clone(), session_id.clone(), start_seq).await {
                    Ok(response) => {
                        info!("Client subscribed to session: {} (client: {})", session_id, client_id);
                        Ok(Some(BusinessMessage::subscribe_response(
                            &session_id,
                            response.current_max_seq,
                            response.history_count,
                        )))
                    }
                    Err(e) => {
                        tracing::error!("Subscribe failed: {}", e);
                        Ok(Some(BusinessMessage::error("SUBSCRIBE_FAILED", &e)))
                    }
                }
            }
            TerminalAction::Unsubscribe => {
                let ws_manager = WebSocketManager::global();
                let subscription_manager = ws_manager.subscription_manager();

                let client_id = ctx.client_id.clone();
                match subscription_manager.unsubscribe(&client_id, &session_id).await {
                    Ok(_) => {
                        info!("Client unsubscribed from session: {} (client: {})", session_id, client_id);
                        Ok(Some(BusinessMessage::unsubscribe_response(&session_id)))
                    }
                    Err(e) => {
                        tracing::error!("Unsubscribe failed: {}", e);
                        Ok(Some(BusinessMessage::error("UNSUBSCRIBE_FAILED", &e)))
                    }
                }
            }
            // 其他动作类型不需要处理（如 Output, SubscribeResponse, UnsubscribeResponse）
            _ => Ok(None),
        }
    }
}
