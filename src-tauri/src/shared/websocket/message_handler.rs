//! WebSocket 消息处理器
//!
//! 处理接收到的 WebSocket 消息，包括解析、路由和响应

use crate::shared::websocket::{WsMessage, WsServerEvent, MessageHandler};
use crate::shared::websocket::traits::ResponseHandler;
use crate::shared::websocket::server::connection_manager::ConnectionManager;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tracing::{debug, info, warn};

/// 消息处理依赖项
pub struct MessageHandlerDeps {
    /// 连接管理器（通过 addr 查询 Connection）
    pub connection_manager: Arc<ConnectionManager>,
    /// 消息发送通道
    pub tx: mpsc::Sender<WsMsg>,
    /// 事件广播通道
    pub event_tx: broadcast::Sender<WsServerEvent>,
    /// 客户端地址
    pub addr: SocketAddr,
    /// 客户端 ID（认证后有值）
    pub client_id: Option<String>,
}

/// 处理接收到的 WebSocket 消息
///
/// 只处理 Text 和 Binary 类型，其他类型（Ping/Pong/Close/Ack）由框架自动处理
pub async fn handle_message(
    ws_msg: WsMsg,
    deps: &MessageHandlerDeps,
    handler: Option<&Arc<dyn MessageHandler>>,
    _response_handler: Option<&Arc<dyn ResponseHandler>>, // 暂时保留参数签名，后续可移除
) {
    // 通过 ConnectionManager 更新心跳时间
    if let Some(id) = deps.connection_manager.get_id_by_addr(&deps.addr).await {
        deps.connection_manager.update_heartbeat(id).await;
    }

    // 只处理 Text 和 Binary 类型，其他类型由框架自动处理
    match &ws_msg {
        WsMsg::Text(text) => {
            debug!("[WsServer] <<< RECV from {}: {}", deps.addr, &text[..text.len().min(200)]);

            // 验证消息是否为有效的 JSON
            if let Err(e) = WsMessage::from_json(text) {
                // 向客户端返回解析错误
                let error_msg = WsMessage::error_with_id(
                    "",
                    "PARSE_ERROR",
                    format!("Failed to parse message: {}", e),
                );
                let _ = deps.tx.send(WsMsg::Text(error_msg.to_json().unwrap_or_default())).await;
                return;
            }
        }
        WsMsg::Binary(data) => {
            debug!("[WsServer] <<< RECV Binary from {}: {} bytes", deps.addr, data.len());
        }
        // 其他消息类型（Ping/Pong/Close/Ack）由框架自动处理，这里不需要额外处理
        _ => {
            info!("[WsServer] Message type {:?} handled by framework", ws_msg);
            return;
        }
    }

    // 调用业务 handler 处理（传递原始 WsMsg，让 handler 自己决定如何解析）
    if let Some(h) = handler {
        // 获取 client_id 的引用用于日志
        let client_id_str = deps.client_id.as_deref().unwrap_or("unauthenticated");

        match h.handle(ws_msg, deps.addr, deps.client_id.as_deref()) {
            Ok(Some(response)) => {
                let resp_json = response.to_json().unwrap_or_default();
                debug!(
                    "[WsServer] >>> SEND to {} (client={}): {}",
                    deps.addr,
                    client_id_str,
                    &resp_json[..resp_json.len().min(200)]
                );
                let _ = deps.tx.send(WsMsg::Text(resp_json)).await;
            }
            Ok(None) => {
                // Handler 不返回响应
                info!("[WsServer] Handler returned None for message from {} (client={})", deps.addr, client_id_str);
            }
            Err(e) => {
                warn!("[WsServer] Handler error for {} (client={}): {}", deps.addr, client_id_str, e);
                let error_msg = WsMessage::error("HANDLER_ERROR", e);
                let _ = deps.tx.send(WsMsg::Text(error_msg.to_json().unwrap_or_default())).await;
            }
        }
    }
}