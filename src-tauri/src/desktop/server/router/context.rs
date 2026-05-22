//! Route Context - 连接上下文
//!
//! 处理器通过此对象与当前连接交互：发送响应、广播、查询连接信息等。

use crate::desktop::server::message::Message;
use crate::shared::websocket::{ConnectionId, ConnectionManager, WsServerEvent};
use crate::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tracing::debug;

/// 消息路由连接上下文
///
/// 封装当前连接的所有元数据和发送能力，处理器无需关心底层 WebSocket 实现。
pub struct RouteContext {
    pub connection_id: ConnectionId,
    pub addr: SocketAddr,
    pub client_id: String,
    pub connection_manager: Arc<ConnectionManager>,
    pub event_tx: broadcast::Sender<WsServerEvent>,
}

impl RouteContext {
    pub fn new(
        connection_id: ConnectionId,
        addr: SocketAddr,
        client_id: String,
        connection_manager: Arc<ConnectionManager>,
        event_tx: broadcast::Sender<WsServerEvent>,
    ) -> Self {
        Self {
            connection_id,
            addr,
            client_id,
            connection_manager,
            event_tx,
        }
    }

    /// 向当前连接发送响应消息
    pub async fn respond(&self, message: Message) -> Result<()> {
        let json = message.to_json()?;
        let ws_msg = WsMsg::Text(json);
        self.connection_manager
            .send_to(self.connection_id, ws_msg)
            .await
            .map_err(|e| crate::AppError::WebSocket(e))?;
        debug!("[RouteContext] Responded to {}", self.client_id);
        Ok(())
    }

    /// 向所有已连接客户端广播消息
    pub async fn broadcast(&self, message: Message) {
        let json = message.to_json().unwrap_or_default();
        let ws_msg = WsMsg::Text(json);
        self.connection_manager.broadcast(&ws_msg).await;
    }

    /// 向除当前连接外的所有客户端广播
    pub async fn broadcast_to_others(&self, message: Message) {
        let json = message.to_json().unwrap_or_default();
        let ws_msg = WsMsg::Text(json);
        self.connection_manager
            .broadcast_to_others(self.connection_id, &ws_msg)
            .await;
    }

    /// 向指定连接发送消息
    pub async fn send_to_connection(&self, connection_id: ConnectionId, message: Message) -> Result<()> {
        let json = message.to_json()?;
        let ws_msg = WsMsg::Text(json);
        self.connection_manager
            .send_to(connection_id, ws_msg)
            .await
            .map_err(|e| crate::AppError::WebSocket(e))
    }

    /// 向指定标签组广播
    pub async fn broadcast_to_tag(&self, tag: &str, message: Message) {
        let json = message.to_json().unwrap_or_default();
        let ws_msg = WsMsg::Text(json);
        self.connection_manager.broadcast_to_tag(tag, &ws_msg).await;
    }

    /// 发送服务器事件（用于与外部模块集成）
    pub fn emit_event(&self, event: WsServerEvent) {
        // 使用 try_send 避免阻塞，事件队列满时丢弃
        let _ = self.event_tx.send(event);
    }
}
