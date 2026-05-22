//! WebSocket 消息处理器
//!
//! 处理接收到的 WebSocket 消息，包括解析、路由和响应

use crate::shared::websocket::{WsMessage, WsServerEvent};
use crate::shared::websocket::traits::{DefaultClientInfo, ResponseHandler};
use base64::Engine;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use std::collections::HashMap;
use tracing::{debug, warn};

/// 消息处理依赖项
pub struct MessageHandlerDeps {
    /// 客户端信息映射
    pub clients: Arc<RwLock<HashMap<SocketAddr, DefaultClientInfo>>>,
    /// 消息发送通道
    pub tx: mpsc::Sender<WsMsg>,
    /// 事件广播通道
    pub event_tx: broadcast::Sender<WsServerEvent>,
    /// 客户端地址
    pub addr: SocketAddr,
}

/// 处理接收到的文本消息
pub async fn handle_text_message(
    text: &str,
    deps: &MessageHandlerDeps,
    handler: Option<&Arc<dyn crate::shared::websocket::MessageHandler>>,
    response_handler: Option<&Arc<dyn ResponseHandler>>,
) {
    debug!("[WsServer] <<< RECV from {}: {}", deps.addr, &text[..text.len().min(200)]);

    // 更新心跳时间
    {
        let mut clients = deps.clients.write().await;
        if let Some(client) = clients.get_mut(&deps.addr) {
            client.last_heartbeat = std::time::Instant::now();
        }
    }

    // 解析消息
    match WsMessage::from_json(text) {
        Ok(ws_msg) => {
            handle_parsed_message(&ws_msg, deps, handler, response_handler).await;
        }
        Err(e) => {
            // 向客户端返回解析错误
            let error_msg = WsMessage::error_with_id(
                "",
                "PARSE_ERROR",
                format!("Failed to parse message: {}", e),
            );
            let _ = deps.tx.send(WsMsg::Text(error_msg.to_json().unwrap_or_default())).await;
        }
    }
}

/// 处理已解析的 WebSocket 消息
async fn handle_parsed_message(
    ws_msg: &WsMessage,
    deps: &MessageHandlerDeps,
    handler: Option<&Arc<dyn crate::shared::websocket::MessageHandler>>,
    response_handler: Option<&Arc<dyn ResponseHandler>>,
) {
    match ws_msg.message_type() {
        _ => {
            tracing::info!(
                "[WsServer] Handling non-Ping message from {}: {}",
                deps.addr,
                &ws_msg.to_json().unwrap_or_default()[..ws_msg.to_json().unwrap_or_default().len().min(200)]
            );
            // 获取客户端信息用于 handler
            let client_info = {
                let clients = deps.clients.read().await;
                clients.get(&deps.addr).cloned()
            };

            // 调用 MessageHandler 处理业务逻辑
            let handler_result = if let (Some(ref h), Some(ref info)) = (handler, &client_info) {
                match h.handle_text(ws_msg, deps.addr, info) {
                    Ok(Some(response)) => {
                        let resp_json = response.to_json().unwrap_or_default();
                        debug!(
                            "[WsServer] >>> SEND to {}: {}",
                            deps.addr,
                            &resp_json[..resp_json.len().min(200)]
                        );
                        let _ = deps.tx.send(WsMsg::Text(resp_json)).await;
                    }
                    Ok(None) => {
                        // Handler 没有返回响应，检查是否需要自动响应
                        if ws_msg.expect_response() {
                            if let Some(ref resp_h) = response_handler {
                                let business_msg = if let WsMessage::Text { payload, .. } = ws_msg {
                                    crate::shared::enums::message::Message::from_json(&payload.content).ok()
                                } else {
                                    None
                                };

                                if let Some(biz_msg) = business_msg {
                                    if let Some(resp) = resp_h.handle_response(ws_msg, &biz_msg) {
                                        let resp_ws_msg = WsMessage::text_with_id(
                                            serde_json::to_string(&resp).unwrap_or_default(),
                                            ws_msg.message_id().unwrap_or("").to_string(),
                                            false,
                                        );
                                        let resp_json = resp_ws_msg.to_json().unwrap_or_default();
                                        debug!(
                                            "[WsServer] >>> AUTO RESPONSE to {}: {}",
                                            deps.addr,
                                            &resp_json[..resp_json.len().min(200)]
                                        );
                                        let _ = deps.tx.send(WsMsg::Text(resp_json)).await;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("[WsServer] Handler error for {}: {}", deps.addr, e);
                        let error_msg = WsMessage::error("HANDLER_ERROR", e.to_string());
                        let _ = deps.tx.send(WsMsg::Text(error_msg.to_json().unwrap_or_default())).await;
                    }
                }
                true
            } else {
                false
            };

            // 如果没有 handler 处理，则发送事件给外部
            if !handler_result {
                let client_id = client_info.as_ref().and_then(|c| c.client_id.clone());

                match ws_msg {
                    WsMessage::Text { message_id, payload, .. } => {
                        let _ = deps.event_tx.send(WsServerEvent::TextMessage {
                            addr: deps.addr,
                            client_id,
                            message_id: Some(message_id.clone()),
                            content: payload.content.clone(),
                        });
                    }
                    WsMessage::Binary { message_id, payload, .. } => {
                        let data = base64::engine::general_purpose::STANDARD
                            .decode(&payload.data)
                            .unwrap_or_else(|e| {
                                warn!("[WsServer] Base64 decode failed for {}: {}", deps.addr, e);
                                Vec::new()
                            });
                        let _ = deps.event_tx.send(WsServerEvent::BinaryMessage {
                            addr: deps.addr,
                            client_id,
                            message_id: Some(message_id.clone()),
                            data,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
}