//! Subscribe Handler - 订阅消息处理器
//!
//! 处理 `Message::Subscribe` / `Message::Unsubscribe` 消息

use crate::desktop::server::message::Message as BusinessMessage;
use crate::shared::websocket::server::context::RouteContext;
use crate::desktop::server::router::handler::RouteHandler;
use crate::desktop::websocket_manager::WebSocketManager;
use crate::Result;
use async_trait::async_trait;
use tracing::info;

pub struct SubscribeHandler;

impl SubscribeHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RouteHandler for SubscribeHandler {
    async fn handle(
        &self,
        message: BusinessMessage,
        ctx: &RouteContext,
    ) -> Result<Option<BusinessMessage>> {
        match message {
            BusinessMessage::Subscribe {
                message_id,
                expect_response: _,
                timestamp: _,
                session_id,
                start_seq,
            } => {
                let ws_manager = WebSocketManager::global();
                let subscription_manager = ws_manager.subscription_manager();

                let client_id = ctx.client_id.clone();
                match subscription_manager.subscribe(client_id.clone(), session_id.clone(), start_seq).await {
                    Ok(response) => {
                        info!("Client subscribed to session: {} (client: {})", session_id, client_id);
                        Ok(Some(BusinessMessage::SubscribeResponse {
                            message_id,
                            expect_response: false,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                            session_id,
                            current_max_seq: response.current_max_seq,
                            history_count: response.history_count,
                        }))
                    }
                    Err(e) => {
                        tracing::error!("Subscribe failed: {}", e);
                        Ok(Some(BusinessMessage::error("SUBSCRIBE_FAILED", &e)))
                    }
                }
            }
            BusinessMessage::Unsubscribe {
                message_id,
                expect_response: _,
                timestamp: _,
                session_id,
            } => {
                let ws_manager = WebSocketManager::global();
                let subscription_manager = ws_manager.subscription_manager();

                let client_id = ctx.client_id.clone();
                match subscription_manager.unsubscribe(&client_id, &session_id).await {
                    Ok(_) => {
                        info!("Client unsubscribed from session: {} (client: {})", session_id, client_id);
                        Ok(Some(BusinessMessage::UnsubscribeResponse {
                            message_id,
                            expect_response: false,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                            session_id,
                        }))
                    }
                    Err(e) => {
                        tracing::error!("Unsubscribe failed: {}", e);
                        Ok(Some(BusinessMessage::error("UNSUBSCRIBE_FAILED", &e)))
                    }
                }
            }
            _ => Ok(None),
        }
    }
}